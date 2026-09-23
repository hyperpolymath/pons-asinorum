<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000004
parent: 0199c4a0-0000-7000-8000-000000000001
position: 30
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# For Developers

For contributors working on the engine or adding rules to the catalogue.

> **Note**
>
> The engine is not built yet. This page is the **contributor contract** — the shape
> of the codebase and the rules for adding to it, taken directly from the ratified
> plan (`docs/PLAN.adoc` + ADRs). Follow it and your work lands under the gate the
> first time.

## The prime directive

Read [[Evidence Tiers]]. The project exists to *never dress a heuristic up as a
proof*. Two mechanisms enforce it and you must respect both:

. **Evidence class is fixed by tier and is not per-finding overridable.** T0→
  `HEURISTIC`, T1→`DATAFLOW`, T2→`PROTOCOL`, T3→`SPECULATIVE`. You do not get to
  make a guess look like a proof.
. **Falsifier-first.** No rule merges without a **negative** corpus, and a rule that
  fires on its own negative corpus is demoted or removed by a build-breaking CI
  gate.

## Architecture at a glance

A Cargo workspace, four crates (full tree in `docs/pons-kickoff.adoc`):

| Crate | Responsibility |
|---|---|
| `pons-core` | The engine: file discovery, language registry, parsing, the CFG + dataflow, the `Finding` model, suppression, and the three reporters. **No rules live here.** |
| `pons-rules` | **The catalogue.** One module per tier (`t0/`…`t3/`), a `registry` that inventories every rule, and `rules/` data files (queries + metadata). |
| `pons-cli` | `pons scan <path> [flags]`. |
| `pons-protocols` | Toy + user-supplied T2 protocol specs. |

Substrate is **tree-sitter native + Rust** — decided, not up for a re-spike
(`docs/adr/0001-substrate.adoc`). If a grammar won't load, fix the version pin;
do not reconsider the substrate.

## The `Rule` trait

```rust
pub trait Rule {
    fn id(&self) -> &'static str;           // e.g. "div-by-literal-zero"
    fn tier(&self) -> Tier;
    fn languages(&self) -> &'static [Lang]; // which grammars it applies to
    fn check(&self, ctx: &RuleCtx) -> Vec<Finding>;
}
```

`RuleCtx` gives you the parsed `Tree`, the source text, the file's `Lang`, and —
for T1/T2 rules — a lazily-built `Cfg` and dataflow facts. *T0 rules touch only
the tree.*

## The Finding model (honesty lives here)

```rust
pub struct Finding {
    pub rule_id:   String,
    pub tier:      Tier,
    pub evidence:  EvidenceClass,          // fixed by tier
    pub severity:  Severity,               // INFO | WARN | ERROR (advisory)
    pub location:  Location,               // file + byte span + line/col span
    pub message:   String,                 // what is wrong, one line
    pub evidence_note: String,             // WHY — the "expert would say…"
    pub counter_condition: Option<String>, // when this smell is actually fine
}
```

`evidence_note` is **non-empty, always**. A finding a human cannot evaluate in
context is a finding they will switch off.

## Adding a rule (the checklist)

. **Classify it.** Which tier? If it needs control flow, it is T1+. If it rests on
  a supplied contract, T2. If it is fundamentally undecidable (real complexity,
  general div-by-zero, non-termination), it is T3 and **must** be `SPECULATIVE`.
  A rule that fits neither "wasted work" nor "contradiction" (nor the small
  "missing escape hatch" shape) is probably out of scope — argue for it first.
. **Write the negative corpus first.** Seed it from the **counter-condition**: the
  cases where the smell is present in syntax but genuinely fine. This is the
  hard, valuable half.
. **Write the positive corpus.** Cases that must fire.
. **Implement `check`.** T0 = a tree-sitter query (stored as a `.scm` data file
  under `pons-rules/rules/<id>/<lang>.scm`) plus a small Rust predicate for what
  a pure query can't say (e.g. identifier equality for `self-assignment`).
. **Fill every finding's `evidence_note`**, and a `counter_condition` where one
  exists.
. **Run the gate:** `just falsify`. All positives fire; zero negatives fire. Green
  or it does not merge.

## The two dataflow analyses (T1)

T1 in v0.1.0 is **Python** (see `docs/adr/0002-t1-language-and-cfg.adoc` for why
Python and not Rust — short version: Rust's `Drop` side effects need type
resolution and inflate false positives; Python's function-scope + no
default-init make the two T1 rules clean). The ADR writes out, ready to
transcribe:

- the statement-level **CFG construction table** (if/while/for/try-except/with/
  match/return/break/continue/del);
- the **exception model** (the one subtle bit — a fixed, sound approximation);
- the **OPAQUE hatch** (functions using `exec`/`eval`/`locals`/`globals`/wildcard
  import emit no T1 findings — implement this **before** the first T1 fixture);
- **live variables** (backward) → `dead-store`, and **may-be-unbound** (forward) →
  `read-before-init`, each with lattice, meet, and transfer functions.

Do not re-derive these; the ADR is the design.

## The typestate walk (T2)

`docs/adr/0003-protocol-spec-and-typestate.adoc` fixes the TOML protocol format
and the per-instance-key typestate semantics. The design law: *pons detects a
violation of a supplied contract; it never guesses the contract.* Matching is
deliberately syntactic (no alias analysis), which yields false **negatives**, never
false positives — the honest direction. Every T2 finding must name two concrete
lines in its `evidence_note`.

## House style

- SPDX header on every file: code `MPL-2.0`, docs `CC-BY-SA-4.0`.
- UK English; AsciiDoc for docs; TOML for config.
- `anyhow::Result`; serde on public types; **zero compiler warnings** (release +
  test), matching panic-attack policy.
- GitHub is the single source of truth. Arm every PR for auto-merge on open
  (squash by default) — the repo is configured for squash/rebase + linear
  history.

## Where to start reading

`docs/PLAN.adoc` → milestones M0–M8, each with an **exit gate**. M0 is the
workspace + a 30-minute substrate smoke test; M2 stands up the falsifier gate
**before** the catalogue grows. Follow the order; do not start N+1 until N's gate
is green.
