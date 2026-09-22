<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000001
parent: null
position: 0
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# pons

**pons** — after the _pons asinorum_ (Euclid I.5, the "bridge of asses"), the
historical test that separates real understanding from rote.

pons is a lightweight, multi-language static scanner that flags the small set of
mistakes that mathematicians and computing experts spot on sight but ordinary
coders miss — **wasted work**, **self-contradiction**, and **missing escape hatches** —
and reports each finding **with an honest label for how strong the evidence is**.

> **Important**
>
> **Status: implementation in progress toward v0.1.0.** Milestones M0–M2 are
> built and merged — the engine, the finding model with its evidence classes,
> the human reporter, eight T0 rules across Python/JavaScript/TypeScript/TSX/Rust,
> and the falsifier gate that holds them honest. M3–M8 remain: JSON and SARIF
> output, Python CFG and dataflow (T1), typestate (T2), the speculative tier
> (T3), and suppression. See [[Roadmap]] for what ships when.
>
> The design is fixed and ratified (see the repository's `docs/` — kickoff,
> ADRs 0001–0005, and PLAN). Where this wiki says "will", the behaviour is
> specified and gated but not yet built; every such claim maps to an acceptance
> criterion in the plan, not to aspiration.

## The one idea

Most linters flatten everything into "warning" and "error". pons refuses to. Its
whole reason to exist is a promise:

> **Never dress a heuristic up as a proof.**

Every finding carries an **evidence class** that says how much you should trust it,
and the weakest class (`SPECULATIVE`) is visually demoted in **every** output
format. A guess never looks like a fact. See [[Evidence Tiers]].

## The two species it hunts

- **Wasted work** — the program computes something it then discards: a dead store,
  a loop whose result is never read, quadratic string-building. "Labour for
  nothing."
- **Contradiction** — the program asserts two incompatible things at once: divide
  by a literal zero, or **suppress a channel and then emit to it**. The flagship.
- (A third, smaller shape: a **missing escape hatch** — e.g. a long loop with no
  way to interrupt it.)

## Pick your path

| You are… | Start here |
|---|---|
| Someone who wants to **run pons on your code** | [[For Users]] |
| A **contributor** adding rules or working on the engine | [[For Developers]] |
| A **platform / CI maintainer** wiring pons into a pipeline | [[For Platform Maintainers]] |
| Curious about the **evidence-honesty model** | [[Evidence Tiers]] |
| Looking for **what it detects** | [[Rule Catalogue]] |
| Wondering **what ships when** | [[Roadmap]] |

## How it relates to panic-attack

pons is the **depth-first** sibling of
[panic-attack](https://github.com/hyperpolymath/panic-attack). panic-attack is
breadth (49 languages, security/panic weak points); pons is depth (a few
languages, real control-flow + dataflow + typestate, waste/contradiction
smells). They share a finding vocabulary (SARIF) and nothing else. Full
reasoning: `docs/adr/0004-companion-to-panic-attack.adoc`.

## Licence

Code `MPL-2.0`; documentation and this wiki `CC-BY-SA-4.0`. Author: Jonathan
D.A. Jewell.
