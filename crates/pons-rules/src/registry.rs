// SPDX-License-Identifier: MPL-2.0

use pons_core::engine::Rule;

/// Builds the full set of registered rules. Empty until the T0 catalogue
/// (M2) lands — `pons-core::Engine` must run correctly with zero rules.
pub struct RuleRegistry;

impl RuleRegistry {
    pub fn all() -> Vec<Box<dyn Rule>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_empty_before_the_t0_catalogue_lands() {
        assert!(RuleRegistry::all().is_empty());
    }
}
