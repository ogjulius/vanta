//! Manually imported games (native Linux or Windows via Wine/Proton).
//! These live only in VANTA's database; there is nothing to scan.

use std::path::Path;

use super::*;

pub struct LocalAdapter;

impl PlatformAdapter for LocalAdapter {
    fn id(&self) -> &'static str {
        "local"
    }
    fn name(&self) -> &'static str {
        "Local"
    }

    fn detect(&self) -> Detection {
        Detection {
            detected: true,
            connected: true,
            account: None,
            details: vec!["Games you add manually or import from desktop entries".into()],
            help: Some("Use “Add game” to import any executable, script or Windows .exe.".into()),
        }
    }

    fn discover(&self) -> Result<Vec<DiscoveredGame>, String> {
        Ok(vec![])
    }

    fn launch_plan(&self, game: &Game, extra_args: &[String]) -> Result<LaunchPlan, String> {
        let exe = game
            .executable
            .as_deref()
            .ok_or("No executable configured for this game.")?;
        direct_plan(
            Path::new(exe),
            game.install_dir.as_deref().map(Path::new),
            extra_args,
        )
    }
}
