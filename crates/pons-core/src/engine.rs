// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeSet;
use std::path::Path;

use tree_sitter::Tree;

use crate::finding::{Finding, RawFinding};
use crate::lang::Lang;
use crate::source::DiscoveredFile;
use crate::{parse, source};

/// Everything a [`Rule`] needs to inspect one source file.
pub struct RuleCtx<'a> {
    pub path: &'a Path,
    pub lang: Lang,
    pub text: &'a str,
    pub tree: &'a Tree,
}

/// A single check. Implementors live in `pons-rules`; `pons-core` stays
/// rule-agnostic. `check` returns [`RawFinding`], not [`Finding`] — a rule
/// has no way to set its own `rule_id`; only [`Engine::scan`] can, from
/// `id()`, so a rule/id mismatch cannot happen.
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    /// One line, no trailing full stop. Surfaced as SARIF's
    /// `shortDescription` and, from M7, as the generated rule catalogue —
    /// so it lives on the rule itself rather than in a table that can drift
    /// away from the code.
    fn description(&self) -> &'static str;
    fn languages(&self) -> &'static [Lang];
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding>;
}

/// A file or directory that was found but never analysed, with the reason.
/// Surfaced so a partial scan can never be mistaken for a clean one.
///
/// Three different layers produce these — a subdirectory the walker could not
/// enumerate, a file that could not be read, a file that could not be parsed —
/// and they deliberately share one type. The distinction matters to the code
/// that noticed; it does not matter to the person reading the report, who
/// needs exactly one question answered: what did you not look at, and why.
///
/// `Serialize` is derived here rather than mirrored by a private struct in the
/// JSON reporter, so the envelope's `scanned.skipped` entries cannot drift
/// from the fields the engine actually fills in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

/// Identity of a registered rule, carried so reporters can describe rules
/// without `pons-core` depending on the rule crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleInfo {
    pub id: &'static str,
    pub description: &'static str,
}

/// The result of one scan.
///
/// `Engine::scan` cannot return bare findings: the Appendix G envelope needs
/// the root, the file count and the language set, the summary line needs the
/// same, and per-file tolerance needs somewhere to put what it skipped.
#[derive(Debug, Clone)]
pub struct ScanReport {
    /// The root exactly as the caller gave it — never canonicalised, so
    /// relative invocations stay relative in the output.
    pub root: String,
    pub findings: Vec<Finding>,
    pub files_scanned: usize,
    pub languages: Vec<Lang>,
    pub skipped: Vec<SkippedFile>,
    pub rules: Vec<RuleInfo>,
}

/// One ordering for skips, wherever they came from.
///
/// Discovery skips and per-file skips are produced in different places and
/// merged in [`Engine::scan`]; a second comparator would be a second source of
/// truth for the report's order, and a golden file would eventually catch the
/// two disagreeing.
fn sort_skips(skips: &mut [SkippedFile]) {
    skips.sort_by(|a, b| a.path.cmp(&b.path));
}

/// Orchestrates discovery, parsing, and rule execution over a directory.
pub struct Engine {
    rules: Vec<Box<dyn Rule>>,
}

impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self {
        Self { rules }
    }

    /// Discover every source file under `root` and scan it.
    ///
    /// Fails only if `root` itself cannot be opened (see [`source::discover`]),
    /// which ADR-0005 makes exit 2. Everything below the root degrades to a
    /// skip: a subdirectory that cannot be enumerated, a file that cannot be
    /// read, a file that cannot be parsed. All three land in
    /// [`ScanReport::skipped`] together, because to a reader of the report they
    /// are the same fact — "this part of your tree was not analysed" — and
    /// splitting them by which layer noticed would only invite a consumer to
    /// handle one and miss the others.
    pub fn scan(&self, root: &Path) -> anyhow::Result<ScanReport> {
        let discovery = source::discover(root)?;
        let mut report = self.scan_files(root, &discovery.files);
        report.skipped.extend(discovery.skipped);
        // `scan_files` sorted what it produced; the discovery skips arrive
        // after that, so the merged list is sorted again rather than left in
        // two concatenated runs.
        sort_skips(&mut report.skipped);
        Ok(report)
    }

    /// Scan an explicit file list. Infallible: every per-file failure becomes
    /// a skip. Public so the tolerance contract is testable without having to
    /// manufacture an unreadable file on disk — a fixture that behaves
    /// differently for root and non-root, and so proves nothing in CI.
    pub fn scan_files(&self, root: &Path, files: &[DiscoveredFile]) -> ScanReport {
        let mut findings = Vec::new();
        let mut skipped = Vec::new();
        let mut languages = BTreeSet::new();
        let mut files_scanned = 0usize;

        for file in files {
            let src = match source::read(file) {
                Ok(src) => src,
                Err(e) => {
                    skipped.push(SkippedFile {
                        path: file.path.display().to_string(),
                        reason: format!("could not read: {e}"),
                    });
                    continue;
                }
            };

            let tree = match parse::parse(&src) {
                Ok(tree) => tree,
                Err(e) => {
                    skipped.push(SkippedFile {
                        path: file.path.display().to_string(),
                        reason: format!("could not parse: {e}"),
                    });
                    continue;
                }
            };

            files_scanned += 1;
            languages.insert(src.lang);

            let ctx = RuleCtx {
                path: &src.path,
                lang: src.lang,
                text: &src.text,
                tree: &tree,
            };

            for rule in &self.rules {
                if rule.languages().contains(&src.lang) {
                    findings.extend(
                        rule.check(&ctx)
                            .into_iter()
                            .map(|raw| raw.into_finding(rule.id())),
                    );
                }
            }
        }

        // PLAN M1: collect, then sort by file then byte offset. Without this
        // the order is whatever `ignore`'s walker yielded, which is
        // filesystem order and therefore machine-dependent — every golden
        // file would flap. `rule_id` breaks the remaining ties so two rules
        // firing on the same span still order deterministically.
        findings.sort_by(|a, b| {
            let (x, y) = (a.location(), b.location());
            x.file
                .cmp(&y.file)
                .then(x.byte_start.cmp(&y.byte_start))
                .then_with(|| a.rule_id().cmp(b.rule_id()))
        });
        sort_skips(&mut skipped);

        ScanReport {
            root: root.display().to_string(),
            findings,
            files_scanned,
            languages: languages.into_iter().collect(),
            skipped,
            rules: self
                .rules
                .iter()
                .map(|r| RuleInfo {
                    id: r.id(),
                    description: r.description(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Severity, Tier};

    struct StubRule;

    impl Rule for StubRule {
        fn id(&self) -> &'static str {
            "stub-rule"
        }

        fn description(&self) -> &'static str {
            "a stub rule used only by engine tests"
        }

        fn languages(&self) -> &'static [Lang] {
            &[Lang::Python]
        }

        fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
            vec![RawFinding::new(
                Tier::T0,
                Severity::Warn,
                crate::finding::Location {
                    file: ctx.path.display().to_string(),
                    byte_start: 0,
                    byte_end: 1,
                    line_start: 1,
                    col_start: 1,
                    line_end: 1,
                    col_end: 2,
                },
                "stub finding",
                "stub evidence",
                None,
            )]
        }
    }

    #[test]
    fn scan_stamps_rule_id_from_the_rule_never_from_the_finding() {
        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();

        let engine = Engine::new(vec![Box::new(StubRule)]);
        let report = engine.scan(&dir).unwrap();

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].rule_id(), "stub-rule");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_with_zero_rules_yields_zero_findings() {
        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();

        let engine = Engine::new(Vec::new());
        let report = engine.scan(&dir).unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.files_scanned, 1);
        assert_eq!(report.languages, vec![Lang::Python]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn one_unreadable_file_is_skipped_and_the_scan_still_reports_the_good_one() {
        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.py"), "x = 1\n").unwrap();

        let mut files = source::discover(&dir).unwrap().files;
        assert_eq!(files.len(), 1);

        // A path that does not exist fails `fs::read` for every user, root
        // included — unlike a chmod-000 fixture, which root reads happily and
        // which would therefore pass vacuously in half the world's CI.
        files.push(DiscoveredFile {
            path: dir.join("vanished.py"),
            lang: Lang::Python,
        });

        let engine = Engine::new(vec![Box::new(StubRule)]);
        let report = engine.scan_files(&dir, &files);

        // The scan COMPLETED rather than aborting...
        assert_eq!(report.files_scanned, 1);
        // ...it still reported the good file's finding...
        assert_eq!(report.findings.len(), 1);
        // ...and it NAMED what it did not look at, so a partial scan can
        // never be read as a clean one.
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].path.ends_with("vanished.py"));
        assert!(report.skipped[0].reason.starts_with("could not read"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn findings_are_sorted_by_file_then_byte_offset_whatever_order_files_arrive_in() {
        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();
        std::fs::write(dir.join("b.py"), "y = 2\n").unwrap();

        let mut files = source::discover(&dir).unwrap().files;
        // Force the walker's order to be wrong, which is exactly what a
        // different filesystem would hand us.
        files.sort_by(|p, q| q.path.cmp(&p.path));

        let engine = Engine::new(vec![Box::new(StubRule)]);
        let report = engine.scan_files(&dir, &files);

        let order: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.location().file.as_str())
            .collect();
        let mut expected = order.clone();
        expected.sort_unstable();
        assert_eq!(order, expected, "findings were not sorted by file");
    }

    #[test]
    fn a_directory_the_walker_could_not_enter_is_reported_alongside_the_findings() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = std::env::temp_dir().join(format!("pons-engine-test-{}", nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("good.py"), "x = 1\n").unwrap();

        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            std::fs::read_dir(&locked).is_err(),
            "fixture is readable despite mode 000 — running as root?"
        );

        let engine = Engine::new(vec![Box::new(StubRule)]);
        // `scan`, not `scan_files`: the point is that a discovery-level skip
        // survives the merge into the report rather than being dropped on the
        // floor between the two layers.
        let report = engine.scan(&dir).unwrap();

        assert_eq!(report.files_scanned, 1, "the readable file was not scanned");
        assert_eq!(report.findings.len(), 1, "the scan did not complete");
        assert_eq!(
            report.skipped.len(),
            1,
            "the unreadable directory never reached the report"
        );
        assert!(report.skipped[0].path.ends_with("locked"));
        assert!(report.skipped[0].reason.starts_with("could not enumerate"));

        let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn nanos() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
