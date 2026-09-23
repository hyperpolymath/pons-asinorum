<!-- berrywiki
id: 0199c4a0-0000-7000-8000-000000000005
parent: 0199c4a0-0000-7000-8000-000000000001
position: 40
kind: page
tags: []
archived: false
-->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# For Platform Maintainers

For people wiring pons into CI, an IDE, an estate pipeline, or a records sink.

> **Note**
>
> Not built yet; this is the integration contract as specified in the plan
> (`docs/PLAN.adoc`, Appendices E–G). Formats and exit behaviour here are frozen so
> your integration can be written ahead of the binary.

## Deployment shape

pons inherits panic-attack's **standalone** posture: a single static binary, zero
runtime dependencies, no build of the target required. It scans *arbitrary
source, lightly* — no compilation, no project database, works air-gapped and on
a USB stick. This is a deliberate design constraint (it is why CodeQL was
rejected as a backend for v0.1.0). Consequence: pons is trivially cacheable and
safe to run on untrusted code.

## Output formats

Three, all carrying the **evidence class** so your pipeline can treat guesses
differently from proofs:

- **`human`** (default) — terminal, `SPECULATIVE` visually demoted.
- **`json`** — a stable envelope (Appendix G of the plan): `schema_version`,
  `tool`, `scanned` counts, `counts.by_evidence`, and a `findings[]` array with
  fixed field order. Compatible in shape with panic-attack's JSON habit, easing a
  shared records sink.
- **`sarif`** — SARIF 2.1.0, for GitHub code scanning, IDEs, and any SARIF viewer.

## SARIF: the demotion survives (this is the important bit)

The evidence-honesty contract is enforced **into SARIF**, so even a viewer that
knows nothing about pons still shows guesses as low-confidence:

| pons concept | SARIF encoding |
|---|---|
| `SPECULATIVE` (T3) | `level: "note"` **always**, plus low `rank` (20) |
| `HEURISTIC` / `DATAFLOW` / `PROTOCOL` | `level` from severity (ERROR→error, WARN→warning, INFO→note); `rank` 50 / 80 / 70 |
| evidence class, tier, note, counter-condition | `result.properties` (`evidenceClass`, `tier`, `evidenceNote`, `counterCondition`) — round-trips |
| message | `message.text`; `SPECULATIVE` gets " (heuristic — not a verdict)" appended |
| rule metadata | `tool.driver.rules[]` reporting descriptors with `defaultConfiguration.level` and `properties.tier` |

So: a dumb consumer reads the demoted `level`/`rank`; a pons-aware consumer reads
`properties.evidenceClass` and can keep the full four-way distinction. Output is
validated against the SARIF 2.1.0 schema in CI and golden-file tested.

## Suggested CI wiring

A high-signal gate that fails the build only on things pons actually analysed,
while still surfacing the rest as annotations:

```yaml
# illustrative — pons binary pending
- name: pons scan (SARIF for annotations)
  run: pons scan . --format sarif > pons.sarif

- name: pons gate (fail only on analysed findings)
  run: pons scan . --min-evidence dataflow --format json > pons.json
  # then fail the job if findings[] with WARN/ERROR severity is non-empty
```

Rationale: `--min-evidence dataflow` restricts the **gate** to `DATAFLOW` +
`PROTOCOL` findings (real analysis), so `SPECULATIVE` guesses inform but never
block. Upload `pons.sarif` to code scanning for inline annotations of everything,
honestly labelled.

## Supplying T2 protocols (the high-value, low-noise checks)

The typestate rules (e.g. `suppress-then-emit`) fire *only against a protocol you
supply* — pons never guesses a contract. A protocol is a small TOML finite-state
machine over an abstract channel:

```toml
# your-channel.toml
schema_version = "0.1.0"
id = "quiet-channel"
states = ["open", "suppressed"]
initial = "open"

[[transition]]
on = "set_quiet"   ; from = "open"       ; to = "suppressed"
[[transition]]
on = "set_loud"    ; from = "suppressed" ; to = "open"

[[operation]]
on = "prompt" ; kind = "emit" ; forbidden_in = ["suppressed"]
message = "prompt() emits on a channel that was put in a suppressed state"
```

Then: `pons scan . --protocol your-channel.toml`. Full schema and semantics:
`docs/adr/0003-protocol-spec-and-typestate.adoc`. *Caveat to publish to your
users:* matching is syntactic (no alias analysis) — it can **miss** real bugs, but
it does not invent them.

## Version pinning and stability

- pons pins **exact** tree-sitter grammar versions; a grammar bump is a
  deliberate, corpus-gated change. Node-type names are part of the tested
  surface, so a grammar change that would alter results breaks fixtures loudly
  rather than silently drifting your findings.
- The JSON envelope and SARIF mapping carry `schema_version`; treat a bump as a
  potential breaking change for your parser.

## Estate integration (AmbientOps)

pons emits SARIF, which the estate already consumes (RSR
`static-analysis-gate.yml`, verisimdb hexads). The **only** planned coupling is
that pons findings can flow into the same records sink panic-attack feeds, with
pons's evidence class riding in `result.properties.evidenceClass`. No shared
library, no build-time dependency. See
`docs/adr/0004-companion-to-panic-attack.adoc`. Whether pons adopts the full RSR
CI template now or after v0.1.0 is an open owner decision (D4 in
`docs/OWNER-DECISIONS.adoc`).
