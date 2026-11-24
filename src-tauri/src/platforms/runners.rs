//! Wine and Proton detection for running Windows games outside a store client.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{home, is_executable, which};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Runner {
    /// "wine" | "proton"
    pub kind: String,
    pub name: String,
    pub path: String,
}

fn steam_roots() -> Vec<PathBuf> {
    let h = home();
    vec![
        h.join(".local/share/Steam"),
        h.join(".steam/root"),
        h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
}

fn protons_in(dir: &Path, out: &mut Vec<Runner>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let script = e.path().join("proton");
        if script.is_file() {
            let path = std::fs::canonicalize(&script).unwrap_or(script);
            let path = path.to_string_lossy().into_owned();
            if !out.iter().any(|r| r.path == path) {
                out.push(Runner {
                    kind: "proton".into(),
                    name: e.file_name().to_string_lossy().into_owned(),
                    path,
                });
            }
        }
    }
}

pub fn detect() -> Vec<Runner> {
    let mut out = vec![];
    for name in ["wine", "wine64", "wine-staging"] {
        if let Some(p) = which(name) {
            let path = p.to_string_lossy().into_owned();
            if !out.iter().any(|r: &Runner| r.path == path) {
                out.push(Runner {
                    kind: "wine".into(),
                    name: format!("System {name}"),
                    path,
                });
            }
        }
    }
    let lutris = home().join(".local/share/lutris/runners/wine");
    if let Ok(rd) = std::fs::read_dir(lutris) {
        for e in rd.flatten() {
            let bin = e.path().join("bin/wine");
            if is_executable(&bin) {
                out.push(Runner {
                    kind: "wine".into(),
                    name: format!("Lutris {}", e.file_name().to_string_lossy()),
                    path: bin.to_string_lossy().into_owned(),
                });
            }
        }
    }
    for root in steam_roots() {
        protons_in(&root.join("steamapps/common"), &mut out);
        protons_in(&root.join("compatibilitytools.d"), &mut out);
    }
    protons_in(Path::new("/usr/share/steam/compatibilitytools.d"), &mut out);
    out
}

/// The Steam root Proton needs as STEAM_COMPAT_CLIENT_INSTALL_PATH.
pub fn steam_client_path() -> Option<PathBuf> {
    steam_roots()
        .into_iter()
        .find(|p| p.join("steamapps").is_dir())
}
