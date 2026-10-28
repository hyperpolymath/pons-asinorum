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
use crate::finding::{EvidenceClass, Finding, Location, Severity, Tier};

/// Version of the envelope itself, not of the tool.
///
/// Bumped 0.1.0 -> 0.2.0 when `scanned.skipped` was added (#29). The field is
/// purely additive and always present, so a 0.1.0 consumer would not have
/// broken — the bump is deliberate discipline, taken before v0.1.0 ships
/// rather than after: an envelope shape change is version-visible from the
/// start. `envelope_shape_is_pinned_to_the_schema_version` below is what
/// stops the next change skipping this line silently.
///
/// 0.4.0 -> 0.5.0 added the three keys an aggregating consumer needs and could
/// not derive: `findings[].semantic_fingerprint` (the cross-producer
/// corroboration key from [`crate::corroborate`]) and the two demoted-count
/// fields `counts.actionable` / `counts.demoted_speculative`. All three are
/// additive and appended, so a 0.4.0-shaped reading of a 0.5.0 document still
/// works; the bump is still taken, because a consumer must be able to tell
/// whether a report carries the corroboration key at all.
///
/// 0.4.0 does not disappear. It is **frozen**: `schemas/scan-0.4.0.json` is
/// byte-pinned by [`FROZEN_0_4_0_SCHEMA_BLAKE3`] and by `schemas/index.json`,
/// `validate-report` still accepts a 0.4.0 report against that schema, and
/// `a_report_pinned_at_the_frozen_0_4_0_version_still_validates` in
/// `pons-cli/tests/contracts.rs` proves it. A frozen
/// version is never edited and never re-issued; a change is a new file.
///
/// Not to be confused with the `schema_version` in a protocol TOML
/// (ADR-0003), which versions a different file entirely.
pub const SCHEMA_VERSION: &str = "0.5.0";

/// Envelope versions this build will still validate, and only validate.
///
/// Pinned consumers keep their old reports; refusing to read them would make
/// the freeze worthless. Each entry is a schema file on disk, byte-attested by
/// [`FROZEN_0_4_0_SCHEMA_BLAKE3`] and `schemas/index.json`.
pub const FROZEN_SCHEMA_VERSIONS: &[&str] = &["0.4.0"];

/// BLAKE3 of `schemas/scan-0.4.0.json`, as committed.
///
/// The freeze is a claim about bytes, so it is checked against bytes: editing
/// the frozen schema — even to "just add a key" — fails
/// `the_frozen_0_4_0_schema_is_still_the_file_this_build_was_tested_against`.
/// A 0.4.0 consumer pinned to this repository therefore reads a document that
/// cannot change underneath it.
pub const FROZEN_0_4_0_SCHEMA_BLAKE3: &str =
    "bdef6c68691ffa0681b56d86adc31450e8c625d7ce5884090efe6fbc8fc46562";

#[derive(Serialize)]
struct Envelope<'a> {
    schema_version: &'static str,
    tool: Tool,
    execution: Execution,
    scanned: Scanned<'a>,
    counts: Counts,
    findings: Vec<FindingView<'a>>,
    evidence_contract: crate::evidence::Contract<'a>,
}

#[derive(Serialize)]
struct Execution {
    status: &'static str,
    exit_code: Option<u8>,
}

#[derive(Serialize)]
struct Tool {
    name: &'static str,
    version: &'static str,
    source_commit: &'static str,
    source_dirty: &'static str,
}

#[derive(Serialize)]
struct Scanned<'a> {
    root: &'a str,
    coverage_status: &'static str,
    inventory: &'a [crate::coverage::InventoryEntry],
    coverage_failures: &'a [String],
    files: usize,
    suppressed_findings: usize,
    languages: Vec<&'static str>,
    transcoded: Vec<&'a str>,
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
    /// `total` minus the demoted count: the number an aggregating consumer adds
    /// up. Published rather than left to subtraction so the demotion is a
    /// number in the document, not a convention a consumer might miss.
    actionable: usize,
    /// Findings that are demoted precisely because they are guesses — the
    /// numeric half of the demotion the human and SARIF reporters apply
    /// visually. Equal to `by_evidence.SPECULATIVE`, and published twice on
    /// purpose: a consumer reading only the aggregate must see it.
    demoted_speculative: usize,
}

impl Counts {
    /// Count all findings by evidence class, including zero counts, and exclude
    /// speculative findings from the actionable total regardless of severity.
    fn tally(findings: &[Finding]) -> Self {
        let by_evidence = ByEvidence::tally(findings);
        let demoted = by_evidence.speculative;
        Counts {
            by_evidence,
            total: findings.len(),
            actionable: findings.len() - demoted,
            demoted_speculative: demoted,
        }
    }
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

/// One finding as the envelope carries it: the finding itself, plus the
/// corroboration key the verdict grid matches other producers against.
///
/// A view rather than a field on [`Finding`] because the key needs the scan
/// root, which no rule and no engine step knows — the reporter does. Field
/// order is the contract: the first eight fields are exactly 0.4.0's, in
/// 0.4.0's order, and `semantic_fingerprint` is appended, so the 0.5.0 finding
/// really is a 0.4.0 finding plus one key rather than a reshuffle.
#[derive(Serialize)]
struct FindingView<'a> {
    rule_id: &'a str,
    tier: Tier,
    evidence: EvidenceClass,
    severity: Severity,
    location: &'a Location,
    message: &'a str,
    evidence_note: &'a str,
    counter_condition: Option<&'a str>,
    semantic_fingerprint: String,
}

impl<'a> FindingView<'a> {
    /// Borrow a finding's fields and add its corroboration key relative to the
    /// scan `root`, using the finding's start line and evidence class.
    fn of(finding: &'a Finding, root: &str) -> Self {
        Self {
            rule_id: finding.rule_id(),
            tier: finding.tier(),
            evidence: finding.evidence(),
            severity: finding.severity(),
            location: finding.location(),
            message: finding.message(),
            evidence_note: finding.evidence_note(),
            counter_condition: finding.counter_condition(),
            semantic_fingerprint: crate::corroborate::semantic_fingerprint(finding, root),
        }
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
            source_commit: env!("PONS_SOURCE_COMMIT"),
            source_dirty: env!("PONS_SOURCE_DIRTY"),
        },
        execution: Execution {
            status: "completed",
            exit_code: report.exit_code,
        },
        scanned: Scanned {
            root: &report.root,
            coverage_status: report.coverage_status(),
            inventory: &report.inventory,
            coverage_failures: &report.coverage_failures,
            files: report.files_scanned,
            suppressed_findings: report.suppressed_findings,
            languages: report.languages.iter().map(|l| l.name()).collect(),
            transcoded: report.transcoded.iter().map(String::as_str).collect(),
            skipped: &report.skipped,
        },
        counts: Counts::tally(&report.findings),
        findings: report
            .findings
            .iter()
            .map(|f| FindingView::of(f, &report.root))
            .collect(),
        evidence_contract: crate::evidence::contract(report),
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
        let out = render(&fixture::report())
            .unwrap()
            .replace(env!("PONS_SOURCE_COMMIT"), "TEST-COMMIT");
        let out = out.replace(
            &format!("\"source_dirty\": \"{}\"", env!("PONS_SOURCE_DIRTY")),
            "\"source_dirty\": \"TEST-DIRTY\"",
        );
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
                &[
                    "counts",
                    "evidence_contract",
                    "execution",
                    "findings",
                    "scanned",
                    "schema_version",
                    "tool",
                ],
            ),
            (
                "tool",
                &["name", "source_commit", "source_dirty", "version"],
            ),
            (
                "scanned",
                &[
                    "coverage_failures",
                    "coverage_status",
                    "files",
                    "inventory",
                    "languages",
                    "root",
                    "skipped",
                    "suppressed_findings",
                    "transcoded",
                ],
            ),
            (
                "counts",
                &["actionable", "by_evidence", "demoted_speculative", "total"],
            ),
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

    /// 0.5.0 promises that a finding gained exactly one key and gained it last.
    /// A key set cannot show order, so the order is asserted on the rendered
    /// text against the *frozen 0.4.0 schema's own* required list — the frozen
    /// file is the specification of "what a 0.4.0 finding was", so this cannot
    /// drift into agreeing with a mistake in this module.
    #[test]
    fn a_0_5_0_finding_is_a_0_4_0_finding_plus_one_appended_key() {
        let frozen: serde_json::Value =
            serde_json::from_str(include_str!("../../../../schemas/scan-0.4.0.json")).unwrap();
        let four_oh: Vec<&str> = frozen["properties"]["findings"]["items"]["required"]
            .as_array()
            .expect("the frozen schema lists required finding keys")
            .iter()
            .map(|v| v.as_str().expect("required keys are strings"))
            .collect();
        assert_eq!(
            four_oh,
            vec![
                "rule_id",
                "tier",
                "evidence",
                "severity",
                "location",
                "message",
                "evidence_note",
                "counter_condition",
            ],
            "the 0.4.0 finding shape is the baseline this test is anchored to"
        );

        let out = render(&fixture::report()).unwrap();
        let tail = &out[out.find("\"findings\": [").expect("findings array")..];
        let positions: Vec<usize> = four_oh
            .iter()
            .map(|key| {
                tail.find(&format!("\"{key}\": "))
                    .unwrap_or_else(|| panic!("0.4.0 key {key} vanished from a finding"))
            })
            .collect();
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        assert_eq!(
            positions, sorted,
            "the 0.4.0 keys are no longer a prefix of the 0.5.0 finding, in order"
        );

        let fingerprint = tail
            .find("\"semantic_fingerprint\": ")
            .expect("0.5.0 findings carry the corroboration key");
        assert!(
            fingerprint > *positions.last().expect("non-empty baseline"),
            "semantic_fingerprint must be appended, not interleaved: a 0.4.0 \
             reader that stops at counter_condition must still have read a \
             complete finding"
        );
    }

    #[test]
    fn every_finding_carries_its_corroboration_key() {
        let out = render(&fixture::report()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        for finding in parsed["findings"].as_array().unwrap() {
            let key = finding["semantic_fingerprint"]
                .as_str()
                .unwrap_or_else(|| panic!("missing key on {finding}"));
            assert_eq!(key.len(), 64, "{key}");
            assert!(
                key.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{key} is not lowercase hex"
            );
        }
        // Tied to the implementation, not just to a shape: recompute the first
        // one from the fields the same document publishes.
        let first = &parsed["findings"][0];
        let class: EvidenceClass =
            serde_json::from_value(first["evidence"].clone()).expect("evidence class");
        let expected = crate::corroborate::fingerprint_of(
            "app/svc.py",
            first["location"]["line_start"].as_u64().unwrap() as usize,
            class,
        );
        assert_eq!(first["semantic_fingerprint"], expected);
    }

    /// The numeric half of the demotion. A guess is reported, never added up:
    /// `actionable` is what a verdict grid sums, and `total` still equals the
    /// array length so no consumer loses sight of a finding.
    #[test]
    fn the_counts_demote_speculative_numerically() {
        let out = render(&fixture::report()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let counts = &parsed["counts"];
        assert_eq!(counts["total"], 4);
        assert_eq!(counts["demoted_speculative"], 1);
        assert_eq!(counts["actionable"], 3);
        let actionable = counts["actionable"].as_u64().unwrap();
        let demoted = counts["demoted_speculative"].as_u64().unwrap();
        assert_eq!(
            actionable + demoted,
            counts["total"].as_u64().unwrap(),
            "actionable + demoted must account for every finding exactly once"
        );
        assert_eq!(
            counts["demoted_speculative"],
            counts["by_evidence"]["SPECULATIVE"]
        );
    }

    /// The freeze, checked against the bytes rather than asserted in prose.
    #[test]
    fn the_frozen_0_4_0_schema_is_still_the_file_this_build_was_tested_against() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/scan-0.4.0.json"),
        )
        .expect("the frozen 0.4.0 schema must be committed");
        let actual = blake3::hash(&bytes).to_hex().to_string();
        assert_eq!(
            actual, FROZEN_0_4_0_SCHEMA_BLAKE3,
            "schemas/scan-0.4.0.json changed. A frozen envelope version is never \
             edited: revert the edit, or publish a new version and add it to \
             FROZEN_SCHEMA_VERSIONS with its own schema file."
        );
    }

    /// A 0.4.0 report remains readable by this build's validator, and the
    /// frozen schema is genuinely closed — so this is not a test that passes
    /// against any JSON at all.
    #[test]
    fn a_frozen_0_4_0_report_still_satisfies_the_frozen_schema() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../../schemas/scan-0.4.0.json")).unwrap();
        let validator = jsonschema::validator_for(&schema).expect("the frozen schema compiles");

        // Both samples carry the golden tests' `TEST-DIRTY` placeholder, which
        // is deliberately not one of the three wire values (`true`, `false`,
        // `unknown`) — an artefact of how the goldens are made, not of the
        // envelope. Normalising that one field keeps this test about the shape;
        // the assertion below proves the placeholder really was there, so the
        // normalisation cannot be hiding an invalid document.
        fn with_unknown_dirty(mut v: serde_json::Value) -> serde_json::Value {
            v["tool"]["source_dirty"] = serde_json::json!("unknown");
            v
        }

        let frozen: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/golden/report-0.4.0.json")).unwrap();
        assert_eq!(frozen["schema_version"], "0.4.0");
        assert_eq!(
            frozen["tool"]["source_dirty"], "TEST-DIRTY",
            "precondition: the frozen sample carries the golden placeholder"
        );
        assert!(
            validator.is_valid(&with_unknown_dirty(frozen)),
            "the committed 0.4.0 sample no longer validates against its own schema"
        );

        let current =
            with_unknown_dirty(serde_json::from_str(&render(&fixture::report()).unwrap()).unwrap());
        assert!(
            !validator.is_valid(&current),
            "the frozen 0.4.0 schema accepted a {} document — it is not closed, \
             and the version dispatch in validate.rs is doing nothing",
            SCHEMA_VERSION
        );
    }

    /// `schemas/index.json` is how a consumer at a pin discovers the artifacts
    /// and checks it has the bytes pons was tested with.
    #[test]
    fn the_schema_index_attests_every_published_contract_file() {
        let index: serde_json::Value =
            serde_json::from_str(include_str!("../../../../schemas/index.json")).unwrap();
        let artifacts = index["artifacts"]
            .as_array()
            .expect("the index lists artifacts");
        assert!(!artifacts.is_empty(), "an empty index attests nothing");

        let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        for artifact in artifacts {
            let file = artifact["file"].as_str().expect("file");
            let version = artifact["version"].as_str().expect("version");
            assert!(
                file.contains(version),
                "schemas/{file} does not carry its version {version} in its name"
            );
            let bytes = std::fs::read(schemas.join(file))
                .unwrap_or_else(|e| panic!("cannot read schemas/{file}: {e}"));
            let actual = blake3::hash(&bytes).to_hex().to_string();
            assert_eq!(
                artifact["blake3"].as_str().expect("blake3"),
                actual.as_str(),
                "schemas/{file} changed: update its blake3 entry in schemas/index.json"
            );
        }

        let envelopes: Vec<&serde_json::Value> = artifacts
            .iter()
            .filter(|a| a["id"] == "scan-envelope")
            .collect();
        let current: Vec<&&serde_json::Value> = envelopes
            .iter()
            .filter(|a| a["status"] == "current")
            .collect();
        assert_eq!(
            current.len(),
            1,
            "exactly one scan envelope version is the one this build emits"
        );
        assert_eq!(current[0]["version"], SCHEMA_VERSION);
        for frozen in envelopes.iter().filter(|a| a["status"] == "frozen") {
            assert!(
                FROZEN_SCHEMA_VERSIONS.contains(&frozen["version"].as_str().expect("version")),
                "schemas/index.json calls {} frozen, FROZEN_SCHEMA_VERSIONS does not",
                frozen["version"]
            );
        }
    }
}
