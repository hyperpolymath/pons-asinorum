// SPDX-License-Identifier: MPL-2.0

//! The M2 CI gate: every registered rule must be falsifiable against its own
//! fixture corpus, per `docs/PLAN.adoc` Appendix C. `just falsify` runs this.

use std::fs;
use std::path::{Path, PathBuf};

use pons_core::engine::{Rule, RuleCtx};
use pons_core::lang::Lang;
use pons_core::parse;
use pons_core::source::SourceFile;
use pons_rules::registry::RuleRegistry;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// A fixture file resolved to its language and parsed tree, checked for
/// tree-sitter error recovery — a broken fixture must fail loudly rather
/// than vacuously pass by firing nothing.
struct Fixture {
    path: PathBuf,
    lang: Lang,
}

fn load_fixtures(dir: &Path, rule_id: &str, side: &str) -> Vec<Fixture> {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{rule_id}: cannot read {side} fixture dir {dir:?}: {e}"));

    let mut fixtures = Vec::new();
    for entry in entries {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        let path = entry.path();
        let lang = path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Lang::from_extension)
            .unwrap_or_else(|| {
                panic!(
                    "{rule_id}: {side} fixture {path:?} doesn't resolve to a known \
                     language — rename or remove it"
                )
            });
        fixtures.push(Fixture { path, lang });
    }
    fixtures
}

fn check_fixture(rule: &dyn Rule, fixture: &Fixture) -> Vec<pons_core::finding::RawFinding> {
    let text = fs::read_to_string(&fixture.path).unwrap();
    let source = SourceFile {
        path: fixture.path.clone(),
        lang: fixture.lang,
        text,
    };
    let tree = parse::parse(&source).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "{}: fixture {:?} has a tree-sitter parse error — fix the fixture, \
         it cannot exercise the rule while broken",
        rule.id(),
        fixture.path,
    );

    let ctx = RuleCtx {
        path: &source.path,
        lang: source.lang,
        text: &source.text,
        tree: &tree,
    };
    rule.check(&ctx)
}

#[test]
fn every_rule_is_proven_by_its_own_fixture_corpus() {
    for rule in RuleRegistry::all() {
        let rule_id = rule.id();
        let rule_dir = fixtures_root().join(rule_id);
        assert!(
            rule_dir.is_dir(),
            "{rule_id}: no fixture directory at {rule_dir:?}"
        );

        let positive = load_fixtures(&rule_dir.join("positive"), rule_id, "positive");
        let negative = load_fixtures(&rule_dir.join("negative"), rule_id, "negative");

        assert!(
            !negative.is_empty(),
            "{rule_id}: empty or missing negative corpus — a rule with no \
             negative fixtures cannot be falsified"
        );
        assert!(
            !positive.is_empty(),
            "{rule_id}: empty or missing positive corpus"
        );

        for fixture in &positive {
            let findings = check_fixture(rule.as_ref(), fixture);
            assert!(
                !findings.is_empty(),
                "{rule_id}: positive fixture {:?} produced no findings",
                fixture.path
            );
        }

        for fixture in &negative {
            let findings = check_fixture(rule.as_ref(), fixture);
            assert!(
                findings.is_empty(),
                "{rule_id}: negative fixture {:?} produced {} finding(s), expected 0",
                fixture.path,
                findings.len()
            );
        }

        for &lang in rule.languages() {
            assert!(
                positive.iter().any(|f| f.lang == lang),
                "{rule_id}: declares {lang:?} in languages() but has no positive \
                 fixture in that language"
            );
            assert!(
                negative.iter().any(|f| f.lang == lang),
                "{rule_id}: declares {lang:?} in languages() but has no negative \
                 fixture in that language"
            );
        }
    }
}
