// SPDX-License-Identifier: MPL-2.0
//! Shared engine primitives. Rules and external tools do not live in this crate.
pub mod config;
pub mod finding;
pub mod lang;
pub mod report;
pub mod source;

pub use finding::{EvidenceClass, Finding, Location, RuleMeta, Severity, Tier};
pub use lang::{Lang, Parsed};
pub use source::Source;

/// A compiled-in language plug-in implements a rule using the parsed source.
pub trait Rule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    fn check(&self, source: &Source, parsed: &Parsed) -> Vec<Finding>;
}
