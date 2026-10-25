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
    Lang::Php,
];

/// Return this rule's query for the selected grammar, including PHP.
fn query_source(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => include_str!("../../rules/unreachable-after-jump/python.scm"),
        Lang::JavaScript => include_str!("../../rules/unreachable-after-jump/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/unreachable-after-jump/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/unreachable-after-jump/tsx.scm"),
        Lang::Rust => include_str!("../../rules/unreachable-after-jump/rust.scm"),
        Lang::Php => include_str!("../../rules/unreachable-after-jump/php.scm"),
    }
}

fn jump_keyword(kind: &str) -> &'static str {
    if kind.contains("return") {
        "return"
    } else if kind.contains("throw") {
        "throw"
    } else if kind.contains("raise") {
        "raise"
    } else if kind.contains("break") {
        "break"
    } else if kind.contains("continue") {
        "continue"
    } else {
        "jump"
    }
}

/// Statements textually after `return`/`throw`/`raise`/`break`/`continue` in
/// the same block. The `.` anchor between captures in each `.scm` pattern
/// asserts the following statement is the *immediate* next named sibling —
/// tree-sitter proves adjacency directly, no manual sibling-walk needed.
pub struct UnreachableAfterJump {
    queries: HashMap<Lang, Query>,
}

impl UnreachableAfterJump {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("unreachable-after-jump query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for UnreachableAfterJump {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UnreachableAfterJump {
    fn id(&self) -> &'static str {
        "unreachable-after-jump"
    }

    fn description(&self) -> &'static str {
        "a statement immediately after return, throw, raise, break or continue"
    }

    fn languages(&self) -> &'static [Lang] {
        LANGUAGES
    }

    /// Return T0 warnings at the first non-comment named sibling after each
    /// matched unconditional jump in the same block. Trailing comments alone
    /// produce no finding; languages without a stored query yield no findings.
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let Some(query) = self.queries.get(&ctx.lang) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, ctx.tree.root_node(), ctx.text.as_bytes());
        let mut findings = Vec::new();

        while let Some(m) = matches.next() {
            let mut jump = None;
            let mut next = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "jump" => jump = Some(cap.node),
                    "next" => next = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(jump), Some(next)) = (jump, next) else {
                continue;
            };

            // Comments are named tree-sitter nodes but are not executable
            // statements. Advance to the next named sibling so a comment
            // cannot either become a finding or hide genuinely dead code.
            let mut next = Some(next);
            while next
                .is_some_and(|n| matches!(n.kind(), "comment" | "line_comment" | "block_comment"))
            {
                next = next.and_then(|n| n.next_named_sibling());
            }
            let Some(next) = next else { continue };

            let keyword = jump_keyword(jump.kind());

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &next, ctx.text),
                "statement is unreachable after an unconditional jump",
                format!("unreachable code — this can never run after `{keyword}`"),
                Some("a label or fallthrough construct the walker doesn't see".to_string()),
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
        let rule = UnreachableAfterJump::new();
        let source = SourceFile {
            path: PathBuf::from("test"),
            lang,
            text: text.to_string(),
            transcoded: false,
        };
        let tree = parse::parse(&source).unwrap();
        let ctx = RuleCtx::new(&source.path, source.lang, &source.text, &tree);
        rule.check(&ctx)
    }

    #[test]
    fn fires_on_python_code_after_return() {
        let findings = findings_for(Lang::Python, "def f():\n    return 1\n    do_thing()\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_return_at_block_end() {
        let findings = findings_for(Lang::Python, "def f():\n    do_thing()\n    return 1\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_js_code_after_break() {
        let findings = findings_for(Lang::JavaScript, "for (;;) { break; doThing(); }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_js_code_in_separate_block() {
        let findings = findings_for(
            Lang::JavaScript,
            "function f(x) { if (x) { return 1; } doThing(); }\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_rust_code_after_return() {
        let findings = findings_for(Lang::Rust, "fn f() -> i32 { return 1; do_thing(); }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_rust_return_as_tail_expression() {
        let findings = findings_for(Lang::Rust, "fn f() -> i32 { do_thing(); return 1 }\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn trailing_comment_is_not_an_unreachable_statement() {
        let findings = findings_for(
            Lang::Rust,
            "fn has_cycle(a: bool, b: bool) -> bool { if a { if b { return true; } } else if b { return true; // Back edge found - cycle detected\n } false }",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn comment_does_not_hide_the_next_unreachable_statement() {
        for (lang, source) in [
            (
                Lang::Rust,
                "fn f() { return; // comment\n /* another */ dead(); }",
            ),
            (
                Lang::JavaScript,
                "function f() { return; // comment\n dead(); }",
            ),
            (
                Lang::Python,
                "def f():\n    return\n    # comment\n    dead()\n",
            ),
        ] {
            assert_eq!(findings_for(lang, source).len(), 1, "{lang:?}");
        }
    }
}
