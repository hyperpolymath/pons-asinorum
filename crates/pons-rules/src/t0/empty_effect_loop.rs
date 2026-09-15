// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{Location, RawFinding, Severity, Tier};
use pons_core::lang::Lang;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

const LANGUAGES: &[Lang] = &[
    Lang::Python,
    Lang::JavaScript,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::Rust,
];

fn query_source(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => include_str!("../../rules/empty-effect-loop/python.scm"),
        Lang::JavaScript => include_str!("../../rules/empty-effect-loop/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/empty-effect-loop/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/empty-effect-loop/tsx.scm"),
        Lang::Rust => include_str!("../../rules/empty-effect-loop/rust.scm"),
    }
}

/// Node kinds that count as a no-op statement — a block containing only
/// these (or nothing at all) has no effect. Rust has no bare no-op statement
/// kind, so only a literally empty block counts there.
fn noop_kinds(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["pass_statement"],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &["empty_statement"],
        Lang::Rust => &[],
    }
}

fn body_is_effect_free(body: Node, noops: &[&str]) -> bool {
    let mut cursor = body.walk();
    let result = body
        .named_children(&mut cursor)
        .all(|child| noops.contains(&child.kind()));
    result
}

/// A loop whose body is empty or contains only no-ops — entered and exited
/// for nothing. Deliberately overlaps `while-true-no-break` on `loop {}` /
/// `while (true) {}` (both fire) — that's documented, not a bug: an empty
/// infinite loop is both "no break" and "no effect" at once.
pub struct EmptyEffectLoop {
    queries: HashMap<Lang, Query>,
}

impl EmptyEffectLoop {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("empty-effect-loop query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for EmptyEffectLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for EmptyEffectLoop {
    fn id(&self) -> &'static str {
        "empty-effect-loop"
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
        let noops = noop_kinds(ctx.lang);

        while let Some(m) = matches.next() {
            let mut body = None;
            let mut stmt = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "body" => body = Some(cap.node),
                    "stmt" => stmt = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(body), Some(stmt)) = (body, stmt) else {
                continue;
            };

            if !body_is_effect_free(body, noops) {
                continue;
            }

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &stmt),
                "empty-effect-loop",
                "loop body is empty or a no-op — entered and exited for nothing".to_string(),
                Some("a deliberate spin-wait on a volatile/side-effecting condition".to_string()),
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
        let rule = EmptyEffectLoop::new();
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
    fn fires_on_python_while_pass() {
        let findings = findings_for(Lang::Python, "while True:\n    pass\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_python_for_pass() {
        let findings = findings_for(Lang::Python, "for x in y:\n    pass\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_loop_with_effect() {
        let findings = findings_for(Lang::Python, "while x:\n    do_thing()\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_js_empty_while() {
        let findings = findings_for(Lang::JavaScript, "while (x) {}\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_js_loop_with_effect() {
        let findings = findings_for(Lang::JavaScript, "while (x) { doThing(); }\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_rust_empty_loop() {
        let findings = findings_for(Lang::Rust, "fn f() { loop {} }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn overlaps_while_true_no_break_on_empty_infinite_loop() {
        let findings = findings_for(Lang::Rust, "fn f() { while true {} }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_rust_loop_with_effect() {
        let findings = findings_for(Lang::Rust, "fn f(x: bool) { loop { if x { break; } } }\n");
        assert!(findings.is_empty());
    }
}
