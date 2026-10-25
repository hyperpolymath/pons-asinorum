// SPDX-License-Identifier: MPL-2.0
//! Fail-closed validation for the 0.4.0 scan exchange envelope.
use crate::{
    coverage::{InventoryEntry, Outcome},
    evidence::Record,
    finding::{EvidenceClass, Tier},
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key).with_context(|| format!("missing {key}"))
}
pub fn validate(text: &str) -> Result<String> {
    let v: Value = serde_json::from_str(text)?;
    validate_value(v)
}

/// Consume the JSON byte stream without a fixed input-size ceiling or a second
/// complete text buffer. Schema/semantic checks still materialise the JSON value;
/// callers handling untrusted input must provide process memory/time limits.
pub fn validate_reader(reader: impl std::io::Read) -> Result<String> {
    validate_value(serde_json::from_reader(reader)?)
}

fn validate_value(v: Value) -> Result<String> {
    // Structural validation precedes semantic checks: hand-picked fields cannot
    // enforce required properties, enums or the closed envelope.
    static SCHEMA: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    let validator = SCHEMA.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(include_str!("../../../../schemas/scan-0.4.0.json"))
                .expect("bundled scan schema must be JSON");
        jsonschema::validator_for(&schema).expect("bundled scan schema must compile")
    });
    validator
        .validate(&v)
        .map_err(|e| anyhow::anyhow!("report schema: {e} at {}", e.instance_path()))?;
    ensure!(
        v["schema_version"] == super::json::SCHEMA_VERSION,
        "unsupported report schema"
    );
    let tool = required(&v, "tool")?;
    for k in ["name", "version", "source_commit", "source_dirty"] {
        ensure!(
            required(tool, k)?.as_str().is_some_and(|s| !s.is_empty()),
            "invalid tool {k}"
        );
    }
    let execution = required(&v, "execution")?;
    ensure!(
        execution["status"] == "completed",
        "invalid execution status"
    );
    let code = required(execution, "exit_code")?;
    ensure!(
        code.is_null() || code.as_u64().is_some_and(|n| n <= 2),
        "invalid execution exit code"
    );
    let s = required(&v, "scanned")?;
    let inventory: Vec<InventoryEntry> = serde_json::from_value(required(s, "inventory")?.clone())?;
    let mut paths = std::collections::BTreeSet::new();
    for e in &inventory {
        ensure!(
            !e.path.is_empty() && !e.reason.is_empty() && paths.insert(&e.path),
            "invalid/duplicate inventory path"
        );
        if e.outcome == Outcome::Analysed {
            ensure!(
                e.parser.is_some()
                    && e.artifact_digest
                        .as_ref()
                        .is_some_and(|s| s.starts_with("blake3:") && s.len() == 71),
                "analysed input lacks parser/digest"
            );
        }
    }
    let analysed = inventory
        .iter()
        .filter(|i| i.outcome == Outcome::Analysed)
        .count();
    ensure!(
        s["files"].as_u64() == Some(analysed as u64),
        "inventory/file count mismatch"
    );
    let skipped = required(s, "skipped")?
        .as_array()
        .context("skipped must be an array")?;
    let failures = required(s, "coverage_failures")?
        .as_array()
        .context("coverage_failures must be an array")?;
    ensure!(
        failures
            .iter()
            .all(|v| v.as_str().is_some_and(|s| !s.is_empty())),
        "invalid coverage failure"
    );
    for e in &inventory {
        if !matches!(e.outcome, Outcome::Analysed | Outcome::ExcludedByPolicy) {
            ensure!(
                skipped
                    .iter()
                    .any(|v| v["path"] == e.path
                        && v["reason"].as_str().is_some_and(|s| !s.is_empty())),
                "missing skipped outcome"
            );
        }
    }
    let status = if analysed == 0 {
        "no-applicable-input"
    } else if !skipped.is_empty()
        || !failures.is_empty()
        || inventory
            .iter()
            .any(|i| i.outcome == Outcome::Analysed && !i.analysis_complete)
    {
        "partial"
    } else {
        "complete-within-policy"
    };
    ensure!(
        s["coverage_status"] == status,
        "coverage status contradicts inventory"
    );
    if analysed == 0 || !failures.is_empty() {
        ensure!(
            code.is_null() || code.as_u64() == Some(2),
            "coverage failure with successful exit"
        );
    }
    let findings = required(&v, "findings")?
        .as_array()
        .context("findings must be an array")?;
    ensure!(
        v["counts"]["total"].as_u64() == Some(findings.len() as u64),
        "finding count mismatch"
    );
    for f in findings {
        let tier: Tier = serde_json::from_value(required(f, "tier")?.clone())?;
        let evidence: EvidenceClass = serde_json::from_value(required(f, "evidence")?.clone())?;
        ensure!(
            evidence == EvidenceClass::from(tier),
            "finding tier/evidence mismatch"
        );
        let location: &Value = required(f, "location")?;
        ensure!(
            location["line_start"].as_u64().is_some_and(|n| n > 0)
                && location["col_start"].as_u64().is_some_and(|n| n > 0),
            "invalid finding location"
        );
        ensure!(
            location["byte_end"]
                .as_u64()
                .zip(location["byte_start"].as_u64())
                .is_some_and(|(a, b)| a >= b),
            "invalid byte range"
        );
        ensure!(
            inventory
                .iter()
                .any(|e| e.outcome == Outcome::Analysed && location["file"] == e.path),
            "finding outside analysed inventory"
        );
    }
    for class in ["HEURISTIC", "DATAFLOW", "PROTOCOL", "SPECULATIVE"] {
        ensure!(
            v["counts"]["by_evidence"][class].as_u64()
                == Some(findings.iter().filter(|f| f["evidence"] == class).count() as u64),
            "evidence count mismatch"
        );
    }
    let contract = required(&v, "evidence_contract")?;
    ensure!(
        contract["schema_version"] == "1.0.0" && contract["execution_outcome"] == status,
        "invalid evidence contract"
    );
    let records: Vec<Record> = serde_json::from_value(required(contract, "records")?.clone())?;
    crate::evidence::validate_records(&records)?;
    ensure!(records.len() == findings.len(), "record count mismatch");
    ensure!(
        contract["parser_inventory"] == s["inventory"],
        "contract coverage differs from scan coverage"
    );
    for (r, f) in records.iter().zip(findings) {
        let file = f["location"]["file"]
            .as_str()
            .context("missing finding file")?;
        let root = s["root"].as_str().context("missing scan root")?;
        let notes = &inventory
            .iter()
            .find(|e| e.path == file)
            .context("finding outside inventory")?
            .analysis_notes;
        let counterconditions: Vec<_> = f["counter_condition"].as_str().into_iter().collect();
        ensure!(
            r.rule == f["rule_id"]
                && serde_json::to_value(r.tier)? == f["tier"]
                && serde_json::to_value(r.evidence)? == f["evidence"]
                && r.path == crate::evidence::relative_path(file, root)
                && Some(r.line as u64) == f["location"]["line_start"].as_u64()
                && r.trace == f["evidence_note"]
                && r.counterconditions == counterconditions
                && &r.unknown_edges == notes
                && r.assumptions == [crate::evidence::ASSUMPTION],
            "record/finding mismatch"
        );
    }
    Ok(status.into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn published_json_schema_accepts_native_report_and_rejects_tier_promotion() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../../schemas/scan-0.4.0.json")).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let text = crate::report::json::render(&crate::report::fixture::report()).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(validator.is_valid(&value));
        value["findings"][0]["evidence"] = serde_json::json!("PROTOCOL");
        assert!(!validator.is_valid(&value));
    }
}
