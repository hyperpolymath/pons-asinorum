// SPDX-License-Identifier: MPL-2.0
//! The consumer-contract tests: what a pinned verdict grid relies on.
//!
//! The library tests prove the envelope's shape. These prove the three things a
//! *consumer* depends on that a shape test cannot see:
//!
//! * a report pinned at the frozen 0.4.0 version still validates, and the
//!   version actually selects the schema, so a 0.4.0 declaration cannot carry
//!   0.5.0 keys;
//! * an unknown version is refused rather than validated against the nearest
//!   shape;
//! * the corroboration key is recomputed rather than trusted, and the demoted
//!   count cannot be inflated.
//!
//! Each of these is checked through the binary — `pons validate-report` — not
//! through the library, because the binary is what a consumer runs.

use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};

/// A source directory and a separate output directory.
///
/// Separate on purpose: writing the report inside the scanned tree would make
/// it part of the next scan's inventory, and `validate-report` would then be
/// validating a document that describes a tree containing itself.
struct Fixture {
    src: PathBuf,
    out: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "pons-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src = base.join("src");
        let out = base.join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&out).unwrap();
        // One file that fires a registered rule, so the report carries at least
        // one finding to point the fingerprint tests at.
        fs::write(
            src.join("sample.php"),
            "<?php function render() { echo $_GET['x']; }\n",
        )
        .unwrap();
        Self { src, out }
    }

    fn scan(&self) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_pons"))
            .arg("scan")
            .arg(&self.src)
            .args(["--format", "json"])
            .output()
            .expect("pons scan runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "the fixture scan must be clean enough to produce a valid report: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).expect("scan emits JSON");
        assert!(
            !report["findings"]
                .as_array()
                .expect("findings array")
                .is_empty(),
            "the fixture must fire at least one rule; a report with no finding \
             would make the fingerprint assertions vacuous"
        );
        report
    }

    fn validate(&self, report: &Value) -> std::process::Output {
        let path = self.out.join("report.json");
        fs::write(&path, serde_json::to_string_pretty(report).unwrap()).unwrap();
        Command::new(env!("CARGO_BIN_EXE_pons"))
            .arg("validate-report")
            .arg(&path)
            .output()
            .expect("pons validate-report runs")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(base) = self.src.parent() {
            let _ = fs::remove_dir_all(base);
        }
    }
}

/// Everything 0.5.0 added, removed again — the projection a pinned 0.4.0
/// consumer holds.
fn as_0_4_0(report: &Value) -> Value {
    let mut pinned = report.clone();
    pinned["schema_version"] = json!("0.4.0");
    let counts = pinned["counts"].as_object_mut().expect("counts object");
    counts.remove("actionable");
    counts.remove("demoted_speculative");
    for finding in pinned["findings"].as_array_mut().expect("findings array") {
        finding
            .as_object_mut()
            .expect("finding object")
            .remove("semantic_fingerprint");
    }
    pinned
}

#[test]
fn a_report_pinned_at_the_frozen_0_4_0_version_still_validates() {
    let fixture = Fixture::new();
    let current = fixture.scan();
    assert_eq!(current["schema_version"], "0.5.0");

    let accepted = fixture.validate(&current);
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "this build's own report must validate: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );

    let accepted = fixture.validate(&as_0_4_0(&current));
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "a consumer pinned to 0.4.0 must still be able to validate the reports \
         it holds; the freeze would be worthless otherwise: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
}

#[test]
fn a_0_4_0_declaration_cannot_carry_a_0_5_0_key() {
    let fixture = Fixture::new();
    let current = fixture.scan();

    let mut mixed = as_0_4_0(&current);
    // Put one 0.5.0 key back, leaving the 0.4.0 declaration in place. If the
    // validator simply used the current schema whatever the document said, this
    // document would pass and the version would mean nothing.
    let key = current["findings"][0]["semantic_fingerprint"].clone();
    mixed["findings"][0]["semantic_fingerprint"] = key;
    let rejected = fixture.validate(&mixed);
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "a 0.4.0 document carrying a 0.5.0 key must be rejected: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
}

#[test]
fn an_unknown_envelope_version_is_refused() {
    let fixture = Fixture::new();
    let mut report = fixture.scan();
    report["schema_version"] = json!("0.6.0");
    let rejected = fixture.validate(&report);
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "an unknown version must not be validated against the nearest shape: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
}

#[test]
fn a_forged_corroboration_key_is_rejected() {
    let fixture = Fixture::new();
    let mut report = fixture.scan();
    let key = report["findings"][0]["semantic_fingerprint"]
        .as_str()
        .expect("the report carries a key")
        .to_string();
    // Flip one hex digit. A validator that only checked the key's shape would
    // pass this — and then a producer could point any finding at any site,
    // which is exactly what the grid deduplicates on.
    let flipped = format!(
        "{}{}{}",
        &key[..1],
        if &key[1..2] == "0" { "1" } else { "0" },
        &key[2..]
    );
    assert_ne!(flipped, key);
    report["findings"][0]["semantic_fingerprint"] = json!(flipped);

    let rejected = fixture.validate(&report);
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "a corroboration key that does not match its own location must be \
         rejected: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
}

#[test]
fn a_speculative_finding_cannot_be_promoted_into_the_actionable_count() {
    let fixture = Fixture::new();
    let mut report = fixture.scan();
    let counts = &report["counts"];
    assert_eq!(
        counts["actionable"].as_u64().unwrap() + counts["demoted_speculative"].as_u64().unwrap(),
        counts["total"].as_u64().unwrap(),
        "the two counts must account for every finding exactly once"
    );
    assert_eq!(
        counts["demoted_speculative"],
        counts["by_evidence"]["SPECULATIVE"]
    );

    // The demotion is a number, so it is enforced as one: adding a demoted
    // finding back into the actionable total is a rejected document, not a
    // judgement call for the reader.
    let inflated = counts["actionable"].as_u64().unwrap() + 1;
    report["counts"]["actionable"] = json!(inflated);
    let rejected = fixture.validate(&report);
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "a report that counts a demoted finding as actionable must be \
         rejected: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
}

#[test]
fn a_finding_outside_the_scan_root_is_rejected() {
    let fixture = Fixture::new();
    let mut report = fixture.scan();
    let original = report["findings"][0]["location"]["file"]
        .as_str()
        .expect("the finding names a file")
        .to_string();
    let outside = "/elsewhere/outside.php";

    // Move the finding AND its inventory entry together, so the only thing
    // wrong with the document is that the path cannot be expressed relative to
    // the scan root. Without the inventory move it would be rejected earlier,
    // for a different reason, and this test would pass without exercising the
    // check it is named for.
    report["findings"][0]["location"]["file"] = json!(outside);
    for entry in report["scanned"]["inventory"]
        .as_array_mut()
        .expect("inventory array")
    {
        if entry["path"] == json!(original) {
            entry["path"] = json!(outside);
        }
    }

    let rejected = fixture.validate(&report);
    assert_eq!(
        rejected.status.code(),
        Some(2),
        "a finding outside the scan root must be rejected: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("not inside the scan root"),
        "it must be rejected BY the root check, not incidentally by another: {}",
        String::from_utf8_lossy(&rejected.stderr)
    );
}
