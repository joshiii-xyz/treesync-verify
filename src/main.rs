use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use treesync_verify::{CompareMode, ComparisonReport, compare_trees, explain_report};

#[derive(Debug, Parser)]
#[command(
    name = "treesync-verify",
    version,
    about = "Verify two local trees under an explicit policy"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Compare two local trees and emit a JSON report.
    Compare {
        left: PathBuf,
        right: PathBuf,
        #[arg(long, default_value = "bytes")]
        mode: CompareMode,
    },
    /// Explain a JSON comparison report.
    Explain { report: PathBuf },
}

fn main() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("treesync-verify: {error}");
            ExitCode::from(2)
        }
    }
}

fn execute(cli: Cli) -> Result<u8, Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Compare { left, right, mode } => {
            let report = compare_trees(&left, &right, mode);
            println!("{}", serde_json::to_string(&report)?);
            Ok(match report.verdict.as_str() {
                "identical_under_policy" => 0,
                "different" => 1,
                _ => 2,
            })
        }
        Commands::Explain { report } => {
            let report: ComparisonReport = serde_json::from_str(&std::fs::read_to_string(report)?)?;
            println!("{}", explain_report(&report));
            Ok(0)
        }
    }
}
