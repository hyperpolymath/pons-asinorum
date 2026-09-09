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
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceClass {
    Heuristic,
    Dataflow,
    Protocol,
    Speculative,
}

impl EvidenceClass {
    pub fn tier(self) -> Tier {
        match self {
            Self::Heuristic => Tier::T0,
            Self::Dataflow => Tier::T1,
            Self::Protocol => Tier::T2,
            Self::Speculative => Tier::T3,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Heuristic => "HEURISTIC",
            Self::Dataflow => "DATAFLOW",
            Self::Protocol => "PROTOCOL",
            Self::Speculative => "SPECULATIVE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
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
    pub line_start: usize,
    pub col_start: usize,
    pub line_end: usize,
    pub col_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn new(meta: &RuleMeta, location: Location, evidence_note: impl Into<String>) -> Self {
        Self {
            rule_id: meta.id.clone(),
            tier: meta.evidence.tier(),
            evidence: meta.evidence,
            severity: meta.severity,
            location,
            message: meta.message.clone(),
            evidence_note: evidence_note.into(),
            counter_condition: Some(meta.counter_condition.clone()),
        }
    }
    pub fn display_message(&self) -> String {
        if self.evidence == EvidenceClass::Speculative {
            format!("{} (heuristic — not a verdict)", self.message)
        } else {
            self.message.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleMeta {
    pub id: String,
    pub evidence: EvidenceClass,
    pub severity: Severity,
    pub languages: Vec<String>,
    pub message: String,
    pub counter_condition: String,
}
