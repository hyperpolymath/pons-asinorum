<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000006
parent: 0199c4a0-0000-7000-8000-000000000001
position: 50
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Rule Catalogue

The v0.1.0 starter catalogue — ~15 rules. Each maps to one of the two species
(**wasted work** / **contradiction**) or the small **missing escape hatch** shape, and
to exactly one tier. Every rule ships positive **and** negative fixture corpora;
the negative corpus is the falsifier (see [[Evidence Tiers]]).

> **Note**
>
> Not implemented yet. This is the ratified catalogue from
> `docs/pons-kickoff.adoc`; once built, `docs/catalogue.adoc` is **generated** from
> the live rule registry and is the authoritative copy.

## T0 — syntactic / structural (`HEURISTIC`)

| Rule | Fires on | When it is actually fine (counter-condition) |
|---|---|---|
| `div-by-literal-zero` | `/` or `%` whose right-hand side is a literal `0` / `0.0`. | Sits in provably dead code (T0 can't see that → stays `HEURISTIC`, never `ERROR`). General **variable** divisor is a separate T3 rule. |
| `self-assignment` | `x = x` — same lexeme both sides, target has no side-effecting setter. | Property setters with effects (`obj.p = obj.p`); volatile reads. (In Rust, `let x = x;` is a **shadow**, not this — excluded.) |
| `constant-condition` | `if (true)`, `if (false)`, `while (false)`. | Debug / feature-flag constants edited in place. |
| `while-true-no-break` | `while (true)` / `loop` with no reachable `break` / `return` / `throw`. | Intentional daemon / event loop → `WARN` + suppressible, **never** `ERROR`. |
| `empty-effect-loop` | A loop whose body is empty or only no-ops — entered and exited for nothing. | Deliberate spin-wait on a volatile / side-effecting condition. |
| `unreachable-after-jump` | Statements textually after `return` / `throw` / `break` / `continue` in the same block. | Labels / fallthrough constructs. |
| `swallowed-error` | A `catch` / `except` whose body is empty or only `pass`. | A documented, intentional swallow (a comment, or a re-raise elsewhere). |
| `string-concat-in-loop` | Immutable-string `+=` / `+` accumulation inside a loop (quadratic build). | Tiny bounded loops; languages with mutable strings / ropes. |

## T1 — intraprocedural dataflow (`DATAFLOW`, Python in v0.1.0)

| Rule | Fires on | Counter-condition |
|---|---|---|
| `dead-store` | A local assigned a value never read on any path before reassignment or scope end (live-variable analysis). | The store is dead but the right-hand side has **side effects** → flagged `INFO` with a note to keep the call, drop the binding. Target named `_`/`_x` → suppressed. |
| `read-before-init` | A path on which a local is read before any assignment (may-be-unbound analysis) — Python raises `UnboundLocalError`. | Essentially none in Python (no default init) — which is **why** Python is the clean T1 language. `global`/`nonlocal`/free names are excluded by scope. |

## T2 — typestate / protocol (`PROTOCOL`, against a supplied contract)

| Rule | Fires on | Note |
|---|---|---|
| `suppress-then-emit` **(flagship)** | An emit to a channel on a path reachable **after** the channel was suppressed, with no intervening un-suppress — a typestate walk against your protocol. | Detects a violation of a **supplied** contract; never guesses the contract. Syntactic channel key, no alias analysis → may miss bugs, never invents them. |
| `resource-acquired-not-released` **(optional v0.1.0)** | Same shape generalised (open/close, lock/unlock) with a `must_exit_in` terminal obligation. | Identical machinery, different protocol. |

## T3 — undecidable / heuristic (`SPECULATIVE` — a smell, not a verdict)

| Rule | Fires on | Why only speculative |
|---|---|---|
| `possible-superlinear` | Nested loops depth ≥ 2 over the **same** collection, or depth ≥ 3 over data-dependent bounds. | Real complexity is undecidable (Rice). Intended pairwise work is often correct → always `INFO`. |
| `no-interrupt-on-long-op` | A loop with a data-dependent / unbounded bound and no cancellation check inside (no break-on-flag, timeout, or cooperative yield). | Heuristic over loop bound + body; bounded loops and I/O-with-timeouts are fine. |
| `general-div-by-zero` | `/` or `%` by a **variable** the analysis cannot prove non-zero. | The honest sibling of `div-by-literal-zero`: an upstream guard the local analysis can't see may make it safe. |

## The severity ceiling

No T0 rule exceeds `WARN`. `div-by-literal-zero` and `while-true-no-break` are
capped as noted. All T3 rules are `INFO`. Severity is advisory; the *evidence
class* is the load-bearing signal — see [[Evidence Tiers]].
