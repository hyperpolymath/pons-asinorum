<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Prompt: establish the strict JSON and canonical-byte reference

Work in hyperpolymath/ijson-jcs, currently the resolving repository for this project. Harden the Rust library/CLI as an independently tested reference for kyq and a reusable strict boundary for Pons.

Before editing, read the current repository instructions, deed, coordination.k9, plans, issues and open PRs. Recheck the pinned observations below against current HEAD. Preserve concurrent work and use bounded, reviewable commits. Implement and test; do not stop at recommendations. Record exact commands, tool/source versions, exit codes, fixed seeds, resource limits and actual outcomes. Missing tooling, skipped inputs and unsupported features must remain visible. Do not replace failing gates with placeholders, broad exclusions or optimistic metadata. Keep tests, finite checks, conjectures and machine-checked proofs distinct. Follow metadatastician/standards for DEED/K9, KYAML and I-JSON/JCS. A migration is complete only when its consumers and validators work, not when extensions are renamed.

Baseline observation: 3a9a4ee326a968a32b2f152b4faec243a2d00765. Read src/lib.rs, src/strict.rs, src/ijson.rs, src/jcs.rs, CLI behavior, tests, affirmations and metadatastician/standards/3-practice/JSON-POLICY.adoc.

1. Reproduce the existing suite and document Loose, Strict and Canonical modes at the raw-byte boundary. Reject duplicate decoded keys before a map can overwrite them; test escaped-equivalent keys, invalid UTF-8, unpaired surrogates, noncharacters, malformed tokens, overflow and trailing input.
2. Define numeric semantics precisely. Distinguish the estate's raw integer-token safety policy from binary64 rounding and RFC requirements. Test safe-integer boundaries, negative zero, subnormals, exponent thresholds and finite/infinite transitions. Do not claim every ordinary decimal fraction has an exact binary64 representation.
3. Audit es_number_to_string against RFC 8785 Appendix B and an independent ECMAScript implementation. Current code derives digits from Rust's scientific formatter; equal shortest lengths do not by themselves establish identical tie-breaking. Treat this as a verification target, not a confirmed defect until reproduced. Reduce mismatches, fix them and retain bit-pattern fixtures.
4. Test UTF-16 code-unit key ordering, escaping and preservation of Unicode sequences without normalization. Separate allowed escaped control characters from invalid raw controls. Resolve conflicting claims in kyq's draft by linking tested contract cases, not copying unsupported prose.
5. Specify check/fix/canon and JSONL behavior, exit codes and file framing. Distinguish no-LF canonical bytes from the estate's one-LF authored files. Test CRLF, multiple trailing newlines, empty/malformed records and missing/unreadable files. Make in-place operations atomic and leave inputs intact on validation or write failure.
6. Add isolated fixed-seed fuzz/property tests and bounded depth/size/time/memory tests. Export a versioned corpus with expected bytes, rejections and diagnostic classes that kyq can consume externally.
7. Publish a capability/evidence matrix and accurate API examples. Value-only validation cannot recover duplicate keys already discarded by another parser. Differential agreement is conformance evidence, not a mathematical proof.

Done means passing RFC and independent differential cases, rejected malformed cases, stable raw-input APIs and CLI contracts, measured resource limits, and a pinned corpus/version handoff. kyq remains AffineScript-only: this crate is its external oracle, not a Rust bootstrap inside kyq.
