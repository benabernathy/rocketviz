//! User settings (`settings.json` in the app's config folder). Holds API keys, so on Unix it is readable by the user
//! only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Imagery provider id -> API key.
    #[serde(default)]
    pub imagery_keys: BTreeMap<String, String>,
    /// A Java to run engines with, instead of the bundled one / JAVA_HOME / PATH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
}

pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(config_dir: &Path) -> Self {
        Self { path: config_dir.join("settings.json") }
    }

    /// The saved settings; defaults if there are none or the file is unreadable (which is logged, not fatal).
    pub fn load(&self) -> Settings {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("rocketviz: ignoring unreadable {}: {e}", self.path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(settings)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)
    }
}
