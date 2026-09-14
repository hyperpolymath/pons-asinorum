// SPDX-License-Identifier: MPL-2.0

use std::fmt::Write as _;

use crate::finding::{EvidenceClass, Finding, Severity};

/// A colour-independent suffix demoting `SPECULATIVE`-tier findings from a
/// verdict to a heuristic — colour is lost in piped CI output, so this text
/// is the only signal that must survive.
const SPECULATIVE_SUFFIX: &str = " (heuristic — not a verdict)";

/// Deterministic, colour-free rendering of findings for terminal/CI output.
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::new();
    for f in findings {
        render_one(f, &mut out);
    }
    out
}

fn render_one(f: &Finding, out: &mut String) {
    let severity = match f.severity {
        Severity::Info => "INFO",
        Severity::Warn => "WARN",
        Severity::Error => "ERROR",
    };

    let suffix = if f.evidence == EvidenceClass::Speculative {
        SPECULATIVE_SUFFIX
    } else {
        ""
    };

    let _ = writeln!(
        out,
        "{}:{}:{} [{}] {}: {}{}",
        f.location.file,
        f.location.line_start,
        f.location.col_start,
        severity,
        f.rule_id,
        f.message,
        suffix
    );
    let _ = writeln!(out, "    evidence: {}", f.evidence_note);
    if let Some(cond) = &f.counter_condition {
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
