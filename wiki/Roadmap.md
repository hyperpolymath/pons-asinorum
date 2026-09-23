<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000007
parent: 0199c4a0-0000-7000-8000-000000000001
position: 60
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Roadmap

pons is **in implementation**: M0–M2 are merged, M3–M8 remain. The full milestone
plan with exit gates lives in `docs/PLAN.adoc`; this is the summary.

## v0.1.0 milestones

Each milestone has an **exit gate**; N+1 does not start until N is green.

| M | Deliverable | Exit gate |
|---|---|---|
| M0 ✅ | Cargo workspace (4 crates) + grammar pins + substrate smoke test | build green + all four grammars parse & query |
| M1 ✅ | `Finding` model + engine skeleton + human reporter (with demotion) | `pons scan` prints an empty report; `SPECULATIVE` demoted in a unit test | M2 |
| **T0 catalogue** (rules 1–8) across Python/JS/TS/Rust + **the falsifier gate** | every T0 rule fires on all positives, zero on negatives (CI) | M3 | JSON + SARIF reporters |
| golden-file + SARIF-schema validation; demotion survives all three formats | M4 | Python CFG + dataflow + **T1 rules** 9–10 |
| corpora pass; CFG snapshots green; `exec(`-containing function → zero T1 findings (OPAQUE) | M5 | T2 typestate + **flagship** `suppress-then-emit` + toy protocol |
| fires on suppress-then-emit, silent on suppress→unsuppress→emit; two-line witness | M6 | **T3 rules** 13–15 + `SPECULATIVE` plumbing + `--no-speculative` | `--no-speculative` removes exactly the T3 findings; demoted everywhere |
| M7 | Suppression (inline + `pons.toml`) + CLI polish + generated `docs/catalogue.adoc` | suppression works both ways; catalogue drift test green | M8 |
| Acceptance sweep on a mixed corpus | every kickoff acceptance box ticked ⇒ **tag v0.1.0** |  |

## The v0.1.0 acceptance gate (definition of done)

Tag `v0.1.0` only when **all** hold:

- Scans a mixed directory (Python + JS/TS + Rust) → human, JSON, and SARIF.
- Every T0 rule fires on all its positives and none of its negatives (CI gate).
- ≥2 T1 rules working on one language with a real CFG + dataflow.
- One T2 protocol demo (`suppress-then-emit`) end-to-end against the toy
  protocol.
- Every finding carries `rule_id`, `tier`, `evidence`, a `location` span, a
  one-line `message`, and a non-empty `evidence_note`.
- All T3 findings visibly non-authoritative in every format.
- Suppression via inline comment **and** `pons.toml`.
- `docs/adr/0001-substrate.adoc` records the substrate decision; `catalogue.adoc`
  generated from the registry.

## Explicitly out of scope for v0.1.0

Interprocedural dataflow · build-system integration / compilation · real
complexity bounds (RAML / amortised analysis) · auto-fix / rewriting · competing
with Semgrep on its own ground.

## Beyond v0.1.0 (noted, not committed)

- More T1 languages — Rust as an **analysed** target once a `Drop`-awareness story
  exists (needs some type resolution).
- More shipped protocols and an `args` predicate in the protocol matcher.
- An optional `--backend codeql` deep mode that could upgrade selected T3 rules
  to `DATAFLOW` — noted in ADR-0001, not planned.
- Estate records-sink integration via SARIF (ADR-0004).

## Open owner decisions

Tracked in `docs/OWNER-DECISIONS.adoc`. Ratified 2026-07-08: name `pons`;
licences MPL-2.0 (code) / CC-BY-SA-4.0 (docs); public; author Jonathan D.A.
Jewell. Still open: D4 — whether to adopt the full estate RSR CI template now or
after v0.1.0.
