//! CLI shim. Clap usage errors exit 2; runner errors exit 2; findings drive 0/1.
//!
//! No network-capable code or dependency is linked.
// Covers: cli~no-network~1

use clap::{Parser, Subcommand};
use reqtrace::runner::{self, CheckOpts, ValidateOpts};
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser)]
#[command(
    name = "reqtrace",
    version,
    about = "Deterministic requirements-coverage checker"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan docs for Covers:/Derived: annotations and run the coverage checks
    Check {
        /// File or directory of inventory YAML
        #[arg(long)]
        inventory: PathBuf,
        /// Config file (default: nearest .reqtrace.toml, else built-in defaults)
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the machine-readable JSON report here
        #[arg(long)]
        json: Option<PathBuf>,
        /// Files/dirs to scan (replaces [scan].globs from config)
        doc_paths: Vec<PathBuf>,
    },
    /// Lint the inventory (and exception file, when configured)
    Validate {
        /// File or directory of inventory YAML
        #[arg(long)]
        inventory: PathBuf,
        /// Config file (default: nearest .reqtrace.toml, else built-in defaults)
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check {
            inventory,
            config,
            json,
            doc_paths,
        } => runner::run_check(&CheckOpts {
            inventory,
            config,
            json,
            doc_paths,
        }),
        Cmd::Validate { inventory, config } => {
            runner::run_validate(&ValidateOpts { inventory, config })
        }
    };
    match result {
        // Covers: cli~exit-codes~1
        Ok(code) => exit(code),
        // Tool-input and I/O errors exit 2.
        // Covers: cli~error-lanes~1
        Err(err) => {
            eprintln!("reqtrace: error: {:#}", err);
            exit(2);
        }
    }
}
