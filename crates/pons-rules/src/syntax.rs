// SPDX-License-Identifier: MPL-2.0
use pons_core::{Finding, Lang, Parsed, Rule, RuleMeta, Source};
use tree_sitter::Node;

pub struct SyntaxRule(pub usize);
impl Rule for SyntaxRule {
    fn meta(&self) -> RuleMeta {
        let (id, message, counter) = match self.0 {
            0 => (
                "div-by-literal-zero",
                "Division or remainder uses a literal zero divisor",
                "Floating-point infinity/NaN, operator overloads or an intentional failure test may be deliberate.",
            ),
            1 => (
                "self-assignment",
                "A plain identifier is assigned to itself",
                "A deliberate binding operation can be suppressed; property setters and destructuring are excluded.",
            ),
            2 => (
                "constant-condition",
                "A branch condition is a literal boolean",
                "Temporary feature toggles and generated branches can be deliberate.",
            ),
            3 => (
                "while-true-no-break",
                "A literal-true loop has no visible exit or call that could exit",
                "An intentional spin loop needs a documented inline or project allowance. This is not a termination proof.",
            ),
            4 => (
                "empty-effect-loop",
                "A loop has an empty body",
                "Iteration can itself have effects; iterator calls, generators and async iteration are excluded.",
            ),
            5 => (
                "unreachable-after-jump",
                "A statement follows an unconditional jump in the same block",
                "Declaration hoisting, conditional compilation and macros need language-specific judgement.",
            ),
            6 => (
                "swallowed-error",
                "A catch-all exception handler has no visible handling action",
                "An intentionally best-effort operation should use an explicit allowance; typed/specific exceptions are excluded.",
            ),
            _ => (
                "string-concat-in-loop",
                "A loop repeatedly appends a string literal",
                "Small bounded strings or runtimes optimising concatenation may make this harmless; no complexity bound is inferred.",
            ),
        };
        let mut languages = crate::source_languages();
        if self.0 == 6 {
            languages.retain(|l| l != "rust");
        }
        crate::meta(id, message, counter, languages)
    }
    fn check(&self, source: &Source, parsed: &Parsed) -> Vec<Finding> {
        if self.0 == 6 && parsed.lang == Lang::Rust {
            return vec![];
        }
        let mut findings = Vec::new();
        let mut stack = vec![parsed.tree.root_node()];
        let meta = self.meta();
        while let Some(node) = stack.pop() {
            if self.matches(node, source, parsed.lang) {
                findings.push(Finding::new(&meta, source.location(node.start_byte(),node.end_byte()), format!("{} grammar matched `{}` at line {}; inspection is limited to this syntax tree.",parsed.lang.name(),node.kind(),node.start_position().row+1)));
            }
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
        findings
    }
}
impl SyntaxRule {
    fn matches(&self, node: Node<'_>, source: &Source, lang: Lang) -> bool {
        let text = |n: Node<'_>| &source.text[n.byte_range()];
        let kind = node.kind();
        let left = node.child_by_field_name("left");
        let right = node.child_by_field_name("right");
        let op = node.child_by_field_name("operator").map(text);
        match self.0 {
            0 => {
                matches!(kind, "binary_operator" | "binary_expression")
                    && matches!(op, Some("/" | "//" | "%"))
                    && right.is_some_and(|r| literal_zero(r, source))
            }
            1 => {
                matches!(kind, "assignment" | "assignment_expression")
                    && left.zip(right).is_some_and(|(l, r)| {
                        l.kind() == "identifier" && r.kind() == "identifier" && text(l) == text(r)
                    })
            }
            2 => {
                matches!(
                    kind,
                    "if_statement"
                        | "if_expression"
                        | "conditional_expression"
                        | "ternary_expression"
                ) && node
                    .child_by_field_name("condition")
                    .is_some_and(|c| literal_bool(c, source).is_some())
            }
            3 => {
                if !matches!(
                    kind,
                    "while_statement" | "while_expression" | "loop_expression"
                ) {
                    return false;
                }
                let always = kind == "loop_expression"
                    || node
                        .child_by_field_name("condition")
                        .is_some_and(|c| literal_bool(c, source) == Some(true));
                always
                    && node.child_by_field_name("body").is_some_and(|b| {
                        !descendants(b).iter().any(|n| {
                            matches!(
                                n.kind(),
                                "break_statement"
                                    | "break_expression"
                                    | "return_statement"
                                    | "return_expression"
                                    | "raise_statement"
                                    | "throw_statement"
                                    | "call"
                                    | "call_expression"
                                    | "macro_invocation"
                                    | "await"
                                    | "await_expression"
                                    | "yield"
                                    | "yield_expression"
                                    | "try_expression"
                            )
                        })
                    })
            }
            4 => {
                if !matches!(
                    kind,
                    "for_statement" | "for_in_statement" | "for_expression"
                ) {
                    return false;
                }
                let iterable = node
                    .child_by_field_name("right")
                    .or_else(|| node.child_by_field_name("value"));
                // Only literal sequences; arbitrary iteration is an effect in its own right.
                iterable.is_some_and(|v| {
                    matches!(
                        v.kind(),
                        "list" | "tuple" | "array" | "array_expression" | "range_expression"
                    )
                }) && node.child_by_field_name("body").is_some_and(empty)
            }
            5 => {
                if !matches!(kind, "block" | "statement_block") {
                    return false;
                }
                let children: Vec<_> = children(node)
                    .into_iter()
                    .filter(|n| !n.kind().contains("comment"))
                    .collect();
                children.windows(2).any(|w| {
                    is_jump(w[0])
                        && !matches!(
                            w[1].kind(),
                            "function_declaration"
                                | "function_item"
                                | "class_declaration"
                                | "attribute_item"
                                | "empty_statement"
                        )
                })
            }
            6 => {
                if kind == "except_clause" && lang == Lang::Python {
                    // A bare except, or explicitly broad Exception/BaseException only.
                    let value = node.child_by_field_name("value");
                    let broad =
                        value.is_none_or(|n| matches!(text(n), "Exception" | "BaseException"));
                    broad
                        && children(node)
                            .last()
                            .is_some_and(|n| n.kind() == "block" && empty(*n))
                } else {
                    kind == "catch_clause" && node.child_by_field_name("body").is_some_and(empty)
                }
            }
            _ => {
                let augmented = matches!(
                    kind,
                    "augmented_assignment"
                        | "augmented_assignment_expression"
                        | "compound_assignment_expr"
                );
                augmented
                    && op == Some("+=")
                    && left.is_some_and(|l| l.kind() == "identifier")
                    && right.is_some_and(|r| matches!(r.kind(), "string" | "string_literal"))
                    && in_loop(node)
            }
        }
    }
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}
fn descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = vec![];
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        stack.extend(children(n));
        out.push(n);
    }
    out
}
fn unparen(mut n: Node<'_>) -> Node<'_> {
    while matches!(n.kind(), "parenthesized_expression") && n.named_child_count() == 1 {
        n = n.named_child(0).expect("one named child");
    }
    n
}
fn literal_bool(node: Node<'_>, source: &Source) -> Option<bool> {
    let node = unparen(node);
    if !matches!(node.kind(), "true" | "false" | "boolean_literal") {
        return None;
    }
    match &source.text[node.byte_range()] {
        "True" | "true" => Some(true),
        "False" | "false" => Some(false),
        _ => None,
    }
}
fn literal_zero(node: Node<'_>, source: &Source) -> bool {
    let node = unparen(node);
    matches!(
        node.kind(),
        "integer" | "float" | "number" | "integer_literal" | "float_literal"
    ) && matches!(
        &source.text[node.byte_range()],
        "0" | "0.0" | "0x0" | "0X0" | "0b0" | "0o0"
    )
}
fn empty(node: Node<'_>) -> bool {
    children(node).iter().all(|n| {
        n.kind().contains("comment") || matches!(n.kind(), "pass_statement" | "empty_statement")
    })
}
fn is_jump(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "return_statement"
            | "return_expression"
            | "break_statement"
            | "break_expression"
            | "continue_statement"
            | "continue_expression"
            | "raise_statement"
            | "throw_statement"
    ) || (node.kind() == "expression_statement" && node.named_child(0).is_some_and(is_jump))
}
fn in_loop(mut node: Node<'_>) -> bool {
    while let Some(p) = node.parent() {
        if matches!(
            p.kind(),
            "function_definition"
                | "function_declaration"
                | "function_item"
                | "arrow_function"
                | "lambda"
                | "closure_expression"
        ) {
            return false;
        }
        if matches!(
            p.kind(),
            "for_statement"
                | "for_in_statement"
                | "for_expression"
                | "while_statement"
                | "while_expression"
                | "loop_expression"
        ) {
            return true;
        }
        node = p;
    }
    false
}
