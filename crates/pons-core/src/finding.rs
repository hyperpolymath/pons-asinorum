// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    T0,
    T1,
    T2,
    T3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceClass {
    Heuristic,
    Dataflow,
    Protocol,
    Speculative,
}

impl From<Tier> for EvidenceClass {
    fn from(tier: Tier) -> Self {
        match tier {
            Tier::T0 => EvidenceClass::Heuristic,
            Tier::T1 => EvidenceClass::Dataflow,
            Tier::T2 => EvidenceClass::Protocol,
            Tier::T3 => EvidenceClass::Speculative,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub byte_start: usize,
    pub byte_end: usize,
    /// 1-based; tree-sitter's `Point` is 0-based, so callers must add 1.
    pub line_start: usize,
    pub col_start: usize,
    pub line_end: usize,
    pub col_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub tier: Tier,
    pub evidence: EvidenceClass,
    pub severity: Severity,
    pub location: Location,
    pub message: String,
    pub evidence_note: String,
    pub counter_condition: Option<String>,
}

impl Finding {
    /// The only way to construct a `Finding`. `evidence` is derived from
    /// `tier`, never taken as a parameter — see ADR-0001.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rule_id: impl Into<String>,
        tier: Tier,
        severity: Severity,
        location: Location,
        message: impl Into<String>,
        evidence_note: impl Into<String>,
        counter_condition: Option<String>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            tier,
            evidence: EvidenceClass::from(tier),
            severity,
            location,
            message: message.into(),
            evidence_note: evidence_note.into(),
            counter_condition,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_class_is_derived_from_tier() {
        let loc = Location {
            file: "x.py".into(),
            byte_start: 0,
            byte_end: 1,
            line_start: 1,
            col_start: 1,
            line_end: 1,
            col_end: 2,
        };
        let f = Finding::new("r", Tier::T3, Severity::Warn, loc, "msg", "note", None);
        assert_eq!(f.evidence, EvidenceClass::Speculative);
    }
}
