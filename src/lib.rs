use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_CONFIG: &str = r#"[handlers.python]
match = "*.py"
command = "python3 {file}"
mode = "capture-stdout"

[handlers.rust]
match = "*.rs"
command = "rustc {file} -o /tmp/findle_out && /tmp/findle_out"
mode = "capture-stdout"
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub handlers: HashMap<String, Handler>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Handler {
    #[serde(rename = "match")]
    pub matcher: String,
    pub command: String,
    pub mode: String,
}

fn dirs_config() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            let home = std::env::var_os("HOME")?;
            let mut p = PathBuf::from(home);
            p.push(".config");
            Some(p)
        })
}

pub fn config_path() -> Result<PathBuf> {
    let mut dir = dirs_config().ok_or_else(|| anyhow!("could not determine config directory"))?;
    dir.push("findle");
    dir.push("handlers.toml");
    Ok(dir)
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;

    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir {}", parent.display()))?;
        }
        fs::write(&path, DEFAULT_CONFIG)
            .with_context(|| format!("failed to write default config to {}", path.display()))?;
        eprintln!("created default config at {}", path.display());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("failed to parse config {}", path.display()))?;
    Ok(config)
}

/// Result of a dispatched run.
#[derive(Clone)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Outcome of dispatching a file through the registry.
pub enum DispatchResult {
    /// No handler's glob matched this file.
    NoHandler,
    /// A handler matched and ran the command.
    Output(RunResult),
}

pub struct Runner<'a> {
    pub config: &'a Config,
}

impl<'a> Runner<'a> {
    pub fn new(config: &'a Config) -> Self {
        Runner { config }
    }

    /// Find the first handler whose `match` glob matches the filename.
    pub fn find_handler(&self, path: &Path) -> Option<&Handler> {
        let filename = path.file_name()?.to_string_lossy();
        self.config.handlers.values().find(|h| {
            glob::Pattern::new(&h.matcher)
                .map(|p| p.matches(&filename))
                .unwrap_or(false)
        })
    }

    /// Dispatch `path` through the matching handler, returning the outcome.
    pub fn dispatch(&self, path: &Path) -> Result<DispatchResult> {
        if !path.exists() {
            return Err(anyhow!("file not found: {}", path.display()));
        }

        let handler = match self.find_handler(path) {
            Some(h) => h,
            None => return Ok(DispatchResult::NoHandler),
        };

        let file_str = path.display().to_string();
        let command = handler.command.replace("{file}", &file_str);

        match handler.mode.as_str() {
            "capture-stdout" => {
                let output = run_capture_stdout(&command)?;
                Ok(DispatchResult::Output(output))
            }
            other => Err(anyhow!("unsupported mode: {}", other)),
        }
    }
}

fn run_capture_stdout(command: &str) -> Result<RunResult> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .with_context(|| format!("failed to spawn command: {}", command))?;

    Ok(RunResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Short uppercase label for a file's type, derived from its extension.
pub fn type_label(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| "?".to_string())
}
