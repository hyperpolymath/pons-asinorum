// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pons_core::engine::Engine;
use pons_core::finding::{EvidenceClass, Severity};
use pons_core::report::human;
use pons_rules::registry::RuleRegistry;

#[derive(Parser)]
#[command(
    name = "pons",
    about = "pons asinorum — a falsifier-first static analyzer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a directory and report findings.
    Scan {
        path: PathBuf,
        /// Exit 1 if any non-speculative finding meets or exceeds this
        /// severity. Without this flag, `scan` always exits 0.
        #[arg(long, value_enum)]
        fail_on: Option<FailOn>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum FailOn {
    Info,
    Warn,
    Error,
}

impl FailOn {
    fn rank(self) -> u8 {
        match self {
            FailOn::Info => 0,
            FailOn::Warn => 1,
            FailOn::Error => 2,
        }
    }
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Info => 0,
        Severity::Warn => 1,
        Severity::Error => 2,
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Scan { path, fail_on } => run_scan(&path, fail_on),
    }
}

fn run_scan(path: &Path, fail_on: Option<FailOn>) -> ExitCode {
    if !path.exists() {
        eprintln!("pons: no such path: {}", path.display());
        return ExitCode::from(2);
    }

    let engine = Engine::new(RuleRegistry::all());
    let findings = match engine.scan(path) {
        Ok(findings) => findings,
        Err(e) => {
            eprintln!("pons: scan failed: {e}");
            return ExitCode::from(2);
        }
    };

    print!("{}", human::render(&findings));

    // SPECULATIVE findings can never trigger a non-zero exit, regardless of
    // --fail-on: the tool never dresses a heuristic as a verdict.
    let should_fail = fail_on.is_some_and(|threshold| {
        findings.iter().any(|f| {
            f.evidence != EvidenceClass::Speculative
                && severity_rank(f.severity) >= threshold.rank()
        })
    });

    if should_fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
