<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Prompt: make the format migration contract executable

Work in metadatastician/standards. Reconcile the estate format contracts used by AffineScript, kyq, ijson-jcs and Pons, and ship executable conformance fixtures and migration guidance.

Before editing, read the current repository instructions, deed, coordination.k9, plans, issues and open PRs. Recheck the pinned observations below against current HEAD. Preserve concurrent work and use bounded, reviewable commits. Implement and test; do not stop at recommendations. Record exact commands, tool/source versions, exit codes, fixed seeds, resource limits and actual outcomes. Missing tooling, skipped inputs and unsupported features must remain visible. Do not replace failing gates with placeholders, broad exclusions or optimistic metadata. Keep tests, finite checks, conjectures and machine-checked proofs distinct. Follow metadatastician/standards for DEED/K9, KYAML and I-JSON/JCS. A migration is complete only when its consumers and validators work, not when extensions are renamed.

Baseline observation: 8e48b1caade08a552646adeab66558ac43b4360d. Read 3-practice/YAML-POLICY.adoc, 3-practice/JSON-POLICY.adoc, docs/ULTRAPLAN-DEED-K9-CONTRACTILES-2026-10-19.adoc, the DEED grammar/ABNF and mappings, K9 component specifications, and 2-protocols/k9-coordination specifications and readiness records.

1. Publish a concise, versioned dispatch/ownership table. Distinguish the DEED front door and repository attestation, K9 component envelopes and typed bodies, and the coordination.k9 protocol. A shared .k9 suffix does not establish identical grammars. Preserve contractiles as policy and descriptiles as recorded reality.
2. Resolve kyq's draft use of “KYAML” against estate policy. Separate any broader YAML input mode from canonical KYAML authoring/output: flow collections, double-quoted string values, two-space indentation, document marker and trailing-comma rules per the authoritative policy. Identify exceptions explicitly; do not silently broaden the canonical profile or equate it with Kubernetes semantics.
3. Provide positive, negative and intentionally similar fixtures for each format/profile, with expected parse/validation outcomes and profile versions. Include duplicate keys, ambiguous scalar spellings, comments, multiline strings, tags/aliases, document boundaries and limits as applicable. Test grammar-based dispatch; do not dispatch solely by suffix.
4. Pin the actual I-JSON/JCS contract. Keep .json and .jsonl extensions; new or rewritten authored files follow the estate's exact one-LF convention. Distinguish raw JCS bytes from file framing, JSONL records, JSONC exceptions, generated tool output and archived byte-exact evidence. State which cases are enforced, report-only or not yet decided.
5. Use yq for YAML field inspection in gates, not grep/sed/awk or a new Python YAML parser. Prove that migration preserves relevant comments and semantics and that formatting is idempotent. Keep tool-required filenames and tool-owned lockfiles intact.
6. Exercise DEED/K9 migrations with actual consumers and validators. Do not rename A2ML into DEED without grammar conversion. Preserve or explicitly bridge live legacy readers until replacement readers pass.
7. Repair reusable-workflow pins and lock closures only where reproduced, with least permissions and exact SHA pins. A skipped or missing validator must not report conformance.

Done means versioned normative profiles, runnable validators/conformance corpora with failing controls, a compatibility table and migration examples. Hand kyq an unambiguous language contract and ijson-jcs/Pons explicit byte/framing requirements. Do not claim a broad estate migration is complete from one repo's green build.
