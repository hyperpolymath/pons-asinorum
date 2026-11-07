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
fn modern_using_declarations_remain_supported() {
    for text in [
        "using resource = acquire();",
        "async function work() { await using resource = acquire(); }",
    ] {
        let source = SourceFile {
            path: "resources.js".into(),
            lang: Lang::JavaScript,
            text: text.into(),
            transcoded: false,
        };
        assert!(
            parse::parse(&source).is_ok(),
            "rejected modern declaration: {text}"
        );
    }
}

#[test]
fn jsx_attribute_patch_does_not_relax_javascript_keywords() {
    for text in ["const in = 1;", "const view = <C value={const} />;"] {
        let source = SourceFile {
            path: "invalid-keyword.js".into(),
            lang: Lang::JavaScript,
            text: text.into(),
            transcoded: false,
        };
        assert!(
            parse::parse(&source).is_err(),
            "accepted invalid keyword: {text}"
        );
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

#[test]
fn reviewed_javascript_boundaries_preserve_valid_syntax() {
    for text in [
        "const view = <Icons.default />;",
        "const view = <default.Icons.class />;",
        "const value = {export: 1}; value.export;",
        "export const value = 1;",
        "const x = 1\rconst y = 2",
        "const x = 1 // comment\rconst y = 2",
        "const x = 1 /* comment\r */ const y = 2",
        "const x = 1\nin_value()",
        "const x = 1\ninstanceof_value()",
        "const x = 1\nin$()",
        "const x = 1\nin0()",
        "const x = 1\ninstanceof0()",
        "const x = 1\niné()",
        "const flag = key\nin object;",
        "const flag = value\ninstanceof Object;",
    ] {
        let src = SourceFile {
            path: "review.jsx".into(),
            lang: Lang::JavaScript,
            text: text.into(),
            transcoded: false,
        };
        assert!(parse::parse(&src).is_ok(), "rejected valid syntax: {text}");
    }
    for text in ["const view = <C value={export} />;", "const x = export;"] {
        let src = SourceFile {
            path: "invalid.jsx".into(),
            lang: Lang::JavaScript,
            text: text.into(),
            transcoded: false,
        };
        assert!(
            parse::parse(&src).is_err(),
            "accepted invalid syntax: {text}"
        );
    }
}

#[test]
fn comments_and_unicode_operator_boundaries_preserve_expression_structure() {
    for newline in ["\r", "\n", "\u{2028}", "\u{2029}"] {
        for (suffix, kind) in [
            ("(x)", "call_expression"),
            (".x", "member_expression"),
            ("+ x", "binary_expression"),
        ] {
            let src = SourceFile {
                path: "comment.js".into(),
                lang: Lang::JavaScript,
                text: format!("f /* comment{newline} */ {suffix};"),
                transcoded: false,
            };
            let tree = parse::parse(&src).unwrap();
            let root = tree.root_node();
            assert_eq!(
                root.named_child_count(),
                1,
                "split expression: {}",
                src.text
            );
            let statement = root.named_child(0).unwrap();
            assert_eq!(statement.kind(), "expression_statement");
            assert_eq!(
                statement.named_child(0).unwrap().kind(),
                kind,
                "{}",
                src.text
            );
        }
    }
    for space in [
        "\u{a0}", "\u{1680}", "\u{2003}", "\u{2028}", "\u{2029}", "\u{202f}", "\u{205f}",
        "\u{3000}", "\u{feff}",
    ] {
        for operator in ["in", "instanceof"] {
            let src = SourceFile {
                path: "operator.js".into(),
                lang: Lang::JavaScript,
                text: format!("value\n{operator}{space}Object;"),
                transcoded: false,
            };
            let tree = parse::parse(&src).unwrap_or_else(|error| panic!("{}: {error}", src.text));
            assert_eq!(tree.root_node().named_child_count(), 1, "{}", src.text);
            assert_eq!(
                tree.root_node()
                    .named_child(0)
                    .unwrap()
                    .named_child(0)
                    .unwrap()
                    .kind(),
                "binary_expression"
            );
        }
    }
}
