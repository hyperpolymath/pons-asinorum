<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Contributing to Pons

Build with `cargo build --workspace --locked`; run `just check` before submitting
a change. Every registered rule needs positive examples that trigger and negative
examples that remain quiet in `fixtures/<rule-id>/`. The falsifier gate rejects
missing or empty corpora. Update the generated catalogue with `just catalogue`.

Describe the concrete mistake, what evidence the detector uses, and when the same
syntax is legitimate. Keep messages plain and respectful. Do not infer whether
the author was a human or a model. Never upgrade evidence by changing a label.

Rust is the implementation language. Python, JavaScript, TypeScript and Rust
fixtures are analysed input, not runtime dependencies. Source is MPL-2.0;
documentation is CC-BY-SA-4.0. Add the corresponding SPDX declaration.
Submit a pull request with validation results and any remaining limitations.
