// SPDX-License-Identifier: MPL-2.0

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use pons_core::config::Config;
use pons_core::engine::{Engine, ScanReport};
use pons_core::finding::{EvidenceClass, Severity};
use pons_core::lang::Lang;
use pons_core::report::{human, json, sarif};
use pons_core::source::DiscoveredFile;
use pons_rules::registry::RuleRegistry;

#[derive(Parser)]
#[command(
    name = "pons",
    version,
    about = "pons asinorum — a falsifier-first static analyzer",
    long_about = "Pons scans source without executing project code. Findings carry an evidence class; SPECULATIVE findings never gate a build. Exit codes: 0 completed below threshold, 1 threshold reached, 2 invalid input or, with --fail-on-skips, an incomplete scan."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a file or directory and report findings.
    Scan {
        path: PathBuf,
        /// Exit 1 if any non-speculative finding meets or exceeds this severity.
        #[arg(long, value_enum)]
        fail_on: Option<FailOn>,
        /// Output format. `human` is the default; `json` and `sarif` emit
        /// the machine-readable envelopes from PLAN Appendices G and E.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
        /// TOML configuration. Defaults to `pons.toml` at the scan root.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Select rule IDs; repeat to select more than one. Unknown IDs fail.
        #[arg(long)]
        rule: Vec<String>,
        /// Force one grammar for a single extensionless file.
        #[arg(long, value_enum)]
        lang: Option<Language>,
        /// Omit findings carrying speculative evidence.
        #[arg(long)]
        no_speculative: bool,
        /// Include evidence class and tier in human output.
        #[arg(long)]
        explain: bool,
        /// Treat any skipped file or subtree as an incomplete scan and exit 2.
        #[arg(long)]
        fail_on_skips: bool,
    },
    /// Print the rule catalogue generated from the live registry.
    Catalogue,
    /// Generate a man page from the same command model as `--help`.
    Man,
    /// Show optional adapter executables without claiming they are tested.
    Doctor,
    /// Check an explicit ordered JSON event trace against a TOML protocol.
    Trace {
        path: PathBuf,
        #[arg(long)]
        protocol: PathBuf,
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

#[derive(Clone, Copy, ValueEnum)]
enum Language {
    Python,
    Javascript,
    Typescript,
    Tsx,
    Rust,
}

impl Language {
    /// Return the grammar corresponding to the CLI language selection.
    fn lang(self) -> Lang {
        match self {
            Language::Python => Lang::Python,
            Language::Javascript => Lang::Javascript,
            Language::Typescript => Lang::Typescript,
            Language::Tsx => Lang::Tsx,
            Language::Rust => Lang::Rust,
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

/// Run the CLI, treating a propagated broken pipe as success and other
/// operational errors as exit status 2.
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            if let Some(io) = e.downcast_ref::<io::Error>() {
                if io.kind() == io::ErrorKind::BrokenPipe {
                    return ExitCode::SUCCESS;
                }
            }
            eprintln!("pons: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Write and flush `text` to stdout without adding a newline.
///
/// Returns any write or flush error, including a broken pipe.
fn emit(text: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

/// Execute a parsed command and emit its output to stdout.
///
/// Returns 0 for informational commands; scans return their gating status,
/// and traces return 1 when violations are found. File access, input
/// validation, rendering and output errors propagate to the caller.
fn run(cli: Cli) -> Result<u8> {
    match cli.command {
        Command::Scan {
            path,
            fail_on,
            format,
            config,
            rule,
            lang,
            no_speculative,
            explain,
            fail_on_skips,
        } => run_scan(
            &path,
            fail_on,
            format,
            config.as_deref(),
            &rule,
            lang,
            no_speculative,
            explain,
            fail_on_skips,
        ),
        Command::Catalogue => {
            emit(&pons_rules::catalogue())?;
            Ok(0)
        }
        Command::Man => {
            let mut rendered = Vec::new();
            clap_mangen::Man::new(Cli::command()).render(&mut rendered)?;
            let text = String::from_utf8(rendered)?;
            emit(
                &(text
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"),
            )?;
            Ok(0)
        }
        Command::Doctor => {
            emit(&doctor())?;
            Ok(0)
        }
        Command::Trace { path, protocol } => {
            let protocol_text = std::fs::read_to_string(&protocol)
                .with_context(|| format!("cannot read protocol {}", protocol.display()))?;
            let trace_text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read trace {}", path.display()))?;
            let protocol = pons_protocols::Protocol::parse(&protocol_text)?;
            let events: Vec<pons_protocols::Event> = serde_json::from_str(&trace_text)?;
            let violations = protocol.check_trace(&events)?;
            let envelope = serde_json::json!({
                "schema_version": "0.1.0",
                "tool": "pons",
                "mode": "explicit-trace",
                "protocol": protocol.id,
                "evidence": "PROTOCOL",
                "scope": "Only the supplied events, their order and instance keys; no source analysis or alias inference.",
                "violations": violations,
            });
            emit(&format!("{}\n", serde_json::to_string_pretty(&envelope)?))?;
            Ok(u8::from(!violations.is_empty()))
        }
    }
}

/// Describe optional adapter file locations found on `PATH`.
///
/// File presence does not verify executable permissions or adapter behaviour.
fn doctor() -> String {
    let mut out = String::from(
        "Optional adapters (presence only; availability is not an integration test):\n",
    );
    for name in [
        "invariant-path",
        "pandoc",
        "aspell",
        "hunspell",
        "agrep",
        "tesseract",
    ] {
        let location = std::env::var_os("PATH")
            .and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join(name))
                    .find(|p| p.is_file())
            })
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not installed".to_string());
        out.push_str(&format!("{name}: {location}\n"));
    }
    out.push_str("Core scans need none of these tools.\n");
    out
}

/// Scan a path and write the selected report to stdout and accounting to stderr.
///
/// An empty `selected` list enables all registered rules. Without `config_path`,
/// load `pons.toml` from the directory scanned or the input file's parent if
/// present. `forced_lang` bypasses extension detection for a single file.
/// `no_speculative` removes speculative findings and counts them as suppressed;
/// `explain` adds evidence and tier annotations only in human output.
///
/// Returns 2 if `fail_on_skips` is set and any input was skipped, otherwise 1
/// if a remaining non-speculative finding meets `fail_on`, or 0.
///
/// Missing paths, a forced grammar for a directory, unknown selected rules,
/// configuration and discovery errors, and rendering or stdout errors propagate.
/// Per-file read and parse errors are reported as skips.
#[allow(clippy::too_many_arguments)]
fn run_scan(
    path: &Path,
    fail_on: Option<FailOn>,
    format: Format,
    config_path: Option<&Path>,
    selected: &[String],
    forced_lang: Option<Language>,
    no_speculative: bool,
    explain: bool,
    fail_on_skips: bool,
) -> Result<u8> {
    if !path.exists() {
        bail!("no such path: {}", path.display());
    }
    if forced_lang.is_some() && path.is_dir() {
        bail!("--lang forces a grammar for a single file; scan directories with automatic language detection");
    }

    let mut rules = RuleRegistry::all();
    let known: Vec<&str> = rules.iter().map(|r| r.id()).collect();
    for id in selected {
        if !known.contains(&id.as_str()) {
            bail!("unknown rule: {id}");
        }
    }
    if !selected.is_empty() {
        rules.retain(|r| selected.iter().any(|id| id == r.id()));
    }

    let default_config = if path.is_dir() {
        path.join("pons.toml")
    } else {
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .join("pons.toml")
    };
    let effective_config = config_path
        .map(Path::to_path_buf)
        .or_else(|| default_config.exists().then_some(default_config));
    let config = match effective_config {
        Some(p) => Config::load(&p)?,
        None => Config::default(),
    };
    let compiled = config.compile(&known)?;
    let engine = Engine::with_config(rules, compiled);

    let mut report = if let Some(lang) = forced_lang {
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let file = DiscoveredFile {
            path: path.to_path_buf(),
            lang: lang.lang(),
        };
        engine.scan_files(root, &[file])
    } else {
        engine.scan(path)?
    };

    if no_speculative {
        let before = report.findings.len();
        report
            .findings
            .retain(|f| f.evidence() != EvidenceClass::Speculative);
        report.suppressed_findings += before - report.findings.len();
    }

    let rendered = match format {
        Format::Human => Ok(human::render_report(&report, explain)),
        Format::Json => json::render(&report),
        Format::Sarif => sarif::render(&report),
    }?;
    emit(&rendered)?;
    report_to_stderr(&report);

    if fail_on_skips && !report.skipped.is_empty() {
        return Ok(2);
    }

    // SPECULATIVE findings can never trigger a non-zero exit, regardless of
    // --fail-on: the tool never dresses a heuristic as a verdict.
    let should_fail = fail_on.is_some_and(|threshold| {
        report.findings.iter().any(|f| {
            f.evidence() != EvidenceClass::Speculative
                && severity_rank(f.severity()) >= threshold.rank()
        })
    });
    Ok(u8::from(should_fail))
}

/// Everything the operator needs and no machine consumer asked for goes to
/// stderr, in every format. That keeps `--format json` and `--format sarif`
/// stdout byte-for-byte parseable while still making a partial scan
/// impossible to mistake for a clean one.
fn report_to_stderr(report: &ScanReport) {
    for skipped in &report.skipped {
        eprintln!("pons: skipped {}: {}", skipped.path, skipped.reason);
    }
    for file in &report.transcoded {
        eprintln!("pons: transcoded {file}; SARIF byteOffset/byteLength omitted");
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
        "pons: scanned {} file{} ({}), {} finding{}, {} suppressed, {} skipped, {} transcoded",
        report.files_scanned,
        if report.files_scanned == 1 { "" } else { "s" },
        languages,
        report.findings.len(),
        if report.findings.len() == 1 { "" } else { "s" },
        report.suppressed_findings,
        report.skipped.len(),
        report.transcoded.len()
    );
}
