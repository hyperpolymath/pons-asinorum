// SPDX-License-Identifier: MPL-2.0
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Corpus(PathBuf);
impl Corpus {
    fn new(files: &[(&str, &str)]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pons-coverage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        for (name, text) in files {
            std::fs::write(path.join(name), text).unwrap();
        }
        Self(path)
    }
    fn scan(&self, extra: &[&str]) -> (i32, serde_json::Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_pons"))
            .args(["scan", self.0.to_str().unwrap(), "--format", "json"])
            .args(extra)
            .output()
            .unwrap();
        let value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stderr)));
        (output.status.code().unwrap(), value)
    }
}
impl Drop for Corpus {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unsupported_source_is_never_a_clean_scan() {
    let corpus = Corpus::new(&[
        ("Main.hs", "module Main where\n"),
        ("gateway.ex", "defmodule Gateway do\nend\n"),
    ]);
    let (code, json) = corpus.scan(&["--fail-on-skips"]);
    assert_eq!(code, 2);
    assert_eq!(json["scanned"]["files"], 0);
    assert_eq!(json["scanned"]["coverage"]["status"], "failed");
    assert_eq!(json["scanned"]["coverage"]["candidate_files"], 2);
    assert_eq!(json["scanned"]["skipped"].as_array().unwrap().len(), 2);
    assert_eq!(corpus.scan(&[]).0, 0); // exploratory mode preserves partial output
}

#[test]
fn empty_corpus_is_explicit_and_strict_mode_fails() {
    let corpus = Corpus::new(&[("README.md", "Documentation")]);
    let (code, json) = corpus.scan(&["--fail-on-skips"]);
    assert_eq!(code, 2);
    assert_eq!(json["scanned"]["coverage"]["status"], "no_applicable_input");
    assert_eq!(
        json["scanned"]["coverage"]["inputs"][0]["status"],
        "excluded"
    );
}

#[test]
fn mixed_corpus_accounts_for_every_enumerated_input() {
    let corpus = Corpus::new(&[
        ("good.php", "<?php function good() { return 1; }"),
        ("stub.php", "<?php function encrypt($data) { /* TODO */ }"),
        ("bad.php", "<?php function broken( {"),
        ("Main.hs", "module Main where\n"),
        ("README.md", "Documentation"),
    ]);
    let (code, json) = corpus.scan(&["--fail-on-skips"]);
    assert_eq!(code, 2);
    assert_eq!(json["schema_version"], "0.4.0");
    let coverage = &json["scanned"]["coverage"];
    assert_eq!(coverage["status"], "partial");
    assert_eq!(coverage["candidate_files"], 4);
    assert_eq!(coverage["analyzed_files"], 2);
    assert_eq!(coverage["inputs"].as_array().unwrap().len(), 5);
    assert!(coverage["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["status"] == "parse_error"));
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["rule_id"] == "empty-php-function" && f["evidence"] == "HEURISTIC"));
}

#[test]
fn selecting_no_applicable_rules_is_an_explicit_gap() {
    let corpus = Corpus::new(&[("good.php", "<?php function good() { return 1; }")]);
    let (code, json) = corpus.scan(&["--rule", "dead-store", "--fail-on-skips"]);
    assert_eq!(code, 2);
    assert_eq!(
        json["scanned"]["coverage"]["inputs"][0]["status"],
        "no_rules"
    );
}

#[test]
fn sarif_carries_the_same_coverage_state() {
    let corpus = Corpus::new(&[("Main.hs", "module Main where\n")]);
    let output = Command::new(env!("CARGO_BIN_EXE_pons"))
        .args([
            "scan",
            corpus.0.to_str().unwrap(),
            "--format",
            "sarif",
            "--fail-on-skips",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["runs"][0]["properties"]["coverage"]["status"],
        "failed"
    );
}

#[test]
fn oversize_source_is_a_visible_incomplete_scan() {
    let corpus = Corpus::new(&[]);
    std::fs::File::create(corpus.0.join("large.php"))
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    let (code, json) = corpus.scan(&["--fail-on-skips"]);
    assert_eq!(code, 2);
    assert_eq!(json["scanned"]["files"], 0);
    assert!(json["scanned"]["skipped"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("16 MiB"));
}
