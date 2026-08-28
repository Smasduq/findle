use anyhow::Result;
use clap::{Parser, Subcommand};
use findle::{load_config, DispatchResult, Runner};
use std::path::PathBuf;

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { path } => {
            let config = load_config()?;
            let runner = Runner::new(&config);

            match runner.dispatch(&path)? {
                DispatchResult::NoHandler => {
                    anyhow::bail!("no handler for this file type ({})", path.display())
                }
                DispatchResult::Output(out) => {
                    print!("{}", out.stdout);
                    eprint!("{}", out.stderr);
                    println!("\n[findle] exit code: {}", out.exit_code);
                }
            }
        }
    }

    Ok(())
}
