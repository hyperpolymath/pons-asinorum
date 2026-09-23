// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use pons_core::engine::{Rule, RuleCtx};
use pons_core::finding::{Location, RawFinding, Severity, Tier};
use pons_core::lang::Lang;
use tree_sitter::{Query, QueryCursor, StreamingIterator};

const LANGUAGES: &[Lang] = &[
    Lang::Python,
    Lang::JavaScript,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::Rust,
];

fn query_source(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => include_str!("../../rules/div-by-literal-zero/python.scm"),
        Lang::JavaScript => include_str!("../../rules/div-by-literal-zero/javascript.scm"),
        Lang::TypeScript => include_str!("../../rules/div-by-literal-zero/typescript.scm"),
        Lang::Tsx => include_str!("../../rules/div-by-literal-zero/tsx.scm"),
        Lang::Rust => include_str!("../../rules/div-by-literal-zero/rust.scm"),
    }
}

/// `/` or `%` (and their augmented-assignment forms) whose right-hand side
/// is a literal zero. Provably dead code the local analysis can't see may
/// make this safe, so it never exceeds `WARN` — see the severity ceiling in
/// `wiki/Rule-Catalogue.asciidoc`.
pub struct DivByLiteralZero {
    queries: HashMap<Lang, Query>,
}

impl DivByLiteralZero {
    pub fn new() -> Self {
        let queries = LANGUAGES
            .iter()
            .map(|&lang| {
                let query = Query::new(&lang.tree_sitter_language(), query_source(lang))
                    .unwrap_or_else(|e| {
                        panic!("div-by-literal-zero query for {lang:?} failed to compile: {e}")
                    });
                (lang, query)
            })
            .collect();
        Self { queries }
    }
}

impl Default for DivByLiteralZero {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DivByLiteralZero {
    fn id(&self) -> &'static str {
        "div-by-literal-zero"
    }

    fn description(&self) -> &'static str {
        "division or modulo whose right-hand side is a literal zero"
    }

    fn languages(&self) -> &'static [Lang] {
        LANGUAGES
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let Some(query) = self.queries.get(&ctx.lang) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, ctx.tree.root_node(), ctx.text.as_bytes());
        let mut findings = Vec::new();

        while let Some(m) = matches.next() {
            let mut lhs = None;
            let mut rhs = None;
            let mut expr = None;
            for cap in m.captures() {
                match query.capture_names()[cap.index as usize] {
                    "lhs" => lhs = Some(cap.node),
                    "rhs" => rhs = Some(cap.node),
                    "expr" => expr = Some(cap.node),
                    _ => {}
                }
            }
            let (Some(lhs), Some(rhs), Some(expr)) = (lhs, rhs, expr) else {
                continue;
            };

            // Python's `%` doubles as string formatting: `"%d" % 0` is not
            // modulo-by-zero at all.
            if ctx.lang == Lang::Python && lhs.kind() == "string" {
                continue;
            }

            let rhs_text = rhs.utf8_text(ctx.text.as_bytes()).unwrap_or_default();
            if !is_literal_zero(rhs_text) {
                continue;
            }

            findings.push(RawFinding::new(
                Tier::T0,
                Severity::Warn,
                Location::from_node(ctx.path.display().to_string(), &expr, ctx.text),
                "division or modulo by a literal zero",
                format!("right-hand side `{rhs_text}` is a constant zero"),
                Some(
                    "sits in provably dead code the local analysis can't see — stays a \
                     heuristic, never a hard error"
                        .to_string(),
                ),
            ));
        }

        findings
    }
}

/// True if `text` is a numeric literal whose value is zero, across every
/// grammar this rule covers: plain decimals and floats (all languages),
/// hex/octal/binary integers and Rust's `iN`/`uN`/`fN`/`size` suffixes, and
/// JavaScript's trailing `n` bigint suffix.
fn is_literal_zero(text: &str) -> bool {
    let cleaned = text.replace('_', "").to_ascii_lowercase();
    let without_bigint = cleaned.strip_suffix('n').unwrap_or(&cleaned);

    const RUST_SUFFIXES: &[&str] = &[
        "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
        "f64", "f32",
    ];
    let mut body = without_bigint;
    for suffix in RUST_SUFFIXES {
        if let Some(stripped) = body.strip_suffix(suffix) {
            body = stripped;
            break;
        }
    }

    if let Some(hex) = body.strip_prefix("0x") {
        return !hex.is_empty() && i128::from_str_radix(hex, 16).is_ok_and(|v| v == 0);
    }
    if let Some(oct) = body.strip_prefix("0o") {
        return !oct.is_empty() && i128::from_str_radix(oct, 8).is_ok_and(|v| v == 0);
    }
    if let Some(bin) = body.strip_prefix("0b") {
        return !bin.is_empty() && i128::from_str_radix(bin, 2).is_ok_and(|v| v == 0);
    }

    body.parse::<f64>().is_ok_and(|v| v == 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_zero_in_every_literal_shape() {
        for text in [
            "0", "0.0", "0.", ".0", "0e0", "00", "0x0", "0X0", "0o0", "0b0", "0u32", "0_i64",
            "0f64", "0n",
        ] {
            assert!(is_literal_zero(text), "expected {text:?} to be zero");
        }
    }

    #[test]
    fn does_not_recognise_nonzero_literals() {
        for text in ["1", "0.1", "10", "0x1", "1u32", "2n", "0.0001"] {
            assert!(!is_literal_zero(text), "expected {text:?} to be nonzero");
        }
    }
}
