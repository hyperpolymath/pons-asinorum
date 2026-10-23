// SPDX-License-Identifier: MPL-2.0

//! Explicit operator configuration.
//!
//! This is deliberately narrower than the deferred 27-check fork. It parses
//! only keys this binary actually honours: project identity (reserved for
//! document checks) and scoped suppression. Unknown keys are rejected rather
//! than silently ignored — a configuration typo must not look like a clean
//! scan.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobMatcher};
use serde::Deserialize;

use crate::finding::Finding;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub identity: Identity,
    pub suppress: Suppress,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Suppress {
    /// Rule IDs suppressed everywhere.
    pub rules: Vec<String>,
    /// Glob paths suppressed for every rule. Matched against both the path
    /// as reported and its scan-root-relative form.
    pub paths: Vec<String>,
    /// A documented, rule-and-path scoped allowance.
    pub allow: Vec<Allowance>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allowance {
    pub rule: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct CompiledConfig {
    rules: BTreeSet<String>,
    paths: Vec<GlobMatcher>,
    allowances: Vec<(String, GlobMatcher)>,
}

impl Config {
    /// Read a UTF-8 TOML configuration, rejecting unknown fields.
    ///
    /// Returns file read, decoding or TOML deserialisation errors. Rule IDs and
    /// globs are validated separately by [`Self::compile`].
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read configuration {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid configuration {}", path.display()))
    }

    /// Validates rule IDs and globs once, before any file is scanned. A bad
    /// suppression must be an operational error (exit 2), not an empty result.
    ///
    /// `known_rules` lists the accepted rule IDs. Returns an error for an
    /// unknown suppressed rule, an invalid glob or an allowance whose reason
    /// is empty after trimming whitespace.
    pub fn compile(self, known_rules: &[&str]) -> Result<CompiledConfig> {
        let known: BTreeSet<&str> = known_rules.iter().copied().collect();
        for id in &self.suppress.rules {
            if !known.contains(id.as_str()) {
                bail!("unknown suppressed rule: {id}");
            }
        }
        let paths = self
            .suppress
            .paths
            .iter()
            .map(|p| Ok(Glob::new(p)?.compile_matcher()))
            .collect::<Result<Vec<_>>>()?;
        let mut allowances = Vec::new();
        for a in &self.suppress.allow {
            if a.reason.trim().is_empty() {
                bail!("suppression for {} needs a reason", a.rule);
            }
            if !known.contains(a.rule.as_str()) {
                bail!("unknown suppressed rule: {}", a.rule);
            }
            allowances.push((a.rule.clone(), Glob::new(&a.path)?.compile_matcher()));
        }
        Ok(CompiledConfig {
            rules: self.suppress.rules.into_iter().collect(),
            paths,
            allowances,
        })
    }

    /// The no-op configuration used by library callers and tests.
    pub fn empty() -> Result<CompiledConfig> {
        Self::default().compile(&[])
    }
}

impl CompiledConfig {
    /// Return a configuration that suppresses no findings.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether a rule, path glob or rule-and-path allowance suppresses `finding`.
    ///
    /// Globs match both the reported path and its form relative to `root` when
    /// the root is a path prefix. Backslashes are normalised to forward slashes;
    /// paths are not canonicalised.
    pub fn suppressed(&self, finding: &Finding, root: &Path) -> bool {
        if self.rules.contains(finding.rule_id()) {
            return true;
        }
        let reported = finding.location().file.replace('\\', "/");
        let relative = Path::new(&finding.location().file)
            .strip_prefix(root)
            .unwrap_or(Path::new(&finding.location().file))
            .to_string_lossy()
            .replace('\\', "/");
        let matched = |glob: &GlobMatcher| glob.is_match(&reported) || glob.is_match(&relative);
        self.paths.iter().any(matched)
            || self
                .allowances
                .iter()
                .any(|(id, glob)| id == finding.rule_id() && matched(glob))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Location, Severity, Tier};

    fn finding(file: &str) -> Finding {
        Finding::new(
            "div-by-literal-zero",
            Tier::T0,
            Severity::Warn,
            Location {
                file: file.to_string(),
                byte_start: 0,
                byte_end: 1,
                line_start: 1,
                col_start: 1,
                line_end: 1,
                col_end: 2,
            },
            "x",
            "y",
            None,
        )
    }

    #[test]
    fn a_relative_suppression_path_matches_a_root_relative_finding() {
        let cfg: Config = toml::from_str(
            r#"[suppress]
paths = ["fixtures/**"]
"#,
        )
        .unwrap();
        let compiled = cfg.compile(&["div-by-literal-zero"]).unwrap();
        assert!(compiled.suppressed(&finding("repo/fixtures/positive/a.py"), Path::new("repo")));
        assert!(!compiled.suppressed(&finding("repo/src/a.py"), Path::new("repo")));
    }

    #[test]
    fn an_unknown_suppressed_rule_is_rejected() {
        let cfg: Config = toml::from_str(
            r#"[suppress]
rules = ["not-a-rule"]
"#,
        )
        .unwrap();
        assert!(cfg.compile(&["div-by-literal-zero"]).is_err());
    }
}
