<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Repository-specific unblock prompts

These are handoffs, not claims that the work is already complete. Copy the relevant prompt into its repository's dedicated thread.

| Repository | Responsibility | Prompt |
|---|---|---|
| [hyperpolymath/affinescript](https://github.com/hyperpolymath/affinescript) | Compiler, module checking, host I/O and usable toolchain provenance | [01-affinescript.md](01-affinescript.md) |
| [metadatastician/standards](https://github.com/metadatastician/standards) | Authoritative format profiles and executable migration contracts | [02-standards.md](02-standards.md) |
| [hyperpolymath/ijson-jcs](https://github.com/hyperpolymath/ijson-jcs) | Strict JSON boundary and canonical byte reference | [03-ijson-jcs.md](03-ijson-jcs.md) |
| [metadatastician/kyq](https://github.com/metadatastician/kyq) | AffineScript source port, KYAML/JSON processing and CLI | [04-kyq.md](04-kyq.md) |
| [hyperpolymath/pons-asinorum](https://github.com/hyperpolymath/pons-asinorum) | Coverage, evidence exchange and downstream format adoption | [05-pons-asinorum.md](05-pons-asinorum.md) |

AffineScript's demonstrated compiler/host contract, standards' profiles and ijson-jcs's tested byte contract feed kyq. Pons can improve inventory and integrate the Rust JSON library independently; it must not wait for kyq or pretend kyq is working. Upstream tests and kyq source porting can proceed independently until their integration gates.

Observations checked 2026-10-25:
- AffineScript: `5af55f57943375e671cf9d60ec8e2f4a13e26f70`.
- Standards: `8e48b1caade08a552646adeab66558ac43b4360d`.
- ijson-jcs: `3a9a4ee326a968a32b2f152b4faec243a2d00765`. The repository currently resolves under hyperpolymath, not metadatastician.
- kyq: `4370c7637486fa8d69453d6d4731aaff81a8868b`. Its October 24 census records 3/11 sources passing check, six direct parse failures and two dependent import failures; it does not establish a working build or CLI.
- Pons PR #58 head at inspection: `e7872fb8f605f8da8779b68f8a8d11c15386c4aa`; further review fixes remain in progress.

Sources: kyq's [A1 census](https://github.com/metadatastician/kyq/blob/4370c7637486fa8d69453d6d4731aaff81a8868b/docs/status/A1_GRAMMAR_CENSUS.adoc), [strict port plan](https://github.com/metadatastician/kyq/blob/4370c7637486fa8d69453d6d4731aaff81a8868b/docs/AFFINESCRIPT_STRICT_PORT.adoc), AffineScript's [consumer on-ramp](https://github.com/hyperpolymath/affinescript/blob/5af55f57943375e671cf9d60ec8e2f4a13e26f70/docs/ON-RAMP.adoc), standards' [YAML policy](https://github.com/metadatastician/standards/blob/8e48b1caade08a552646adeab66558ac43b4360d/3-practice/YAML-POLICY.adoc) and [JSON policy](https://github.com/metadatastician/standards/blob/8e48b1caade08a552646adeab66558ac43b4360d/3-practice/JSON-POLICY.adoc).

MetaManifold-WebUI is named as a later consumer in kyq's plan. This inspection has not established a separate current blocker in that repository; do not fabricate one or make its migration a prerequisite for fixing the toolchain.
