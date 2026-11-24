use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Persistent application settings. Every field has a default so missing keys
/// (older databases, partial writes) never break startup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // General
    pub start_minimized: bool,
    pub launch_on_startup: bool,
    pub confirm_before_close: bool,
    pub default_layout: String,
    // Appearance
    pub theme: String,
    pub accent: String,
    pub density: String,
    pub grid_size: String,
    pub animations: String,
    pub sidebar: String,
    // Library
    pub auto_scan: bool,
    pub download_artwork: bool,
    pub default_sort: String,
    // Performance
    pub monitor_interval_ms: u64,
    pub session_poll_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            start_minimized: false,
            launch_on_startup: false,
            confirm_before_close: false,
            default_layout: "grid".into(),
            theme: "midnight".into(),
            accent: "#8b5cf6".into(),
            density: "comfortable".into(),
            grid_size: "medium".into(),
            animations: "full".into(),
            sidebar: "expanded".into(),
            auto_scan: true,
            download_artwork: true,
            default_sort: "recent".into(),
            monitor_interval_ms: 2000,
            session_poll_secs: 5,
        }
    }
}

fn one_of(field: &str, value: &str, allowed: &[&str]) -> AppResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "invalid value '{value}' for {field}; expected one of {}",
            allowed.join(", ")
        )))
    }
}

impl Settings {
    /// Validate and normalize. Rejects unknown enum values; clamps numeric ranges.
    pub fn validated(mut self) -> AppResult<Self> {
        one_of("defaultLayout", &self.default_layout, &["grid", "list"])?;
        one_of("theme", &self.theme, &["midnight", "graphite"])?;
        one_of("density", &self.density, &["comfortable", "compact"])?;
        one_of("gridSize", &self.grid_size, &["small", "medium", "large"])?;
        one_of("animations", &self.animations, &["full", "reduced", "off"])?;
        one_of("sidebar", &self.sidebar, &["expanded", "collapsed"])?;
        one_of(
            "defaultSort",
            &self.default_sort,
            &["name", "recent", "playtime", "added"],
        )?;
        let a = self.accent.trim();
        let valid_hex =
            a.len() == 7 && a.starts_with('#') && a[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !valid_hex {
            return Err(AppError::Invalid(format!(
                "accent must be a #rrggbb color, got '{a}'"
            )));
        }
        self.accent = a.to_ascii_lowercase();
        self.monitor_interval_ms = self.monitor_interval_ms.clamp(500, 10_000);
        self.session_poll_secs = self.session_poll_secs.clamp(2, 30);
        Ok(self)
    }

    /// Parse stored JSON, recovering from corruption instead of failing.
    /// Unknown/invalid individual values fall back to defaults.
    pub fn from_stored(raw: &str) -> (Self, bool) {
        match serde_json::from_str::<Settings>(raw) {
            Ok(s) => match s.clone().validated() {
                Ok(v) => (v, false),
                Err(_) => (Self::repair(s), true),
            },
            Err(_) => {
                // Try field-by-field recovery from whatever JSON object is there.
                let mut base = serde_json::to_value(Settings::default()).unwrap_or_default();
                if let (Ok(serde_json::Value::Object(stored)), Some(obj)) = (
                    serde_json::from_str::<serde_json::Value>(raw),
                    base.as_object_mut(),
                ) {
                    for (k, v) in stored {
                        if let Some(slot) = obj.get_mut(&k) {
                            if std::mem::discriminant(slot) == std::mem::discriminant(&v) {
                                *slot = v;
                            }
                        }
                    }
                }
                let s = serde_json::from_value::<Settings>(base).unwrap_or_default();
                (Self::repair(s), true)
            }
        }
    }

    fn repair(s: Settings) -> Settings {
        let d = Settings::default();
        let fix = |v: String, allowed: &[&str], dv: String| {
            if allowed.contains(&v.as_str()) {
                v
            } else {
                dv
            }
        };
        let mut r = Settings {
            default_layout: fix(
                s.default_layout,
                &["grid", "list"],
                d.default_layout.clone(),
            ),
            theme: fix(s.theme, &["midnight", "graphite"], d.theme.clone()),
            density: fix(s.density, &["comfortable", "compact"], d.density.clone()),
            grid_size: fix(
                s.grid_size,
                &["small", "medium", "large"],
                d.grid_size.clone(),
            ),
            animations: fix(
                s.animations,
                &["full", "reduced", "off"],
                d.animations.clone(),
            ),
            sidebar: fix(s.sidebar, &["expanded", "collapsed"], d.sidebar.clone()),
            default_sort: fix(
                s.default_sort,
                &["name", "recent", "playtime", "added"],
                d.default_sort.clone(),
            ),
            ..s
        };
        if r.clone().validated().is_err() {
            r.accent = d.accent;
        }
        r.validated().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(Settings::default().validated().is_ok());
    }

    #[test]
    fn rejects_bad_values() {
        let s = Settings {
            theme: "neon".into(),
            ..Default::default()
        };
        assert!(s.validated().is_err());
        let s = Settings {
            accent: "red".into(),
            ..Default::default()
        };
        assert!(s.validated().is_err());
    }

    #[test]
    fn clamps_intervals() {
        let s = Settings {
            monitor_interval_ms: 1,
            session_poll_secs: 999,
            ..Default::default()
        }
        .validated()
        .unwrap();
        assert_eq!(s.monitor_interval_ms, 500);
        assert_eq!(s.session_poll_secs, 30);
    }

    #[test]
    fn recovers_from_corrupt_json() {
        let (s, repaired) = Settings::from_stored("{not json");
        assert!(repaired);
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn recovers_partially_invalid() {
        let (s, repaired) = Settings::from_stored(
            r#"{"theme":"neon","autoScan":false,"monitorIntervalMs":"fast"}"#,
        );
        assert!(repaired);
        assert_eq!(s.theme, "midnight");
        assert!(!s.auto_scan, "valid fields must survive recovery");
        assert_eq!(s.monitor_interval_ms, 2000);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let (s, repaired) = Settings::from_stored(r##"{"accent":"#22D3EE"}"##);
        assert!(!repaired);
        assert_eq!(s.accent, "#22d3ee");
        assert!(s.auto_scan);
    }
}
