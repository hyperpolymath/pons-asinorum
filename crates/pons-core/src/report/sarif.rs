// SPDX-License-Identifier: MPL-2.0
//! SARIF 2.1.0 output, per `docs/PLAN.adoc` Appendix E.
//!
//! As in [`super::json`], the shape comes from derived `Serialize` structs
//! rather than `serde_json::json!`, so key order is fixed by the source and
//! the golden file cannot flap.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::engine::ScanReport;
use crate::finding::{EvidenceClass, Finding, Severity, Tier};
use crate::report::SPECULATIVE_SUFFIX;

const SARIF_SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const INFORMATION_URI: &str = "https://github.com/hyperpolymath/pons-asinorum";

/// The unit `region.startColumn`/`endColumn` are measured in, declared on every
/// run. See [`Run::column_kind`]; the conversion itself lives in
/// [`Location::from_node`](crate::finding::Location::from_node).
const COLUMN_KIND: &str = "utf16CodeUnits";

/// SARIF severity for a finding.
///
/// The `SPECULATIVE` check comes **first** and unconditionally: Appendix E
/// requires T3 to render as `"note"` whatever its own severity says. A
/// speculative finding must never arrive in a code-scanning UI wearing the
/// colour of a verdict.
fn level_of(f: &Finding) -> &'static str {
    if f.evidence() == EvidenceClass::Speculative {
        return "note";
    }
    match f.severity() {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "note",
    }
}

/// Appendix E's fixed rank table: how much attention the evidence class has
/// earned, independent of severity.
fn rank_of(evidence: EvidenceClass) -> f64 {
    match evidence {
        EvidenceClass::Heuristic => 50.0,
        EvidenceClass::Dataflow => 80.0,
        EvidenceClass::Protocol => 70.0,
        EvidenceClass::Speculative => 20.0,
    }
}

/// `div-by-literal-zero` -> `DivByLiteralZero`. SARIF wants a readable
/// identifier in `name` alongside the stable `id`; deriving it keeps the two
/// from drifting apart.
fn descriptor_name(id: &str) -> String {
    id.split('-')
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn message_text(f: &Finding) -> String {
    if f.evidence() == EvidenceClass::Speculative {
        format!("{}{}", f.message(), SPECULATIVE_SUFFIX)
    } else {
        f.message().to_string()
    }
}

#[derive(Serialize)]
struct Sarif {
    #[serde(rename = "$schema")]
    schema: &'static str,
    version: &'static str,
    runs: Vec<Run>,
}

#[derive(Serialize)]
struct Run {
    tool: SarifTool,
    /// Declared explicitly rather than left to the default.
    ///
    /// SARIF 2.1.0 admits exactly two values here — `utf16CodeUnits` and
    /// `unicodeCodePoints` — and *no* byte option, which is why
    /// [`Location`](crate::finding::Location) converts columns at construction
    /// rather than the reporter reinterpreting them here. A consumer that
    /// assumed the other kind would be off by one per non-BMP character, and
    /// saying nothing would leave that disagreement to the default.
    #[serde(rename = "columnKind")]
    column_kind: &'static str,
    results: Vec<SarifResult>,
}

#[derive(Serialize)]
struct SarifTool {
    driver: Driver,
}

#[derive(Serialize)]
struct Driver {
    name: &'static str,
    version: &'static str,
    #[serde(rename = "informationUri")]
    information_uri: &'static str,
    rules: Vec<ReportingDescriptor>,
}

#[derive(Serialize)]
struct ReportingDescriptor {
    id: String,
    name: String,
    #[serde(rename = "shortDescription")]
    short_description: Text,
    #[serde(rename = "defaultConfiguration")]
    default_configuration: DefaultConfiguration,
    properties: RuleProperties,
}

#[derive(Serialize)]
struct Text {
    text: String,
}

#[derive(Serialize)]
struct DefaultConfiguration {
    level: &'static str,
}

#[derive(Serialize)]
struct RuleProperties {
    tier: Tier,
    #[serde(rename = "evidenceClass")]
    evidence_class: EvidenceClass,
}

#[derive(Serialize)]
struct SarifResult {
    #[serde(rename = "ruleId")]
    rule_id: String,
    #[serde(rename = "ruleIndex")]
    rule_index: usize,
    level: &'static str,
    rank: f64,
    message: Text,
    locations: Vec<SarifLocation>,
    properties: ResultProperties,
}

#[derive(Serialize)]
struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    physical_location: PhysicalLocation,
}

#[derive(Serialize)]
struct PhysicalLocation {
    #[serde(rename = "artifactLocation")]
    artifact_location: ArtifactLocation,
    region: Region,
}

#[derive(Serialize)]
struct ArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
struct Region {
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "startColumn")]
    start_column: usize,
    #[serde(rename = "endLine")]
    end_line: usize,
    #[serde(rename = "endColumn")]
    end_column: usize,
    #[serde(rename = "byteOffset")]
    byte_offset: usize,
    #[serde(rename = "byteLength")]
    byte_length: usize,
}

#[derive(Serialize)]
struct ResultProperties {
    #[serde(rename = "evidenceClass")]
    evidence_class: EvidenceClass,
    tier: Tier,
    #[serde(rename = "evidenceNote")]
    evidence_note: String,
    /// Appendix E: present only when the finding carries one.
    #[serde(rename = "counterCondition", skip_serializing_if = "Option::is_none")]
    counter_condition: Option<String>,
}

/// Render a [`ScanReport`] as SARIF 2.1.0, with a trailing newline.
pub fn render(report: &ScanReport) -> anyhow::Result<String> {
    let descriptions: BTreeMap<&str, &str> =
        report.rules.iter().map(|r| (r.id, r.description)).collect();

    // Only rules that actually fired are emitted, sorted by id, so
    // `ruleIndex` is a deterministic function of the findings rather than of
    // registry order.
    let emitted: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.rule_id())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let index_of: BTreeMap<&str, usize> =
        emitted.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    let rules = emitted
        .iter()
        .map(|id| {
            // Tier is a property of the rule, but it is stated in exactly one
            // place — the `RawFinding` the rule built — so it is read back
            // from a finding rather than declared a second time on the trait
            // where the two could disagree. Every emitted rule has at least
            // one finding by construction.
            let first = report
                .findings
                .iter()
                .find(|f| f.rule_id() == *id)
                .expect("an emitted rule id came from the findings themselves");
            ReportingDescriptor {
                id: (*id).to_string(),
                name: descriptor_name(id),
                short_description: Text {
                    text: descriptions.get(id).copied().unwrap_or(id).to_string(),
                },
                default_configuration: DefaultConfiguration {
                    level: level_of(first),
                },
                properties: RuleProperties {
                    tier: first.tier(),
                    evidence_class: first.evidence(),
                },
            }
        })
        .collect();

    let results = report
        .findings
        .iter()
        .map(|f| {
            let loc = f.location();
            SarifResult {
                rule_id: f.rule_id().to_string(),
                rule_index: index_of[f.rule_id()],
                level: level_of(f),
                rank: rank_of(f.evidence()),
                message: Text {
                    text: message_text(f),
                },
                locations: vec![SarifLocation {
                    physical_location: PhysicalLocation {
                        // The path exactly as the finding carries it: a
                        // relative scan stays relative, which is both what
                        // SARIF consumers want and what keeps the golden file
                        // machine-independent.
                        artifact_location: ArtifactLocation {
                            uri: loc.file.clone(),
                        },
                        region: Region {
                            start_line: loc.line_start,
                            start_column: loc.col_start,
                            end_line: loc.line_end,
                            end_column: loc.col_end,
                            byte_offset: loc.byte_start,
                            byte_length: loc.byte_end.saturating_sub(loc.byte_start),
                        },
                    },
                }],
                properties: ResultProperties {
                    evidence_class: f.evidence(),
                    tier: f.tier(),
                    evidence_note: f.evidence_note().to_string(),
                    counter_condition: f.counter_condition().map(str::to_string),
                },
            }
        })
        .collect();

    let sarif = Sarif {
        schema: SARIF_SCHEMA,
        version: "2.1.0",
        runs: vec![Run {
            tool: SarifTool {
                driver: Driver {
                    name: "pons",
                    version: env!("CARGO_PKG_VERSION"),
                    information_uri: INFORMATION_URI,
                    rules,
                },
            },
            column_kind: COLUMN_KIND,
            results,
        }],
    };

    let mut out = serde_json::to_string_pretty(&sarif)?;
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{assert_golden, fixture};

    /// The SARIF 2.1.0 schema, bundled rather than fetched: a validator that
    /// reaches the network is a validator that passes vacuously the day the
    /// network is unavailable.
    const SCHEMA: &str = include_str!("../../tests/schemas/sarif-2.1.0.json");

    fn validator() -> jsonschema::Validator {
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).expect("bundled SARIF schema is not valid JSON");
        jsonschema::validator_for(&schema).expect("bundled SARIF schema did not compile")
    }

    #[test]
    fn output_validates_against_the_sarif_210_schema() {
        let out = render(&fixture::report()).unwrap();
        let instance: serde_json::Value = serde_json::from_str(&out).unwrap();
        let errors: Vec<String> = validator()
            .iter_errors(&instance)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "SARIF output failed validation:\n{errors:#?}"
        );
    }

    /// MUTANT. Without this, the test above could be passing because the
    /// validator accepts anything — a schema that failed to compile, a
    /// validator wired to the wrong draft, an `iter_errors` that never
    /// yields. Corrupt one field the schema constrains by enum and the
    /// validator must reject it. A validator suite that only ever goes green
    /// proves nothing.
    #[test]
    fn the_validator_rejects_a_corrupted_sarif_field() {
        let out = render(&fixture::report()).unwrap();
        let mut instance: serde_json::Value = serde_json::from_str(&out).unwrap();

        // `level` is constrained to none|note|warning|error.
        instance["runs"][0]["results"][0]["level"] = serde_json::json!("catastrophic");
        assert_eq!(
            instance["runs"][0]["results"][0]["level"], "catastrophic",
            "the mutation was not applied — a no-op mutant is a fake red"
        );
        assert!(
            validator().iter_errors(&instance).next().is_some(),
            "the validator accepted an invalid `level`: it is not actually validating"
        );

        // And a second, structurally different corruption, so the first is
        // not passing by accident of one enum.
        let mut other: serde_json::Value = serde_json::from_str(&out).unwrap();
        other["version"] = serde_json::json!("9.9.9");
        assert!(
            validator().iter_errors(&other).next().is_some(),
            "the validator accepted version 9.9.9: it is not actually validating"
        );
    }

    #[test]
    fn matches_the_golden_sarif_output() {
        let out = render(&fixture::report()).unwrap();
        assert_golden(
            "report.sarif.json",
            &out,
            include_str!("../../tests/golden/report.sarif.json"),
        );
    }

    #[test]
    fn a_speculative_finding_is_demoted_to_note_even_when_its_severity_is_error() {
        let report = fixture::report();
        let spec = report
            .findings
            .iter()
            .find(|f| f.rule_id() == "possible-superlinear")
            .unwrap();

        // The control: the fixture's T3 finding really is ERROR, so this
        // asserts an override rather than a coincidence.
        assert_eq!(spec.severity(), Severity::Error);
        assert_eq!(level_of(spec), "note");
        assert_eq!(rank_of(spec.evidence()), 20.0);
        assert!(message_text(spec).ends_with(SPECULATIVE_SUFFIX));
    }

    #[test]
    fn a_non_speculative_error_keeps_error_level_and_carries_no_suffix() {
        let report = fixture::report();
        let f = report
            .findings
            .iter()
            .find(|f| f.rule_id() == "dead-store")
            .unwrap();
        assert_eq!(level_of(f), "error");
        assert_eq!(rank_of(f.evidence()), 80.0);
        assert!(!message_text(f).contains("heuristic"));
    }

    #[test]
    fn rule_index_points_at_the_matching_driver_rule() {
        let out = render(&fixture::report()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let rules = v["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();

        // Only rules that fired are listed; `never-fires` is registered in
        // the fixture and must not appear.
        assert_eq!(rules.len(), 4);
        assert!(!out.contains("never-fires"));

        for result in v["runs"][0]["results"].as_array().unwrap() {
            let idx = result["ruleIndex"].as_u64().unwrap() as usize;
            assert_eq!(
                rules[idx]["id"], result["ruleId"],
                "ruleIndex does not point at the rule named by ruleId"
            );
        }
    }

    #[test]
    fn counter_condition_is_omitted_from_properties_when_absent() {
        let out = render(&fixture::report()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let results = v["runs"][0]["results"].as_array().unwrap();

        let without = results
            .iter()
            .find(|r| r["ruleId"] == "dead-store")
            .unwrap();
        assert!(!without["properties"]
            .as_object()
            .unwrap()
            .contains_key("counterCondition"));

        let with = results
            .iter()
            .find(|r| r["ruleId"] == "div-by-literal-zero")
            .unwrap();
        assert_eq!(
            with["properties"]["counterCondition"],
            "the surrounding branch is provably dead"
        );
    }

    #[test]
    fn descriptor_names_are_derived_from_the_rule_id() {
        assert_eq!(descriptor_name("div-by-literal-zero"), "DivByLiteralZero");
        assert_eq!(descriptor_name("dead-store"), "DeadStore");
    }
}
