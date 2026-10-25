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

This page mirrors the **live registry** (`crates/pons-rules/src/registry.rs`):
13 registered rules, each shipping positive **and** negative fixture corpora;
the negative corpus is the falsifier (see [[Evidence Tiers]]). The generated,
authoritative copy is `docs/catalogue.adoc` (`pons catalogue`); the doc-drift
gate fails CI if the docs and the binary disagree.

## Registered — T0 (syntactic / structural, `HEURISTIC`)

| Rule | Fires on | When it is actually fine (counter-condition) |
|---|---|---|
| `div-by-literal-zero` | `/` or `%` whose right-hand side is a literal `0` / `0.0`. | Sits in provably dead code (T0 can't see that → stays `HEURISTIC`, never `ERROR`). |
| `self-assignment` | `x = x` — same lexeme both sides, target has no side-effecting setter. | Property setters with effects (`obj.p = obj.p`); volatile reads. (`let x = x;` in Rust is a shadow, not this — excluded.) |
| `constant-condition` | A literal boolean in a condition slot, so the branch is decided before it runs. | Debug / feature-flag constants edited in place. |
| `while-true-no-break` | An unconditional loop with no reachable `break` / `return` / `throw`. | Intentional daemon / event loop → `WARN` + suppressible, **never** `ERROR`. |
| `empty-effect-loop` | A loop whose body is empty or does nothing observable. | Deliberate spin-wait on a volatile / side-effecting condition. |
| `unreachable-after-jump` | A statement immediately after `return` / `throw` / `raise` / `break` / `continue`. | Labels / fallthrough constructs. |
| `swallowed-error` | A `catch` / `except` block that discards the error without acting on it. | A documented, intentional swallow (a comment, or a re-raise elsewhere). |
| `string-concat-in-loop` | Repeated string concatenation inside a loop — quadratic where a join is linear. | Tiny bounded loops; languages with mutable strings / ropes. |
| `php-empty-security-function` *(PHP)* | A security-named PHP function has an empty executable body. | The body carries a real check the name doesn't reveal. |

## Registered — T1 (intraprocedural dataflow, `DATAFLOW`)

| Rule | Fires on | Languages / counter-condition |
|---|---|---|
| `dead-store` | A value stored and never read before it is redefined or the function ends (live-variable analysis). | Python. The store is dead but the right-hand side has side effects → noted, keep the call drop the binding. |
| `read-before-init` | A local read on a path where it may not yet be bound (may-be-unbound analysis). Bounded correlated-guard pruning; handler-exit and loop-aware. | Python. `global`/`nonlocal`/free names excluded by scope. |
| `php-context-flow` *(PHP)* | Request data may reach an output context without a matching escape contract. | PHP; escapes/contracts the analysis can see make it silent — it never invents a violation. |
| `php-unresolved-include` *(PHP)* | A literal PHP include is empty or absent relative to the source directory. | PHP; dynamic/constructed paths are out of T0/T1 sight. |

## Registered — planned but **not** built

These come from the ratified plan and are **not registered**; no rule by these
names runs today:

- **T2 typestate** (`PROTOCOL`, against a supplied contract): `suppress-then-emit`
  (flagship), `resource-acquired-not-released`. Groundwork only: `pons trace`
  checks an explicit ordered event trace against a TOML protocol; no
  source-derived typestate rule exists.
- **T3 speculative** (`SPECULATIVE`): `possible-superlinear`,
  `no-interrupt-on-long-op`, `general-div-by-zero`.

## The severity ceiling

T0 findings can never be more than `WARN`; a T0 rule that "proves" an error is
lying about what syntax can show. Only `DATAFLOW` evidence may raise a finding
to `ERROR`, and even then the counter-condition column is part of the message.
`SPECULATIVE` findings can never fail a scan.

## Adding a rule

See [[For Developers]]. In short: the rule ships with both corpora, registers
in `registry.rs` (the single source of truth for the CLI and the falsifier
gate), and the falsifier fails the build if it ever fires in its own negative
corpus.
