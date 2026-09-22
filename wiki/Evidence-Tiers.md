<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000002
parent: 0199c4a0-0000-7000-8000-000000000001
position: 10
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Evidence Tiers

This is the heart of pons. Read it once and everything else makes sense.

## Why tiers exist

The checks that **sound** most impressive are the least decidable. "Does this run
in quadratic time?" and "can this divide by zero?" are, in general, undecidable
(Rice's theorem — they are non-trivial semantic properties). A tool that answers
them with a confident red X is lying. Conversely, the checks pons **can** prove
(unreachable code after `return`, a store dead on every path) are exactly what
ordinary linters already do — just inconsistently and one language at a time.

So pons's edge is not a magic check nobody thought of. It is a *curated,
cross-language catalogue with honest labelling*. Every rule declares which of
four tiers it rests on, and the tier fixes the default **evidence class** — the
trust label that travels with the finding into every report.

## The four tiers

| Tier | Rests on | Evidence class |
|---|---|---|
| **T0 — Syntactic / structural** | Concrete syntax only (tree-sitter queries + small local checks). No control flow, no types. Cheap, language-parametric, context-blind. | `HEURISTIC` |
| **T1 — Intraprocedural dataflow** | A real per-function control-flow graph + live-variable / reaching-definition analysis. Decidable, sound **modulo** reflection / FFI / macros. | `DATAFLOW` |
| **T2 — Typestate / protocol** | A **you-supplied** protocol over an abstract channel/resource (open/close, quiet/emit). A contract violation on a reachable path. Decidable **given a correct protocol**. | `PROTOCOL` |
| **T3 — Undecidable / heuristic** | A pattern catalogue only (real complexity, general divide-by-zero, non-termination, missing interrupt). A **smell**, never a verdict. | `SPECULATIVE` |

## What the classes mean when you read a report

- `DATAFLOW` / `PROTOCOL` — pons did real analysis. If it fired, there is a path
  in your code that exhibits the property (subject to the honest caveats each
  rule prints — e.g. "no alias analysis").
- `HEURISTIC` — a syntactic match. Almost always right for its narrow pattern
  (e.g. `x / 0` with a **literal** zero), but it did not reason about reachability.
- `SPECULATIVE` — a guess. pons thinks this **might** be worth a look and is
  telling you so **without pretending to know**. In every format it is demoted:
  in the terminal it is set apart and suffixed "`(heuristic — not a verdict)`";
  in SARIF it is forced to `level: note` with a low `rank`; in JSON it carries
  `"evidence": "SPECULATIVE"`. You can hide all of them with `--no-speculative`.

## The rule that makes this real: lossy labelling

A `SPECULATIVE` finding **cannot be recovered into certainty**, and the report is
forbidden from pretending otherwise. This is a hard requirement, tested in CI
across all three output formats — not a nicety. If pons ever showed you a guess
with the same visual authority as a proof, that would be a bug in pons.

## The discipline that keeps it honest: falsifier-first

Every rule ships with **two** fixture corpora:

- a **positive** corpus — code where the smell is present and the rule **must** fire;
- a **negative** corpus — code where the smell is present **in syntax** but is
  actually fine, and the rule **must not** fire.

The negative corpus **is** the rule's falsifier. A rule that fires anywhere in its
own negative corpus is demoted (e.g. T0→T3) or removed — and this is a
build-breaking CI gate. This mirrors how an expert reads code: they spot the
smell **and** instantly know when it is fine. A tool that cannot do the second half
is noise. See [[For Developers]] for how to add a rule under this gate.

## Filtering by evidence

```text
pons scan .                       # everything, honestly labelled
pons scan . --min-evidence dataflow   # hide HEURISTIC and SPECULATIVE
pons scan . --no-speculative      # keep everything except the guesses
pons scan . --tier 1              # run only up to T1 (skip T2 protocol + T3)
```
