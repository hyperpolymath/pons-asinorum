// SPDX-License-Identifier: MPL-2.0
//! A per-finding *semantic* fingerprint: the stable key an aggregating verdict
//! grid uses to mark one site once instead of counting it twice.
//!
//! A pons report already carries a fingerprint per record in
//! `evidence_contract.records[].fingerprint`, and that one answers a different
//! question. It hashes the rule id and a whitespace-folded source anchor, so
//! two rules firing on the same line are two records — which is right for
//! provenance and wrong for aggregation. The key here hashes the root-relative
//! path, the 1-based line and the evidence class, and **deliberately ignores
//! the rule id and the producer**, so a `panic-attack` or Hypatia finding at the
//! same path, line and class can be matched to it once rather than added to it.
//!
//! Consequences a consumer must know, all of them intended:
//!
//! * two pons findings at one site in one evidence class share a key; the grid
//!   counts that site once and keeps both findings' provenance;
//! * the key carries no severity, so it stays stable when a rule's severity is
//!   tuned — severity is advisory, the evidence class is the honesty invariant;
//! * the key is not a proof of semantic equivalence, and a `SPECULATIVE` key
//!   corroborates nothing at all. Agreement between producers is corroboration,
//!   not independent proof — see `docs/PURPOSE-AND-PROOF.adoc` and the
//!   published policy in `schemas/verdict-matrix-1.0.0.json`.
//!
//! The field layout is versioned by [`SCHEME`], which is hashed as the first
//! field: a future change cannot collide with keys minted by this version.

use crate::finding::{EvidenceClass, Finding};

/// Scheme identifier, hashed as the first field of the canonical input.
pub const SCHEME: &str = "pons-corroboration-v1";

/// Field separator. NUL is outside the alphabet of paths, decimal line numbers
/// and evidence class names, so the canonical input cannot be read two ways.
const SEPARATOR: char = '\0';

/// The exact string that is hashed.
///
/// Joins [`SCHEME`], `path`, the decimal `line` and the wire name of `class`
/// with NUL separators and no trailing separator. Supply a root-relative,
/// `/`-separated path without a leading `./` and a 1-based start line;
/// these conventions are not normalised or checked here.
///
/// Public because a consumer must be able to mint the same key without pons —
/// for a panic-attack or Hypatia finding, once the grid has mapped it onto an
/// evidence class using `schemas/verdict-matrix-1.0.0.json`. An implementation
/// that cannot reproduce this string byte for byte has not implemented the
/// contract.
pub fn canonical_input(path: &str, line: usize, class: EvidenceClass) -> String {
    format!(
        "{scheme}{sep}{path}{sep}{line}{sep}{class}",
        scheme = SCHEME,
        sep = SEPARATOR,
        path = path,
        line = line,
        class = class.name()
    )
}

/// BLAKE3 of [`canonical_input`], as 64 lowercase hexadecimal characters.
pub fn fingerprint_of(path: &str, line: usize, class: EvidenceClass) -> String {
    blake3::hash(canonical_input(path, line, class).as_bytes())
        .to_hex()
        .to_string()
}

/// The fingerprint of one finding, relative to the scan root.
///
/// Uses the finding's 1-based start line and evidence class. The path matches
/// the evidence contract's records ([`crate::evidence::relative_path`]): strip
/// `root` when it is a prefix, use the basename when `root` is the file itself,
/// and otherwise retain the path. Backslashes become `/`; no filesystem lookup
/// is performed.
pub fn semantic_fingerprint(finding: &Finding, root: &str) -> String {
    let loc = finding.location();
    fingerprint_of(
        &crate::evidence::relative_path(&loc.file, root),
        loc.line_start,
        finding.evidence(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Location, Severity, Tier};

    fn finding(rule: &str, file: &str, line: usize, tier: Tier) -> Finding {
        Finding::new(
            rule,
            tier,
            Severity::Warn,
            Location {
                file: file.to_string(),
                byte_start: 0,
                byte_end: 1,
                line_start: line,
                col_start: 1,
                line_end: line,
                col_end: 2,
            },
            "msg",
            "note",
            None,
        )
    }

    #[test]
    fn the_canonical_input_is_four_nul_separated_fields() {
        let input = canonical_input("app/svc.py", 41, EvidenceClass::Dataflow);
        assert_eq!(
            input.split('\0').collect::<Vec<_>>(),
            vec![SCHEME, "app/svc.py", "41", "DATAFLOW"],
            "the field order and the separator are the whole contract: a \
             consumer reproducing this string byte for byte must get the same key"
        );
    }

    /// A pinned digest, not a recomputed one. If the scheme name, the field
    /// order, the separator or the class spelling changes, this number changes
    /// and the diff says so — whereas a self-consistent test (hash, then compare
    /// with the same function) would pass through any of those changes and
    /// quietly invalidate every key a consumer had stored.
    #[test]
    fn a_pinned_key_pins_the_scheme() {
        assert_eq!(
            fingerprint_of("app/svc.py", 41, EvidenceClass::Dataflow),
            "a54bb8fa2b7f74ff5c9d1fa745ac22e61abb49847ddbac0bd220fef2442baaf1"
        );
    }

    #[test]
    fn a_key_is_64_lowercase_hex_characters() {
        let key = fingerprint_of("app/svc.py", 41, EvidenceClass::Dataflow);
        assert_eq!(key.len(), 64);
        assert!(key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn the_key_changes_with_the_path_the_line_and_the_class() {
        let base = fingerprint_of("app/svc.py", 41, EvidenceClass::Dataflow);
        assert_ne!(
            base,
            fingerprint_of("app/svc.ts", 41, EvidenceClass::Dataflow)
        );
        assert_ne!(
            base,
            fingerprint_of("app/svc.py", 42, EvidenceClass::Dataflow)
        );
        assert_ne!(
            base,
            fingerprint_of("app/svc.py", 41, EvidenceClass::Heuristic)
        );
    }

    #[test]
    fn the_key_ignores_the_rule_id_because_overlap_is_the_point() {
        // Two rules naming the same line in the same evidence class are one
        // site. If the rule id were hashed, the grid would double-count exactly
        // the overlap this key exists to collapse.
        let a = finding("dead-store", "app/svc.py", 41, Tier::T1);
        let b = finding("read-before-init", "app/svc.py", 41, Tier::T1);
        assert_eq!(semantic_fingerprint(&a, "."), semantic_fingerprint(&b, "."));
        // ...but a different class at the same site is a different claim, and
        // must not be collapsed into the same corroborated row.
        let c = finding("dead-store", "app/svc.py", 41, Tier::T3);
        assert_ne!(semantic_fingerprint(&a, "."), semantic_fingerprint(&c, "."));
    }

    #[test]
    fn the_key_uses_the_contract_path_spelling_and_survives_a_checkout_move() {
        let from_root = semantic_fingerprint(&finding("r", "app/svc.py", 41, Tier::T1), ".");
        let from_absolute = semantic_fingerprint(
            &finding("r", "/home/dev/checkout/app/svc.py", 41, Tier::T1),
            "/home/dev/checkout",
        );
        assert_eq!(from_root, from_absolute);
        assert_eq!(
            from_root,
            fingerprint_of("app/svc.py", 41, EvidenceClass::Dataflow)
        );
    }

    #[test]
    fn a_single_file_scan_keys_the_file_name_not_the_absolute_path() {
        // root == file for a one-file scan; the contract's relative_path strips
        // to the basename, so the key does not encode the build machine.
        let key = semantic_fingerprint(
            &finding("r", "/tmp/probe/a.py", 3, Tier::T1),
            "/tmp/probe/a.py",
        );
        assert_eq!(key, fingerprint_of("a.py", 3, EvidenceClass::Dataflow));
    }
}
