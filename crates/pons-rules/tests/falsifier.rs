// SPDX-License-Identifier: MPL-2.0
use pons_core::{Parsed, Source, config::Config};
use std::path::{Path, PathBuf};

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            found.extend(files(&p));
        } else {
            found.push(p);
        }
    }
    found.sort();
    found
}
#[test]
fn every_rule_has_positive_and_negative_falsifiers() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let syntax = pons_rules::registry();
    for meta in pons_rules::metadata() {
        for (group, should_fire) in [("positive", true), ("negative", false)] {
            let root = fixtures.join(&meta.id).join(group);
            assert!(root.is_dir(), "{} needs a {group} corpus", meta.id);
            let cases: Vec<_> = std::fs::read_dir(&root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            assert!(!cases.is_empty(), "{} has an empty {group} corpus", meta.id);
            for case in cases {
                let base = if case.is_dir() {
                    case.clone()
                } else {
                    root.clone()
                };
                let paths = if case.is_dir() {
                    files(&case)
                } else {
                    vec![case.clone()]
                };
                let sources: Vec<_> = paths
                    .iter()
                    .map(|p| {
                        let mut source = Source::read(p, &base).unwrap();
                        if [".py.json", ".ts.json", ".tsx.json"]
                            .iter()
                            .any(|ext| source.relative.ends_with(ext))
                        {
                            let fixture: serde_json::Value =
                                serde_json::from_str(&source.text).unwrap();
                            source.text = fixture["source"].as_str().unwrap().to_owned();
                            source.relative.truncate(source.relative.len() - 5);
                            source.path = source.path.with_extension("");
                            source.lang = source
                                .path
                                .extension()
                                .and_then(|e| e.to_str())
                                .and_then(pons_core::Lang::from_extension);
                            assert_eq!(fixture["language"].as_str(), source.lang.map(|l| l.name()));
                        }
                        source
                    })
                    .collect();
                let config = if base.join("pons.toml").is_file() {
                    Config::load(&base.join("pons.toml")).unwrap()
                } else {
                    Config::default()
                };
                let mut findings =
                    pons_rules::repository::check(&sources, config.i18n.as_ref()).unwrap();
                findings.extend(
                    pons_rules::documents::check(&sources, config.identity.as_ref()).unwrap(),
                );
                findings.extend(pons_rules::assets::check(&sources, &config.asset).unwrap());
                for source in &sources {
                    if let Some(lang) = source.lang {
                        let parsed = Parsed::new(lang, &source.text)
                            .unwrap_or_else(|e| panic!("{}: {e}", source.path.display()));
                        for rule in &syntax {
                            findings.extend(rule.check(source, &parsed));
                        }
                        findings.extend(
                            pons_rules::budget::check(source, &parsed, &config.budget).unwrap(),
                        );
                    }
                }
                let hits: Vec<_> = findings.iter().filter(|f| f.rule_id == meta.id).collect();
                assert_eq!(
                    !hits.is_empty(),
                    should_fire,
                    "{} {group} case {}: {hits:?}",
                    meta.id,
                    case.display()
                );
            }
        }
    }
}
