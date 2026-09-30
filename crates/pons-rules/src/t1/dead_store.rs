// SPDX-License-Identifier: MPL-2.0

use pons_core::cfg::FunctionUnit;
use pons_core::dataflow;
use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{RawFinding, Severity, Tier};
use pons_core::lang::Lang;

const LANGUAGES: &[Lang] = &[Lang::Python];

const EFFECT_COUNTER_CONDITION: &str = "the binding is dead but its right-hand side may have \
     side effects worth keeping; drop the binding, not the call.";

/// Rule 9: a store to a local that no path reads before it is redefined or the
/// function ends. Counter-conditions in ADR-0002's order: a `_`-prefixed
/// target is a declared throwaway and is not reported; a right-hand side with
/// a call, `await`, `yield` or walrus is reported at `INFO`; anything else is
/// `WARN`. Never `ERROR` — a program can be correct with a dead store.
pub struct DeadStore;

impl DeadStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DeadStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DeadStore {
    fn id(&self) -> &'static str {
        "dead-store"
    }

    fn description(&self) -> &'static str {
        "a value stored and then overwritten before any read"
    }

    fn languages(&self) -> &'static [Lang] {
        LANGUAGES
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let mut findings = Vec::new();
        for unit in ctx.units() {
            let FunctionUnit::Analysable(cfg) = unit else {
                continue;
            };
            for (event, report) in dataflow::dead_stores(cfg) {
                let name = &cfg.locals()[event.local];
                if name.starts_with('_') {
                    continue;
                }
                let (severity, counter_condition) = if report.rhs_has_effect {
                    (Severity::Info, Some(EFFECT_COUNTER_CONDITION.to_string()))
                } else {
                    (Severity::Warn, None)
                };
                findings.push(RawFinding::new(
                    Tier::T1,
                    severity,
                    event.loc.clone(),
                    "value stored but never read",
                    format!(
                        "`{name}` is assigned here, and every path from this point in `{}` \
                         reassigns it or returns before reading it",
                        cfg.name()
                    ),
                    counter_condition,
                ));
            }
        }
        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::t1::test_util::findings_for;

    fn check(text: &str) -> Vec<RawFinding> {
        findings_for(&DeadStore::new(), text)
    }

    #[test]
    fn a_plain_dead_store_is_warn_with_no_counter_condition() {
        let f = check("def f():\n    x = 1\n    x = 2\n    return x\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity(), Severity::Warn);
        assert_eq!(f[0].tier(), Tier::T1);
        assert_eq!(f[0].counter_condition(), None);
        assert_eq!(f[0].location().line_start, 2);
    }

    #[test]
    fn a_dead_store_of_a_call_is_info_with_the_adr_counter_condition() {
        let f = check("def f():\n    x = g()\n    x = 2\n    return x\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity(), Severity::Info);
        assert_eq!(f[0].counter_condition(), Some(EFFECT_COUNTER_CONDITION));
    }

    #[test]
    fn an_underscore_target_is_suppressed_and_its_twin_is_not() {
        assert!(check("def f():\n    _ = g()\n    _tmp = 1\n    return 0\n").is_empty());
        assert_eq!(check("def f():\n    tmp = 1\n    return 0\n").len(), 1);
    }

    #[test]
    fn a_function_calling_exec_produces_no_finding_and_its_twin_does() {
        // PLAN.adoc M4 exit gate.
        assert!(
            check("def f():\n    x = 1\n    exec('x = 2')\n    x = 3\n    return x\n").is_empty()
        );
        assert_eq!(
            check("def f():\n    x = 1\n    run('x = 2')\n    x = 3\n    return x\n").len(),
            1
        );
    }

    #[test]
    fn module_level_code_is_not_a_unit() {
        assert!(check("x = 1\nx = 2\n").is_empty());
    }
}
