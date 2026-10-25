// SPDX-License-Identifier: MPL-2.0
use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{Location, RawFinding, Severity, Tier};
use pons_core::lang::Lang;

/// A local syntax signal, not an assertion that every no-op is a defect.
pub struct EmptyPhpFunction;

impl Rule for EmptyPhpFunction {
    fn id(&self) -> &'static str {
        "empty-php-function"
    }
    fn description(&self) -> &'static str {
        "a concrete PHP function or method has no executable body"
    }
    fn languages(&self) -> &'static [Lang] {
        &[Lang::Php]
    }
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let mut findings = Vec::new();
        let mut pending = vec![ctx.tree.root_node()];
        while let Some(node) = pending.pop() {
            if matches!(node.kind(), "function_definition" | "method_declaration") {
                if let Some(body) = node.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    if body
                        .named_children(&mut cursor)
                        .all(|n| n.kind() == "comment")
                    {
                        findings.push(RawFinding::new(
                            Tier::T0,
                            Severity::Warn,
                            Location::from_node(ctx.path.display().to_string(), &body, ctx.text),
                            "concrete PHP callable has an empty body",
                            "AST body contains no executable statements; callers may receive null",
                            Some(
                                "an intentional no-op hook or override; check its API contract"
                                    .to_string(),
                            ),
                        ));
                    }
                }
            }
            let mut cursor = node.walk();
            pending.extend(node.named_children(&mut cursor));
        }
        findings
    }
}
