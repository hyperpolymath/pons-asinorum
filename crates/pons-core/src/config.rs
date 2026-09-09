// SPDX-License-Identifier: MPL-2.0
use crate::{EvidenceClass, Finding, RuleMeta, Severity, Source};
use anyhow::{Context, Result, bail};
use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde::Deserialize;
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub suppress: Suppress,
    pub pattern: Vec<Pattern>,
    pub i18n: Option<I18n>,
    pub budget: Vec<Budget>,
    pub identity: Option<Identity>,
    pub asset: Vec<Asset>,
}
/// Author-supplied requirements, not inferred legal obligations.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Asset {
    pub path: String,
    pub decorative: bool,
    pub rights_required: bool,
    pub rights: String,
    pub attribution_required: bool,
    pub attribution: String,
    pub ai_generated: bool,
    pub ai_disclosure: String,
    pub description_required: bool,
    pub description: String,
    pub title_required: bool,
    pub title: String,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Suppress {
    pub rules: Vec<String>,
    pub paths: Vec<String>,
    pub allow: Vec<Allowance>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allowance {
    pub rule: String,
    pub path: String,
    pub reason: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pattern {
    pub id: String,
    pub path: String,
    pub regex: String,
    pub message: String,
    pub counter_condition: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct I18n {
    pub base: String,
    pub translations: Vec<String>,
    /// Local LOL export containing an array of {"code":"eng","name":"English"} records.
    pub lol_catalogue: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub path: String,
    pub resource: String,
    pub available: u64,
    /// Exact Python callee spelling; the user asserts each call consumes units.
    pub consume: String,
    pub units: u64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}
pub struct CompiledConfig {
    pub config: Config,
    paths: Vec<GlobMatcher>,
    allowances: Vec<(String, GlobMatcher)>,
    patterns: Vec<(RuleMeta, GlobMatcher, Regex)>,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = Source::read(path, path.parent().unwrap_or(Path::new(".")))?.text;
        toml::from_str(&text).with_context(|| format!("invalid configuration {}", path.display()))
    }
    pub fn compile(self, builtins: &[RuleMeta]) -> Result<CompiledConfig> {
        let mut asset_paths = BTreeSet::new();
        for a in &self.asset {
            if a.path.trim().is_empty() || !asset_paths.insert(&a.path) {
                bail!("asset declarations need a non-empty, unique path");
            }
        }
        for b in &self.budget {
            Glob::new(&b.path)?;
            if b.resource.trim().is_empty() || b.consume.trim().is_empty() || b.units == 0 {
                bail!("resource budgets need a resource, consume call and positive units");
            }
        }
        let mut ids: BTreeSet<String> = builtins.iter().map(|r| r.id.clone()).collect();
        let mut patterns = Vec::new();
        for p in &self.pattern {
            if !p.id.starts_with("user-")
                || !p
                    .id
                    .chars()
                    .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '-')
                || !ids.insert(p.id.clone())
            {
                bail!("custom rule id must be unique and begin user-: {}", p.id);
            }
            if p.message.trim().is_empty() || p.counter_condition.trim().is_empty() {
                bail!("custom rule {} needs a message and counter_condition", p.id);
            }
            let meta = RuleMeta {
                id: p.id.clone(),
                evidence: EvidenceClass::Heuristic,
                severity: Severity::Warn,
                languages: vec!["text".into()],
                message: p.message.clone(),
                counter_condition: p.counter_condition.clone(),
            };
            patterns.push((
                meta,
                Glob::new(&p.path)?.compile_matcher(),
                Regex::new(&p.regex)?,
            ));
        }
        for id in self
            .suppress
            .rules
            .iter()
            .chain(self.suppress.allow.iter().map(|a| &a.rule))
        {
            if !ids.contains(id) {
                bail!("unknown suppressed rule: {id}");
            }
        }
        let paths = self
            .suppress
            .paths
            .iter()
            .map(|p| Ok(Glob::new(p)?.compile_matcher()))
            .collect::<Result<_>>()?;
        let allowances = self
            .suppress
            .allow
            .iter()
            .map(|a| {
                if a.reason.trim().is_empty() {
                    bail!("suppression for {} needs a reason", a.rule);
                }
                Ok((a.rule.clone(), Glob::new(&a.path)?.compile_matcher()))
            })
            .collect::<Result<_>>()?;
        Ok(CompiledConfig {
            config: self,
            paths,
            allowances,
            patterns,
        })
    }
}
impl CompiledConfig {
    pub fn metadata(&self) -> Vec<RuleMeta> {
        self.patterns.iter().map(|p| p.0.clone()).collect()
    }
    pub fn pattern_findings(&self, source: &Source) -> Vec<Finding> {
        self.patterns.iter().filter(|(_, path, _)| path.is_match(&source.relative)).flat_map(|(meta, _, regex)| {
            regex.find_iter(&source.text).map(|m| Finding::new(meta, source.location(m.start(), m.end()), "Matched an explicitly configured text pattern; no type or control-flow inference was performed.")).collect::<Vec<_>>()
        }).collect()
    }
    pub fn suppressed(&self, finding: &Finding) -> bool {
        self.config.suppress.rules.contains(&finding.rule_id)
            || self
                .paths
                .iter()
                .any(|p| p.is_match(&finding.location.file))
            || self
                .allowances
                .iter()
                .any(|(id, p)| id == &finding.rule_id && p.is_match(&finding.location.file))
    }
}

/// Only syntax-tree comment nodes can suppress source findings. Strings cannot.
pub fn inline_allowed(source: &Source, parsed: &crate::Parsed, finding: &Finding) -> bool {
    let mut stack = vec![parsed.tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind().contains("comment")
            && node.start_position().row < finding.location.line_start
            && node.end_position().row + 1 >= finding.location.line_start
        {
            let comment = &source.text[node.byte_range()];
            if let Some((_, rest)) = comment.split_once("pons:allow ") {
                let ids = rest.split_once(" -- ").map_or(rest, |x| x.0);
                if ids
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .any(|id| id == finding.rule_id)
                {
                    return true;
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    false
}
