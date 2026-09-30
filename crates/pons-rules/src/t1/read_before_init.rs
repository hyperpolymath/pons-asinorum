// SPDX-License-Identifier: MPL-2.0

use pons_core::cfg::FunctionUnit;
use pons_core::dataflow;
use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{RawFinding, Severity, Tier};
use pons_core::lang::Lang;

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

    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let mut findings = Vec::new();
        for unit in ctx.units() {
            let FunctionUnit::Analysable(cfg) = unit else {
                continue;
            };
            for event in dataflow::unbound_reads(cfg) {
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
    fn a_function_calling_exec_produces_no_finding_and_its_twin_does() {
        assert!(check("def f():\n    exec('y = 1')\n    return y\n    y = 2\n").is_empty());
        assert_eq!(
            check("def f():\n    run('y = 1')\n    return y\n    y = 2\n").len(),
            1
        );
    }
}
