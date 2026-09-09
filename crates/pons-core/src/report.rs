// SPDX-License-Identifier: MPL-2.0
use crate::{EvidenceClass, Finding, RuleMeta, Severity};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub message: String,
}
#[derive(Debug, Serialize)]
pub struct Scanned {
    pub root: String,
    pub files: usize,
    pub parsed_files: usize,
    pub skipped_files: usize,
    pub suppressed_findings: usize,
    pub languages: BTreeSet<String>,
    pub complete: bool,
}
#[derive(Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub tool: Tool,
    pub scanned: Scanned,
    pub counts: Counts,
    pub findings: Vec<Finding>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub version: &'static str,
}
#[derive(Serialize)]
pub struct Counts {
    pub by_evidence: BTreeMap<&'static str, usize>,
    pub total: usize,
}
impl Report {
    pub fn new(scanned: Scanned, mut findings: Vec<Finding>, diagnostics: Vec<Diagnostic>) -> Self {
        findings.sort_by(|a, b| {
            (&a.location.file, a.location.byte_start, &a.rule_id).cmp(&(
                &b.location.file,
                b.location.byte_start,
                &b.rule_id,
            ))
        });
        let mut by_evidence = BTreeMap::from([
            ("HEURISTIC", 0),
            ("DATAFLOW", 0),
            ("PROTOCOL", 0),
            ("SPECULATIVE", 0),
        ]);
        for f in &findings {
            *by_evidence.entry(f.evidence.label()).or_default() += 1;
        }
        Self {
            schema_version: "0.1.0",
            tool: Tool {
                name: "pons",
                version: env!("CARGO_PKG_VERSION"),
            },
            counts: Counts {
                by_evidence,
                total: findings.len(),
            },
            scanned,
            findings,
            diagnostics,
        }
    }
    pub fn human(&self, explain: bool) -> String {
        let mut result = String::new();
        for f in &self.findings {
            result.push_str(&format!(
                "{}:{}:{}: [{:?}/{}] {}: {}\n",
                f.location.file,
                f.location.line_start,
                f.location.col_start,
                f.severity,
                f.evidence.label(),
                f.rule_id,
                f.display_message()
            ));
            if explain {
                result.push_str(&format!("  Evidence: {}\n", f.evidence_note));
                if let Some(c) = &f.counter_condition {
                    result.push_str(&format!("  May be intentional: {c}\n"));
                }
            }
        }
        for d in &self.diagnostics {
            result.push_str(&format!("SCAN INCOMPLETE {}: {}\n", d.file, d.message));
        }
        result.push_str(&format!("{} findings; {} text files inspected, {} source files parsed, {} skipped, {} suppressed; complete={}\n", self.counts.total, self.scanned.files, self.scanned.parsed_files, self.scanned.skipped_files, self.scanned.suppressed_findings, self.scanned.complete));
        result
    }
    pub fn sarif(&self, metadata: &[RuleMeta]) -> Value {
        let rules: Vec<Value> = metadata.iter().map(|m| json!({"id": m.id, "shortDescription": {"text":m.message}, "defaultConfiguration":{"level":level(m.evidence,m.severity)}, "properties":{"evidenceClass":m.evidence,"tier":m.evidence.tier()}})).collect();
        let results: Vec<Value> = self.findings.iter().map(|f| {
            let l = &f.location;
            let mut result = json!({"ruleId":f.rule_id,"level":level(f.evidence,f.severity),"rank":match f.evidence {EvidenceClass::Heuristic=>50, EvidenceClass::Dataflow=>80, EvidenceClass::Protocol=>70, EvidenceClass::Speculative=>20},"message":{"text":f.display_message()},"locations":[{"physicalLocation":{"artifactLocation":{"uri":uri(&l.file),"uriBaseId":"%SRCROOT%"},"region":{"startLine":l.line_start,"startColumn":l.col_start,"endLine":l.line_end,"endColumn":l.col_end,"byteOffset":l.byte_start,"byteLength":l.byte_end-l.byte_start}}}],"properties":{"evidenceClass":f.evidence,"tier":f.tier,"evidenceNote":f.evidence_note,"counterCondition":f.counter_condition}});
            if let Some(index) = metadata.iter().position(|m| m.id == f.rule_id) { result["ruleIndex"] = json!(index); }
            result
        }).collect();
        let notifications: Vec<Value> = self
            .diagnostics
            .iter()
            .map(|d| json!({"level":"error","message":{"text":format!("{}: {}",d.file,d.message)}}))
            .collect();
        json!({"$schema":"https://json.schemastore.org/sarif-2.1.0.json","version":"2.1.0","runs":[{"tool":{"driver":{"name":"pons","semanticVersion":self.tool.version,"informationUri":"https://github.com/hyperpolymath/pons-asinorum","rules":rules}},"columnKind":"unicodeCodePoints","results":results,"invocations":[{"executionSuccessful":self.scanned.complete,"toolExecutionNotifications":notifications}],"properties":{"scanned":self.scanned}}]})
    }
}
fn level(e: EvidenceClass, s: Severity) -> &'static str {
    if e == EvidenceClass::Speculative {
        return "note";
    }
    match s {
        Severity::Info => "note",
        Severity::Warn => "warning",
        Severity::Error => "error",
    }
}
fn uri(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
