// SPDX-License-Identifier: MPL-2.0

//! T1: intraprocedural dataflow over the Python CFG (ADR-0002).
//!
//! These rules depart from the `.scm` house pattern: a CFG needs structural
//! recursion, not pattern matching. They read [`RuleCtx::units`], which is
//! built once per file and shared, and they never see an opaque function —
//! [`FunctionUnit::Opaque`] carries no CFG to analyse.
//!
//! [`RuleCtx::units`]: pons_core::engine::RuleCtx::units
//! [`FunctionUnit::Opaque`]: pons_core::cfg::FunctionUnit::Opaque

pub mod dead_store;
pub mod read_before_init;

#[cfg(test)]
pub(crate) mod test_util {
    use pons_core::engine::{Rule, RuleCtx};
    use pons_core::finding::RawFinding;
    use pons_core::lang::Lang;
    use pons_core::parse;
    use pons_core::source::SourceFile;
    use std::path::PathBuf;

    pub fn findings_for(rule: &dyn Rule, text: &str) -> Vec<RawFinding> {
        let source = SourceFile {
            path: PathBuf::from("test.py"),
            lang: Lang::Python,
            text: text.to_string(),
        };
        let tree = parse::parse(&source).unwrap();
        let ctx = RuleCtx::new(&source.path, source.lang, &source.text, &tree);
        rule.check(&ctx)
    }
}
