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

use crate::engine::{ScanReport, SkippedFile};
use crate::finding::{EvidenceClass, Finding};

/// Version of the envelope itself, not of the tool.
///
/// Bumped 0.1.0 -> 0.2.0 when `scanned.skipped` was added (#29). The field is
/// purely additive and always present, so a 0.1.0 consumer would not have
/// broken — the bump is deliberate discipline, taken before v0.1.0 ships
/// rather than after: an envelope shape change is version-visible from the
/// start. `envelope_shape_is_pinned_to_the_schema_version` below is what
/// stops the next change skipping this line silently.
///
/// Not to be confused with the `schema_version` in a protocol TOML
/// (ADR-0003), which versions a different file entirely.
pub const SCHEMA_VERSION: &str = "0.2.0";

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
    /// What the scan did not look at, and why: a subdirectory that
    /// could not be enumerated, a file that could not be read, a file
    /// that could not be parsed. Empty array when nothing was skipped,
    /// never omitted — a consumer must be able to distinguish "clean"
    /// from "the key is missing so I cannot tell".
    skipped: &'a [SkippedFile],
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
            skipped: &report.skipped,
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
    fn skipped_is_an_empty_array_when_nothing_was_skipped_never_omitted() {
        // A consumer must be able to tell "this scan skipped nothing" from
        // "this envelope predates the field, so I cannot tell". Serde would
        // happily omit an empty slice under `skip_serializing_if`; Appendix G
        // says it must not, so this is the control that keeps it out.
        let mut report = fixture::report();
        report.skipped.clear();
        let out = render(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

        let scanned = parsed["scanned"].as_object().unwrap();
        assert!(
            scanned.contains_key("skipped"),
            "scanned.skipped vanished when the scan was clean"
        );
        assert_eq!(parsed["scanned"]["skipped"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_skipped_entry_carries_the_path_and_the_reason() {
        let out = render(&fixture::report()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let first = &parsed["scanned"]["skipped"][0];
        assert_eq!(first["path"], "app/broken.py");
        assert!(
            first["reason"]
                .as_str()
                .unwrap()
                .starts_with("could not read"),
            "a skip without a reason is not actionable: {first:?}"
        );
    }

    #[test]
    fn every_evidence_class_is_counted_even_at_zero() {
        let empty = ByEvidence::tally(&[]);
        let json = serde_json::to_string(&empty).unwrap();
        for class in ["HEURISTIC", "DATAFLOW", "PROTOCOL", "SPECULATIVE"] {
            assert!(json.contains(class), "{class} vanished from an empty tally");
        }
    }

    /// The envelope's shape and its `schema_version` are one fact, so they are
    /// asserted as one fact. Adding, removing or renaming any key below fails
    /// this test, and the only way to go green again is to arrive at this line
    /// — whose failure message names `SCHEMA_VERSION` — and decide deliberately
    /// whether the change warrants a bump.
    ///
    /// Written because the 0.1.0 -> 0.2.0 bump (#29 added `scanned.skipped`)
    /// was very nearly shipped without one: the golden file absorbed the new
    /// key silently and every other test in this module stayed green. A version
    /// constant nothing checks is a comment, not a contract.
    #[test]
    fn envelope_shape_is_pinned_to_the_schema_version() {
        // (path, the complete key set at that path). "" is the root object.
        const SHAPE: &[(&str, &[&str])] = &[
            (
                "",
                &["counts", "findings", "scanned", "schema_version", "tool"],
            ),
            ("tool", &["name", "version"]),
            ("scanned", &["files", "languages", "root", "skipped"]),
            ("counts", &["by_evidence", "total"]),
            (
                "counts.by_evidence",
                &["DATAFLOW", "HEURISTIC", "PROTOCOL", "SPECULATIVE"],
            ),
        ];

        let out = render(&fixture::report()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

        for (path, expected) in SHAPE {
            let node = if path.is_empty() {
                &parsed
            } else {
                path.split('.').fold(&parsed, |v, seg| &v[seg])
            };
            let actual: Vec<&str> = node
                .as_object()
                .unwrap_or_else(|| panic!("{path:?} is not an object in the envelope"))
                .keys()
                .map(String::as_str)
                .collect();
            // serde_json without `preserve_order` hands back a sorted map, and
            // SHAPE is written sorted to match; declaration order is a separate
            // contract, asserted by `field_order_follows_appendix_g_not_the_alphabet`.
            assert_eq!(
                actual, *expected,
                "the key set at {path:?} no longer matches the shape pinned for \
                 schema_version {SCHEMA_VERSION}. Decide whether this is a \
                 breaking or additive envelope change, bump SCHEMA_VERSION in \
                 src/report/json.rs accordingly, update Appendix G in \
                 docs/PLAN.adoc, then update SHAPE here in the same commit."
            );
        }

        // The constant is the source of truth; the golden must follow it. These
        // drift apart silently because `PONS_BLESS=1` rewrites the golden from
        // whatever the code currently emits.
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/golden/report.json")).unwrap();
        assert_eq!(
            golden["schema_version"], SCHEMA_VERSION,
            "the golden envelope declares a different schema_version than the \
             constant — one of the two was bumped without the other"
        );
    }
}
