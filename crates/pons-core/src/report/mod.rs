// SPDX-License-Identifier: MPL-2.0

pub mod human;
pub mod json;
pub mod sarif;

/// A colour-independent suffix demoting `SPECULATIVE` findings from a verdict
/// to a heuristic.
///
/// Declared once, here, because more than one reporter must carry the exact
/// same text: colour is lost in piped CI output, so this string is the only
/// demotion signal that survives. Three copies of a string that must be
/// identical is three chances for it to stop being identical.
pub(crate) const SPECULATIVE_SUFFIX: &str = " (heuristic — not a verdict)";

#[cfg(test)]
pub(crate) fn assert_golden(name: &str, actual: &str, expected: &str) {
    // `PONS_BLESS=1 cargo test` rewrites the golden files. It is deliberately
    // an env var and not a flag: blessing must be something you opt into by
    // hand, never something a CI run can do to itself and call it green.
    if std::env::var_os("PONS_BLESS").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name);
        std::fs::write(&path, actual).expect("could not write golden file");
        return;
    }
    assert_eq!(
        actual, expected,
        "golden file tests/golden/{name} drifted — inspect the diff, then \
         re-run with PONS_BLESS=1 only if the new output is correct"
    );
}

#[cfg(test)]
pub(crate) mod fixture {
    //! One fixed corpus, shared by the human, JSON and SARIF golden tests.
    //!
    //! Shared on purpose: the M3 exit gate requires a `SPECULATIVE` finding
    //! to be demoted in *all three* formats, and that is only a meaningful
    //! claim if all three are asked about the same finding. Three separately
    //! hand-built fixtures could drift until each format demoted a different
    //! thing and every test still passed.
    //!
    //! It covers all four evidence classes. The `SPECULATIVE` entry carries
    //! severity `ERROR` deliberately — it is the control proving the SARIF
    //! demotion to `"note"` overrides severity rather than merely agreeing
    //! with it.

    use crate::engine::{RuleInfo, ScanReport, SkippedFile};
    use crate::finding::{Finding, Location, Severity, Tier};
    use crate::lang::Lang;

    fn loc(file: &str, byte_start: usize, line: usize) -> Location {
        Location {
            file: file.to_string(),
            byte_start,
            byte_end: byte_start + 7,
            line_start: line,
            col_start: 5,
            line_end: line,
            col_end: 12,
        }
    }

    pub(crate) fn report() -> ScanReport {
        ScanReport {
            root: ".".to_string(),
            // Already in the order `Engine::scan_files` sorts into: by file,
            // then byte offset.
            findings: vec![
                Finding::new(
                    "dead-store",
                    Tier::T1,
                    Severity::Error,
                    loc("app/svc.py", 812, 41),
                    "value assigned to `total` is never read before it is overwritten",
                    "live-variable analysis: `total` not in LiveOut after this store",
                    None,
                ),
                Finding::new(
                    "div-by-literal-zero",
                    Tier::T0,
                    Severity::Warn,
                    loc("app/svc.py", 904, 47),
                    "division by a literal zero",
                    "constant zero on the right-hand side",
                    Some("the surrounding branch is provably dead".to_string()),
                ),
                Finding::new(
                    "suppress-then-emit",
                    Tier::T2,
                    Severity::Warn,
                    loc("lib/util.ts", 120, 9),
                    "channel enters `suppressed` and reaches `prompt` with no `set_loud`",
                    "enters `suppressed` at line 4; reaches `prompt` at line 9",
                    Some("a different channel instance reaches the prompt".to_string()),
                ),
                Finding::new(
                    "possible-superlinear",
                    Tier::T3,
                    // ERROR on purpose: the SARIF level must still be "note".
                    Severity::Error,
                    loc("lib/util.ts", 300, 22),
                    "nested iteration may be superlinear in the input size",
                    "two loops over collections derived from the same source",
                    None,
                ),
            ],
            files_scanned: 7,
            languages: vec![Lang::Python, Lang::TypeScript],
            skipped: vec![SkippedFile {
                path: "app/broken.py".to_string(),
                reason: "could not read: No such file or directory (os error 2)".to_string(),
            }],
            rules: vec![
                RuleInfo {
                    id: "dead-store",
                    description: "a value stored and then overwritten before any read",
                },
                RuleInfo {
                    id: "div-by-literal-zero",
                    description: "division or modulo whose right-hand side is a literal zero",
                },
                RuleInfo {
                    id: "suppress-then-emit",
                    description: "output suppressed on one path and emitted on another",
                },
                RuleInfo {
                    id: "possible-superlinear",
                    description: "nested iteration that may be superlinear in the input size",
                },
                RuleInfo {
                    // Registered but never fired: proves the SARIF driver
                    // lists only rules that actually produced a result.
                    id: "never-fires",
                    description: "a registered rule with no findings in this corpus",
                },
            ],
        }
    }
}
