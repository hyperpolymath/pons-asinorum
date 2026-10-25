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

pons is **in implementation and daily verification**: the engine, the T0 and T1
catalogues, the reporters, suppression and the CLI surface are shipped and
gated; the T2 typestate rules and the T3 speculative tier are not. The full
milestone plan with exit gates lives in `docs/PLAN.adoc`; the strategic
exnovation roadmap (proof obligations, statistics, legacy-language scanning)
lives in `docs/EXNOVATION-ROADMAP.adoc`. This page is the summary, reconciled
against the live registry.

## Shipped (verified by CI, not asserted)

| Area | State | Evidence |
|---|---|---|
| M0 — Cargo workspace (4 crates) + grammar pins | ✅ | `cargo build --workspace` green in CI |
| M1 — `Finding` model + engine + human reporter with evidence-tier demotion | ✅ | unit tests |
| M3 — JSON + SARIF reporters (schema-versioned; SARIF invocation notifications for skips) | ✅ | golden-file tests; e2e |
| M4 — Python CFG + dataflow + T1 rules `dead-store`, `read-before-init` | ✅ | CFG snapshot tests; falsifier gate |
| 8 T0 rules across Python, JavaScript, TypeScript, TSX, Rust | ✅ | falsifier gate over positive+negative corpora |
| PHP family — 6th grammar; `php-context-flow` (T1), `php-empty-security-function` (T0), `php-unresolved-include` (T0) | ✅ | same gate |
| M7 — Suppression (inline `pons:allow` comments + `pons.toml` project allowances) | ✅ | unit + e2e tests |
| Generated `docs/catalogue.adoc` + `pons catalogue` / `man` / `doctor` / `trace` commands | ✅ | doc-drift gate (docs must match the live binary) |
| Explicit ordered trace-protocol checker (`pons trace` + TOML protocols) | ✅ | e2e good/bad trace fixtures |
| ADR-0005 exit-code matrix; broken-pipe tolerance; `--fail-on`, `--fail-on-skips` | ✅ | e2e suite |
| Estate deed-era metadata: `pons-asinorum_chora.deed`, `0-AI-MANIFEST.deed`, `coordination.k9`; contractile tridents validated against K9 contract v1.0.0 at L1 only | Metadata present; CI conformance pending | Local deed lint and K9 L1 structural validation recorded in the workflow files; L2/L3 not run; `deed-conformance` and `k9-conformance` are status-only stubs pending estate enablement |

## Remaining for v0.1.0

| M | Deliverable | Exit gate |
|---|---|---|
| M5 | T2 typestate rules (`suppress-then-emit` flagship) against a supplied protocol | fires on the violation, silent on the compliant sequence; two-line witness |
| M6 | T3 speculative rules + `SPECULATIVE` plumbing end-to-end | `--no-speculative` removes exactly the T3 findings; demoted everywhere |
| M8 | Acceptance sweep on a mixed corpus | every kickoff acceptance box ticked ⇒ tag `v0.1.0` |

Groundwork exists for M5: `pons-protocols` parses TOML protocol contracts and
`pons trace` checks ordered event traces against them — but **no source-derived
typestate rule is registered**, and none is claimed.

## The precision debt (open, measured, not hidden)

Issue #47 measured the flatbuffers run: **3 of 78 findings true positives**
(Wilson 95% CI ≈ [1.3%, 10.7%]). Nothing in this roadmap changes that number,
and no release may be called production-ready until the statistical evaluation
campaign in `docs/EXNOVATION-ROADMAP.adoc` (§ "Statistical plan") has run with
pre-registered corpus sizes (n ≥ 385 for ±5pp at 95%).

## Explicitly out of scope for v0.1.0

Interprocedural dataflow · build-system integration / compilation · real
complexity bounds (RAML / amortised analysis) · auto-fix / rewriting · competing
with Semgrep on its own ground.

## Beyond v0.1.0 (noted, not committed)

- More T1 languages — Rust as an **analysed** target once a `Drop`-awareness story
  exists (needs some type resolution).
- More shipped protocols and an `args` predicate in the protocol matcher.
- Supervised scanning of COBOL / FORTRAN / BASIC / C / C++ legacy estates —
  staged in `docs/EXNOVATION-ROADMAP.adoc`, Phase 3+.
- Incorrectness-logic counterexamples, CEGAR-style refinement, typestate with
  aliasing — proof obligations in `docs/EXNOVATION-ROADMAP.adoc`.
- An optional `--backend codeql` deep mode that could upgrade selected T3 rules
  to `DATAFLOW` — noted in ADR-0001, not planned.
- Estate records-sink integration via SARIF (ADR-0004).

## Owner decisions

Tracked in `docs/OWNER-DECISIONS.adoc`. Ratified 2026-07-08: name `pons`;
licences MPL-2.0-or-later (code) / CC-BY-SA-4.0 (docs); public; author Jonathan
D.A. Jewell. D4 (adopt estate CI now or after v0.1.0): the deed-conformance
and k9-conformance estate lanes remain pending estate enablement; their
workflows currently report status only. Contract conformance is validated
locally at L1 only; L2 and L3 have not been run.
