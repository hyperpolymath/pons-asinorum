<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Prompt: finish Pons's format adoption without overstating analysis

Continue the existing hyperpolymath/pons-asinorum upgrade and PR #58. Preserve the original eleven-target PHP/WordPress/Wharf audit scope, coverage correction, false-positive control and evidence tiers. This prompt supplements that work.

Before editing, read the current repository instructions, deed, coordination.k9, plans, issues and open PRs. Recheck the pinned observations below against current HEAD. Preserve concurrent work and use bounded, reviewable commits. Implement and test; do not stop at recommendations. Record exact commands, tool/source versions, exit codes, fixed seeds, resource limits and actual outcomes. Missing tooling, skipped inputs and unsupported features must remain visible. Do not replace failing gates with placeholders, broad exclusions or optimistic metadata. Keep tests, finite checks, conjectures and machine-checked proofs distinct. Follow metadatastician/standards for DEED/K9, KYAML and I-JSON/JCS. A migration is complete only when its consumers and validators work, not when extensions are renamed.

At inspection, PR #58 head e7872fb8f605f8da8779b68f8a8d11c15386c4aa includes native PHP parsing, bounded intraprocedural taint, finite Python dataflow worklists, supplied-event protocol models and a purpose/proof guide. It does not establish a source-derived universal verifier or machine-checked soundness.

1. Resolve current review findings, rerun meaningful checks and complete the authorized push/merge only with required protection intact. Reconcile overlapping PR #57 rather than merging incompatible schemas independently.
2. Inventory .deed, .k9, .kyaml, .json and .jsonl before adapter selection. Account for unsupported configuration and legacy A2ML explicitly. Filename recognition is not parsing or security analysis; strict coverage must remain non-green for unsupported inputs.
3. Migrate repository metadata using metadatastician/standards' actual DEED grammar and K9 coordination contract. Keep the front door pointer-only, put attested reality in the repository deed, and audit live readers before retiring legacy files. Do not copy stale “planning” or unimplemented proof claims.
4. Repair workflow/configuration field checks to use yq and test both block YAML and canonical flow KYAML, malformed inputs and missing-tool failure. Preserve tool-required filenames, comments, exact action pins and lockfiles.
5. Adopt a tested, pinned hyperpolymath/ijson-jcs strict raw-input API where reports cross the trust boundary. Reject duplicate keys before serde map construction; then apply the versioned JSON schema and semantic cross-record checks. Compare copied evidence locations, traces, assumptions and counterconditions to their findings.
6. Define canonical report bytes and LF framing through a documented schema/CLI migration. Do not pretend sorted serde output is RFC 8785. Keep historical raw evidence unchanged; convert newly authored policy/schema files only with tested tooling and record exceptions.
7. Consume kyq only after its installed CLI passes the required contract. In the meantime use functioning, pinned tools; missing kyq must never become a successful format check. DEED/K9 validation and source security analysis remain separate capabilities.
8. Keep Pons's purpose explicit: structural, dataflow and protocol evidence with traces, assumptions, counterconditions, unknown edges and complete coverage accounting. panic-attack remains an independently identifiable producer. Shared fixtures and correlated heuristics do not become independent proof.
9. Extend proof work only through bounded abstractions with executable conformance tests, then checker artifacts tied to the same semantics. Report existing fixed-point analyses separately from future SMT integration, source-to-protocol extraction and mechanized soundness.

Done means tested code and report migration notes, truthful purpose/capability/proof documentation, current review resolution and reproducible portfolio coverage/evidence. If adding new coverage categories changes measured counts, rerun and label the new baseline instead of rewriting historical outcomes.

---

## Status update — 2026-10-25 (CI/CD, format policy, testing framework)

Work done on the CI/CD and format side of this prompt, and what remains.

**Done.**

* *Obligation 4 (workflow checks use `yq`, and both dialects are tested).* The
  workflow validator already used `yq`; it is now registered in the
  provenance ledger as a suite, and its regression test (`tests/workflows/
  validate_workflows_regression.sh`) exercises it against a block-YAML
  document and a canonical flow-KYAML document. The repository's own
  workflows are deliberately **not** converted to KYAML: step 6 of
  YAML-POLICY §5 (`standards#1025`) has not started in the estate, and
  converting them here would break the bot that updates them. The exclusion
  is recorded in `ci/yaml-ratchet.kyaml` with its reason.
* *Obligation 4 (missing-tool failure).* Every gate added here exits `2`,
  never `0`, when a tool it needs is missing. `tests/format_ratchet.sh`
  refuses to run at all if the `yq` on PATH cannot write KYAML, because a
  formatter that cannot write cannot report a file clean.
* *Obligation 4 (comments, exact pins, lockfiles).* No workflow's comments
  were touched, no action pin was changed, and no lockfile was reformatted.
  The only workflow edits were the `uses:` owner spelling (below) and the
  addition of new workflows.
* *Obligation 6 (canonical report bytes, honestly).* This is **not** done,
  and the honest position is now written down: the JSON report's field
  order is contractual (`docs/PLAN.adoc` Appendix G) and therefore not
  RFC 8785 canonical, and the estate policy's framing requirement (exactly
  one final LF) is enforced for every JSON file by `tests/format_ratchet.sh`.
  The distinction between a *published* form and a *canonical* form — and
  whether the estate wants one for artefacts that cross a trust boundary —
  is asked in `docs/handoffs/upstream-prompts.adoc`, prompt 3. Historical
  raw evidence under `docs/evidence/` is unchanged, and the reasons are
  recorded per file in `ci/json-ratchet.kyaml`.
* *Obligation 6 (record exceptions).* `ci/json-ratchet.kyaml` and
  `ci/yaml-ratchet.kyaml` are the exception records. They are closed
  inventories: every YAML and JSON file in the tree is accounted for, and a
  file that no ratchet accounts for fails the build.
* *Obligation 5 (ijson-jcs).* The estate tool is now wired in
  (`.github/workflows/format-policy.yml`), pinned to a commit and verified
  by sha256, and every JSON file is checked for I-JSON validity. The
  *library*-level adoption the prompt describes — a strict raw-input API
  that rejects duplicate keys before serde map construction, applied to
  reports as they cross the trust boundary — is **not** done, and cannot be
  done here without regenerating `Cargo.lock` against a git dependency.
  That is the remaining part of obligation 5.
* *Obligation 7 (kyq).* Unchanged: kyq is not consumed. Its installed CLI
  does not pass the required contract, and the estate's own tooling
  (`yq`, `ijson-jcs`) is used instead, pinned and verified. A missing kyq is
  never a successful format check: the gate here does not use kyq at all,
  so there is nothing for it to silently pass.

**The CI/CD failure mode that was found, measured and recorded.** The
shared secret scanner's caller has three ways of losing the
estate-required context `scan / gitleaks`, and all three are silent. The
one this repository was actually bitten by is the *owner spelling*: the
`standards` repository was transferred to the `metadatastician`
organisation, GitHub's REST API follows the old name, and the Actions
resolver does not. Measured here on the same pinned commit,
`uses: hyperpolymath/standards/...` starts no job at all — the run is
attributed to the workflow *file*, with zero jobs and no log — while
`uses: metadatastician/standards/...` starts, runs and publishes the
check. The other two are a stale pin to a reusable revision that asks for
more permission than the caller grants (`startup_failure`, no job), and
`secrets: inherit` against a reusable that declares no secrets.

None of the three looks like a failure of the workflow you are looking
at, which is why all three are now written down at the call site in
`.github/workflows/secret-scanner.yml` and in `docs/CI-CD.adoc`. The
estate-wide consequence — every live caller still spelling the old owner,
in `standards` itself, `cicd-suite`, `rsr-template-repo` and
`cicd-squabbler` — is asked about upstream in
`docs/handoffs/upstream-prompts.adoc`, prompt 1.

**Still open, and deliberately so.**

* The generic empty-PHP-function rule that PR #57 proposed. It contradicts
  #63's reconciled contract — main's `php-empty-security-function` negative
  corpus asserts that an empty *non-security* function produces no finding,
  and PR #57's rule would fire on it. The narrower rule that would be
  correct (empty *anonymous* callables) is specified in
  `docs/PR57-RECONCILIATION.adoc` and needs a compiler in hand.
* The benchmark *regression* comparison, which the estate's taxonomy
  requires to be cut from ten CI runs on main. The measurement is running;
  the comparison is recorded as an Unproven gap rather than as a passing
  gate, because a baseline cut from fewer than ten runs would be
  mathematically inert and the framework's own checker says so.
* A machine-checked proof artefact. Pons has none, so the Actually-Proven
  tier is unreachable and the provenance gate fails any claim of it. That
  is the correct state, and the gate enforces it rather than accepting the
  claim.
