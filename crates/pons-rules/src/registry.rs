// SPDX-License-Identifier: MPL-2.0

use pons_core::engine::Rule;

use crate::t0::constant_condition::ConstantCondition;
use crate::t0::div_by_literal_zero::DivByLiteralZero;
use crate::t0::empty_effect_loop::EmptyEffectLoop;
use crate::t0::self_assignment::SelfAssignment;
use crate::t0::string_concat_in_loop::StringConcatInLoop;
use crate::t0::swallowed_error::SwallowedError;
use crate::t0::unreachable_after_jump::UnreachableAfterJump;
use crate::t0::while_true_no_break::WhileTrueNoBreak;

/// Builds the full set of registered rules — the single source of truth
/// both `pons-cli` and the falsifier gate (`tests/falsifier.rs`) read from,
/// so a rule that exists but was never registered here fails loudly rather
/// than silently never running.
pub struct RuleRegistry;

impl RuleRegistry {
    pub fn all() -> Vec<Box<dyn Rule>> {
        vec![
            Box::new(DivByLiteralZero::new()),
            Box::new(SwallowedError::new()),
            Box::new(SelfAssignment::new()),
            Box::new(ConstantCondition::new()),
            Box::new(WhileTrueNoBreak::new()),
            Box::new(EmptyEffectLoop::new()),
            Box::new(UnreachableAfterJump::new()),
            Box::new(StringConcatInLoop::new()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_registers_the_t0_catalogue_as_it_lands() {
        let ids: Vec<&str> = RuleRegistry::all().iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "div-by-literal-zero",
                "swallowed-error",
                "self-assignment",
                "constant-condition",
                "while-true-no-break",
                "empty-effect-loop",
                "unreachable-after-jump",
                "string-concat-in-loop"
            ]
        );
    }
}
