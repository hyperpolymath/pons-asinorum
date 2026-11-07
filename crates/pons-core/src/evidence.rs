// SPDX-License-Identifier: MPL-2.0
//! Versioned exchange contract. Producer correlation is explicit: agreement
//! between AST heuristics is never multiplied into independent proof.
use crate::{
    engine::ScanReport,
    finding::{EvidenceClass, Tier},
};
use serde::{Deserialize, Serialize};
pub(crate) const ASSUMPTION: &str = "Only registered rule semantics and declared parser/model coverage; no runtime safety certification";

pub(crate) fn relative_path(file: &str, root: &str) -> String {
    relative_path_details(file, root).0
}

pub(crate) fn relative_path_details(file: &str, root: &str) -> (String, bool) {
    let windows = root.as_bytes().get(1) == Some(&b':')
        || root.starts_with("\\\\")
        || (cfg!(windows) && !root.starts_with('/'));
    let file = if windows {
        file.replace('\\', "/")
    } else {
        file.into()
    };
    let root = if windows {
        root.replace('\\', "/")
    } else {
        root.into()
    };
    // A file scan has root == file. Do not consult the validator's filesystem:
    // the original checkout may no longer exist where evidence is consumed.
    let base = if file == root {
        root.rsplit_once('/').map_or("", |(parent, _)| parent)
    } else {
        root.trim_end_matches('/')
    };
    let prefix = format!("{base}/");
    let relative = if base.is_empty() && !file.starts_with('/') {
        Some(file.as_str())
    } else {
        file.strip_prefix(&prefix)
    };
    (
        relative.unwrap_or(&file).replace('\\', "/"),
        relative.is_some(),
    )
}

#[cfg(test)]
mod portable_paths {
    #[test]
    fn windows_reports_have_the_same_keys_on_every_reader_host() {
        assert_eq!(
            super::relative_path(r"C:\repo\sample.php", r"C:\repo"),
            "sample.php"
        );
        assert_eq!(
            super::relative_path(r"C:\repo\sample.php", r"C:\repo\sample.php"),
            "sample.php"
        );
        assert_eq!(
            super::relative_path(r"C:\outside\sample.php", r"C:\repo"),
            "C:/outside/sample.php"
        );
    }
    #[test]
    fn posix_filename_boundaries_survive_single_file_scans() {
        for file in [r"/tmp/a\b.py", "/tmp/a:b.py"] {
            assert_eq!(
                super::relative_path(file, file),
                super::relative_path(file, "/tmp")
            );
            assert!(super::relative_path_details(file, "/tmp").1);
        }
        assert_eq!(
            super::relative_path(r"/tmp/a\b.py", r"/tmp/a\b.py"),
            "a/b.py"
        );
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub commit: Option<String>,
    pub dirty: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub fingerprint: String,
    pub producer: String,
    pub correlation_group: String,
    pub rule: String,
    pub tier: Tier,
    pub evidence: EvidenceClass,
    pub confidence: String,
    pub path: String,
    pub line: usize,
    pub trace: String,
    pub assumptions: Vec<String>,
    pub unknown_edges: Vec<String>,
    pub counterconditions: Vec<String>,
}
#[derive(Serialize)]
pub struct Contract<'a> {
    pub schema_version: &'static str,
    pub target: &'a Target,
    pub execution_outcome: &'static str,
    pub parser_inventory: &'a [crate::coverage::InventoryEntry],
    pub records: Vec<Record>,
}
/// Root-relative fingerprint keys remain stable when a checkout moves. Source
/// anchors are supplied by the engine from the bytes actually analysed.
pub fn contract(report: &ScanReport) -> Contract<'_> {
    let records = report
        .findings
        .iter()
        .map(|f| {
            let relative = relative_path(&f.location().file, &report.root);
            let key = format!(
                "{}:{}:{}:{}",
                f.rule_id(),
                f.location().file,
                f.location().byte_start,
                f.location().byte_end
            );
            let anchor = report.anchors.get(&key).cloned().unwrap_or_else(|| {
                format!(
                    "line:{}:column:{}",
                    f.location().line_start,
                    f.location().col_start
                )
            });
            let fingerprint = blake3::hash(
                format!(
                    "pons-fingerprint-v1\0{}\0{}\0{}",
                    f.rule_id(),
                    relative,
                    anchor
                )
                .as_bytes(),
            )
            .to_hex()
            .to_string();
            let unknown = report
                .inventory
                .iter()
                .find(|i| i.path == f.location().file)
                .map(|i| i.analysis_notes.clone())
                .unwrap_or_default();
            Record {
                fingerprint,
                producer: "pons".into(),
                correlation_group: format!("pons:{:?}", f.tier()),
                rule: f.rule_id().into(),
                tier: f.tier(),
                evidence: f.evidence(),
                // One table, in verdict.rs, because the same label is
                // published to consumers in schemas/verdict-matrix-1.0.0.json.
                confidence: crate::verdict::confidence(f.evidence()).into(),
                path: relative,
                line: f.location().line_start,
                trace: f.evidence_note().into(),
                assumptions: vec![ASSUMPTION.into()],
                unknown_edges: unknown,
                counterconditions: f
                    .counter_condition()
                    .map(|s| vec![s.into()])
                    .unwrap_or_default(),
            }
        })
        .collect();
    Contract {
        schema_version: "1.0.0",
        target: &report.target,
        execution_outcome: report.coverage_status(),
        parser_inventory: &report.inventory,
        records,
    }
}
/// Validate exchange records before consuming them. Never silently promote or
/// repair contradictory tier/evidence claims from a producer.
pub fn validate_records(records: &[Record]) -> anyhow::Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    for r in records {
        anyhow::ensure!(
            r.evidence == EvidenceClass::from(r.tier),
            "tier/evidence mismatch"
        );
        anyhow::ensure!(
            r.fingerprint.len() == 64 && r.fingerprint.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid fingerprint"
        );
        anyhow::ensure!(
            ids.insert((&r.producer, &r.fingerprint)),
            "duplicate producer fingerprint"
        );
        anyhow::ensure!(
            !r.producer.trim().is_empty()
                && !r.rule.trim().is_empty()
                && !r.path.is_empty()
                && r.line > 0
                && !r.trace.is_empty()
                && !r.assumptions.is_empty(),
            "incomplete evidence record"
        );
    }
    Ok(())
}
