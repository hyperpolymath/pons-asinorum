// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{Location, RawFinding, Severity, Tier};
use pons_core::lang::Lang;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

/// No Rust variant: `String += &str` is amortised O(1) there, so the pattern
/// this rule targets (quadratic rebuild from an immutable string type) does
/// not apply.
const LANGUAGES: &[Lang] = &[Lang::Python, Lang::JavaScript, Lang::TypeScript, Lang::Tsx];

fn query_source(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => include_str!("../../rules/string-concat-in-loop/python.scm"),
        Lang::JavaScript => include_str!("../../rules/string-concat-in-loop/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/string-concat-in-loop/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/string-concat-in-loop/tsx.scm"),
        Lang::Rust => unreachable!("string-concat-in-loop has no Rust variant"),
    }
}

fn loop_kinds(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["for_statement", "while_statement"],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &[
            "for_statement",
            "for_in_statement",
            "while_statement",
            "do_statement",
        ],
        Lang::Rust => &[],
    }
}

/// A function/closure boundary walked through on the way up from the
/// assignment to a loop means the assignment executes once per *call*, not
/// once per loop iteration — that's not the quadratic-rebuild pattern this
/// rule targets, so the walk stops there rather than crediting the loop.
fn boundary_kinds(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["function_definition", "lambda"],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &[
            "function_declaration",
            "function_expression",
            "arrow_function",
            "generator_function_declaration",
            "generator_function",
            "method_definition",
        ],
        Lang::Rust => &[],
    }
}

/// String-literal node kinds a syntactic (T0) check can recognise directly.
/// This is the "string-typed-RHS narrowing" needed so `count += 1` never
/// matches: without it, any augmented assignment would qualify.
fn string_literal_kinds(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["string"],
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => &["string", "template_string"],
        Lang::Rust => &[],
    }
}

fn subtree_has_string_literal(node: Node, string_kinds: &[&str]) -> bool {
    if string_kinds.contains(&node.kind()) {
        return true;
    }
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .any(|child| subtree_has_string_literal(child, string_kinds));
    result
}

/// Walks up from `node` looking for an enclosing loop, refusing to cross a
/// function/closure boundary first (see [`boundary_kinds`]).
fn enclosed_by_loop(node: Node, loops: &[&str], boundaries: &[&str]) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        let kind = n.kind();
        if loops.contains(&kind) {
            return true;
        }
        if boundaries.contains(&kind) {
            return false;
        }
        current = n.parent();
    }
    false
}

/// True if `node` is a statement that assigns `target_name` a value whose
/// subtree contains a string literal — i.e. it looks like the accumulator's
/// own string-typed initialisation (`s = ''`, `let s = "";`), not the loop
/// body's `+=`.
fn declares_target_as_string(
    node: Node,
    target_name: &str,
    text: &[u8],
    lang: Lang,
    string_kinds: &[&str],
) -> bool {
    let text_of = |n: Node| n.utf8_text(text).unwrap_or_default();

    match lang {
        Lang::Python => {
            let candidate = if node.kind() == "expression_statement" {
                node.named_child(0)
            } else {
                Some(node)
            };
            let Some(candidate) = candidate.filter(|c| c.kind() == "assignment") else {
                return false;
            };
            let (Some(left), Some(right)) = (
                candidate.child_by_field_name("left"),
                candidate.child_by_field_name("right"),
            ) else {
                return false;
            };
            text_of(left) == target_name && subtree_has_string_literal(right, string_kinds)
        }
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => {
            if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
                let mut cursor = node.walk();
                let result = node.named_children(&mut cursor).any(|declarator| {
                    declarator.kind() == "variable_declarator"
                        && declarator
                            .child_by_field_name("name")
                            .is_some_and(|n| text_of(n) == target_name)
                        && declarator
                            .child_by_field_name("value")
                            .is_some_and(|v| subtree_has_string_literal(v, string_kinds))
                });
                return result;
            }
            let candidate = if node.kind() == "expression_statement" {
                node.named_child(0)
            } else {
                Some(node)
            };
            let Some(candidate) = candidate.filter(|c| c.kind() == "assignment_expression") else {
                return false;
            };
            let (Some(left), Some(right)) = (
                candidate.child_by_field_name("left"),
                candidate.child_by_field_name("right"),
            ) else {
                return false;
            };
            text_of(left) == target_name && subtree_has_string_literal(right, string_kinds)
        }
        Lang::Rust => false,
    }
}

/// Finds the block that scopes `node` — the body of the nearest enclosing
/// function/closure (see [`boundary_kinds`]), or `fallback_root` (the whole
/// file) if there is none. Declarations outside this block can't be the
/// accumulator's own initialisation without risking a same-named variable
/// in an unrelated function.
fn enclosing_scope<'a>(node: Node<'a>, boundaries: &[&str], fallback_root: Node<'a>) -> Node<'a> {
    let mut current = node.parent();
    while let Some(n) = current {
        if boundaries.contains(&n.kind()) {
            return n.child_by_field_name("body").unwrap_or(n);
        }
        current = n.parent();
    }
    fallback_root
}

/// Searches `scope` for a statement textually before `before_byte` that
/// declares `target_name` as a string, without descending into a nested
/// function/closure boundary — a same-named variable local to an inner
/// function is a different variable, not this accumulator's declaration.
fn scope_has_earlier_string_declaration(
    scope: Node,
    target_name: &str,
    before_byte: usize,
    boundaries: &[&str],
    text: &[u8],
    lang: Lang,
    string_kinds: &[&str],
) -> bool {
    let mut cursor = scope.walk();
    let result = scope.children(&mut cursor).any(|child| {
        if boundaries.contains(&child.kind()) {
            return false;
        }
        (child.start_byte() < before_byte
            && declares_target_as_string(child, target_name, text, lang, string_kinds))
            || scope_has_earlier_string_declaration(
                child,
                target_name,
                before_byte,
                boundaries,
                text,
                lang,
                string_kinds,
            )
    });
    result
}

/// Immutable-string `+=` / `x = x + ...` accumulation inside a loop: each
/// append rebuilds the whole string, so an N-iteration loop does O(N^2)
/// work. Detection is structural: query for the augmented-assignment (or
/// self-referential `+`) shape directly, then walk *up* the parent chain
/// from the matched node to check for an enclosing loop — the inverse
/// direction from `while-true-no-break`/`empty-effect-loop`'s walk down
/// from a loop to its body, per `docs/PLAN.adoc` Appendix D.
///
/// String-typed-RHS narrowing (so `count += 1` never matches): the
/// accumulation counts as string-typed if *either* the appended value's own
/// subtree contains a string literal (`s += "x"`), *or* the target was
/// itself declared with a string initialiser earlier in its enclosing scope
/// (`s = ""` ... `s += x`) — the latter is what makes the common
/// "append each loop item onto an accumulator string" shape detectable even
/// though the appended item itself is a plain identifier, not a literal.
pub struct StringConcatInLoop {
    queries: HashMap<Lang, Query>,
}

impl StringConcatInLoop {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("string-concat-in-loop query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for StringConcatInLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for StringConcatInLoop {
    fn id(&self) -> &'static str {
        "string-concat-in-loop"
    }

    fn description(&self) -> &'static str {
        "repeated string concatenation inside a loop, quadratic where a join is linear"
    }

    fn languages(&self) -> &'static [Lang] {
        LANGUAGES
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let Some(query) = self.queries.get(&ctx.lang) else {
            return Vec::new();
        };

        let loops = loop_kinds(ctx.lang);
        let boundaries = boundary_kinds(ctx.lang);
        let string_kinds = string_literal_kinds(ctx.lang);

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, ctx.tree.root_node(), ctx.text.as_bytes());
        let mut findings = Vec::new();

        while let Some(m) = matches.next() {
            let mut assign = None;
            let mut target = None;
            let mut addend = None;
            let mut addend_base = None;
            let mut binop = None;

            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "assign" => assign = Some(cap.node),
                    "target" => target = Some(cap.node),
                    "addend" => addend = Some(cap.node),
                    "addend_base" => addend_base = Some(cap.node),
                    "binop" => binop = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(assign), Some(target), Some(addend)) = (assign, target, addend) else {
                continue;
            };

            // The `binop`/`addend_base` captures are only present for the
            // self-referential `x = x + ...` shape; when present, the
            // operator must be `+` and the base must be the same identifier
            // as the assignment target, or this isn't accumulation.
            if let Some(binop) = binop {
                let is_plus = binop
                    .child_by_field_name("operator")
                    .is_some_and(|op| op.kind() == "+");
                let same_target = addend_base.is_some_and(|base| {
                    base.utf8_text(ctx.text.as_bytes()) == target.utf8_text(ctx.text.as_bytes())
                });
                if !is_plus || !same_target {
                    continue;
                }
            } else {
                let is_augmented_plus = assign
                    .child_by_field_name("operator")
                    .is_some_and(|op| op.kind() == "+=");
                if !is_augmented_plus {
                    continue;
                }
            }

            let target_name = target.utf8_text(ctx.text.as_bytes()).unwrap_or_default();
            let scope = enclosing_scope(assign, boundaries, ctx.tree.root_node());
            let looks_stringy = subtree_has_string_literal(addend, string_kinds)
                || scope_has_earlier_string_declaration(
                    scope,
                    target_name,
                    assign.start_byte(),
                    boundaries,
                    ctx.text.as_bytes(),
                    ctx.lang,
                    string_kinds,
                );
            if !looks_stringy {
                continue;
            }

            if !enclosed_by_loop(assign, loops, boundaries) {
                continue;
            }

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &assign, ctx.text),
                "string built by repeated concatenation in a loop",
                "string accumulation inside a loop rebuilds the whole string each iteration \
                 (quadratic)"
                    .to_string(),
                Some("tiny bounded loops; languages with mutable strings / ropes".to_string()),
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
        let rule = StringConcatInLoop::new();
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
    fn fires_on_python_augmented_string_concat_in_for_loop() {
        let findings = findings_for(
            Lang::Python,
            "def f(items):\n    s = ''\n    for x in items:\n        s += 'a'\n    return s\n",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_python_self_referential_concat_in_while_loop() {
        let findings = findings_for(
            Lang::Python,
            "def f(y):\n    s = ''\n    while y:\n        s = s + 'a'\n    return s\n",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_python_numeric_accumulation() {
        let findings = findings_for(
            Lang::Python,
            "def f(items):\n    n = 0\n    for x in items:\n        n += 1\n    return n\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_python_concat_outside_loop() {
        let findings = findings_for(
            Lang::Python,
            "def f():\n    s = ''\n    s += 'a'\n    return s\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_on_numeric_target_with_identifier_addend() {
        let findings = findings_for(
            Lang::Python,
            "def f(deltas):\n    n = 0\n    for d in deltas:\n        n += d\n    return n\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn does_not_fire_across_a_nested_function_boundary() {
        let findings = findings_for(
            Lang::Python,
            "def f(items):\n    for x in items:\n        def g():\n            t = ''\n            t += 'z'\n            return t\n        g()\n",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_js_augmented_string_concat_in_for_of_loop() {
        let findings = findings_for(
            Lang::JavaScript,
            "function f(items) { let s = ''; for (const x of items) { s += 'a'; } return s; }",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn fires_on_python_augmented_concat_of_identifier_onto_string_typed_target() {
        let findings = findings_for(
            Lang::Python,
            "def f(items):\n    s = ''\n    for x in items:\n        s += x\n    return s\n",
        );
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn does_not_fire_on_js_numeric_accumulation() {
        let findings = findings_for(
            Lang::JavaScript,
            "function f(items) { let n = 0; for (const x of items) { n += 1; } return n; }",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn fires_on_typescript_self_referential_concat_in_while_loop() {
        let findings = findings_for(
            Lang::TypeScript,
            "function f(y: boolean): string { let s = ''; while (y) { s = s + 'a'; } return s; }",
        );
        assert_eq!(findings.len(), 1);
    }
}
