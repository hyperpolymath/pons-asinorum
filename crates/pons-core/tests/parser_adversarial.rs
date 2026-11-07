// SPDX-License-Identifier: MPL-2.0
//! Deterministic bounded mutation smoke corpus, not a claim of exhaustive fuzzing.
use pons_core::{lang::Lang, parse, source::SourceFile};

#[test]
fn jsx_keyword_attributes_are_valid_javascript() {
    // Reduced from echo-types' warrant debugger. `in` is an SVG attribute,
    // not JavaScript's binary operator. The 0.25.0 grammar rejects it.
    for text in [
        r#"const view = <svg><feMergeNode in="SourceGraphic" /></svg>;"#,
        r#"const view = <label for="field" class="hint" default="yes" />;"#,
    ] {
        let source = SourceFile {
            path: "keyword-attributes.jsx".into(),
            lang: Lang::JavaScript,
            text: text.into(),
            transcoded: false,
        };
        let tree = parse::parse(&source).expect("legal JSX keyword attributes must parse");
        assert!(!tree.root_node().has_error());
    }
}

#[test]
fn malformed_jsx_is_not_accepted_as_a_parser_fallback() {
    let source = SourceFile {
        path: "invalid.jsx".into(),
        lang: Lang::JavaScript,
        text: r#"const view = <svg><feMergeNode in= /></svg>;"#.into(),
        transcoded: false,
    };
    assert!(parse::parse(&source).is_err());
}

#[test]
fn fixed_seed_parser_mutations() {
    let mut seed = 0x504f4e53u64;
    for lang in [
        Lang::Php,
        Lang::Python,
        Lang::Rust,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::Tsx,
    ] {
        for _ in 0..128 {
            let mut text = String::from(if lang == Lang::Php { "<?php " } else { "" });
            for _ in 0..256 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                text.push((32 + (seed >> 32) as u8 % 95) as char);
            }
            let src = SourceFile {
                path: "mutant".into(),
                lang,
                text,
                transcoded: false,
            };
            // Either a valid tree or an explicit error; no panic or empty-success claim.
            if let Ok(tree) = parse::parse(&src) {
                assert!(!tree.root_node().has_error());
            }
        }
    }
}
#[test]
fn deep_ast_is_rejected_before_recursive_rules() {
    let src = SourceFile {
        path: "deep.php".into(),
        lang: Lang::Php,
        text: format!("<?php $x = {}1{};", "(".repeat(300), ")".repeat(300)),
        transcoded: false,
    };
    assert!(parse::parse(&src)
        .unwrap_err()
        .to_string()
        .contains("budget exhausted"));
}
