// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{Location, RawFinding, Severity, Tier};
use pons_core::lang::Lang;
use tree_sitter::{Query, QueryCursor, StreamingIterator};

const LANGUAGES: &[Lang] = &[Lang::Python, Lang::JavaScript, Lang::TypeScript, Lang::Tsx];

fn query_source(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => include_str!("../../rules/swallowed-error/python.scm"),
        Lang::JavaScript => include_str!("../../rules/swallowed-error/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/swallowed-error/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/swallowed-error/tsx.scm"),
        Lang::Rust => unreachable!("swallowed-error does not run on Rust — no exceptions"),
    }
}

/// A `catch` / `except` whose body is empty (or, in Python, only `pass`).
/// Rust has no exceptions and is excluded entirely.
pub struct SwallowedError {
    queries: HashMap<Lang, Query>,
}

impl SwallowedError {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("swallowed-error query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for SwallowedError {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SwallowedError {
    fn id(&self) -> &'static str {
        "swallowed-error"
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
            let mut body = None;
            let mut clause = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "body" => body = Some(cap.node),
                    "clause" => clause = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(body), Some(clause)) = (body, clause) else {
                continue;
            };

            if is_swallowed(&body, ctx.lang) {
                findings.push(RawFinding::new(
                    Tier::T0,
                    Severity::Warn,
                    Location::from_node(ctx.path.display().to_string(), &clause),
                    "empty or pass-only exception handler",
                    "the caught exception is discarded with no handling and no note",
                    Some(
                        "a documented, intentional swallow (a comment, or a re-raise \
                         elsewhere)"
                            .to_string(),
                    ),
                ));
            }
        }

        findings
    }
}

/// True if `body` (the handler's block/statement_block) has no effective
/// content — ignoring a Python `pass` (the only way to write an empty
/// Python block) — and carries no comment documenting the swallow as
/// intentional.
fn is_swallowed(body: &tree_sitter::Node, lang: Lang) -> bool {
    let mut has_comment = false;
    let mut statements = 0usize;
    let mut only_pass = true;

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "comment" {
            has_comment = true;
            continue;
        }
        statements += 1;
        if child.kind() != "pass_statement" {
            only_pass = false;
        }
    }

    if has_comment {
        return false;
    }

    match lang {
        Lang::Python => statements == 1 && only_pass,
        _ => statements == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pons_core::parse;
    use pons_core::source::SourceFile;
    use std::path::PathBuf;

    fn findings_for(lang: Lang, text: &str) -> Vec<RawFinding> {
        let rule = SwallowedError::new();
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
    fn fires_on_python_except_pass() {
        let findings = findings_for(
            Lang::Python,
            "try:\n    risky()\nexcept Exception:\n    pass\n",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_except_pass_with_comment() {
        let findings = findings_for(
            Lang::Python,
            "try:\n    risky()\nexcept Exception:\n    pass  # intentional\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_js_empty_catch() {
        let findings = findings_for(Lang::JavaScript, "try { risky(); } catch (e) {}\n");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_js_catch_with_comment() {
        let findings = findings_for(
            Lang::JavaScript,
            "try { risky(); } catch (e) {\n  // intentional\n}\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_js_catch_that_handles_the_error() {
        let findings = findings_for(Lang::JavaScript, "try { risky(); } catch (e) { log(e); }\n");
        assert!(findings.is_empty());
    }
}
