// SPDX-License-Identifier: MPL-2.0
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "pons-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn scan(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_pons"))
            .arg("scan")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn no_input_is_not_success_even_without_strict_flag() {
    let t = Temp::new();
    for format in ["human", "json", "sarif"] {
        let out = t.scan(&["--format", format]);
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stdout).contains("no-applicable-input"));
    }
}
#[test]
fn inventory_accounts_for_supported_unsupported_excluded_and_parse_error() {
    let t = Temp::new();
    for (name, text) in [
        ("good.py", "x = 1"),
        ("bad.py", "def (:"),
        ("app.hs", "main = pure ()"),
        ("config.json", "{}"),
        ("notes.txt", "words"),
        ("ignored.py", "x = 2"),
        (".ponsignore", "ignored.py\n"),
    ] {
        fs::write(t.0.join(name), text).unwrap();
    }
    let out = t.scan(&["--format", "json", "--fail-on-skips"]);
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "0.4.0");
    let inv = v["scanned"]["inventory"].as_array().unwrap();
    assert_eq!(inv.len(), 7);
    for (name, status) in [
        ("good.py", "analysed"),
        ("bad.py", "parse-error"),
        ("app.hs", "unsupported"),
        ("config.json", "unsupported"),
        ("notes.txt", "excluded-by-policy"),
        ("ignored.py", "excluded-by-policy"),
    ] {
        assert_eq!(
            inv.iter()
                .find(|e| e["path"].as_str().unwrap().ends_with(name))
                .unwrap()["outcome"],
            status
        );
    }
    assert_eq!(v["scanned"]["coverage_status"], "partial");
}
#[test]
fn absent_required_language_cannot_be_suppressed() {
    let t = Temp::new();
    fs::write(t.0.join("ok.py"), "x = 1").unwrap();
    let out = t.scan(&["--format", "json", "--require-language", "haskell"]);
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["scanned"]["coverage_failures"].as_array().unwrap().len(),
        1
    );
}
#[test]
fn generated_report_validates_and_malformed_claims_fail_closed() {
    let t = Temp::new();
    fs::write(
        t.0.join("sample.php"),
        "<?php function render() { echo $_GET['x']; }",
    )
    .unwrap();
    let out = t.scan(&["--format", "json"]);
    assert_eq!(out.status.code(), Some(0));
    let original = String::from_utf8(out.stdout).unwrap();
    assert!(pons_core::report::validate::validate(&original).is_ok());
    let v: Value = serde_json::from_str(&original).unwrap();
    for (path, bad) in [
        ("/scanned/files", serde_json::json!(0)),
        ("/findings/0/evidence", serde_json::json!("PROTOCOL")),
        ("/counts/total", serde_json::json!(999)),
        ("/schema_version", serde_json::json!("0.3.0")),
        (
            "/evidence_contract/records/0/fingerprint",
            serde_json::json!("fake"),
        ),
    ] {
        let mut m = v.clone();
        *m.pointer_mut(path).unwrap() = bad;
        assert!(
            pons_core::report::validate::validate(&m.to_string()).is_err(),
            "accepted {path}"
        );
    }
    assert!(pons_core::report::validate::validate("{}").is_err());
    assert!(pons_core::report::validate::validate("not json").is_err());
}
#[test]
fn fingerprints_survive_checkout_move_and_blank_line_insertion() {
    let a = Temp::new();
    let b = Temp::new();
    fs::write(
        a.0.join("x.php"),
        "<?php function render() { echo $_GET['x']; }",
    )
    .unwrap();
    fs::write(
        b.0.join("x.php"),
        "<?php\n\nfunction render() { echo $_GET['x']; }",
    )
    .unwrap();
    let av: Value = serde_json::from_slice(&a.scan(&["--format", "json"]).stdout).unwrap();
    let bv: Value = serde_json::from_slice(&b.scan(&["--format", "json"]).stdout).unwrap();
    assert_eq!(
        av["evidence_contract"]["records"][0]["fingerprint"],
        bv["evidence_contract"]["records"][0]["fingerprint"]
    );
}
#[test]
fn suppression_metadata_is_required_and_never_hides_coverage() {
    let t = Temp::new();
    fs::write(t.0.join("a.py"), "x = x\n").unwrap();
    fs::write(t.0.join("b.ex"), "defmodule B do end\n").unwrap();
    fs::write(
        t.0.join("pons.toml"),
        r#"[[suppress.allow]]
rule="self-assignment"
path="a.py"
reason="fixture"
owner="test"
expires=4102444800
"#,
    )
    .unwrap();
    let out = t.scan(&["--format", "json", "--fail-on-skips"]);
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["scanned"]["suppressed_findings"], 1);
    assert!(v["scanned"]["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["path"].as_str().unwrap().ends_with("b.ex")));
    for config in ["[suppress]\npaths=['**']", "[[suppress.allow]]\nrule='self-assignment'\npath='a.py'\nreason='x'\nowner='test'\nexpires=1"] {
        fs::write(t.0.join("pons.toml"),config).unwrap();assert_eq!(t.scan(&[]).status.code(),Some(2));
    }
}
#[test]
fn dir_include_resolution_agrees_for_relative_and_absolute_scan_roots() {
    let t = Temp::new();
    fs::write(
        t.0.join("load.php"),
        "<?php require __DIR__ . '/ready.php';",
    )
    .unwrap();
    fs::write(
        t.0.join("ready.php"),
        "<?php function ready() { return true; }",
    )
    .unwrap();
    let absolute = t.scan(&["--format", "json", "--rule", "php-unresolved-include"]);
    let relative = Command::new(env!("CARGO_BIN_EXE_pons"))
        .current_dir(&t.0)
        .args([
            "scan",
            ".",
            "--format",
            "json",
            "--rule",
            "php-unresolved-include",
        ])
        .output()
        .unwrap();
    for out in [absolute, relative] {
        assert_eq!(out.status.code(), Some(0));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["findings"].as_array().unwrap().len(), 0);
    }
}
#[test]
fn forced_parser_failure_preserves_language_version_and_artifact_identity() {
    let t = Temp::new();
    let file = t.0.join("extensionless");
    fs::write(&file, "<?php function (").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pons"))
        .arg("scan")
        .arg(&file)
        .args(["--lang", "php", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let entry = &v["scanned"]["inventory"][0];
    assert_eq!(entry["language"], "php");
    assert_eq!(entry["outcome"], "parse-error");
    assert!(entry["parser"].as_str().unwrap().contains("0.25.1"));
    assert!(entry["artifact_digest"]
        .as_str()
        .unwrap()
        .starts_with("blake3:"));
}
