// SPDX-License-Identifier: MPL-2.0
//! ADR-0001 residual smoke test: each pinned grammar must load and answer a
//! trivial query. A failure here means fix the pin table, not the substrate.

use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

fn assert_parses_and_queries(language: Language, source: &str, query_src: &str) {
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .expect("grammar failed to load under the pinned tree-sitter core");

    let tree = parser.parse(source, None).expect("parse produced no tree");

    let query = Query::new(&language, query_src).expect("query failed to compile");
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut count = 0;
    while matches.next().is_some() {
        count += 1;
    }
    assert!(count >= 1, "expected at least one match, got {count}");
}

#[test]
fn python_grammar_loads_and_queries() {
    assert_parses_and_queries(
        tree_sitter_python::LANGUAGE.into(),
        "def foo():\n    return 1\n",
        "(identifier) @id",
    );
}

#[test]
fn javascript_grammar_loads_and_queries() {
    assert_parses_and_queries(
        tree_sitter_javascript::LANGUAGE.into(),
        "function foo() { return 1; }",
        "(identifier) @id",
    );
}

#[test]
fn typescript_grammar_loads_and_queries() {
    assert_parses_and_queries(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "function foo(): number { return 1; }",
        "(identifier) @id",
    );
}

#[test]
fn tsx_grammar_loads_and_queries() {
    assert_parses_and_queries(
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        "const x = <div>hi</div>;",
        "(identifier) @id",
    );
}

#[test]
fn rust_grammar_loads_and_queries() {
    assert_parses_and_queries(
        tree_sitter_rust::LANGUAGE.into(),
        "fn foo() -> i32 { 1 }",
        "(identifier) @id",
    );
}
