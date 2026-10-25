// SPDX-License-Identifier: MPL-2.0

pub mod php;
pub mod registry;
pub mod t0;
pub mod t1;

/// The catalogue is generated from the same registry the CLI runs, so a
/// registered rule cannot be omitted and an unregistered rule cannot be
/// advertised. Until the M7 generator is complete this is the authoritative
/// text source; `docs/catalogue.adoc` is a reviewed snapshot plus a drift test.
pub fn catalogue() -> String {
    use std::fmt::Write as _;

    let mut out = String::from(
        "= Registered Pons rules\n\
         :toc:\n\n\
         Generated from `pons_rules::registry::RuleRegistry::all()`. Counts and order are asserted by a test.\n\n",
    );
    for (index, rule) in registry::RuleRegistry::all().iter().enumerate() {
        let _ = writeln!(out, "== {}. `{}`", index + 1, rule.id());
        let _ = writeln!(out, "{}.\n", rule.description());
        let languages = rule
            .languages()
            .iter()
            .map(|l| l.name())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "Languages: {languages}.\n");
    }
    out.trim_end().to_string() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_lists_every_registered_rule_in_registry_order() {
        let text = catalogue();
        let ids: Vec<&str> = registry::RuleRegistry::all()
            .iter()
            .map(|r| r.id())
            .collect();
        let mut cursor = 0;
        for id in &ids {
            let marker = format!("`{id}`");
            let found = text[cursor..].find(&marker).map(|i| cursor + i);
            assert!(found.is_some(), "{id} is missing from the catalogue");
            cursor = found.unwrap() + marker.len();
        }
        assert_eq!(text.matches("== ").count(), ids.len());
    }
}
