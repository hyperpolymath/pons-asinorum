// SPDX-License-Identifier: MPL-2.0
//! The severity × evidence matrix, published as data.
//!
//! An aggregating consumer — the estate's verdict grid — needs three things it
//! must not re-invent: which evidence class a tier implies, what a severity
//! plus a class is *worth* when findings are added up, and how a `SPECULATIVE`
//! finding is kept out of that sum. Appendix B of `docs/PLAN.adoc` states the
//! policy in prose; this module is the same policy as data, generated from the
//! constants the reporters themselves use:
//!
//! * [`sarif_level`] and [`sarif_rank`] are what `report/sarif.rs` emits;
//! * [`confidence`] is what `evidence.rs` writes into every record;
//! * [`fail_on_rank`] is what `--fail-on` compares;
//! * [`SPECULATIVE_SUFFIX`] is the demotion text the human and SARIF reporters
//!   append.
//!
//! The document is committed at [`MATRIX_PATH`] and
//! `the_published_matrix_matches_the_code` fails if the two ever disagree, so a
//! consumer can read the file at a pinned commit and trust that it describes the
//! binary at that commit.
//!
//! Two numbers look similar and mean different things, which is why both are
//! published under distinct names. `sarif_rank` is Appendix E's *attention*
//! rank (50/80/70/20): it tells a viewer which class to look at first, and
//! `SPECULATIVE` keeps its 20 so it stays visible. `weight` is what a class
//! contributes to an aggregate total, and for `SPECULATIVE` it is **zero** — a
//! guess adds nothing to a sum, however visible it is in a table.

use serde::Serialize;

use crate::finding::{EvidenceClass, Severity, Tier};
use crate::report::SPECULATIVE_SUFFIX;

/// Version of the published matrix document, not of the tool.
pub const MATRIX_VERSION: &str = "1.0.0";

/// Where the generated document lives, relative to the repository root.
pub const MATRIX_PATH: &str = "schemas/verdict-matrix-1.0.0.json";

static ALL_SEVERITIES: [Severity; 3] = [Severity::Error, Severity::Warn, Severity::Info];
static ALL_EVIDENCE: [EvidenceClass; 4] = [
    EvidenceClass::Heuristic,
    EvidenceClass::Dataflow,
    EvidenceClass::Protocol,
    EvidenceClass::Speculative,
];

/// What `--fail-on` compares: `ERROR` 2 > `WARN` 1 > `INFO` 0.
///
/// Kept separate from [`severity_weight`] on purpose. `INFO` is rank 0 here
/// because "meets or exceeds INFO" must select everything; it is weight 1 there
/// because an informational finding still counts once when findings are summed.
pub fn fail_on_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Warn => 1,
        Severity::Error => 2,
    }
}

/// Ordinal weight of a severity in an aggregate total: `INFO` 1, `WARN` 2,
/// `ERROR` 3. Never zero — a finding that arrived is a finding.
pub fn severity_weight(severity: Severity) -> u32 {
    match severity {
        Severity::Info => 1,
        Severity::Warn => 2,
        Severity::Error => 3,
    }
}

/// Appendix E's fixed SARIF `rank` for an evidence class: how much attention
/// the class has earned, independent of severity.
pub fn sarif_rank(evidence: EvidenceClass) -> u32 {
    match evidence {
        EvidenceClass::Heuristic => 50,
        EvidenceClass::Dataflow => 80,
        EvidenceClass::Protocol => 70,
        EvidenceClass::Speculative => 20,
    }
}

/// What a class contributes to an aggregate count.
///
/// Equal to [`sarif_rank`] for the three verdict classes, and **zero** for
/// `SPECULATIVE`: the demotion is numeric as well as visual, so no summing
/// consumer can turn guesses into weight by adding them up.
pub fn evidence_weight(evidence: EvidenceClass) -> u32 {
    if counts_toward_verdict(evidence) {
        sarif_rank(evidence)
    } else {
        0
    }
}

/// Whether a finding in this class may contribute to a verdict at all.
pub fn counts_toward_verdict(evidence: EvidenceClass) -> bool {
    evidence != EvidenceClass::Speculative
}

/// The single aggregate weight for a (severity, evidence) pair. Zero for every
/// `SPECULATIVE` cell, whatever its severity claims.
pub fn aggregate_weight(evidence: EvidenceClass, severity: Severity) -> u32 {
    severity_weight(severity) * evidence_weight(evidence)
}

/// The SARIF level for a finding, which is also the visual demotion.
///
/// The `SPECULATIVE` check comes **first** and unconditionally (Appendix E): a
/// guess must never arrive in a code-scanning UI wearing the colour of a
/// verdict, so an `ERROR`-severity speculative finding still renders as `note`.
pub fn sarif_level(evidence: EvidenceClass, severity: Severity) -> &'static str {
    if evidence == EvidenceClass::Speculative {
        return "note";
    }
    level_for_severity(severity)
}

/// The SARIF level a severity earns when its class is not demoted.
fn level_for_severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "note",
    }
}

/// The qualitative confidence label the evidence contract publishes per record.
pub fn confidence(evidence: EvidenceClass) -> &'static str {
    match evidence {
        EvidenceClass::Heuristic => "heuristic",
        EvidenceClass::Dataflow => "model-dependent",
        EvidenceClass::Protocol => "premise-dependent",
        EvidenceClass::Speculative => "speculative",
    }
}

/// Which aggregate bucket a class feeds: the actionable count, or the demoted
/// one that is reported separately and never added to it.
pub fn count_bucket(evidence: EvidenceClass) -> &'static str {
    if counts_toward_verdict(evidence) {
        "actionable"
    } else {
        "demoted"
    }
}

/// The tier that fixes this class. The inverse of [`EvidenceClass::from`], kept
/// beside it so the published `tier` column cannot be typed by hand.
pub fn tier_of(evidence: EvidenceClass) -> Tier {
    match evidence {
        EvidenceClass::Heuristic => Tier::T0,
        EvidenceClass::Dataflow => Tier::T1,
        EvidenceClass::Protocol => Tier::T2,
        EvidenceClass::Speculative => Tier::T3,
    }
}

/// The demotion text for a class: the exact suffix the human and SARIF
/// reporters append, and the empty string for every class that is a verdict.
pub fn demotion_suffix(evidence: EvidenceClass) -> &'static str {
    if counts_toward_verdict(evidence) {
        ""
    } else {
        SPECULATIVE_SUFFIX
    }
}

#[derive(Debug, Serialize)]
pub struct Matrix {
    pub schema_version: &'static str,
    pub producer: &'static str,
    pub generated_from: &'static str,
    pub severity_scale: Vec<SeverityRow>,
    pub evidence_scale: Vec<EvidenceRow>,
    pub corroboration: Corroboration,
    pub grid: Vec<Cell>,
    pub aggregation: Aggregation,
}

#[derive(Debug, Serialize)]
pub struct SeverityRow {
    pub severity: Severity,
    pub weight: u32,
    pub fail_on_rank: u8,
    pub sarif_level_when_not_demoted: &'static str,
}

#[derive(Debug, Serialize)]
pub struct EvidenceRow {
    pub evidence: EvidenceClass,
    pub tier: Tier,
    pub sarif_rank: u32,
    pub weight: u32,
    pub confidence: &'static str,
    pub counts_toward_verdict: bool,
}

/// How a consumer mints the corroboration key for a finding, including a
/// finding from another producer. Mirrors [`crate::corroborate`].
#[derive(Debug, Serialize)]
pub struct Corroboration {
    pub scheme: &'static str,
    pub hash: &'static str,
    pub encoding: &'static str,
    pub input_template: &'static str,
    pub class_source: &'static str,
    pub path_convention: &'static str,
    pub line_convention: &'static str,
    pub caution: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Cell {
    pub severity: Severity,
    pub evidence: EvidenceClass,
    pub sarif_level: &'static str,
    pub aggregate_weight: u32,
    pub counts_toward_verdict: bool,
    pub gate_eligible: bool,
    pub count_bucket: &'static str,
    pub demotion_suffix: &'static str,
}

/// The counting rules, stated where a consumer will actually read them.
#[derive(Debug, Serialize)]
pub struct Aggregation {
    pub demoted_evidence: Vec<&'static str>,
    pub demoted_sarif_level: &'static str,
    pub demotion_suffix: &'static str,
    pub demoted_count_field: &'static str,
    pub actionable_count_field: &'static str,
    pub total_count_field: &'static str,
    pub total_includes_demoted: bool,
    pub actionable_definition: &'static str,
    pub rules: Vec<&'static str>,
}

/// Build the published matrix from the constants the reporters use.
pub fn matrix() -> Matrix {
    let mut grid = Vec::new();
    for severity in ALL_SEVERITIES.iter().copied() {
        for evidence in ALL_EVIDENCE.iter().copied() {
            grid.push(Cell {
                severity,
                evidence,
                sarif_level: sarif_level(evidence, severity),
                aggregate_weight: aggregate_weight(evidence, severity),
                counts_toward_verdict: counts_toward_verdict(evidence),
                // A finding is gate-eligible when `--fail-on` may act on it,
                // which is exactly when it counts toward a verdict: the CLI
                // skips SPECULATIVE before comparing severity.
                gate_eligible: counts_toward_verdict(evidence),
                count_bucket: count_bucket(evidence),
                demotion_suffix: demotion_suffix(evidence),
            });
        }
    }

    Matrix {
        schema_version: MATRIX_VERSION,
        producer: "pons",
        generated_from: "crates/pons-core/src/verdict.rs",
        severity_scale: ALL_SEVERITIES
            .iter()
            .map(|&severity| SeverityRow {
                severity,
                weight: severity_weight(severity),
                fail_on_rank: fail_on_rank(severity),
                sarif_level_when_not_demoted: level_for_severity(severity),
            })
            .collect(),
        evidence_scale: ALL_EVIDENCE
            .iter()
            .map(|&evidence| EvidenceRow {
                evidence,
                tier: tier_of(evidence),
                sarif_rank: sarif_rank(evidence),
                weight: evidence_weight(evidence),
                confidence: confidence(evidence),
                counts_toward_verdict: counts_toward_verdict(evidence),
            })
            .collect(),
        corroboration: Corroboration {
            scheme: crate::corroborate::SCHEME,
            hash: "blake3",
            encoding: "64 lowercase hexadecimal characters",
            input_template: "scheme, then one NUL, then the root-relative path, one NUL, \
                             the 1-based start line in decimal, one NUL, then the evidence \
                             class name; the whole string is hashed",
            class_source: "The producer's own evidence class, or the class the consuming grid \
                           has explicitly mapped a foreign producer onto. Never a raw foreign \
                           severity, and never an upgrade of one.",
            path_convention: "Root-relative, '/' separated, no leading './' — the same spelling \
                              as evidence_contract.records[].path, so a key survives a checkout \
                              move.",
            line_convention: "1-based line of the finding's start; a finding spanning lines is \
                              keyed on the line it starts on.",
            caution: "Equal keys assert the same path, line and evidence class and nothing more. \
                      They are not proof of semantic equivalence, and a demoted class \
                      corroborates no verdict.",
        },
        grid,
        aggregation: Aggregation {
            demoted_evidence: vec!["SPECULATIVE"],
            demoted_sarif_level: "note",
            demotion_suffix: SPECULATIVE_SUFFIX,
            demoted_count_field: "counts.demoted_speculative",
            actionable_count_field: "counts.actionable",
            total_count_field: "counts.total",
            total_includes_demoted: true,
            actionable_definition: "counts.total - counts.demoted_speculative",
            rules: vec![
                "A demoted finding contributes zero aggregate_weight and is counted only in \
                 counts.demoted_speculative; it is never part of counts.actionable.",
                "Demotion is unconditional and overrides the finding's own severity: ERROR + \
                 SPECULATIVE is still not a verdict, and its SARIF level is 'note'.",
                "An aggregating consumer reports counts.actionable as its finding total, or \
                 reports both numbers; a total that includes demoted findings must say so.",
                "Corroboration collapses a site ACROSS producers, not findings within one \
                 report: two producers minting the same semantic_fingerprint are one \
                 corroborated site, while counts.* count findings, so two Pons rules at one \
                 site in one class are two findings sharing one key.",
            ],
        },
    }
}

/// The published document, with a trailing newline so it is a well-formed text
/// stream. Pretty-printed for review, like the scan envelope.
pub fn matrix_json() -> String {
    let mut out =
        serde_json::to_string_pretty(&matrix()).expect("the verdict matrix is plain data");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(MATRIX_PATH),
        )
        .expect("the committed verdict matrix must be readable")
    }

    /// The point of the file: a consumer reads it at a pin, so it must be the
    /// document the code would emit at that pin, byte for byte.
    #[test]
    fn the_published_matrix_matches_the_code() {
        let generated = matrix_json();
        if std::env::var_os("PONS_BLESS").is_some() {
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(MATRIX_PATH),
                &generated,
            )
            .expect("could not write the verdict matrix");
            return;
        }
        assert_eq!(
            published(),
            generated,
            "{MATRIX_PATH} drifted from the code — inspect the diff, then re-run \
             with PONS_BLESS=1 only if the new document is correct"
        );
    }

    #[test]
    fn every_severity_and_class_pair_appears_exactly_once() {
        let grid = matrix().grid;
        assert_eq!(grid.len(), 12);
        for severity in ALL_SEVERITIES.iter().copied() {
            for evidence in ALL_EVIDENCE.iter().copied() {
                let hits = grid
                    .iter()
                    .filter(|c| c.severity == severity && c.evidence == evidence)
                    .count();
                assert_eq!(
                    hits, 1,
                    "grid cell {severity:?} x {evidence:?} is not unique"
                );
            }
        }
    }

    #[test]
    fn a_speculative_cell_weighs_nothing_at_any_severity() {
        for cell in matrix().grid {
            if cell.evidence != EvidenceClass::Speculative {
                assert!(cell.counts_toward_verdict && cell.gate_eligible);
                assert_eq!(cell.count_bucket, "actionable");
                assert_eq!(cell.demotion_suffix, "");
                assert!(cell.aggregate_weight > 0, "{cell:?} weighs nothing");
                continue;
            }
            assert_eq!(cell.aggregate_weight, 0, "{cell:?}");
            assert!(!cell.counts_toward_verdict, "{cell:?}");
            assert!(!cell.gate_eligible, "{cell:?}");
            assert_eq!(cell.count_bucket, "demoted");
            assert_eq!(cell.demotion_suffix, SPECULATIVE_SUFFIX);
            assert_eq!(
                cell.sarif_level, "note",
                "an ERROR-severity speculation must still render as a note"
            );
        }
    }

    #[test]
    fn speculative_stays_visible_in_sarif_even_though_it_weighs_nothing() {
        // The two numbers are different jobs; if someone "fixes" the weight by
        // setting the rank to 0 too, the demotion stops being visible in a
        // SARIF viewer and the finding silently disappears instead.
        assert_eq!(sarif_rank(EvidenceClass::Speculative), 20);
        assert_eq!(evidence_weight(EvidenceClass::Speculative), 0);
    }

    #[test]
    fn a_published_level_is_appendix_e_pinned() {
        // Pinned literals, because the SARIF reporter now delegates here: a
        // "fix" that made every level "warning" would otherwise be invisible.
        assert_eq!(
            sarif_level(EvidenceClass::Dataflow, Severity::Error),
            "error"
        );
        assert_eq!(
            sarif_level(EvidenceClass::Dataflow, Severity::Warn),
            "warning"
        );
        assert_eq!(sarif_level(EvidenceClass::Dataflow, Severity::Info), "note");
        assert_eq!(
            sarif_level(EvidenceClass::Protocol, Severity::Error),
            "error"
        );
        assert_eq!(
            sarif_level(EvidenceClass::Speculative, Severity::Error),
            "note"
        );
        assert_eq!(
            sarif_level(EvidenceClass::Speculative, Severity::Info),
            "note"
        );
    }

    #[test]
    fn the_two_ordinal_scales_stay_distinct() {
        // INFO is fail-on rank 0 but weight 1. Collapsing them would make every
        // informational finding weigh nothing, which is a different bug from
        // the demotion above and must not be "fixed" the same way.
        assert_eq!(fail_on_rank(Severity::Info), 0);
        assert_eq!(severity_weight(Severity::Info), 1);
        assert_eq!(fail_on_rank(Severity::Error), 2);
        assert_eq!(severity_weight(Severity::Error), 3);
    }

    #[test]
    fn the_tier_and_class_columns_are_inverses() {
        for evidence in ALL_EVIDENCE {
            assert_eq!(EvidenceClass::from(tier_of(evidence)), evidence);
        }
    }

    #[test]
    fn corroboration_fields_match_the_implementation() {
        // The document tells a consumer how to mint the key without pons. If
        // that prose stops describing what corroborate.rs actually hashes, the
        // grid starts minting keys that never match a pons finding.
        let c = matrix().corroboration;
        assert_eq!(c.scheme, crate::corroborate::SCHEME);
        let sample = crate::corroborate::canonical_input("a.py", 7, EvidenceClass::Dataflow);
        assert_eq!(
            sample, "pons-corroboration-v1\u{0}a.py\u{0}7\u{0}DATAFLOW",
            "the template in the published document must describe this string"
        );
    }
}
