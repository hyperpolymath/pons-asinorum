// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pons_core::engine::Engine;
use pons_core::engine::ScanReport;
use pons_core::finding::{EvidenceClass, Severity};
use pons_core::report::{human, json, sarif};
use pons_rules::registry::RuleRegistry;

#[derive(Parser)]
#[command(
    name = "pons",
    version,
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
        /// Output format. `human` is the default; `json` and `sarif` emit
        /// the machine-readable envelopes from PLAN Appendices G and E.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Human,
    Json,
    Sarif,
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
        Command::Scan {
            path,
            fail_on,
            format,
        } => run_scan(&path, fail_on, format),
    }
}

/// Everything the operator needs and no machine consumer asked for goes to
/// stderr, in every format. That keeps `--format json` and `--format sarif`
/// stdout byte-for-byte parseable while still making a partial scan
/// impossible to mistake for a clean one.
fn report_to_stderr(report: &ScanReport) {
    for skipped in &report.skipped {
        eprintln!("pons: skipped {}: {}", skipped.path, skipped.reason);
    }

    let languages = if report.languages.is_empty() {
        "none".to_string()
    } else {
        report
            .languages
            .iter()
            .map(|l| l.name())
            .collect::<Vec<_>>()
            .join(", ")
    };

    eprintln!(
        "pons: scanned {} file{} ({}), {} finding{}{}",
        report.files_scanned,
        if report.files_scanned == 1 { "" } else { "s" },
        languages,
        report.findings.len(),
        if report.findings.len() == 1 { "" } else { "s" },
        if report.skipped.is_empty() {
            String::new()
        } else {
            format!(", {} skipped", report.skipped.len())
        }
    );
}

fn run_scan(path: &Path, fail_on: Option<FailOn>, format: Format) -> ExitCode {
    if !path.exists() {
        eprintln!("pons: no such path: {}", path.display());
        return ExitCode::from(2);
    }

    let engine = Engine::new(RuleRegistry::all());
    let report = match engine.scan(path) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("pons: scan failed: {e}");
            return ExitCode::from(2);
        }
    };

    let rendered = match format {
        Format::Human => Ok(human::render(&report.findings)),
        Format::Json => json::render(&report),
        Format::Sarif => sarif::render(&report),
    };

    match rendered {
        Ok(text) => print!("{text}"),
        Err(e) => {
            eprintln!("pons: could not render report: {e}");
            return ExitCode::from(2);
        }
    }

    report_to_stderr(&report);

    let findings = &report.findings;

    // SPECULATIVE findings can never trigger a non-zero exit, regardless of
    // --fail-on: the tool never dresses a heuristic as a verdict.
    let should_fail = fail_on.is_some_and(|threshold| {
        findings.iter().any(|f| {
            f.evidence() != EvidenceClass::Speculative
                && severity_rank(f.severity()) >= threshold.rank()
        })
    });

    if should_fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
