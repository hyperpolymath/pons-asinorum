// SPDX-License-Identifier: MPL-2.0

use std::fmt::Write as _;

use crate::engine::ScanReport;
use crate::finding::{EvidenceClass, Finding, Severity};
use crate::report::SPECULATIVE_SUFFIX;

/// Deterministic, colour-free rendering of findings for terminal/CI output.
pub fn render(findings: &[Finding]) -> String {
    render_detailed(findings, false)
}

/// Full terminal report, including the accounting a reader needs to know
/// whether a clean result was a complete scan. `--explain` adds the evidence
/// class and tier that back (or qualify) each finding.
/// The verdict summary excludes speculative findings from the actionable count
/// and reports their demoted count separately; the scanned finding total includes both.
pub fn render_report(report: &ScanReport, explain: bool) -> String {
    let mut out = render_detailed(&report.findings, explain);
    let _ = writeln!(
        out,
        "scanned: {} file(s), {} finding(s), {} suppressed, {} skipped, {} transcoded",
        report.files_scanned,
        report.findings.len(),
        report.suppressed_findings,
        report.skipped.len(),
        report.transcoded.len()
    );
    // The numeric half of the demotion. The per-finding suffix above demotes a
    // guess visually; this line stops it being added to the count a reader
    // takes away, which is the same distinction the JSON envelope publishes as
    // counts.actionable / counts.demoted_speculative.
    let speculative = report
        .findings
        .iter()
        .filter(|f| f.evidence() == EvidenceClass::Speculative)
        .count();
    let _ = writeln!(
        out,
        "verdict: {} actionable finding(s); {} speculative demoted, not counted",
        report.findings.len() - speculative,
        speculative
    );
    let _ = writeln!(out, "coverage: {}", report.coverage_status());
    for failure in &report.coverage_failures {
        let _ = writeln!(out, "coverage failure: {failure}");
    }
    for entry in &report.inventory {
        if entry.outcome != crate::coverage::Outcome::Analysed || !entry.analysis_complete {
            let _ = writeln!(
                out,
                "  {:?}: {} — {}",
                entry.outcome, entry.path, entry.reason
            );
            for note in &entry.analysis_notes {
                let _ = writeln!(out, "    analysis incomplete: {note}");
            }
        }
    }
    out
}

/// Deterministic rendering with optional evidence/tier annotations.
pub fn render_detailed(findings: &[Finding], explain: bool) -> String {
    let mut out = String::new();
    for f in findings {
        render_one(f, &mut out, explain);
    }
    out
}

/// Append one finding, its evidence note and any counter-condition to `out`.
/// `explain` also includes its evidence class and tier.
fn render_one(f: &Finding, out: &mut String, explain: bool) {
    let severity = match f.severity() {
        Severity::Info => "INFO",
        Severity::Warn => "WARN",
        Severity::Error => "ERROR",
    };

    let suffix = if f.evidence() == EvidenceClass::Speculative {
        SPECULATIVE_SUFFIX
    } else {
        ""
    };

    let loc = f.location();
    let _ = writeln!(
        out,
        "{}:{}:{} [{}] {}: {}{}",
        loc.file,
        loc.line_start,
        loc.col_start,
        severity,
        f.rule_id(),
        f.message(),
        suffix
    );
    let _ = writeln!(out, "    evidence: {}", f.evidence_note());
    if explain {
        let _ = writeln!(
            out,
            "    evidence class: {:?}, tier: {:?}",
            f.evidence(),
            f.tier()
        );
    }
    if let Some(cond) = f.counter_condition() {
        let _ = writeln!(out, "    when fine: {cond}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Location, Tier};

    fn loc() -> Location {
        Location {
            file: "x.py".into(),
            byte_start: 0,
            byte_end: 1,
            line_start: 3,
            col_start: 5,
            line_end: 3,
            col_end: 6,
        }
    }

    #[test]
    fn speculative_findings_are_demoted_in_the_message_line() {
        let f = Finding::new(
            "speculative-rule",
            Tier::T3,
            Severity::Warn,
            loc(),
            "looks suspicious",
            "no hard evidence",
            None,
        );

        let rendered = render(&[f]);
        assert!(rendered.contains("looks suspicious (heuristic — not a verdict)"));
    }

    #[test]
    fn heuristic_findings_are_not_demoted() {
        let f = Finding::new(
            "t0-rule",
            Tier::T0,
            Severity::Error,
            loc(),
            "divide by literal zero",
            "constant zero on the RHS",
            Some("never".to_string()),
        );

        let rendered = render(&[f]);
        assert!(!rendered.contains("heuristic — not a verdict"));
        assert!(rendered.contains("x.py:3:5 [ERROR] t0-rule: divide by literal zero"));
        assert!(rendered.contains("when fine: never"));
    }

    #[test]
    fn missing_counter_condition_omits_the_when_fine_line() {
        let f = Finding::new("r", Tier::T0, Severity::Info, loc(), "msg", "note", None);
        let rendered = render(&[f]);
        assert!(!rendered.contains("when fine"));
    }
}

#[cfg(test)]
mod golden_tests {
    use super::*;
    use crate::report::{assert_golden, fixture};

    #[test]
    fn matches_the_golden_human_output() {
        let out = render(&fixture::report().findings);
        assert_golden(
            "report.txt",
            &out,
            include_str!("../../tests/golden/report.txt"),
        );
    }

    #[test]
    fn the_summary_demotes_speculative_numerically() {
        // The shared fixture carries one SPECULATIVE finding of four.
        let out = render_report(&fixture::report(), false);
        assert!(
            out.contains("verdict: 3 actionable finding(s); 1 speculative demoted, not counted"),
            "the aggregate line must separate guesses from the count a reader \
             takes away; got:\n{out}"
        );
    }

    #[test]
    fn the_speculative_finding_is_demoted_in_the_shared_fixture() {
        let out = render(&fixture::report().findings);
        assert!(out.contains(
            "nested iteration may be superlinear in the input size (heuristic — not a verdict)"
        ));
        // ...and nothing else in the same corpus picked up the suffix.
        assert_eq!(out.matches("heuristic — not a verdict").count(), 1);
    }
}
