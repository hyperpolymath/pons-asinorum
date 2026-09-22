<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000003
parent: 0199c4a0-0000-7000-8000-000000000001
position: 20
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# For Users

For people who want to **run pons on a codebase** and read the results. No Rust
knowledge needed.

> **Note**
>
> pons is not built yet — this page documents the v0.1.0 command surface *as
> specified*, so it is ready the day the binary lands. Commands and flags here are
> frozen in the plan; they will work as written.

## What pons will tell you

pons scans source and reports three families of problem:

- **Wasted work** — dead stores, empty-effect loops, quadratic string building,
  values computed and thrown away.
- **Contradictions** — divide-by-zero, a channel suppressed and then emitted to,
  `if (false)` / `while (true)` with no exit.
- **Missing escape hatches** — long/unbounded loops with no way to interrupt them.

Each finding is **honestly labelled** for how much to trust it. Read
[[Evidence Tiers]] first — it is short and it is the whole point.

## Installing

pons will ship as a single static binary (no runtime, no Python, scans
air-gapped). Once released:

```text
# from a release artefact (planned)
pons --version

# or from source
cargo build --release
./target/release/pons --version
```

## Basic use

```text
pons scan path/to/project
```

That scans every recognised file (Python, JavaScript, TypeScript, Rust in
v0.1.0), auto-detected by extension, and prints a human report. Findings are
sorted by file then position. `SPECULATIVE` guesses are visually set apart and
labelled "`(heuristic — not a verdict)`".

## Reading a finding

Every finding shows you:

- **what rule** fired (e.g. `dead-store`) and its **tier** + **evidence class**;
- **where** — file, line, column;
- **what is wrong**, in one line;
- **why pons thinks so** — the `evidence_note` ("the expert would say…"). If you
  cannot evaluate a finding in context, that is a bug — every finding must carry
  this;
- **when the smell is actually fine** — the `counter_condition`, where one exists.
  This is the line that stops you deleting the tool in week one.

## The flags you will actually use

| Flag | Effect |
|---|---|
| `--format human\|json\|sarif` | Output format (default `human`). Use `sarif` for CI/IDE integration, `json` for scripting. |
| `--no-speculative` | Hide every T3 guess. Great for a first, high-signal pass. |
| `--min-evidence heuristic\|dataflow\|protocol` | Drop findings weaker than the given class. `--min-evidence dataflow` = "only show me things pons actually analysed." |
| `--tier 0\|1\|2\|3` | Run rules only **up to** this tier. `--tier 1` = fast syntactic + dataflow, no protocol/heuristic passes. |
| `--lang py\|js\|ts\|rust` | Restrict to one language (default: auto by extension). |
| `--protocol <file>` | Supply your own T2 protocol (see below). |

## Turning off a finding you have judged fine

Two ways, inline wins over config:

**Inline**, on the offending line:
```python
result = compute()  # pons:allow dead-store
```

```rust
let _x = compute(); // pons:allow dead-store
```

**Repo-level**, in `pons.toml` at the project root:
```toml
[suppress]
# by rule id, optionally scoped to path globs
dead-store = ["legacy/**"]
possible-superlinear = ["*"]
```

Precedence: inline `pons:allow` > `pons.toml` > defaults.

## The flagship check: suppress-then-emit (T2)

This is the "a logician would spot it" case: code that puts an output channel
into a **quiet** state and then **emits** to it anyway on a reachable path. pons
catches it — but **only against a protocol you supply** (it ships one toy protocol
to demonstrate). pons detects a violation of a contract; it never guesses the
contract. That honesty is deliberate — details in [[Rule Catalogue]] and, for
authors, [[For Platform Maintainers]].

## A realistic first run

```text
# high-signal pass: only things pons actually analysed, no guesses
pons scan . --min-evidence dataflow

# then widen to see the heuristics and guesses, clearly labelled
pons scan .

# machine-readable for your editor / CI
pons scan . --format sarif > pons.sarif
```
