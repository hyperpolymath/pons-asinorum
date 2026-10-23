// SPDX-License-Identifier: MPL-2.0

use std::collections::{BTreeMap, BTreeSet};

use pons_core::cfg::FunctionUnit;
use pons_core::dataflow;
use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{RawFinding, Severity, Tier};
use pons_core::lang::Lang;
use tree_sitter::Node;

const LANGUAGES: &[Lang] = &[Lang::Python];

/// Rule 10: a read of a local that some path reaches before the name is
/// bound — Python raises `UnboundLocalError`. `WARN` uniformly in v0.1.0:
/// raising the every-path case to `ERROR` would need a must-analysis, which
/// ADR-0002 declines. Python has no default initialisation, so the rule has
/// no counter-condition. Opacity and scope are handled by the OPAQUE hatch and
/// the locals pre-pass; the analysis is also path-insensitive, so correlated
/// guards and loops over non-empty literals yield infeasible-path false
/// positives that ADR-0002 does not yet name (#47).
pub struct ReadBeforeInit;

impl ReadBeforeInit {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReadBeforeInit {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ReadBeforeInit {
    fn id(&self) -> &'static str {
        "read-before-init"
    }

    fn description(&self) -> &'static str {
        "a local read on a path where it may not yet be bound"
    }

    fn languages(&self) -> &'static [Lang] {
        LANGUAGES
    }

    /// Reports possible unbound local reads in analysable functions as `WARN`,
    /// without counter-conditions. Opaque functions produce no findings.
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let mut findings = Vec::new();
        // A bounded, explicitly documented pruning pass (#47 class A). It
        // suppresses only a read inside a later branch guarded by the SAME
        // pure `X is [not] None` condition that guarded an earlier branch
        // which bound the name. Any assignment to `X` in between disables the
        // pruning. This is not general path sensitivity or a soundness proof.
        let correlated = correlated_guard_reads(ctx);
        for unit in ctx.units() {
            let FunctionUnit::Analysable(cfg) = unit else {
                continue;
            };
            for event in dataflow::unbound_reads(cfg) {
                if correlated.contains(&event.loc.byte_start) {
                    continue;
                }
                let name = &cfg.locals()[event.local];
                findings.push(RawFinding::new(
                    Tier::T1,
                    Severity::Warn,
                    event.loc.clone(),
                    "local may be read before it is assigned",
                    format!(
                        "on at least one path through `{}`, `{name}` is read here before it \
                         is assigned — Python raises `UnboundLocalError`",
                        cfg.name()
                    ),
                    None,
                ));
            }
        }
        findings
    }
}


/// A pure, attribute/identifier `is [not] None` guard. Call-bearing or
/// otherwise impure conditions deliberately do not qualify.
fn guard_key(cond: Node<'_>, text: &str) -> Option<(String, String)> {
    let raw = &text[cond.byte_range()];
    let (base, polarity) = if let Some(base) = raw.strip_suffix(" is not None") {
        (base, "not-none")
    } else if let Some(base) = raw.strip_suffix(" is None") {
        (base, "none")
    } else {
        return None;
    };
    let base = base.trim();
    if base.is_empty()
        || !base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
    {
        return None;
    }
    Some((base.to_string(), polarity.to_string()))
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn assigned_names(node: Node<'_>, text: &str, out: &mut BTreeSet<String>) {
    if node.kind() == "assignment" {
        if let Some(left) = node.child_by_field_name("left") {
            if left.kind() == "identifier" {
                out.insert(text[left.byte_range()].to_string());
            }
        }
    }
    for child in children(node) {
        assigned_names(child, text, out);
    }
}

fn identifier_offsets(node: Node<'_>, text: &str, names: &BTreeSet<String>, out: &mut BTreeSet<usize>) {
    if node.kind() == "identifier" && names.contains(&text[node.byte_range()]) {
        out.insert(node.start_byte());
    }
    for child in children(node) {
        identifier_offsets(child, text, names, out);
    }
}

/// Conservative invalidation: any assignment/delete whose target is the guard
/// base itself, or an attribute of it, disables the correlation.
fn base_mutated_between(node: Node<'_>, start: usize, end: usize, base: &str, text: &str) -> bool {
    if node.start_byte() >= end {
        return false;
    }
    if node.start_byte() >= start
        && matches!(node.kind(), "assignment" | "augmented_assignment" | "delete_statement")
    {
        let left = node.child_by_field_name("left");
        if let Some(left) = left {
            let target = &text[left.byte_range()];
            if target == base || target.starts_with(&format!("{base}.")) {
                return true;
            }
        }
    }
    children(node)
        .into_iter()
        .any(|child| base_mutated_between(child, start, end, base, text))
}

#[derive(Clone)]
struct GuardIf<'t> {
    key: String,
    base: String,
    start: usize,
    end: usize,
    consequence: Node<'t>,
}

/// Byte offsets of reads that the bounded correlated-guard pass may prune.
fn correlated_guard_reads(ctx: &RuleCtx<'_>) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    visit_functions(ctx.tree.root_node(), ctx.text, &mut out);
    out
}

fn visit_functions(node: Node<'_>, text: &str, out: &mut BTreeSet<usize>) {
    if node.kind() == "function_definition" {
        prune_function(node, text, out);
        // Nested functions are separate analysis units; recurse to find them.
        for child in children(node) {
            visit_functions(child, text, out);
        }
        return;
    }
    for child in children(node) {
        visit_functions(child, text, out);
    }
}

fn prune_function(func: Node<'_>, text: &str, out: &mut BTreeSet<usize>) {
    let mut guards = Vec::new();
    let mut stack = vec![func];
    while let Some(node) = stack.pop() {
        if node.kind() == "function_definition" && node.start_byte() != func.start_byte() {
            continue;
        }
        if node.kind() == "if_statement" {
            let Some(cond) = node.child_by_field_name("condition") else {
                continue;
            };
            if let Some((base, polarity)) = guard_key(cond, text) {
                let key = format!("{base}::{polarity}");
                if let Some(consequence) = node.child_by_field_name("consequence") {
                    guards.push(GuardIf {
                        key,
                        base: base.clone(),
                        start: node.start_byte(),
                        end: node.end_byte(),
                        consequence,
                    });
                }
            }
        }
        stack.extend(children(node));
    }

    let mut first: BTreeMap<String, GuardIf<'_>> = BTreeMap::new();
    for guard in guards {
        if let Some(earlier) = first.get(&guard.key).cloned() {
            if base_mutated_between(func, earlier.end, guard.start, &earlier.base, text) {
                continue;
            }
            let mut names = BTreeSet::new();
            assigned_names(earlier.consequence, text, &mut names);
            identifier_offsets(guard.consequence, text, &names, out);
        } else {
            first.insert(guard.key.clone(), guard);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::t1::test_util::findings_for;

    fn check(text: &str) -> Vec<RawFinding> {
        findings_for(&ReadBeforeInit::new(), text)
    }

    #[test]
    fn a_read_after_an_else_less_if_is_warn_and_the_two_armed_twin_is_clean() {
        let f = check("def f(c):\n    if c:\n        y = 1\n    return y\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity(), Severity::Warn);
        assert_eq!(f[0].tier(), Tier::T1);
        assert_eq!(f[0].location().line_start, 4);
        assert!(check(
            "def f(c):\n    if c:\n        y = 1\n    else:\n        y = 2\n    return y\n"
        )
        .is_empty());
    }

    #[test]
    fn augmented_assignment_with_no_binding_is_a_genuine_read() {
        assert_eq!(check("def f():\n    n += 1\n    return n\n").len(), 1);
    }

    #[test]
    fn a_global_name_is_never_checked() {
        assert!(check("def f():\n    global n\n    n += 1\n    return n\n").is_empty());
    }

    #[test]
    fn a_correlated_guard_read_is_pruned_but_a_mismatched_guard_is_not() {
        let correlated = "def pack(self):\n    if self.name is not None:\n        name = 1\n    if self.name is not None:\n        MonsterAddName(name)\n";
        assert!(check(correlated).is_empty(), "identical pure guard was not pruned");

        let mismatched = "def pack(self):\n    if self.name is not None:\n        name = 1\n    if self.other is not None:\n        MonsterAddName(name)\n";
        assert_eq!(check(mismatched).len(), 1, "a different guard is a real may-unbound path");
    }

    #[test]
    fn a_mutation_between_identical_guards_disables_pruning() {
        let src = "def pack(self):\n    if self.name is not None:\n        name = 1\n    self = other\n    if self.name is not None:\n        MonsterAddName(name)\n";
        assert_eq!(check(src).len(), 1, "the guard base was rebound between branches");
    }

    #[test]
    fn a_function_calling_exec_produces_no_finding_and_its_twin_does() {
        assert!(check("def f():\n    exec('y = 1')\n    return y\n    y = 2\n").is_empty());
        assert_eq!(
            check("def f():\n    run('y = 1')\n    return y\n    y = 2\n").len(),
            1
        );
    }
}
