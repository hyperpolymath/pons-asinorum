// SPDX-License-Identifier: MPL-2.0
//! The machine-readable envelope from `docs/PLAN.adoc` Appendix G.
//!
//! Field order is part of the contract ("stable, golden-file tested"), which
//! rules out `serde_json::json!`: without the `preserve_order` feature it
//! sorts keys alphabetically, and Appendix G's order — `schema_version`,
//! `tool`, `scanned`, `counts`, `findings` — is not alphabetical. Derived
//! `Serialize` structs emit fields in declaration order, so the order is
//! fixed by the source itself.
//!
//! Note where a `SPECULATIVE` finding is demoted here. The human and SARIF
//! reporters append a text suffix; this one does not, and deliberately so.
//! Appendix G fixes the field set of a finding exactly, and the demotion it
//! specifies *is* the `evidence` field — which `Finding` re-derives from
//! `tier` on every deserialize and can never be overridden. Appending prose
//! to `message` would corrupt the raw message a machine consumer asked for
//! and duplicate a signal that is already structural.

use serde::Serialize;

use crate::engine::ScanReport;
use crate::finding::{EvidenceClass, Finding};

/// Version of the envelope itself, not of the tool.
pub const SCHEMA_VERSION: &str = "0.1.0";

#[derive(Serialize)]
struct Envelope<'a> {
    schema_version: &'static str,
    tool: Tool,
    scanned: Scanned<'a>,
    counts: Counts,
    findings: &'a [Finding],
}

#[derive(Serialize)]
struct Tool {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct Scanned<'a> {
    root: &'a str,
    files: usize,
    languages: Vec<&'static str>,
}

#[derive(Serialize)]
struct Counts {
    by_evidence: ByEvidence,
    total: usize,
}

/// A fixed four-field struct rather than a map: every evidence class must
/// appear even at zero, and in tier order. A map would emit only the classes
/// that happened to occur, so a consumer could not tell "no dataflow
/// findings" from "this tool does not do dataflow".
#[derive(Serialize)]
struct ByEvidence {
    #[serde(rename = "HEURISTIC")]
    heuristic: usize,
    #[serde(rename = "DATAFLOW")]
    dataflow: usize,
    #[serde(rename = "PROTOCOL")]
    protocol: usize,
    #[serde(rename = "SPECULATIVE")]
    speculative: usize,
}

impl ByEvidence {
    fn tally(findings: &[Finding]) -> Self {
        let mut c = ByEvidence {
            heuristic: 0,
            dataflow: 0,
            protocol: 0,
            speculative: 0,
        };
        for f in findings {
            match f.evidence() {
                EvidenceClass::Heuristic => c.heuristic += 1,
                EvidenceClass::Dataflow => c.dataflow += 1,
                EvidenceClass::Protocol => c.protocol += 1,
                EvidenceClass::Speculative => c.speculative += 1,
            }
        }
        c
    }
}

/// Render a [`ScanReport`] as the Appendix G envelope, with a trailing
/// newline so the output is a well-formed text stream.
pub fn render(report: &ScanReport) -> anyhow::Result<String> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        tool: Tool {
            name: "pons",
            version: env!("CARGO_PKG_VERSION"),
        },
        scanned: Scanned {
            root: &report.root,
            files: report.files_scanned,
            languages: report.languages.iter().map(|l| l.name()).collect(),
        },
        counts: Counts {
            by_evidence: ByEvidence::tally(&report.findings),
            total: report.findings.len(),
        },
        findings: &report.findings,
    };

    let mut out = serde_json::to_string_pretty(&envelope)?;
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{assert_golden, fixture};

    #[test]
    fn matches_the_appendix_g_golden_envelope() {
        let out = render(&fixture::report()).unwrap();
        assert_golden(
            "report.json",
            &out,
            include_str!("../../tests/golden/report.json"),
        );
    }

    #[test]
    fn field_order_follows_appendix_g_not_the_alphabet() {
        let out = render(&fixture::report()).unwrap();
        let order: Vec<usize> = [
            "schema_version",
            "\"tool\"",
            "\"scanned\"",
            "\"counts\"",
            "\"findings\"",
        ]
        .iter()
        .map(|k| out.find(k).unwrap_or_else(|| panic!("missing key {k}")))
        .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "envelope keys are out of Appendix G order — a map or json! macro \
             has crept in and sorted them alphabetically"
        );
    }

    #[test]
    fn a_speculative_finding_is_demoted_by_its_evidence_field() {
        let out = render(&fixture::report()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let spec = parsed["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule_id"] == "possible-superlinear")
            .expect("the fixture's T3 finding is missing");
        // The demotion in JSON is structural: the evidence class, re-derived
        // from tier and impossible to override.
        assert_eq!(spec["evidence"], "SPECULATIVE");
        assert_eq!(spec["tier"], "T3");
    }

    #[test]
    fn counter_condition_is_null_when_absent_never_omitted() {
        let out = render(&fixture::report()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let f = &parsed["findings"][0];
        assert!(
            f.as_object().unwrap().contains_key("counter_condition"),
            "Appendix G: counter_condition is null when absent, never omitted"
        );
        assert!(f["counter_condition"].is_null());
    }

    #[test]
    fn every_evidence_class_is_counted_even_at_zero() {
        let empty = ByEvidence::tally(&[]);
        let json = serde_json::to_string(&empty).unwrap();
        for class in ["HEURISTIC", "DATAFLOW", "PROTOCOL", "SPECULATIVE"] {
            assert!(json.contains(class), "{class} vanished from an empty tally");
        }
    }
}
