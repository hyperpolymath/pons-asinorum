// SPDX-License-Identifier: MPL-2.0

use std::path::Path;

use tree_sitter::Tree;

use crate::finding::Finding;
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
/// rule-agnostic.
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn languages(&self) -> &'static [Lang];
    fn check(&self, ctx: &RuleCtx) -> Vec<Finding>;
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
                    findings.extend(rule.check(&ctx));
                }
            }
        }

        Ok(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
