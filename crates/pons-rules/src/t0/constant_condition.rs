// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{Location, RawFinding, Severity, Tier};
use pons_core::lang::Lang;
use tree_sitter::{Query, QueryCursor, StreamingIterator};

const LANGUAGES: &[Lang] = &[
    Lang::Python,
    Lang::JavaScript,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::Rust,
];

fn query_source(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => include_str!("../../rules/constant-condition/python.scm"),
        Lang::JavaScript => include_str!("../../rules/constant-condition/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/constant-condition/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/constant-condition/tsx.scm"),
        Lang::Rust => include_str!("../../rules/constant-condition/rust.scm"),
    }
}

/// `if (true)`, `if (false)`, `while (false)` — a literal boolean in the
/// condition slot. `while (true)` / `loop` is deliberately excluded here —
/// that's `while-true-no-break`'s territory (rule 5), which judges it by
/// reachable exits rather than flagging the literal outright.
pub struct ConstantCondition {
    queries: HashMap<Lang, Query>,
}

impl ConstantCondition {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("constant-condition query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for ConstantCondition {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ConstantCondition {
    fn id(&self) -> &'static str {
        "constant-condition"
    }

    fn languages(&self) -> &'static [Lang] {
        LANGUAGES
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let Some(query) = self.queries.get(&ctx.lang) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, ctx.tree.root_node(), ctx.text.as_bytes());
        let mut findings = Vec::new();

        while let Some(m) = matches.next() {
            let mut cond = None;
            let mut stmt = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "cond" => cond = Some(cap.node),
                    "stmt" => stmt = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(cond), Some(stmt)) = (cond, stmt) else {
                continue;
            };

            let is_while = stmt.kind().contains("while");
            let cond_text = cond.utf8_text(ctx.text.as_bytes()).unwrap_or_default();
            let is_true = cond_text.eq_ignore_ascii_case("true");

            // `while (true)` / `loop` is rule 5's territory, not this rule's.
            if is_while && is_true {
                continue;
            }

            let keyword = if is_while { "while" } else { "if" };
            let literal = if is_true { "true" } else { "false" };

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &stmt),
                "constant condition",
                format!("`{keyword} ({literal})` — the condition can never vary"),
                Some("a debug or feature-flag constant that gets edited in place".to_string()),
            ));
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pons_core::parse;
    use pons_core::source::SourceFile;
    use std::path::PathBuf;

    fn findings_for(lang: Lang, text: &str) -> Vec<RawFinding> {
        let rule = ConstantCondition::new();
        let source = SourceFile {
            path: PathBuf::from("test"),
            lang,
            text: text.to_string(),
        };
        let tree = parse::parse(&source).unwrap();
        let ctx = RuleCtx {
            path: &source.path,
            lang: source.lang,
            text: &source.text,
            tree: &tree,
        };
        rule.check(&ctx)
    }

    #[test]
    fn fires_on_python_if_true() {
        let findings = findings_for(Lang::Python, "if True:\n    pass\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_python_while_false() {
        let findings = findings_for(Lang::Python, "while False:\n    pass\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_while_true() {
        let findings = findings_for(Lang::Python, "while True:\n    pass\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_python_variable_condition() {
        let findings = findings_for(Lang::Python, "if x:\n    pass\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_js_if_false() {
        let findings = findings_for(Lang::JavaScript, "if (false) { f(); }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_js_while_true() {
        let findings = findings_for(Lang::JavaScript, "while (true) { f(); }\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_rust_if_true() {
        let findings = findings_for(Lang::Rust, "fn f() { if true { g(); } }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_rust_while_true() {
        let findings = findings_for(Lang::Rust, "fn f() { while true { g(); } }\n");
        assert!(findings.is_empty());
    }
}
