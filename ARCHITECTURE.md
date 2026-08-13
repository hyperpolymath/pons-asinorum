# Architecture

## Overview

*pons-asinorum* is a depth-first, multi-language static scanner that flags wasted work, self-contradiction, and missing escape hatches. The architecture is designed around the substrate decision (ADR-0001): tree-sitter native with a Rust workspace.

## Core Design Decisions

### Substrate: tree-sitter native + Rust
- **Decision**: ADR-0001 accepted tree-sitter native with Rust workspace
- **Rationale**: 
  - Evidence class must be structural, not conventional
  - T2 typestate is not expressible in Semgrep
  - T1 needs custom CFG anyway
  - Deployment as single static binary (like panic-attack)
  - Avoids LGPL-2.1 licence gravity from Semgrep

### T1 Language: Python
- **Decision**: ADR-0002 selects Python as the first language
- **Rationale**: Clear CFG + dataflow design with complete specification

### Protocol Spec and Typestate: TOML format
- **Decision**: ADR-0003 defines protocol spec format (TOML) and typestate semantics
- **Rationale**: Standardized, machine-readable specification format

## High-Level Architecture

```
.
├── docs/                  # Architecture Decision Records (ADRs) and planning
│   └── adr/               # ADR-0001 through ADR-0004
│   ├── 0001-substrate.adoc
│   ├── 0002-t1-language-and-cfg.adoc
│   ├── 0003-protocol-spec-and-typestate.adoc
│   └── 0004-companion-to-panic-attack.adoc
├── docs/PLAN.adoc          # Milestone-by-milestone implementation plan
├── docs/pons-kickoff.adoc  # Mission, species, decidability wall, evidence tiers
├── .machine_readable/      # RSR compliance infrastructure
│   ├── contractiles/      # Machine-readable contracts
│   ├── descriptiles/      # Machine-readable descriptions
│   └── scripts/           # Verification and lifecycle scripts
├── tests/                 # Test suites (planning phase)
│   ├── e2e.sh             # End-to-end validation of artefacts
│   ├── aspect_tests.sh    # Cross-cutting architectural invariants
│   └── workflows/         # CI workflow validation
├── benches/               # Benchmarks (planning phase)
│   └── pons_bench.sh      # Performance benchmarks
├── LICENSE                # MPL-2.0 for source code
├── LICENSE.adoc           # Licence documentation
├── LICENSES/              # Full licence texts
├── README.adoc            # Project documentation
└── .github/               # GitHub configuration
    ├── workflows/         # CI/CD workflows
    ├── CODEOWNERS         # No owner lines (Rule 1 - solo maintained)
    ├── FUNDING.yml        # Funding configuration
    └── dependabot.yml     # Dependency updates
```

## Component Architecture

### Planned Implementation (Post-Planning Phase)

Once implementation begins (post-v0.1.0 planning), the architecture will include:

1. **Parser Layer**
   - tree-sitter grammars for Python, JavaScript/TypeScript, Rust
   - Grammar version pinning via workspace dependencies
   - Exact grammar crate versions pinned (PLAN Appendix F)

2. **CFG + Dataflow Engine**
   - Per-language CFG extraction (Python first, per ADR-0002)
   - Reaching definitions for `read-before-init`
   - Liveness analysis for `dead-store`
   - Typestate semantics (ADR-0003)

3. **Rule Engine**
   - Evidence classes: PROTOCOL, DATAFLOW, HEURISTIC, SPECULATIVE
   - Negative corpus for each rule (falsification testing)
   - Automatic demotion/removal of rules firing on negative corpus

4. **Reporter**
   - Evidence class visualization
   - SPECULATIVE findings visually demoted
   - Multiple output formats (JSON, SARIF, human-readable)

## Evidence Classes

The core architectural invariant is the evidence class taxonomy:

- **PROTOCOL**: Findings with mathematical certainty
- **DATAFLOW**: Findings from dataflow analysis
- **HEURISTIC**: Pattern-based findings
- **SPECULATIVE**: Weakest evidence, visually demoted

Every finding carries its evidence class, and the reporter enforces visual distinction between classes, especially demoting SPECULATIVE findings.

## Data Flow

```
Input Source → Parser (tree-sitter) → AST → CFG Extraction → Dataflow Analysis
                                                      ↓
                                               Typestate Tracking → Rule Matching
                                                      ↓
                                               Finding Generation → Reporter
```

## Separation of Concerns

- **Engine**: Core analysis logic (Rust)
- **Rules**: Rule definitions and negative corpora
- **Reporter**: Output formatting and visualization
- **CLI**: Command-line interface

## Test Strategy

- **Positive fixtures**: Test that rules fire on known-bad code
- **Negative fixtures**: Test that rules do NOT fire on known-good code
- **Falsification testing**: Rules that fire on negative corpus are demoted/removed
- **Evidence class validation**: Verify correct classification

## Deployment

- Single static binary (inherited from panic-attack design)
- Standalone mode: scans arbitrary source on air-gapped machines
- No external runtime dependencies (tree-sitter grammars compiled in)

## Security Considerations

- No network access required for scanning
- Grammars pinned to exact versions
- All external dependencies audited
- No secrets in repository

## Maintainability

- All public APIs documented
- Configuration externalized
- Consistent style guidelines
- Pull requests require review and CI checks
- Issues tracked transparently

---

*Last updated: 2026-08-13*
