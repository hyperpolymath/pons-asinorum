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

/// Bytes that may appear unescaped in a URI path segment, beyond the
/// alphanumerics: RFC 3986's unreserved set (`-._~`) plus the sub-delimiters
/// and `:@` that `pchar` admits. Everything else — space, `#`, `?`, `%`, and
/// every non-ASCII byte — is percent-escaped.
const URI_SEGMENT_SAFE: &[u8] = b"-._~!$&'()*+,;=:@";

/// Percent-escapes one path segment into `out`, byte by byte over its UTF-8.
fn push_escaped_segment(segment: &str, out: &mut String) {
    use std::fmt::Write as _;
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || URI_SEGMENT_SAFE.contains(&byte) {
            out.push(char::from(byte));
        } else {
            // Non-ASCII is escaped per UTF-8 byte, which is what RFC 3986
            // requires and what makes the URI safe to put in JSON and in a
            // code-scanning API payload alike.
            let _ = write!(out, "%{byte:02X}");
        }
    }
}

/// Renders a finding's path as a SARIF `artifactLocation.uri`.
///
/// Two things have to be true at once, and neither is what the path in the
/// finding gives you:
///
/// * **The URI must not depend on how the scan was invoked.** `pons scan .` and
///   `pons scan /abs/path/to/repo` visit the same files, and before this the
///   first emitted `./app/svc.py` while the second emitted the machine's full
///   filesystem layout. A consumer cannot annotate a line from the second
///   form, and two runs of the same scan produced artefacts that did not
///   compare equal. Stripping the scan root makes the URI a property of the
///   file, not of the command line.
/// * **It must be a URI, not a path.** A segment containing a space or a `#`
///   is not a valid URI reference as-is; `#` in particular would truncate the
///   path at a fragment delimiter.
///
/// `originalUriBaseIds` is deliberately **not** emitted. It would have to name
/// an absolute base, which is precisely the build-machine detail this function
/// exists to remove — it would make the artefact non-reproducible and move the
/// leak from `uri` into a sibling key. GitHub code scanning resolves a bare
/// relative URI against the repository root, which is the consumer that
/// motivated #28.
///
/// **Honest limitation:** the URI is relative to the *scan root*, not to the
/// repository root. `pons scan .` from the repository root — the normal case,
/// and the one code scanning runs — makes those the same thing. Scanning a
/// subdirectory does not: `pons scan src/` yields paths relative to `src/`, and
/// a consumer resolving them against the repo root will miss. Fixing that would
/// require discovering a VCS root, which is a scanner concern rather than a
/// reporter one and is not in scope here.
fn artifact_uri(file: &str, root: &str) -> String {
    use std::path::{Component, Path};

    let path = Path::new(file);
    // `strip_prefix` fails rather than panicking when the path is not under
    // the root, which is the case that matters: the fixture's `app/svc.py` is
    // already relative and `strip_prefix(".")` does NOT match it, because
    // `Path::components` keeps a leading `.` only when the path actually
    // begins with one. Falling back to the path unchanged is therefore the
    // common case for an already-relative finding, not an error path.
    let relative = path.strip_prefix(root).unwrap_or(path);
    // `pons scan foo.py` makes root and file the same string, and stripping
    // one from the other leaves nothing. Name the file rather than emit "".
    let relative = if relative.as_os_str().is_empty() {
        path.file_name().map_or(path, Path::new)
    } else {
        relative
    };

    let mut absolute = false;
    let mut segments: Vec<String> = Vec::new();
    for component in relative.components() {
        match component {
            // Only reachable when the finding lies outside the scan root, which
            // discovery does not produce. Keep the leading slash rather than
            // silently presenting an absolute path as a relative one.
            Component::RootDir | Component::Prefix(_) => absolute = true,
            // A leading `./` carries no information and is exactly the prefix
            // #28 asks to be rid of.
            Component::CurDir => {}
            Component::ParentDir => segments.push("..".to_string()),
            Component::Normal(segment) => {
                let mut escaped = String::new();
                push_escaped_segment(&segment.to_string_lossy(), &mut escaped);
                segments.push(escaped);
            }
        }
    }

    let joined = segments.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

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
                        // Normalised against the scan root and escaped, so the
                        // URI is a property of the file rather than of how the
                        // scan was invoked. See `artifact_uri`, which also
                        // records why `originalUriBaseIds` is omitted.
                        artifact_location: ArtifactLocation {
                            uri: artifact_uri(&loc.file, &report.root),
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

    /// A one-finding report standing for the same scan reached by a different
    /// spelling of its root. Deliberately not the shared fixture: what is under
    /// test is the relationship between `root` and `location.file`, and the
    /// fixture fixes both.
    fn report_rooted(root: &str, file: &str) -> ScanReport {
        use crate::engine::RuleInfo;
        use crate::finding::Location;
        use crate::lang::Lang;

        ScanReport {
            root: root.to_string(),
            findings: vec![Finding::new(
                "div-by-literal-zero",
                Tier::T0,
                Severity::Warn,
                Location {
                    file: file.to_string(),
                    byte_start: 15,
                    byte_end: 20,
                    line_start: 1,
                    col_start: 16,
                    line_end: 1,
                    col_end: 21,
                },
                "division by a literal zero",
                "constant zero on the right-hand side",
                None,
            )],
            files_scanned: 1,
            languages: vec![Lang::Python],
            skipped: Vec::new(),
            rules: vec![RuleInfo {
                id: "div-by-literal-zero",
                description: "division or modulo whose right-hand side is a literal zero",
            }],
        }
    }

    #[test]
    fn the_uri_does_not_depend_on_how_the_scan_root_was_spelled() {
        // The two invocations #28 measured. Before the fix the first emitted
        // `./app/svc.py` and the second the scanning machine's full filesystem
        // layout — two artefacts for one scan, and code scanning could not
        // annotate a line from either.
        let dot = render(&report_rooted(".", "./app/svc.py")).unwrap();
        let abs = render(&report_rooted("/home/me/repo", "/home/me/repo/app/svc.py")).unwrap();

        assert!(
            dot.contains(r#""uri": "app/svc.py""#),
            "relative root: expected a bare repo-relative URI, got:\n{dot}"
        );
        assert!(
            abs.contains(r#""uri": "app/svc.py""#),
            "absolute root: expected a bare repo-relative URI, got:\n{abs}"
        );

        // Each spelling gets its own negative assertion, so a regression in
        // either one goes red on its own terms rather than hiding behind the
        // equality below.
        assert!(
            !dot.contains("./app/svc.py"),
            "the `./` prefix survived; consumers treat it as a distinct path"
        );
        assert!(
            !abs.contains("/home/me/repo"),
            "the scanning machine's filesystem layout leaked into the artefact"
        );
    }

    #[test]
    fn an_already_relative_path_is_unchanged_by_a_dot_root() {
        // `Path::strip_prefix(".")` does NOT match `app/svc.py`: `components`
        // keeps a leading `.` only when the path really begins with one. The
        // fallback is therefore the ordinary case for a finding that is already
        // relative, and it is what holds the golden corpus still.
        assert_eq!(artifact_uri("app/svc.py", "."), "app/svc.py");
        assert_eq!(artifact_uri("./app/svc.py", "."), "app/svc.py");
    }

    #[test]
    fn scanning_a_single_file_names_that_file_rather_than_nothing() {
        // `pons scan foo.py` makes root and file the same string; stripping one
        // from the other leaves an empty path, and an empty `uri` is not a
        // location.
        assert_eq!(artifact_uri("foo.py", "foo.py"), "foo.py");
        assert_eq!(artifact_uri("/tmp/foo.py", "/tmp/foo.py"), "foo.py");
    }

    #[test]
    fn uri_segments_are_percent_escaped_but_separators_are_not() {
        // A `#` is the one that actually breaks a consumer: unescaped, it
        // truncates the path at a fragment delimiter, so the annotation lands
        // on a file that does not exist.
        assert_eq!(
            artifact_uri("./my docs/a#b.py", "."),
            "my%20docs/a%23b.py",
            "space and # must be escaped, and the separator must not be"
        );
        // Non-ASCII is escaped per UTF-8 byte, not per character.
        assert_eq!(artifact_uri("./café.py", "."), "caf%C3%A9.py");
        // Percent itself, or the URI would decode to something else entirely.
        assert_eq!(artifact_uri("./100%.py", "."), "100%25.py");
    }

    #[test]
    fn a_normalised_uri_still_validates_against_the_bundled_schema() {
        // Criterion 5 of #28. Noted for the reader, because it is the reason
        // the assertions above are written by hand: the schema constrains
        // shape only and accepts an absolute unescaped path just as happily,
        // so it can confirm this change broke nothing and can never be the
        // evidence that the change was needed.
        let out = render(&report_rooted(
            "/home/me/repo",
            "/home/me/repo/my docs/a#b.py",
        ))
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(validator().validate(&parsed).is_ok());
        assert!(out.contains(r#""uri": "my%20docs/a%23b.py""#));
    }

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
