//! `engine.json`: how to find and start an engine without running it (docs/engine-protocol.md §1).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

pub const MANIFEST_FILE: &str = "engine.json";
pub const PROTOCOL_ID: &str = "rocketviz-engine/1";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub protocol: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub input_formats: Vec<InputFormat>,
    pub launch: Launch,
    #[serde(default)]
    pub instances: Option<Instances>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputFormat {
    pub extensions: Vec<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Launch {
    pub runtime: Runtime,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Java,
    Python,
    Exe,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Instances {
    pub recommended: u32,
    #[serde(default)]
    pub memory_mb: Option<u32>,
}

/// An engine found on disk: its folder and manifest.
#[derive(Debug, Clone)]
pub struct EngineSpec {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl EngineSpec {
    /// Reads `<dir>/engine.json`.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = std::path::absolute(dir.as_ref())?;
        let path = dir.join(MANIFEST_FILE);
        let err = |message: String| Error::Manifest { path: path.clone(), message };
        let text = std::fs::read_to_string(&path).map_err(|e| err(e.to_string()))?;
        let manifest: Manifest = serde_json::from_str(&text).map_err(|e| err(e.to_string()))?;
        if manifest.protocol != PROTOCOL_ID {
            return Err(err(format!("unsupported protocol {:?} (expected {PROTOCOL_ID:?})", manifest.protocol)));
        }
        if manifest.launch.runtime == Runtime::Exe && manifest.launch.args.is_empty() {
            return Err(err("launch.args[0] must name the executable for runtime \"exe\"".into()));
        }
        Ok(Self { dir, manifest })
    }

    /// Every engine folder (a direct subfolder containing `engine.json`) under `root`. Unreadable manifests are
    /// returned as errors alongside the good ones so the app can report them.
    pub fn discover(root: impl AsRef<Path>) -> Vec<Result<Self>> {
        let Ok(entries) = std::fs::read_dir(root.as_ref()) else { return Vec::new() };
        let mut dirs: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join(MANIFEST_FILE).is_file()).collect();
        dirs.sort();
        dirs.into_iter().map(Self::load).collect()
    }

    /// Whether this engine opens files with the given extension (case-insensitive, without the dot).
    pub fn accepts(&self, extension: &str) -> bool {
        self.manifest
            .input_formats
            .iter()
            .flat_map(|f| &f.extensions)
            .any(|x| x.eq_ignore_ascii_case(extension.trim_start_matches('.')))
    }

    /// `launch.args` with `${engine_dir}` and `${path_sep}` substituted.
    pub fn args(&self) -> Vec<String> {
        let dir = self.dir.to_string_lossy();
        let sep = if cfg!(windows) { ";" } else { ":" };
        self.manifest.launch.args.iter().map(|a| a.replace("${engine_dir}", &dir).replace("${path_sep}", sep)).collect()
    }

    pub fn recommended_instances(&self) -> u32 {
        self.manifest.instances.map_or(1, |i| i.recommended.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, json: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE), json).unwrap();
    }

    const OK: &str = r#"{
        "protocol": "rocketviz-engine/1", "id": "demo", "name": "Demo", "version": "1",
        "input_formats": [{"extensions": ["ork"], "description": "d"}],
        "launch": {"runtime": "java", "args": ["-cp", "${engine_dir}/a.jar${path_sep}${engine_dir}/b.jar", "Main"]},
        "instances": {"recommended": 2, "memory_mb": 470}
    }"#;

    #[test]
    fn substitutes_placeholders() {
        let root = std::env::temp_dir().join(format!("rv-manifest-{}", std::process::id()));
        write(&root.join("demo"), OK);
        let spec = EngineSpec::load(root.join("demo")).unwrap();
        let args = spec.args();
        let sep = if cfg!(windows) { ";" } else { ":" };
        assert_eq!(args[1], format!("{0}/a.jar{sep}{0}/b.jar", spec.dir.display()));
        assert!(spec.accepts("ORK") && spec.accepts(".ork") && !spec.accepts("rkt"));
        assert_eq!(spec.recommended_instances(), 2);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn discovers_and_reports_bad_manifests() {
        let root = std::env::temp_dir().join(format!("rv-discover-{}", std::process::id()));
        write(&root.join("good"), OK);
        write(&root.join("bad"), r#"{"protocol": "something-else/9"}"#);
        std::fs::create_dir_all(root.join("not-an-engine")).unwrap();
        let found = EngineSpec::discover(&root);
        assert_eq!(found.len(), 2);
        assert!(found.iter().filter(|r| r.is_ok()).count() == 1);
        std::fs::remove_dir_all(root).ok();
    }
}
