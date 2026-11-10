// SPDX-License-Identifier: MPL-2.0
//! Versioned exchange contract. Producer correlation is explicit: agreement
//! between AST heuristics is never multiplied into independent proof.
use crate::{
    engine::ScanReport,
    finding::{EvidenceClass, Tier},
};
use serde::{Deserialize, Serialize};
pub(crate) const ASSUMPTION: &str = "Only registered rule semantics and declared parser/model coverage; no runtime safety certification";

// Writers retain the native path semantics of the historical format.
pub(crate) fn relative_path(file: &str, root: &str) -> String {
    let path = std::path::Path::new(file);
    let root = std::path::Path::new(root);
    let base = if path == root {
        root.parent().unwrap_or(root)
    } else {
        root
    };
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The components `std::path` compares, each with its offset in `text`: the
/// root, then every name. Empty components are dropped, and so is every `.`
/// except a leading one; `..` is kept. Only the Windows reading tells a `//`
/// root from `/`. Drive and verbatim prefixes are ordinary names here, as
/// they were under the literal rule this replaces.
fn components(text: &str, windows: bool) -> Vec<(&str, usize)> {
    let body = text.trim_start_matches('/');
    let lead = text.len() - body.len();
    let mut parts = Vec::new();
    if lead > 0 {
        parts.push((&text[..lead.min(1 + usize::from(windows))], 0));
    }
    let mut start = lead;
    for name in body.split('/') {
        if !name.is_empty() && (name != "." || start == 0) {
            parts.push((name, start));
        }
        start += name.len() + 1;
    }
    parts
}

// One reading of `relative_path`: the same component comparison, and the same
// remainder, without `std::path` and therefore without the reader's host.
fn path_for_style(file: &str, root: &str, windows: bool) -> (String, bool) {
    let (file, root) = if windows {
        (file.replace('\\', "/"), root.replace('\\', "/"))
    } else {
        (file.to_owned(), root.to_owned())
    };
    let parts = components(&file, windows);
    let base = components(&root, windows);
    let same = |a: &[(&str, usize)], b: &[(&str, usize)]| {
        a.iter().map(|part| part.0).eq(b.iter().map(|part| part.0))
    };
    // A file that is the root itself is keyed below the root's parent. A bare
    // root has no parent.
    let has_parent = base.last().is_some_and(|last| !last.0.starts_with('/'));
    let depth = base.len() - usize::from(has_parent && same(&parts, &base));
    let relative = (parts.len() >= depth && same(&parts[..depth], &base[..depth])).then(|| {
        match (parts.get(depth), parts.last()) {
            // As `Components::as_path`: from the next component to the end of
            // the last one, spelled as the producer spelled it.
            (Some(first), Some(last)) => &file[first.1..last.1 + last.0.len()],
            _ => "",
        }
    });
    (
        relative.unwrap_or(&file).replace('\\', "/"),
        relative.is_some(),
    )
}

// Historical relative roots do not identify the producer's platform. Readers
// enumerate both lexical interpretations and require the record and fingerprint
// to agree on ONE of them; they never consult their own filesystem or host OS.
pub(crate) fn path_candidates(file: &str, root: &str) -> Vec<(String, bool)> {
    let drive_absolute = root.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && root.as_bytes().get(1) == Some(&b':')
        && root
            .as_bytes()
            .get(2)
            .is_some_and(|c| matches!(c, b'/' | b'\\'));
    if drive_absolute || root.starts_with("\\\\") {
        return vec![path_for_style(file, root, true)];
    }
    let posix = path_for_style(file, root, false);
    if root.starts_with('/') && !root.starts_with("//") {
        return vec![posix];
    }
    let windows = path_for_style(file, root, true);
    if windows == posix {
        vec![posix]
    } else {
        vec![posix, windows]
    }
}

#[cfg(test)]
mod portable_paths {
    use super::path_candidates;
    #[test]
    fn windows_reports_have_the_same_keys_on_every_reader_host() {
        assert_eq!(
            path_candidates(r"C:\repo\sample.php", r"C:\repo"),
            vec![("sample.php".into(), true)]
        );
        assert_eq!(
            path_candidates(r"C:\repo\sample.php", r"C:\repo\sample.php"),
            vec![("sample.php".into(), true)]
        );
        assert_eq!(
            path_candidates(r"C:\outside\sample.php", r"C:\repo"),
            vec![("C:/outside/sample.php".into(), false)]
        );
        assert!(path_candidates(r"repo\a.py", "repo").contains(&("a.py".into(), true)));
        assert!(path_candidates(r".\a.py", ".").contains(&("a.py".into(), true)));
        assert!(
            path_candidates(r"//server/share\sample.php", "//server/share")
                .contains(&("sample.php".into(), true))
        );
    }
    #[test]
    fn posix_boundaries_and_ambiguous_historical_keys_are_preserved() {
        assert_eq!(
            path_candidates(r"/tmp/a\b.py", r"/tmp/a\b.py"),
            vec![("a/b.py".into(), true)]
        );
        assert!(!path_candidates(r"/tmp/repo\outside/a.py", "/tmp/repo")[0].1);
        for (file, root, expected) in [
            (r"repo\sub/a.py", "repo", "repo/sub/a.py"),
            (r"repo\sub/a.py", "repo", "sub/a.py"),
            (r"src\a.py", r"src\a.py", "a.py"),
            (r"src\a.py", r"src\a.py", "src/a.py"),
            (r"a:b\c.py", r"a:b\c.py", "a:b/c.py"),
        ] {
            assert!(path_candidates(file, root)
                .iter()
                .any(|(path, _)| path == expected));
        }
    }
    // The writer strips by component (`Path::strip_prefix`), so `.` and
    // repeated separators next to the root never reach a key. Each row gives
    // the exact POSIX and Windows readings; the native writer is compared with
    // the reading of the host that runs the test.
    #[test]
    fn component_equivalent_spellings_have_the_writer_key() {
        let inside = |key: &str| (key.to_string(), true);
        let outside = |key: &str| (key.to_string(), false);
        for (file, root, posix, windows) in [
            ("repo/./a.php", "repo", inside("a.php"), inside("a.php")),
            (
                "repo/./sub/a.php",
                "repo",
                inside("sub/a.php"),
                inside("sub/a.php"),
            ),
            ("repo//a.php", "repo", inside("a.php"), inside("a.php")),
            ("repo/a.php/.", "repo", inside("a.php"), inside("a.php")),
            ("repo/a.php", "repo/.", inside("a.php"), inside("a.php")),
            ("repo/a.php", "repo//", inside("a.php"), inside("a.php")),
            // The same file named twice: the key is its name below the parent.
            (
                "repo/./a.php",
                "repo/a.php",
                inside("a.php"),
                inside("a.php"),
            ),
            ("./a.php", ".", inside("a.php"), inside("a.php")),
            (
                "/tmp/repo/./a.php",
                "/tmp/repo",
                inside("a.php"),
                inside("a.php"),
            ),
            // Only the components next to the root are dropped. The remainder
            // keeps its spelling, `..` is never resolved, and a leading `.` is
            // a component of its own.
            (
                "repo/sub/./a.php",
                "repo",
                inside("sub/./a.php"),
                inside("sub/./a.php"),
            ),
            (
                "repo/../repo/a.php",
                "repo",
                inside("../repo/a.php"),
                inside("../repo/a.php"),
            ),
            (
                "./repo/a.php",
                "repo",
                outside("./repo/a.php"),
                outside("./repo/a.php"),
            ),
            // A backslash separates components only in the Windows reading.
            (
                r"repo\.\a.php",
                "repo",
                outside("repo/./a.php"),
                inside("a.php"),
            ),
            (
                r"repo\.\sub\a.php",
                r"repo\",
                outside("repo/./sub/a.php"),
                inside("sub/a.php"),
            ),
        ] {
            let native = if cfg!(windows) { &windows } else { &posix };
            assert_eq!(
                super::relative_path(file, root),
                native.0,
                "writer: {file} in {root}"
            );
            let mut expected = vec![posix.clone()];
            // An absolute POSIX root has one reading.
            if windows != posix && !root.starts_with('/') {
                expected.push(windows);
            }
            assert_eq!(
                path_candidates(file, root),
                expected,
                "reader: {file} in {root}"
            );
        }
        assert_eq!(
            path_candidates(r"C:\repo\.\a.php", r"C:\repo"),
            vec![inside("a.php")]
        );
        assert_eq!(
            path_candidates(r"//server/share\.\a.php", "//server/share")[1],
            inside("a.php")
        );
    }
    // Every short path over a name, `.` and the host's separators: the reading
    // for this host is the native writer, string for string. Windows prefixes
    // need `:` or two leading separators; the reader does not model them, so
    // they are not generated.
    #[test]
    fn the_host_reading_is_the_native_writer_on_every_short_path() {
        let alphabet: &[char] = if cfg!(windows) {
            &['a', '.', '/', '\\']
        } else {
            &['a', '.', '/']
        };
        let mut paths = vec![String::new()];
        let mut from = 0;
        for _ in 0..6 {
            let until = paths.len();
            for index in from..until {
                for letter in alphabet {
                    paths.push(format!("{}{letter}", paths[index]));
                }
            }
            from = until;
        }
        let prefixed = |path: &str| {
            cfg!(windows) && path.len() > 1 && path[..2].chars().all(|c| matches!(c, '/' | '\\'))
        };
        paths.retain(|path| !prefixed(path));
        let mut compared = 0usize;
        for file in &paths {
            for root in paths.iter().filter(|root| root.len() < 5) {
                assert_eq!(
                    super::path_for_style(file, root, cfg!(windows)).0,
                    super::relative_path(file, root),
                    "{file:?} in {root:?}"
                );
                compared += 1;
            }
        }
        assert!(compared > 100_000, "the comparison was vacuous: {compared}");
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
