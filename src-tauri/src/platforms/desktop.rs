//! XDG desktop entry discovery: finds installed applications in the `Game`
//! category so they can be imported into the library with one click.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{home, which};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGame {
    pub name: String,
    pub executable: String,
    pub args: Vec<String>,
    pub desktop_file: String,
    pub icon: Option<String>,
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![dirs::data_dir().unwrap_or_else(|| home().join(".local/share"))];
    let sys =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs.push(home().join(".local/share/flatpak/exports/share"));
    dirs.into_iter().map(|d| d.join("applications")).collect()
}

/// Split a desktop-entry Exec value into argv, honoring double quotes and
/// dropping field codes (%f, %U, ...). No shell is involved.
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            '\\' if in_quotes => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out.into_iter()
        .filter(|t| !(t.len() == 2 && t.starts_with('%')))
        .map(|t| t.replace("%%", "%"))
        .collect()
}

pub fn parse_desktop_file(path: &Path) -> Option<DesktopGame> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let (mut name, mut exec, mut cats, mut icon) = (None, None, String::new(), None);
    let (mut hidden, mut app_type) = (false, String::from("Application"));
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "Name" => name = Some(v.trim().to_string()),
            "Exec" => exec = Some(v.trim().to_string()),
            "Categories" => cats = v.to_string(),
            "Icon" => icon = Some(v.trim().to_string()),
            "NoDisplay" | "Hidden" => hidden |= v.trim() == "true",
            "Type" => app_type = v.trim().to_string(),
            _ => {}
        }
    }
    if hidden || app_type != "Application" || !cats.split(';').any(|c| c == "Game") {
        return None;
    }
    let exec = exec?;
    // Store and launcher shortcuts are already covered by their adapters.
    let lower = exec.to_ascii_lowercase();
    if ["steam://", "heroic://", "legendary", "itch://", "lutris:"]
        .iter()
        .any(|s| lower.contains(s))
    {
        return None;
    }
    let mut argv = split_exec(&exec);
    if argv.first().map(|p| p == "env").unwrap_or(false) {
        return None; // env-prefixed commands need shell-like handling; skip
    }
    if argv.is_empty() {
        return None;
    }
    let program = argv.remove(0);
    // Store clients themselves are integrations, not games.
    let base = Path::new(&program)
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    if [
        "steam",
        "heroic",
        "legendary",
        "itch",
        "lutris",
        "steam-runtime",
    ]
    .contains(&base.as_str())
    {
        return None;
    }
    let resolved = which(&program)?;
    Some(DesktopGame {
        name: name?,
        executable: resolved.to_string_lossy().into_owned(),
        args: argv,
        desktop_file: path.to_string_lossy().into_owned(),
        icon,
    })
}

pub fn discover() -> Vec<DesktopGame> {
    let mut out: Vec<DesktopGame> = vec![];
    for dir in data_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "desktop").unwrap_or(false) {
                if let Some(g) = parse_desktop_file(&p) {
                    if !out.iter().any(|o| o.name == g.name) {
                        out.push(g);
                    }
                }
            }
        }
    }
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_exec_lines() {
        assert_eq!(
            split_exec(r#"/usr/bin/game --flag "two words" %U"#),
            vec!["/usr/bin/game", "--flag", "two words"]
        );
        assert_eq!(
            split_exec(r#""/opt/My Game/run" -x"#),
            vec!["/opt/My Game/run", "-x"]
        );
        assert_eq!(split_exec("a 100%%"), vec!["a", "100%"]);
    }

    #[test]
    fn parses_game_entries_only() {
        let dir = tempfile::tempdir().unwrap();
        let g = dir.path().join("g.desktop");
        std::fs::write(&g, "[Desktop Entry]\nType=Application\nName=SuperTux\nExec=sh -c true %f\nCategories=Game;ArcadeGame;\n").unwrap();
        let parsed = parse_desktop_file(&g).unwrap();
        assert_eq!(parsed.name, "SuperTux");
        assert!(parsed.executable.ends_with("/sh"));
        assert_eq!(parsed.args, vec!["-c", "true"]);

        let s = dir.path().join("s.desktop");
        std::fs::write(&s, "[Desktop Entry]\nType=Application\nName=CS2\nExec=steam steam://rungameid/730\nCategories=Game;\n").unwrap();
        assert!(
            parse_desktop_file(&s).is_none(),
            "Steam shortcuts are handled by the Steam adapter"
        );

        let t = dir.path().join("t.desktop");
        std::fs::write(
            &t,
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=sh\nCategories=Utility;\n",
        )
        .unwrap();
        assert!(parse_desktop_file(&t).is_none());
    }
}
