//! Game launch engine.
//!
//! * Commands are always executed with an explicit argv (`tokio::process::Command`),
//!   never through a shell, so user-provided arguments cannot inject commands.
//! * Launch arguments and wrapper commands are tokenized with a small quote-aware
//!   splitter that performs no expansion of any kind.
//! * Every launch is tracked asynchronously; the UI never blocks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::{now_ts, Game, GameProfile, Session};
use crate::platforms::{self, is_executable, which, LaunchPlan, Tracking};

const MAX_ARGS_LEN: usize = 8192;
/// A non-zero exit faster than this is treated as a failed launch.
const QUICK_FAIL_SECS: i64 = 10;

/// Tokenize an argument string like a POSIX shell would split words — quotes
/// and backslash escapes only. No variable, glob, or command expansion.
pub fn split_args(input: &str) -> AppResult<Vec<String>> {
    if input.len() > MAX_ARGS_LEN {
        return Err(AppError::Invalid("launch arguments are too long".into()));
    }
    if input
        .chars()
        .any(|c| c == '\0' || (c.is_control() && !c.is_whitespace()))
    {
        return Err(AppError::Invalid(
            "launch arguments contain control characters".into(),
        ));
    }
    let mut out = vec![];
    let mut cur = String::new();
    let mut has = false;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                has = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => {
                            return Err(AppError::Invalid(
                                "unterminated single quote in arguments".into(),
                            ))
                        }
                    }
                }
            }
            '"' => {
                has = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(e @ ('"' | '\\' | '$' | '`')) => cur.push(e),
                            Some(e) => {
                                cur.push('\\');
                                cur.push(e);
                            }
                            None => {
                                return Err(AppError::Invalid(
                                    "unterminated escape in arguments".into(),
                                ))
                            }
                        },
                        Some(ch) => cur.push(ch),
                        None => {
                            return Err(AppError::Invalid(
                                "unterminated double quote in arguments".into(),
                            ))
                        }
                    }
                }
            }
            '\\' => match chars.next() {
                Some(e) => {
                    cur.push(e);
                    has = true;
                }
                None => return Err(AppError::Invalid("trailing backslash in arguments".into())),
            },
            c if c.is_whitespace() => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    Ok(out)
}

fn valid_env_key(k: &str) -> bool {
    let mut chars = k.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && k.len() <= 128
}

fn check_abs_file(label: &str, p: &Option<String>) -> AppResult<()> {
    if let Some(p) = p.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let path = Path::new(p);
        if !path.is_absolute() {
            return Err(AppError::Invalid(format!(
                "{label} must be an absolute path"
            )));
        }
        if !path.is_file() {
            return Err(AppError::Invalid(format!("{label} not found: {p}")));
        }
    }
    Ok(())
}

/// Validate a profile before it is saved or used.
pub fn validate_profile(p: &GameProfile) -> AppResult<()> {
    split_args(&p.launch_args)?;
    if !["default", "native", "wine", "proton"].contains(&p.runner.as_str()) {
        return Err(AppError::Invalid(format!("unknown runner '{}'", p.runner)));
    }
    if p.env.len() > 64 {
        return Err(AppError::Invalid(
            "too many environment variables (max 64)".into(),
        ));
    }
    for e in &p.env {
        if !valid_env_key(e.key.trim()) {
            return Err(AppError::Invalid(format!(
                "invalid environment variable name '{}' (use letters, digits and _)",
                e.key
            )));
        }
        if e.value.contains('\0') || e.value.len() > 4096 {
            return Err(AppError::Invalid(format!(
                "invalid value for environment variable '{}'",
                e.key
            )));
        }
    }
    if let Some(wd) = p
        .working_dir
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let path = Path::new(wd);
        if !path.is_absolute() || !path.is_dir() {
            return Err(AppError::Invalid(format!(
                "working directory not found: {wd}"
            )));
        }
    }
    check_abs_file("executable override", &p.executable_override)?;
    check_abs_file("Wine executable", &p.wine_binary)?;
    check_abs_file("Proton script", &p.proton_path)?;
    if let Some(prefix) = p
        .wine_prefix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !Path::new(prefix).is_absolute() {
            return Err(AppError::Invalid(
                "Wine prefix must be an absolute path".into(),
            ));
        }
    }
    let wrapper = split_args(&p.wrapper)?;
    if let Some(bin) = wrapper.first() {
        if which(bin).is_none() {
            return Err(AppError::Invalid(format!(
                "wrapper command '{bin}' was not found in PATH"
            )));
        }
    }
    Ok(())
}

fn is_windows_binary(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| ["exe", "msi", "bat"].contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Fully resolved command, ready to spawn.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub tracking: Tracking,
    /// A prefix directory inside VANTA's data dir that must exist before launch.
    pub create_dir: Option<PathBuf>,
}

/// Combine the platform's launch plan with the user's profile.
pub fn prepare(
    game: &Game,
    profile: &GameProfile,
    prefixes_dir: &Path,
) -> AppResult<PreparedLaunch> {
    validate_profile(profile)?;
    let adapter = platforms::adapter_for(&game.platform)
        .ok_or_else(|| AppError::Launch(format!("no adapter for platform '{}'", game.platform)))?;
    let extra = split_args(&profile.launch_args)?;

    // Executable override applies to directly-launched games only.
    let mut game = game.clone();
    if let Some(ov) = profile
        .executable_override
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        if game.platform == "steam" {
            return Err(AppError::Invalid(
                "executable override is not supported for Steam games; Steam launches them".into(),
            ));
        }
        game.executable = Some(ov.trim().to_string());
        game.platform = "local".into();
    }
    let effective_adapter = platforms::adapter_for(&game.platform).unwrap_or(adapter);
    let plan: LaunchPlan = effective_adapter
        .launch_plan(&game, &extra)
        .map_err(AppError::Launch)?;

    let mut prepared = PreparedLaunch {
        program: plan.program.clone(),
        args: plan.args.clone(),
        env: plan.env.clone(),
        cwd: plan.cwd.clone(),
        tracking: plan.tracking.clone(),
        create_dir: None,
    };

    if !plan.supports_profile_env {
        return Ok(prepared);
    }

    if let Some(wd) = profile
        .working_dir
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        prepared.cwd = Some(PathBuf::from(wd.trim()));
    }

    // Legendary manages Wine itself; forward the profile's Wine settings as its flags.
    if game.platform == "epic" && prepared.args.len() >= 2 {
        let mut flags = vec![];
        if let Some(w) = profile
            .wine_binary
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            flags.extend(["--wine".to_string(), w.trim().to_string()]);
        }
        if let Some(p) = profile
            .wine_prefix
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            flags.extend(["--wine-prefix".to_string(), p.trim().to_string()]);
        }
        prepared.args.splice(2..2, flags);
    }

    // Wine / Proton wrapping for direct launches.
    let direct = plan.tracking == Tracking::Child && game.platform != "epic";
    let runner = match profile.runner.as_str() {
        "default" if direct && is_windows_binary(&plan.program) => {
            if profile.wine_binary.is_some() || which("wine").is_some() {
                "wine"
            } else {
                "proton"
            }
        }
        "default" => "native",
        r => r,
    };
    let default_prefix = prefixes_dir.join(sanitize_id(&game.id));
    match runner {
        "wine" if direct => {
            let wine = profile
                .wine_binary
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| which("wine"))
                .ok_or_else(|| AppError::Launch("Wine is not installed (try `sudo pacman -S wine`), or set a Wine executable in this game's profile.".into()))?;
            let prefix = match profile
                .wine_prefix
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                Some(p) => PathBuf::from(p.trim()),
                None => {
                    prepared.create_dir = Some(default_prefix.clone());
                    default_prefix
                }
            };
            let mut args = vec![plan.program.to_string_lossy().into_owned()];
            args.extend(plan.args.iter().cloned());
            prepared.program = wine;
            prepared.args = args;
            prepared
                .env
                .push(("WINEPREFIX".into(), prefix.to_string_lossy().into_owned()));
        }
        "proton" if direct => {
            let proton = profile
                .proton_path
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    platforms::runners::detect()
                        .into_iter()
                        .find(|r| r.kind == "proton")
                        .map(|r| PathBuf::from(r.path))
                })
                .ok_or_else(|| AppError::Launch("No Proton installation found. Install Proton from Steam or set the Proton path in this game's profile.".into()))?;
            let client = platforms::runners::steam_client_path().ok_or_else(|| {
                AppError::Launch(
                    "Proton requires a Steam installation (STEAM_COMPAT_CLIENT_INSTALL_PATH)."
                        .into(),
                )
            })?;
            let prefix = match profile
                .wine_prefix
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                Some(p) => PathBuf::from(p.trim()),
                None => default_prefix,
            };
            prepared.create_dir = Some(prefix.clone()).filter(|p| p.starts_with(prefixes_dir));
            let mut args = vec![
                "run".to_string(),
                plan.program.to_string_lossy().into_owned(),
            ];
            args.extend(plan.args.iter().cloned());
            prepared.program = proton;
            prepared.args = args;
            prepared.env.push((
                "STEAM_COMPAT_DATA_PATH".into(),
                prefix.to_string_lossy().into_owned(),
            ));
            prepared.env.push((
                "STEAM_COMPAT_CLIENT_INSTALL_PATH".into(),
                client.to_string_lossy().into_owned(),
            ));
        }
        "native" | "default" if direct && !is_executable(&prepared.program) => {
            // Shell scripts without the exec bit can still be run via sh; never chmod user files.
            if prepared
                .program
                .extension()
                .map(|e| e == "sh")
                .unwrap_or(false)
            {
                let script = prepared.program.to_string_lossy().into_owned();
                prepared.program = PathBuf::from("/bin/sh");
                prepared.args.insert(0, script);
            } else if is_windows_binary(&prepared.program) {
                return Err(AppError::Launch("This is a Windows executable. Choose Wine or Proton as the runner in the game's profile.".into()));
            } else {
                return Err(AppError::Launch(format!(
                        "{} is not marked executable. Run `chmod +x` on it, or pick a different executable.",
                        prepared.program.display()
                    )));
            }
        }
        _ => {}
    }

    for e in &profile.env {
        prepared
            .env
            .push((e.key.trim().to_string(), e.value.clone()));
    }

    let wrapper = split_args(&profile.wrapper)?;
    if let Some((bin, rest)) = wrapper.split_first() {
        let bin = which(bin)
            .ok_or_else(|| AppError::Launch(format!("wrapper '{bin}' not found in PATH")))?;
        let mut args: Vec<String> = rest.to_vec();
        args.push(prepared.program.to_string_lossy().into_owned());
        args.append(&mut prepared.args);
        prepared.program = bin;
        prepared.args = args;
    }
    Ok(prepared)
}

// ------------------------------------------------------------------ tracking

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    pub game_id: String,
    /// "starting" | "running" | "ended" | "failed" | "untracked"
    pub state: String,
    pub message: Option<String>,
    pub session: Option<Session>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSession {
    pub game_id: String,
    pub game_name: String,
    pub session_id: Option<i64>,
    pub pid: Option<u32>,
    pub started_at: i64,
    pub state: String,
}

pub type Emitter = Arc<dyn Fn(SessionEvent) + Send + Sync>;
pub type ActiveMap = Arc<Mutex<HashMap<String, ActiveSession>>>;

fn read_cmdline(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if raw.is_empty() {
        return None;
    }
    Some(
        raw.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
    )
}

fn proc_pids() -> Vec<u32> {
    std::fs::read_dir("/proc")
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Find the Steam reaper process for an app (`reaper SteamLaunch AppId=<id> -- ...`).
pub fn find_steam_app(app_id: &str) -> Option<u32> {
    let needle = format!("AppId={app_id}");
    proc_pids().into_iter().find(|pid| {
        read_cmdline(*pid)
            .map(|cl| cl.iter().any(|a| a == &needle) && cl.iter().any(|a| a == "SteamLaunch"))
            .unwrap_or(false)
    })
}

/// Find a process whose executable, arguments or Wine path lies inside `dir`.
pub fn find_in_dir(dir: &Path, exclude: &[u32]) -> Option<u32> {
    let unix = dir.to_string_lossy().into_owned();
    if unix.len() < 4 {
        return None;
    }
    let wine = format!("Z:{}", unix.replace('/', "\\")).to_ascii_lowercase();
    let me = std::process::id();
    proc_pids()
        .into_iter()
        .filter(|p| *p != me && !exclude.contains(p))
        .find(|pid| {
            let exe_hit = std::fs::read_link(format!("/proc/{pid}/exe"))
                .map(|e| e.starts_with(dir))
                .unwrap_or(false);
            exe_hit
                || read_cmdline(*pid)
                    .map(|cl| {
                        cl.iter().take(2).any(|a| {
                            a.starts_with(&unix) || a.to_ascii_lowercase().starts_with(&wine)
                        })
                    })
                    .unwrap_or(false)
        })
}

fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
        && std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| {
                !s.rsplit(')')
                    .next()
                    .unwrap_or("")
                    .trim_start()
                    .starts_with('Z')
            })
            .unwrap_or(false)
}

fn tail(path: &Path, lines: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if v.is_empty() {
        return None;
    }
    Some(v[v.len().saturating_sub(lines)..].join("\n"))
}

pub struct LaunchContext {
    pub db: Arc<Mutex<Db>>,
    pub active: ActiveMap,
    pub emit: Emitter,
    pub logs_dir: PathBuf,
    pub poll_secs: u64,
}

fn db_lock(db: &Arc<Mutex<Db>>) -> std::sync::MutexGuard<'_, Db> {
    db.lock().unwrap_or_else(|p| p.into_inner())
}

fn set_active(active: &ActiveMap, game_id: &str, f: impl FnOnce(&mut ActiveSession)) {
    if let Some(a) = active
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(game_id)
    {
        f(a);
    }
}

fn remove_active(active: &ActiveMap, game_id: &str) {
    active
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(game_id);
}

/// Record a failed launch and notify the UI.
fn report_failure(ctx: &LaunchContext, game: &Game, msg: String) -> AppError {
    tracing::error!(game = %game.id, error = %msg, "launch failed");
    let session = db_lock(&ctx.db).record_failed_launch(&game.id, &msg).ok();
    remove_active(&ctx.active, &game.id);
    (ctx.emit)(SessionEvent {
        game_id: game.id.clone(),
        state: "failed".into(),
        message: Some(msg.clone()),
        session,
    });
    AppError::Launch(msg)
}

/// Spawn the game and track it in the background. Returns once the process has
/// been started (or failed to start); never waits for the game to exit.
pub async fn launch(ctx: LaunchContext, game: Game, prepared: PreparedLaunch) -> AppResult<()> {
    {
        let mut active = ctx.active.lock().unwrap_or_else(|p| p.into_inner());
        if active.contains_key(&game.id) {
            return Err(AppError::Launch(format!(
                "{} is already running",
                game.name
            )));
        }
        active.insert(
            game.id.clone(),
            ActiveSession {
                game_id: game.id.clone(),
                game_name: game.name.clone(),
                session_id: None,
                pid: None,
                started_at: now_ts(),
                state: "starting".into(),
            },
        );
    }
    (ctx.emit)(SessionEvent {
        game_id: game.id.clone(),
        state: "starting".into(),
        message: None,
        session: None,
    });

    if let Some(dir) = &prepared.create_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return Err(report_failure(
                &ctx,
                &game,
                format!("could not create prefix {}: {e}", dir.display()),
            ));
        }
    }
    if let Some(cwd) = &prepared.cwd {
        if !cwd.is_dir() {
            return Err(report_failure(
                &ctx,
                &game,
                format!("working directory not found: {}", cwd.display()),
            ));
        }
    }

    let _ = std::fs::create_dir_all(&ctx.logs_dir);
    let log_path = ctx.logs_dir.join(format!("{}.log", sanitize_id(&game.id)));
    let log = std::fs::File::create(&log_path).ok();

    let mut cmd = tokio::process::Command::new(&prepared.program);
    cmd.args(&prepared.args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(false)
        .process_group(0);
    match log.and_then(|f| f.try_clone().ok().map(|g| (f, g))) {
        Some((out, err)) => {
            cmd.stdout(out).stderr(err);
        }
        None => {
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
    }
    for (k, v) in &prepared.env {
        cmd.env(k, v);
    }
    if let Some(cwd) = &prepared.cwd {
        cmd.current_dir(cwd);
    }
    tracing::info!(game = %game.id, program = %prepared.program.display(), args = ?prepared.args, "launching game");

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(report_failure(
                &ctx,
                &game,
                format!("failed to start {}: {e}", prepared.program.display()),
            ))
        }
    };
    let child_pid = child.id();

    match prepared.tracking.clone() {
        Tracking::Child => {
            let started = now_ts();
            let session_id = db_lock(&ctx.db).start_session(&game.id, started)?;
            set_active(&ctx.active, &game.id, |a| {
                a.session_id = Some(session_id);
                a.pid = child_pid;
                a.started_at = started;
                a.state = "running".into();
            });
            let session = db_lock(&ctx.db).get_session(session_id).ok();
            (ctx.emit)(SessionEvent {
                game_id: game.id.clone(),
                state: "running".into(),
                message: None,
                session,
            });
            tokio::spawn(async move {
                let status = child.wait().await;
                let ended = now_ts();
                let (code, err) = match status {
                    Ok(s) if s.success() => (s.code(), None),
                    Ok(s) => {
                        let quick = ended - started < QUICK_FAIL_SECS;
                        let msg = quick.then(|| {
                            let detail = tail(&log_path, 6)
                                .map(|t| format!("\n{t}"))
                                .unwrap_or_default();
                            match s.code() {
                                Some(c) => {
                                    format!("exited with code {c} right after launch{detail}")
                                }
                                None => {
                                    format!("was terminated by a signal right after launch{detail}")
                                }
                            }
                        });
                        (s.code(), msg)
                    }
                    Err(e) => (None, Some(format!("lost track of the game process: {e}"))),
                };
                finish(&ctx, &game, session_id, ended, code, err);
            });
        }
        Tracking::SteamApp(app_id) => {
            // The `steam` process just forwards the request (or *is* the client);
            // reap it in the background without treating its exit as the game's.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            tokio::spawn(track_external(ctx, game, move |_| find_steam_app(&app_id)));
        }
        Tracking::PathMatch(dir) => {
            let exclude: Vec<u32> = child_pid.into_iter().collect();
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            tokio::spawn(track_external(ctx, game, move |_| {
                find_in_dir(&dir, &exclude)
            }));
        }
    }
    Ok(())
}

fn finish(
    ctx: &LaunchContext,
    game: &Game,
    session_id: i64,
    ended: i64,
    code: Option<i32>,
    err: Option<String>,
) {
    let session = db_lock(&ctx.db).end_session(session_id, ended, code, err.as_deref());
    remove_active(&ctx.active, &game.id);
    match session {
        Ok(s) => {
            let state = if err.is_some() { "failed" } else { "ended" };
            tracing::info!(game = %game.id, duration = s.duration_s, exit = ?code, "session ended");
            (ctx.emit)(SessionEvent {
                game_id: game.id.clone(),
                state: state.into(),
                message: err.map(|e| format!("{} {e}", game.name)),
                session: Some(s),
            });
        }
        Err(e) => {
            tracing::error!(game = %game.id, error = %e, "failed to record session end");
            (ctx.emit)(SessionEvent {
                game_id: game.id.clone(),
                state: "failed".into(),
                message: Some(format!("could not record session: {e}")),
                session: None,
            });
        }
    }
}

/// Track a game started through another client by polling `/proc`.
async fn track_external<F>(ctx: LaunchContext, game: Game, finder: F)
where
    F: Fn(()) -> Option<u32> + Send + 'static,
{
    let detect_deadline = Instant::now() + Duration::from_secs(180);
    let pid = loop {
        if let Some(pid) = finder(()) {
            break Some(pid);
        }
        if Instant::now() > detect_deadline {
            break None;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    };
    let Some(pid) = pid else {
        remove_active(&ctx.active, &game.id);
        (ctx.emit)(SessionEvent {
            game_id: game.id.clone(),
            state: "untracked".into(),
            message: Some(format!(
                "The launch request for {} was sent, but VANTA could not detect the game process within 3 minutes, so this session is not tracked.",
                game.name
            )),
            session: None,
        });
        return;
    };
    let started = now_ts();
    let session_id = match db_lock(&ctx.db).start_session(&game.id, started) {
        Ok(id) => id,
        Err(e) => {
            remove_active(&ctx.active, &game.id);
            (ctx.emit)(SessionEvent {
                game_id: game.id.clone(),
                state: "failed".into(),
                message: Some(format!("could not record session: {e}")),
                session: None,
            });
            return;
        }
    };
    set_active(&ctx.active, &game.id, |a| {
        a.session_id = Some(session_id);
        a.pid = Some(pid);
        a.started_at = started;
        a.state = "running".into();
    });
    let session = db_lock(&ctx.db).get_session(session_id).ok();
    (ctx.emit)(SessionEvent {
        game_id: game.id.clone(),
        state: "running".into(),
        message: None,
        session,
    });
    let mut current = pid;
    loop {
        tokio::time::sleep(Duration::from_secs(ctx.poll_secs.max(2))).await;
        if pid_alive(current) {
            continue;
        }
        // The tracked process can hand off to another (launchers, updaters).
        match finder(()) {
            Some(next) => {
                current = next;
                set_active(&ctx.active, &game.id, |a| a.pid = Some(next));
            }
            None => break,
        }
    }
    finish(&ctx, &game, session_id, now_ts(), None, None);
}

/// Ask a running game to quit (SIGTERM to its process group, then the pid).
pub fn stop(active: &ActiveMap, game_id: &str) -> AppResult<()> {
    let pid = active
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(game_id)
        .and_then(|a| a.pid)
        .ok_or_else(|| {
            AppError::NotFound("this game is not running or its process is unknown".into())
        })?;
    let kill = which("kill").ok_or_else(|| AppError::Other("`kill` utility not found".into()))?;
    let pgid_ok = std::process::Command::new(&kill)
        .args(["-TERM", "--", &format!("-{pid}")])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !pgid_ok {
        let status = std::process::Command::new(&kill)
            .args(["-TERM", &pid.to_string()])
            .status()?;
        if !status.success() {
            return Err(AppError::Other(format!("could not signal process {pid}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EnvVar, ManualGameInput};

    #[test]
    fn splits_arguments_safely() {
        assert_eq!(
            split_args("-novid  -high").unwrap(),
            vec!["-novid", "-high"]
        );
        assert_eq!(
            split_args(r#"--name "My Player" 'a b'"#).unwrap(),
            vec!["--name", "My Player", "a b"]
        );
        assert_eq!(split_args(r"a\ b").unwrap(), vec!["a b"]);
        // Shell metacharacters are plain data, never interpreted.
        assert_eq!(
            split_args("; rm -rf ~ && $(id) `x` | y").unwrap(),
            vec![";", "rm", "-rf", "~", "&&", "$(id)", "`x`", "|", "y"]
        );
        assert_eq!(split_args("").unwrap(), Vec::<String>::new());
        assert!(split_args("\"open").is_err());
        assert!(split_args("'open").is_err());
        assert!(split_args("bad\0arg").is_err());
        assert!(split_args(&"x".repeat(MAX_ARGS_LEN + 1)).is_err());
    }

    #[test]
    fn validates_profiles() {
        let mut p = GameProfile::default_for("g");
        assert!(validate_profile(&p).is_ok());
        p.env = vec![EnvVar {
            key: "1BAD".into(),
            value: "x".into(),
        }];
        assert!(validate_profile(&p).is_err());
        p.env = vec![EnvVar {
            key: "DXVK_HUD".into(),
            value: "fps".into(),
        }];
        assert!(validate_profile(&p).is_ok());
        p.runner = "dosbox".into();
        assert!(validate_profile(&p).is_err());
        p.runner = "wine".into();
        p.wine_binary = Some("/no/such/wine".into());
        assert!(validate_profile(&p).is_err());
        p.wine_binary = None;
        p.working_dir = Some("relative".into());
        assert!(validate_profile(&p).is_err());
        p.working_dir = None;
        p.wrapper = "definitely-not-a-real-wrapper-binary".into();
        assert!(validate_profile(&p).is_err());
    }

    fn manual_game(dir: &Path, script: &str, mode: u32) -> (Db, Game) {
        use std::os::unix::fs::PermissionsExt;
        let exe = dir.join(script);
        std::fs::write(&exe, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(mode)).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        let g = db
            .add_manual_game(&ManualGameInput {
                name: "T".into(),
                executable: exe.to_string_lossy().into(),
                install_dir: None,
                launch_args: None,
                runner: None,
                platform: None,
                cover: None,
            })
            .unwrap();
        (db, g)
    }

    #[test]
    fn prepares_native_launch_with_args_env_and_wrapper() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, g) = manual_game(dir.path(), "run.sh", 0o755);
        let mut p = GameProfile::default_for(&g.id);
        p.launch_args = "--fullscreen \"--name=A B\"".into();
        p.env = vec![EnvVar {
            key: "FOO".into(),
            value: "bar baz".into(),
        }];
        p.wrapper = "env".into(); // `env` exists everywhere and acts as a transparent wrapper
        let prep = prepare(&g, &p, dir.path()).unwrap();
        assert!(prep.program.ends_with("env"));
        assert!(prep.args[0].ends_with("run.sh"));
        assert_eq!(&prep.args[1..], ["--fullscreen", "--name=A B"]);
        assert!(prep.env.contains(&("FOO".into(), "bar baz".into())));
        assert_eq!(prep.tracking, Tracking::Child);
        assert_eq!(prep.cwd.as_deref(), Some(dir.path()));
    }

    #[test]
    fn non_executable_scripts_run_via_sh_others_fail() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, g) = manual_game(dir.path(), "run.sh", 0o644);
        let prep = prepare(&g, &GameProfile::default_for(&g.id), dir.path()).unwrap();
        assert_eq!(prep.program, PathBuf::from("/bin/sh"));
        let (_db, g) = manual_game(dir.path(), "game.bin", 0o644);
        let err = prepare(&g, &GameProfile::default_for(&g.id), dir.path()).unwrap_err();
        assert!(err.to_string().contains("not marked executable"));
    }

    #[test]
    fn missing_executable_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, mut g) = manual_game(dir.path(), "run.sh", 0o755);
        std::fs::remove_file(dir.path().join("run.sh")).unwrap();
        g.executable = Some(dir.path().join("run.sh").to_string_lossy().into());
        let err = prepare(&g, &GameProfile::default_for(&g.id), dir.path()).unwrap_err();
        assert!(err.to_string().contains("executable not found"), "{err}");
    }

    #[test]
    fn wine_runner_wraps_windows_exe() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Game.exe");
        std::fs::write(&exe, "MZ").unwrap();
        // A stand-in "wine" binary so the test doesn't depend on Wine being installed.
        let fake_wine = dir.path().join("wine");
        std::fs::write(&fake_wine, "#!/bin/sh\n").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        let g = db
            .add_manual_game(&ManualGameInput {
                name: "W".into(),
                executable: exe.to_string_lossy().into(),
                install_dir: None,
                launch_args: None,
                runner: Some("wine".into()),
                platform: None,
                cover: None,
            })
            .unwrap();
        let mut p = db.get_profile(&g.id).unwrap();
        p.wine_binary = Some(fake_wine.to_string_lossy().into());
        p.launch_args = "-windowed".into();
        let prefixes = dir.path().join("prefixes");
        let prep = prepare(&g, &p, &prefixes).unwrap();
        assert_eq!(prep.program, fake_wine);
        assert_eq!(
            prep.args,
            vec![exe.to_string_lossy().into_owned(), "-windowed".into()]
        );
        let prefix = prep.env.iter().find(|(k, _)| k == "WINEPREFIX").unwrap();
        assert!(prefix.1.starts_with(prefixes.to_str().unwrap()));
        assert!(prep.create_dir.is_some());
    }

    #[tokio::test]
    async fn tracks_child_session_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let (db, g) = manual_game(dir.path(), "run.sh", 0o755);
        let db = Arc::new(Mutex::new(db));
        let events: Arc<Mutex<Vec<SessionEvent>>> = Arc::default();
        let ev = events.clone();
        let ctx = LaunchContext {
            db: db.clone(),
            active: Arc::default(),
            emit: Arc::new(move |e| ev.lock().unwrap().push(e)),
            logs_dir: dir.path().join("logs"),
            poll_secs: 2,
        };
        let active = ctx.active.clone();
        let prep = prepare(&g, &GameProfile::default_for(&g.id), dir.path()).unwrap();
        launch(ctx, g.clone(), prep).await.unwrap();
        for _ in 0..50 {
            if events.lock().unwrap().iter().any(|e| e.state == "ended") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let states: Vec<String> = events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.state.clone())
            .collect();
        assert_eq!(states, vec!["starting", "running", "ended"]);
        assert!(active.lock().unwrap().is_empty());
        let sessions = db.lock().unwrap().game_sessions(&g.id, 5).unwrap();
        assert_eq!(sessions[0].status, "completed");
        assert_eq!(sessions[0].exit_code, Some(0));
    }

    #[tokio::test]
    async fn quick_nonzero_exit_is_a_failed_launch() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("crash.sh");
        std::fs::write(&exe, "#!/bin/sh\necho 'missing libfoo.so' >&2\nexit 3\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        let g = db
            .add_manual_game(&ManualGameInput {
                name: "C".into(),
                executable: exe.to_string_lossy().into(),
                install_dir: None,
                launch_args: None,
                runner: None,
                platform: None,
                cover: None,
            })
            .unwrap();
        let db = Arc::new(Mutex::new(db));
        let events: Arc<Mutex<Vec<SessionEvent>>> = Arc::default();
        let ev = events.clone();
        let ctx = LaunchContext {
            db: db.clone(),
            active: Arc::default(),
            emit: Arc::new(move |e| ev.lock().unwrap().push(e)),
            logs_dir: dir.path().join("logs"),
            poll_secs: 2,
        };
        let prep = prepare(&g, &GameProfile::default_for(&g.id), dir.path()).unwrap();
        launch(ctx, g.clone(), prep).await.unwrap();
        for _ in 0..50 {
            if events.lock().unwrap().iter().any(|e| e.state == "failed") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let evs = events.lock().unwrap();
        let failed = evs
            .iter()
            .find(|e| e.state == "failed")
            .expect("failure event");
        let msg = failed.message.as_deref().unwrap();
        assert!(
            msg.contains("code 3") && msg.contains("missing libfoo.so"),
            "{msg}"
        );
        assert_eq!(db.lock().unwrap().statistics().unwrap().failed_launches, 1);
    }

    #[tokio::test]
    async fn double_launch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (db, g) = manual_game(dir.path(), "run.sh", 0o755);
        let active: ActiveMap = Arc::default();
        active.lock().unwrap().insert(
            g.id.clone(),
            ActiveSession {
                game_id: g.id.clone(),
                game_name: "T".into(),
                session_id: None,
                pid: None,
                started_at: 0,
                state: "running".into(),
            },
        );
        let ctx = LaunchContext {
            db: Arc::new(Mutex::new(db)),
            active,
            emit: Arc::new(|_| {}),
            logs_dir: dir.path().into(),
            poll_secs: 2,
        };
        let prep = prepare(&g, &GameProfile::default_for(&g.id), dir.path()).unwrap();
        assert!(launch(ctx, g, prep)
            .await
            .unwrap_err()
            .to_string()
            .contains("already running"));
    }
}
