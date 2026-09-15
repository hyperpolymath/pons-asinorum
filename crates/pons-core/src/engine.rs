// SPDX-License-Identifier: MPL-2.0

use std::path::Path;

use tree_sitter::Tree;

use crate::finding::{Finding, RawFinding};
use crate::lang::Lang;
use crate::{parse, source};

/// Everything a [`Rule`] needs to inspect one source file.
pub struct RuleCtx<'a> {
    pub path: &'a Path,
    pub lang: Lang,
    pub text: &'a str,
    pub tree: &'a Tree,
}

/// A single check. Implementors live in `pons-rules`; `pons-core` stays
/// rule-agnostic. `check` returns [`RawFinding`], not [`Finding`] — a rule
/// has no way to set its own `rule_id`; only [`Engine::scan`] can, from
/// `id()`, so a rule/id mismatch cannot happen.
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn languages(&self) -> &'static [Lang];
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding>;
}

/// Orchestrates discovery, parsing, and rule execution over a directory.
pub struct Engine {
    rules: Vec<Box<dyn Rule>>,
}

impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self {
        Self { rules }
    }

    pub fn scan(&self, root: &Path) -> anyhow::Result<Vec<Finding>> {
        let mut findings = Vec::new();

        for src in source::discover(root)? {
            let tree = parse::parse(&src)?;
            let ctx = RuleCtx {
                path: &src.path,
                lang: src.lang,
                text: &src.text,
                tree: &tree,
            };

            for rule in &self.rules {
                if rule.languages().contains(&src.lang) {
                    findings.extend(
                        rule.check(&ctx)
                            .into_iter()
                            .map(|raw| raw.into_finding(rule.id())),
                    );
                }
            }
        }

        Ok(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Severity, Tier};

    struct StubRule;

    impl Rule for StubRule {
        fn id(&self) -> &'static str {
            "stub-rule"
        }

        fn languages(&self) -> &'static [Lang] {
            &[Lang::Python]
        }

        fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
            vec![RawFinding::new(
                Tier::T0,
                Severity::Warn,
                crate::finding::Location {
                    file: ctx.path.display().to_string(),
                    byte_start: 0,
                    byte_end: 1,
                    line_start: 1,
                    col_start: 1,
                    line_end: 1,
                    col_end: 2,
                },
                "stub finding",
                "stub evidence",
                None,
            )]
        }
    }

    #[test]
    fn scan_stamps_rule_id_from_the_rule_never_from_the_finding() {
        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();

        let engine = Engine::new(vec![Box::new(StubRule)]);
        let findings = engine.scan(&dir).unwrap();

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id(), "stub-rule");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_with_zero_rules_yields_zero_findings() {
        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();

        let engine = Engine::new(Vec::new());
        let findings = engine.scan(&dir).unwrap();
        assert!(findings.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn nanos() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
