use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "findle", version, about = "A customizable file-runner tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a file through the matching handler
    Run {
        /// Path to the file to run
        path: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
struct Config {
    handlers: std::collections::HashMap<String, Handler>,
}

#[derive(Debug, Deserialize)]
struct Handler {
    #[serde(rename = "match")]
    matcher: String,
    command: String,
    mode: String,
}

fn config_path() -> Result<PathBuf> {
    let mut dir = dirs_config()
        .ok_or_else(|| anyhow!("could not determine config directory"))?;
    dir.push("findle");
    dir.push("handlers.toml");
    Ok(dir)
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

const DEFAULT_CONFIG: &str = r#"[handlers.python]
match = "*.py"
command = "python3 {file}"
mode = "capture-stdout"

[handlers.rust]
match = "*.rs"
command = "rustc {file} -o /tmp/findle_out && /tmp/findle_out"
mode = "capture-stdout"
"#;

fn load_config() -> Result<Config> {
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

struct Runner<'a> {
    config: &'a Config,
}

impl<'a> Runner<'a> {
    fn new(config: &'a Config) -> Self {
        Runner { config }
    }

    fn find_handler(&self, path: &Path) -> Option<&Handler> {
        let filename = path.file_name()?.to_string_lossy();
        self.config
            .handlers
            .values()
            .find(|h| glob::Pattern::new(&h.matcher).map(|p| p.matches(&filename)).unwrap_or(false))
    }

    fn run(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(anyhow!("file not found: {}", path.display()));
        }

        let handler = self
            .find_handler(path)
            .ok_or_else(|| anyhow!("no handler for this file type ({})", path.display()))?;

        let file_str = path.display().to_string();
        let command = handler.command.replace("{file}", &file_str);

        match handler.mode.as_str() {
            "capture-stdout" => run_capture_stdout(&command),
            other => Err(anyhow!("unsupported mode: {}", other)),
        }
    }
}

fn run_capture_stdout(command: &str) -> Result<()> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .with_context(|| format!("failed to spawn command: {}", command))?;

    use std::io::Write;
    std::io::stdout()
        .write_all(&output.stdout)
        .context("failed to write captured stdout")?;
    std::io::stderr()
        .write_all(&output.stderr)
        .context("failed to write captured stderr")?;

    println!("\n[findle] exit code: {}", output.status.code().unwrap_or(-1));
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { path } => {
            let config = load_config()?;
            let runner = Runner::new(&config);
            runner.run(&path)?;
        }
    }

    Ok(())
}
