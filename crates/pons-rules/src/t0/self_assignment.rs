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
        Lang::Python => include_str!("../../rules/self-assignment/python.scm"),
        Lang::JavaScript => include_str!("../../rules/self-assignment/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/self-assignment/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/self-assignment/tsx.scm"),
        Lang::Rust => include_str!("../../rules/self-assignment/rust.scm"),
    }
}

/// `x = x` — a plain identifier assigned to itself. Restricting the query to
/// bare identifiers on both sides naturally excludes property/attribute
/// setters with effects (`obj.p = obj.p`, a different node shape) and, in
/// Rust, `let x = x;` (a `let_declaration` shadow, not an
/// `assignment_expression`) — see the counter-condition in
/// `wiki/Rule-Catalogue.asciidoc`. "Volatile reads" is not syntactically
/// checkable at T0 and is out of scope.
pub struct SelfAssignment {
    queries: HashMap<Lang, Query>,
}

impl SelfAssignment {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("self-assignment query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for SelfAssignment {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SelfAssignment {
    fn id(&self) -> &'static str {
        "self-assignment"
    }

    fn description(&self) -> &'static str {
        "an identifier assigned to itself, doing no work"
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
            let mut left = None;
            let mut right = None;
            let mut stmt = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "left" => left = Some(cap.node),
                    "right" => right = Some(cap.node),
                    "stmt" => stmt = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(left), Some(right), Some(stmt)) = (left, right, stmt) else {
                continue;
            };

            let text = ctx.text.as_bytes();
            let left_name = left.utf8_text(text).unwrap_or_default();
            let right_name = right.utf8_text(text).unwrap_or_default();
            if left_name != right_name {
                continue;
            }

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &stmt),
                "variable is assigned to itself",
                format!("`{left_name}` is assigned to itself — this has no effect"),
                Some(
                    "a property/attribute setter with side effects, or a volatile read \
                     (not checkable here)"
                        .to_string(),
                ),
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
        let rule = SelfAssignment::new();
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
    fn fires_on_python_self_assign() {
        let findings = findings_for(Lang::Python, "x = 1\nx = x\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_attribute_self_assign() {
        let findings = findings_for(Lang::Python, "obj.p = obj.p\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_python_different_names() {
        let findings = findings_for(Lang::Python, "x = y\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_js_self_assign() {
        let findings = findings_for(Lang::JavaScript, "let x = 1;\nx = x;\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_rust_self_assign() {
        let findings = findings_for(Lang::Rust, "fn f(mut x: i32) { x = x; }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_rust_let_shadow() {
        let findings = findings_for(Lang::Rust, "fn f() { let x = 1; let x = x; }\n");
        assert!(findings.is_empty());
    }
}
