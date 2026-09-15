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

impl Location {
    /// Builds a [`Location`] from a tree-sitter node, converting its
    /// 0-based `Point`s to the 1-based line/column convention every rule
    /// must use — this is the one place that conversion happens, so no
    /// rule re-derives it by hand.
    pub fn from_node(file: impl Into<String>, node: &tree_sitter::Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        Self {
            file: file.into(),
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            line_start: start.row + 1,
            col_start: start.column + 1,
            line_end: end.row + 1,
            col_end: end.column + 1,
        }
    }
}

/// All fields are private: `tier` and `evidence` must never disagree, and
/// the only way to guarantee that is to deny direct construction and direct
/// mutation alike — see ADR-0001. `new()` is the sole constructor; `Finding`
/// is otherwise read-only via the accessors below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "FindingWire")]
pub struct Finding {
    rule_id: String,
    tier: Tier,
    evidence: EvidenceClass,
    severity: Severity,
    location: Location,
    message: String,
    evidence_note: String,
    counter_condition: Option<String>,
}

/// Deserialization target. Deliberately has no `evidence` field: an
/// incoming `evidence` key (e.g. from a previously-serialized `Finding`) is
/// dropped by serde's default unknown-field handling rather than trusted —
/// `Finding::from` below re-derives it from `tier` unconditionally.
#[derive(Deserialize)]
struct FindingWire {
    rule_id: String,
    tier: Tier,
    severity: Severity,
    location: Location,
    message: String,
    evidence_note: String,
    counter_condition: Option<String>,
}

impl From<FindingWire> for Finding {
    fn from(w: FindingWire) -> Self {
        Finding::new(
            w.rule_id,
            w.tier,
            w.severity,
            w.location,
            w.message,
            w.evidence_note,
            w.counter_condition,
        )
    }
}

impl Finding {
    /// The only way to construct a `Finding`, and deliberately
    /// crate-private: rules (in `pons-rules`, a different crate) build a
    /// [`RawFinding`] instead and never see this constructor, so a rule can
    /// never set its own `rule_id` — only [`Engine::scan`][crate::engine::Engine::scan]
    /// can, via [`RawFinding::into_finding`], using `Rule::id()` as the sole
    /// source of truth. `evidence` is likewise derived from `tier`, never
    /// taken as a parameter — see ADR-0001.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
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

    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    pub fn tier(&self) -> Tier {
        self.tier
    }

    pub fn evidence(&self) -> EvidenceClass {
        self.evidence
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn evidence_note(&self) -> &str {
        &self.evidence_note
    }

    pub fn counter_condition(&self) -> Option<&str> {
        self.counter_condition.as_deref()
    }
}

/// What a [`Rule`][crate::engine::Rule] builds. Has no `rule_id` field at
/// all — a rule cannot report an id other than its own, because it has no
/// way to report one; only [`Engine::scan`][crate::engine::Engine::scan]
/// assigns `rule_id`, from `Rule::id()`, when it turns each `RawFinding`
/// into a [`Finding`]. This makes a rule/id mismatch a type error rather
/// than a typo waiting in one of the T0 catalogue's 8 rule files.
pub struct RawFinding {
    tier: Tier,
    severity: Severity,
    location: Location,
    message: String,
    evidence_note: String,
    counter_condition: Option<String>,
}

impl RawFinding {
    pub fn new(
        tier: Tier,
        severity: Severity,
        location: Location,
        message: impl Into<String>,
        evidence_note: impl Into<String>,
        counter_condition: Option<String>,
    ) -> Self {
        Self {
            tier,
            severity,
            location,
            message: message.into(),
            evidence_note: evidence_note.into(),
            counter_condition,
        }
    }

    pub(crate) fn into_finding(self, rule_id: impl Into<String>) -> Finding {
        Finding::new(
            rule_id,
            self.tier,
            self.severity,
            self.location,
            self.message,
            self.evidence_note,
            self.counter_condition,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_node_converts_zero_based_point_to_one_based_location() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let src = "x = 1\ny = a / 0\n";
        let tree = parser.parse(src, None).unwrap();

        // The division is on source line 2 (0-based row 1); tree-sitter's
        // `Node::start_position` reports row 1 for it.
        let module = tree.root_node();
        let second_stmt = module.child(1).unwrap();
        let assignment = second_stmt.child(0).unwrap();
        let rhs = assignment.child_by_field_name("right").unwrap();

        let loc = Location::from_node("y.py", &rhs);
        assert_eq!(loc.line_start, 2);
    }

    fn loc() -> Location {
        Location {
            file: "x.py".into(),
            byte_start: 0,
            byte_end: 1,
            line_start: 1,
            col_start: 1,
            line_end: 1,
            col_end: 2,
        }
    }

    #[test]
    fn evidence_class_is_derived_from_tier() {
        let f = Finding::new("r", Tier::T3, Severity::Warn, loc(), "msg", "note", None);
        assert_eq!(f.evidence(), EvidenceClass::Speculative);
    }

    #[test]
    fn deserializing_never_trusts_a_wire_evidence_value() {
        // FindingWire carries no `evidence` field at all — this constructs
        // the wire type directly (both live in this module) to prove the
        // `From` impl re-derives `evidence` from `tier` unconditionally,
        // with no path for a stale or tampered wire value to survive.
        let wire = FindingWire {
            rule_id: "r".into(),
            tier: Tier::T0,
            severity: Severity::Warn,
            location: loc(),
            message: "msg".into(),
            evidence_note: "note".into(),
            counter_condition: None,
        };
        let f = Finding::from(wire);
        assert_eq!(f.evidence(), EvidenceClass::Heuristic);
    }
}
