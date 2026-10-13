//! Finding the program that runs an engine (`launch.runtime` in engine.json).

use std::path::{Path, PathBuf};

use super::manifest::{EngineSpec, Runtime};
use crate::error::{Error, Result};

/// Where to find each runtime. The desktop app sets `java` to its bundled JRE; otherwise `JAVA_HOME` and `PATH`
/// are searched, which is what development and tests use.
#[derive(Debug, Clone, Default)]
pub struct Runtimes {
    pub java: Option<PathBuf>,
    pub python: Option<PathBuf>,
}

impl Runtimes {
    /// The program and arguments that start `spec`.
    pub fn command(&self, spec: &EngineSpec) -> Result<(PathBuf, Vec<String>)> {
        let mut args = spec.args();
        let program = match spec.manifest.launch.runtime {
            Runtime::Java => self.java()?,
            Runtime::Python => self.python()?,
            Runtime::Exe => {
                let exe = spec.dir.join(args.remove(0));
                if !exe.is_file() {
                    return Err(Error::RuntimeNotFound {
                        runtime: "exe".into(),
                        message: format!("{} doesn't exist", exe.display()),
                    });
                }
                exe
            }
        };
        Ok((program, args))
    }

    pub fn java(&self) -> Result<PathBuf> {
        if let Some(p) = &self.java {
            return existing(p, "java");
        }
        if let Some(home) = std::env::var_os("JAVA_HOME") {
            let p = Path::new(&home).join("bin").join(exe_name("java"));
            if p.is_file() {
                return Ok(p);
            }
        }
        on_path("java").ok_or_else(|| Error::RuntimeNotFound {
            runtime: "java".into(),
            message: "no bundled runtime, JAVA_HOME isn't set, and java isn't on PATH".into(),
        })
    }

    pub fn python(&self) -> Result<PathBuf> {
        if let Some(p) = &self.python {
            return existing(p, "python");
        }
        on_path("python3")
            .or_else(|| on_path("python"))
            .ok_or_else(|| Error::RuntimeNotFound { runtime: "python".into(), message: "python3 isn't on PATH".into() })
    }
}

fn existing(p: &Path, runtime: &str) -> Result<PathBuf> {
    if p.is_file() {
        Ok(p.to_path_buf())
    } else {
        Err(Error::RuntimeNotFound { runtime: runtime.into(), message: format!("{} doesn't exist", p.display()) })
    }
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn on_path(name: &str) -> Option<PathBuf> {
    let exe = exe_name(name);
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&exe)).find(|p| p.is_file())
}
