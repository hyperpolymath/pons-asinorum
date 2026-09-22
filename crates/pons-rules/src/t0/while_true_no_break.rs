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
        Lang::Python => include_str!("../../rules/while-true-no-break/python.scm"),
        Lang::JavaScript => include_str!("../../rules/while-true-no-break/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/while-true-no-break/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/while-true-no-break/tsx.scm"),
        Lang::Rust => include_str!("../../rules/while-true-no-break/rust.scm"),
    }
}

/// Node kinds that count as an exit from the loop: `break`, `return`, and the
/// language's raise/throw equivalent. Found anywhere in the body (even inside
/// a nested `for`/inner loop whose own `break` wouldn't really reach the
/// outer loop) is deliberately treated as "has an exit" — this rule would
/// rather under-flag a loop that turns out to still be infinite than
/// false-positive on one that isn't; see the module doc comment.
fn exit_kinds(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["break_statement", "return_statement", "raise_statement"],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => {
            &["break_statement", "return_statement", "throw_statement"]
        }
        Lang::Rust => &["break_expression", "return_expression"],
    }
}

/// Node kinds that introduce a new function scope — the walker stops
/// descending here, since a `break`/`return`/`throw` inside a nested
/// function or closure exits *that* function, not the loop being checked.
fn boundary_kinds(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["function_definition", "lambda"],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &[
            "function_declaration",
            "function_expression",
            "arrow_function",
            "method_definition",
            "generator_function",
            "generator_function_declaration",
        ],
        Lang::Rust => &["closure_expression", "function_item"],
    }
}

fn body_has_exit(node: Node, exits: &[&str], boundaries: &[&str]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if exits.contains(&kind) {
            return true;
        }
        if boundaries.contains(&kind) {
            continue;
        }
        if body_has_exit(child, exits, boundaries) {
            return true;
        }
    }
    false
}

/// `while (true)` / `loop` with no reachable `break`/`return`/`throw` inside
/// it — a plausible infinite loop. `while (false)`/`if (true|false)` are
/// `constant-condition`'s territory (rule 4), not this rule's.
pub struct WhileTrueNoBreak {
    queries: HashMap<Lang, Query>,
}

impl WhileTrueNoBreak {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("while-true-no-break query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for WhileTrueNoBreak {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for WhileTrueNoBreak {
    fn id(&self) -> &'static str {
        "while-true-no-break"
    }

    fn description(&self) -> &'static str {
        "an unconditional loop with no reachable break, return or throw"
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
            let mut body = None;
            let mut stmt = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "cond" => cond = Some(cap.node),
                    "body" => body = Some(cap.node),
                    "stmt" => stmt = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(body), Some(stmt)) = (body, stmt) else {
                continue;
            };

            // Rust's `while_expression` pattern matches both `while true` and
            // `while false` (one `boolean_literal` kind covers both) —
            // `while false` is `constant-condition`'s territory, not this
            // rule's.
            if let Some(cond) = cond {
                let cond_text = cond.utf8_text(ctx.text.as_bytes()).unwrap_or_default();
                if cond_text != "true" {
                    continue;
                }
            }

            if body_has_exit(body, exit_kinds(ctx.lang), boundary_kinds(ctx.lang)) {
                continue;
            }

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &stmt),
                "no way to interrupt this loop",
                "infinite loop with no reachable break/return/throw".to_string(),
                Some(
                    "an intentional daemon/event loop that exits via an external \
                     signal rather than a break"
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
        let rule = WhileTrueNoBreak::new();
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
    fn fires_on_python_while_true_no_break() {
        let findings = findings_for(Lang::Python, "while True:\n    do_thing()\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_while_true_with_break() {
        let findings = findings_for(
            Lang::Python,
            "while True:\n    if x:\n        break\n    do_thing()\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_python_while_true_with_nested_def_break() {
        let findings = findings_for(
            Lang::Python,
            "while True:\n    def inner():\n        break\n    do_thing()\n",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_js_while_true_no_break() {
        let findings = findings_for(Lang::JavaScript, "while (true) { doThing(); }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_js_while_true_with_throw() {
        let findings = findings_for(Lang::JavaScript, "while (true) { throw new Error(); }\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_rust_loop_no_break() {
        let findings = findings_for(Lang::Rust, "fn f() { loop { do_thing(); } }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_rust_while_true_no_break() {
        let findings = findings_for(Lang::Rust, "fn f() { while true { do_thing(); } }\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_rust_loop_with_break() {
        let findings = findings_for(Lang::Rust, "fn f() { loop { if x { break; } } }\n");
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_rust_while_false() {
        let findings = findings_for(Lang::Rust, "fn f() { while false { do_thing(); } }\n");
        assert!(findings.is_empty());
    }
}
