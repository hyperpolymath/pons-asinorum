// SPDX-License-Identifier: MPL-2.0
mod adapters;
use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use pons_core::{
    EvidenceClass, Lang, Parsed, Severity, Source,
    config::{Config, inline_allowed},
    report::{Diagnostic, Report, Scanned},
};
use std::{
    collections::BTreeSet,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    name = "pons",
    version,
    about = "Spot rookie mistakes beyond successful compilation",
    long_about = "Pons scans source and repository documents without executing project code. Findings are evidence-labelled suggestions. Exit codes: 0 completed below the chosen failure threshold, 1 threshold reached, 2 invalid input or incomplete scan."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Scan a file or tree. Does not execute scanned code or external plug-ins.
    Scan {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "human")]
        format: Format,
        /// TOML configuration; defaults to pons.toml at the scan root.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Force one source grammar for a single file (including extensionless input).
        #[arg(long, value_enum)]
        lang: Option<Language>,
        /// Select rule IDs; repeat to select more than one.
        #[arg(long)]
        rule: Vec<String>,
        /// Default is advisory. Set warn or info for a CI gate.
        #[arg(long, value_enum, default_value = "never")]
        fail_on: FailOn,
        /// Omit findings carrying speculative evidence.
        #[arg(long)]
        no_speculative: bool,
        /// Include technical evidence and the conditions under which a finding is intentional.
        #[arg(long)]
        explain: bool,
    },
    /// Print the built-in rule catalogue (generated from the registry).
    Catalogue,
    /// Generate a man page from the same command model as --help.
    Man,
    /// Show installed optional tools without claiming they are tested.
    Doctor,
    /// Ask the separately installed Invariant Path to examine a document's claims.
    /// Produces JSON; does not persist or accept annotations.
    Challenge { path: PathBuf },
    /// Convert a document/image to text using a named optional local tool.
    Extract {
        path: PathBuf,
        #[arg(long, value_enum)]
        via: Extractor,
        #[arg(long, default_value = "eng")]
        language: String,
    },
    /// List dictionary misses using aspell or hunspell; no automatic rewrites.
    Spell {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "aspell")]
        via: Speller,
        #[arg(long, default_value = "en_GB")]
        dictionary: String,
    },
    /// Approximate text search via the installed agrep executable.
    Search {
        pattern: String,
        path: PathBuf,
        #[arg(long,default_value_t=1,value_parser=clap::value_parser!(u8).range(0..=8))]
        errors: u8,
    },
    /// Check an explicit ordered JSON event trace against a supplied TOML contract.
    /// Does not infer events or control flow from source code.
    Trace {
        path: PathBuf,
        #[arg(long)]
        protocol: PathBuf,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
    Sarif,
}
#[derive(Clone, Copy, ValueEnum)]
enum FailOn {
    Never,
    Info,
    Warn,
    Error,
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
    fn lang(self) -> Lang {
        match self {
            Self::Python => Lang::Python,
            Self::Javascript => Lang::Javascript,
            Self::Typescript => Lang::Typescript,
            Self::Tsx => Lang::Tsx,
            Self::Rust => Lang::Rust,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
pub enum Extractor {
    Pandoc,
    Ocr,
}
#[derive(Clone, Copy, ValueEnum)]
pub enum Speller {
    Aspell,
    Hunspell,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            eprintln!("pons: {e:#}");
            ExitCode::from(2)
        }
    }
}
fn emit(text: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()?;
    Ok(())
}
fn run(cli: Cli) -> Result<u8> {
    match cli.command {
        Command::Scan {
            path,
            format,
            config,
            lang,
            rule,
            fail_on,
            no_speculative,
            explain,
        } => {
            let (report, metadata) = scan(
                &path,
                config.as_deref(),
                lang.map(|l| l.lang()),
                &rule,
                no_speculative,
            )?;
            let text = match format {
                Format::Human => report.human(explain),
                Format::Json => format!("{}\n", serde_json::to_string_pretty(&report)?),
                Format::Sarif => format!(
                    "{}\n",
                    serde_json::to_string_pretty(&report.sarif(&metadata))?
                ),
            };
            emit(&text)?;
            if !report.scanned.complete {
                return Ok(2);
            }
            let minimum = match fail_on {
                FailOn::Never => None,
                FailOn::Info => Some(Severity::Info),
                FailOn::Warn => Some(Severity::Warn),
                FailOn::Error => Some(Severity::Error),
            };
            Ok(u8::from(minimum.is_some_and(|min| {
                report.findings.iter().any(|f| f.severity >= min)
            })))
        }
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
            emit(&adapters::doctor())?;
            Ok(0)
        }
        Command::Challenge { path } => {
            let (report, _) = scan(&path, None, None, &[], false)?;
            if !report.scanned.complete {
                bail!("Pons scan is incomplete; resolve its diagnostics before a claim hand-off");
            }
            let challenge = adapters::challenge(&path)?;
            emit(&format!(
                "{}\n",
                serde_json::to_string_pretty(
                    &serde_json::json!({"schema_version":"0.1.0","pons":report,"invariant_path":challenge,"status":"unreviewed suggestions; no annotations were written or accepted"})
                )?
            ))?;
            Ok(0)
        }
        Command::Extract {
            path,
            via,
            language,
        } => {
            emit(&adapters::extract(&path, via, &language)?)?;
            Ok(0)
        }
        Command::Spell {
            path,
            via,
            dictionary,
        } => {
            emit(&adapters::spell(&path, via, &dictionary)?)?;
            Ok(0)
        }
        Command::Search {
            pattern,
            path,
            errors,
        } => {
            emit(&adapters::search(&pattern, &path, errors)?)?;
            Ok(0)
        }
        Command::Trace { path, protocol } => {
            let protocol = pons_protocols::Protocol::parse(&read_text(&protocol)?)?;
            let events: Vec<pons_protocols::Event> = serde_json::from_str(&read_text(&path)?)?;
            let violations = protocol.check_trace(&events)?;
            emit(&format!(
                "{}\n",
                serde_json::to_string_pretty(
                    &serde_json::json!({"schema_version":"0.1.0","tool":"pons","mode":"explicit-trace","protocol":protocol.id,"evidence":"PROTOCOL","scope":"Only the supplied events, their order and instance keys; no source analysis or alias inference.","violations":violations})
                )?
            ))?;
            Ok(u8::from(!violations.is_empty()))
        }
    }
}
fn read_text(path: &Path) -> Result<String> {
    Ok(Source::read(path, path.parent().unwrap_or(Path::new(".")))?.text)
}

fn scan(
    path: &Path,
    config_path: Option<&Path>,
    forced_lang: Option<Lang>,
    selected: &[String],
    no_speculative: bool,
) -> Result<(Report, Vec<pons_core::RuleMeta>)> {
    if path.symlink_metadata()?.file_type().is_symlink() {
        bail!("scan root must not be a symlink");
    }
    let path = path
        .canonicalize()
        .with_context(|| format!("scan root {}", path.display()))?;
    let root = if path.is_dir() {
        path.clone()
    } else {
        path.parent().context("file needs a parent")?.to_path_buf()
    };
    let default_config = root.join("pons.toml");
    if path.is_dir() && forced_lang.is_some() {
        bail!(
            "--lang forces a grammar for a single file; scan trees with automatic language detection"
        );
    }
    let config = match config_path {
        Some(p) => Config::load(p)?,
        None if default_config.exists() => Config::load(&default_config)?,
        None => Config::default(),
    };
    let mut metadata = pons_rules::metadata();
    let config = config.compile(&metadata)?;
    metadata.extend(config.metadata());
    for id in selected {
        if !metadata.iter().any(|m| m.id == *id) {
            bail!("unknown rule: {id}");
        }
    }
    let paths = pons_core::source::discover(&path)?;
    let mut sources = vec![];
    let mut diagnostics = vec![];
    let mut skipped = 0;
    for p in paths {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
        let text_input = forced_lang.is_some()
            || Lang::from_extension(ext).is_some()
            || matches!(
                ext,
                "md" | "adoc"
                    | "asciidoc"
                    | "txt"
                    | "rst"
                    | "toml"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "sh"
                    | "a2ml"
                    | "ncl"
                    | "html"
                    | "htm"
            )
            || name.starts_with("LICENSE")
            || matches!(
                name,
                "CODEOWNERS"
                    | "CODOWNERS"
                    | "Justfile"
                    | "justfile"
                    | "Containerfile"
                    | "Dockerfile"
                    | "MAINTAINERS"
                    | "FUNDING"
            );
        if !text_input {
            if path.is_file() {
                bail!(
                    "unsupported input {}; use --lang for a source grammar or an explicit extract adapter for documents/images",
                    path.display()
                );
            }
            skipped += 1;
            continue;
        }
        match Source::read(&p, &root) {
            Ok(mut s) => {
                if let Some(l) = forced_lang {
                    s.lang = Some(l);
                }
                sources.push(s)
            }
            Err(e) => diagnostics.push(Diagnostic {
                file: p.strip_prefix(&root).unwrap_or(&p).display().to_string(),
                message: format!("{e:#}"),
            }),
        }
    }
    let mut findings = vec![];
    let mut parsed_files = 0;
    let mut languages = BTreeSet::new();
    let mut suppressed = 0;
    let rules = pons_rules::registry();
    for source in &sources {
        let patterns = config.pattern_findings(source);
        if source.lang.is_none() {
            findings.extend(patterns.clone());
        }
        if let Some(lang) = source.lang {
            match Parsed::new(lang, &source.text) {
                Ok(parsed) => {
                    parsed_files += 1;
                    languages.insert(lang.name().into());
                    for f in patterns {
                        if inline_allowed(source, &parsed, &f) {
                            suppressed += 1;
                        } else {
                            findings.push(f);
                        }
                    }
                    for f in pons_rules::budget::check(source, &parsed, &config.config.budget)? {
                        if inline_allowed(source, &parsed, &f) {
                            suppressed += 1;
                        } else {
                            findings.push(f);
                        }
                    }
                    for rule in &rules {
                        for f in rule.check(source, &parsed) {
                            if inline_allowed(source, &parsed, &f) {
                                suppressed += 1;
                            } else {
                                findings.push(f);
                            }
                        }
                    }
                }
                Err(e) => diagnostics.push(Diagnostic {
                    file: source.relative.clone(),
                    message: format!("{e:#}"),
                }),
            }
        }
    }
    match pons_rules::repository::check(&sources, config.config.i18n.as_ref()) {
        Ok(f) => findings.extend(f),
        Err(e) => diagnostics.push(Diagnostic {
            file: "pons.toml / repository checks".into(),
            message: format!("{e:#}"),
        }),
    }
    findings.extend(pons_rules::documents::check(
        &sources,
        config.config.identity.as_ref(),
    )?);
    findings.extend(pons_rules::assets::check(&sources, &config.config.asset)?);
    findings.retain(|f| {
        if config.suppressed(f) {
            suppressed += 1;
            return false;
        }
        (selected.is_empty() || selected.contains(&f.rule_id))
            && (!no_speculative || f.evidence != EvidenceClass::Speculative)
    });
    let scanned = Scanned {
        root: root.display().to_string(),
        files: sources.len(),
        parsed_files,
        skipped_files: skipped,
        suppressed_findings: suppressed,
        languages,
        complete: diagnostics.is_empty(),
    };
    Ok((Report::new(scanned, findings, diagnostics), metadata))
}
