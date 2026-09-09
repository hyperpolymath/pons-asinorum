// SPDX-License-Identifier: MPL-2.0
//! Conditional resource arithmetic for simple literal-bound Python loops.
use anyhow::{Context, Result};
use globset::Glob;
use pons_core::{EvidenceClass, Finding, Lang, Parsed, RuleMeta, Source, config::Budget};
use tree_sitter::Node;

pub fn metadata() -> RuleMeta {
    let mut m = crate::meta(
        "resource-budget-exceeded",
        "A bounded loop needs more consumable resources than allocated",
        "Conditional on completing this loop, Python's built-in range semantics, and the supplied consumption contract. Early exits, branches, aliasing, replenishment and indirect calls are not analysed.",
        vec!["python".into()],
    );
    m.evidence = EvidenceClass::Protocol;
    m
}
pub fn check(source: &Source, parsed: &Parsed, budgets: &[Budget]) -> Result<Vec<Finding>> {
    if parsed.lang != Lang::Python {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for budget in budgets {
        if !Glob::new(&budget.path)?
            .compile_matcher()
            .is_match(&source.relative)
        {
            continue;
        }
        let mut stack = vec![parsed.tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.kind() == "for_statement"
                && let Some(calls) = loop_calls(node, source, &budget.consume)
            {
                let required = calls
                    .checked_mul(u128::from(budget.units))
                    .context("resource requirement exceeds arithmetic limit")?;
                if required > u128::from(budget.available) {
                    let mut finding = Finding::new(
                        &metadata(),
                        source.location(node.start_byte(), node.end_byte()),
                        format!(
                            "{} consuming calls × {} {} per call, compared with the explicit allocation of {}. Contract call: {}.",
                            calls, budget.units, budget.resource, budget.available, budget.consume
                        ),
                    );
                    finding.message = format!(
                        "If this loop completes, it needs {required} {}. You allocated {}, so you are {} short.",
                        budget.resource,
                        budget.available,
                        required - u128::from(budget.available)
                    );
                    out.push(finding);
                }
                // This complete outer loop already includes its nested loops.
                continue;
            }
            let mut c = node.walk();
            stack.extend(node.named_children(&mut c));
        }
    }
    Ok(out)
}
fn loop_calls(node: Node<'_>, source: &Source, consume: &str) -> Option<u128> {
    let iterable = node.child_by_field_name("right")?;
    if iterable.kind() != "call"
        || text(iterable.child_by_field_name("function")?, source) != "range"
    {
        return None;
    }
    let args = iterable.child_by_field_name("arguments")?;
    if args.named_child_count() != 1 {
        return None;
    }
    let bound = args.named_child(0)?;
    if bound.kind() != "integer" {
        return None;
    }
    let count = text(bound, source).replace('_', "").parse::<u64>().ok()?;
    // Loop-else is a separate operation; reject rather than silently omit it.
    if node.child_by_field_name("alternative").is_some() {
        return None;
    }
    let body = node.child_by_field_name("body")?;
    let mut total = 0u128;
    let mut cursor = body.walk();
    for statement in body.named_children(&mut cursor) {
        let calls = match statement.kind() {
            "comment" | "pass_statement" => 0,
            "for_statement" => loop_calls(statement, source, consume)?,
            "expression_statement" => {
                let call = statement.named_child(0)?;
                if call.kind() != "call"
                    || text(call.child_by_field_name("function")?, source) != consume
                {
                    return None;
                }
                let args = call.child_by_field_name("arguments")?;
                let mut c = args.walk();
                if args
                    .named_children(&mut c)
                    .any(|a| !matches!(a.kind(), "identifier" | "integer" | "string"))
                {
                    return None;
                }
                1
            }
            _ => return None,
        };
        total = total.checked_add(calls)?;
    }
    total.checked_mul(u128::from(count))
}
fn text<'a>(node: Node<'_>, source: &'a Source) -> &'a str {
    &source.text[node.byte_range()]
}
