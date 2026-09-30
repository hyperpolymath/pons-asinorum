// SPDX-License-Identifier: MPL-2.0

//! What the pinned Python grammar actually produces.
//!
//! ADR-0002 is the design authority for the CFG, and it names node kinds and
//! field names in prose. Prose is not a measurement. This probe parses one
//! corpus exercising every construct in the ADR's table and asserts the kinds
//! and fields the builder will depend on, so that a grammar bump which renames
//! or removes one of them fails *here*, naming the thing that moved, rather
//! than silently turning a CFG edge into a no-op.
//!
//! Several assertions below record facts ADR-0002 does **not** state, each of
//! which changes what the builder must do. They are called out in the test
//! names rather than buried in comments. Two of them contradict the plan
//! outright and were found by running this probe:
//!
//! * `except*` does **not** produce an error node (see
//!   [`except_star_parses_clean_so_has_error_cannot_be_its_opaque_trigger`]);
//! * the `except … as e` binding is **not** on `except_clause.alias` (see
//!   [`the_except_as_binding_is_nested_in_an_as_pattern_not_a_field_of_the_clause`]),
//!   even though the grammar's own `node-types.json` advertises such a field —
//!   a reminder that the generated schema is not what the parser emits.
//!
//! The corpus is an embedded raw string rather than a `.py` file on purpose:
//! Python is banned outside `fixtures/` estate-wide, and a probe corpus is not
//! a fixture.

use std::collections::BTreeSet;

use tree_sitter::{Node, Parser, Tree};

use super::is_except_group;
use crate::lang::Lang;

/// Every construct in ADR-0002's table, in one parse-clean module.
const CORPUS: &str = r#"
import os
from sys import path as syspath

GLOBAL_NAME = 0


@decorator
class Widget:
    kind: str = "widget"

    def method(self, a, b=1, *args, **kwargs):
        return a + b


class Sub(Widget):
    pass


def plain(a, b):
    total: int = 0
    annotated_only: int
    total += a

    if a > b:
        total = a
    elif a == b:
        total = b
    else:
        total = 0

    while total > 0:
        total -= 1
        if total == 3:
            break
        continue
    else:
        total = -1

    for item in range(a):
        total += item
    else:
        total += 100

    try:
        risky = compute(a)
    except ValueError as exc:
        total = len(str(exc))
    except (TypeError, KeyError):
        total = 0
    else:
        total += risky
    finally:
        cleanup(force=True)

    with open("f") as handle:
        data = handle.read()

    del data

    match total:
        case 0:
            label = "zero"
        case [first, *rest] if first > 0:
            label = "positive-head"
        case {"key": value}:
            label = value
        case Widget(kind=k):
            label = k
        case _:
            label = "other"

    squares = [n * n for n in range(a) if n]
    gen = (n for n in range(b))
    pairs = {k: v for k, v in items}
    uniq = {n for n in range(a)}

    if (found := lookup(a)) is not None:
        total += found

    fn = lambda x: x + 1
    assert total >= 0, "non-negative"
    raise SystemExit(total)


async def fetch(url):
    async with session() as s:
        async for chunk in s.stream(url):
            yield chunk
        result = await s.get(url)
    return result


def generator():
    yield 1
    received = yield
    return received


def scopes():
    global GLOBAL_NAME
    captured = 1

    def inner():
        nonlocal captured
        captured = 2

    if GLOBAL_NAME:
        pass

    inner()
    return captured
"#;

fn parse_python(text: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&Lang::Python.tree_sitter_language())
        .expect("the pinned tree-sitter-python grammar loads");
    parser
        .parse(text, None)
        .expect("tree-sitter produced a tree")
}

/// Every *named* kind appearing anywhere in the tree.
fn kinds(tree: &Tree) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut cursor = tree.walk();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_named() {
            found.insert(node.kind().to_string());
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    found
}

/// Depth-first search for the first node of `kind`.
fn find<'t>(root: Node<'t>, kind: &str) -> Option<Node<'t>> {
    find_where(root, &|n| n.kind() == kind)
}

fn find_where<'t>(root: Node<'t>, pred: &dyn Fn(Node<'t>) -> bool) -> Option<Node<'t>> {
    if pred(root) {
        return Some(root);
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(hit) = find_where(child, pred) {
            return Some(hit);
        }
    }
    None
}

/// Which of `candidates` this node actually exposes, in the order given.
fn field_names(node: Node<'_>, candidates: &[&str]) -> Vec<String> {
    candidates
        .iter()
        .filter(|name| node.child_by_field_name(*name).is_some())
        .map(|name| (*name).to_string())
        .collect()
}

fn handlers_of_the_corpus_try(tree: &Tree) -> Vec<Node<'_>> {
    let try_stmt = find(tree.root_node(), "try_statement").expect("corpus has a try");
    let mut cursor = try_stmt.walk();
    try_stmt
        .children(&mut cursor)
        .filter(|c| c.kind() == "except_clause")
        .collect()
}

#[test]
fn the_probe_corpus_parses_without_error() {
    // The positive control for every other test in this file. If the corpus
    // itself is malformed, "kind absent" below would mean "I wrote bad Python",
    // not "the grammar changed".
    let tree = parse_python(CORPUS);
    assert!(
        !tree.root_node().has_error(),
        "the probe corpus must parse clean, or every assertion below is measuring my typo"
    );
}

#[test]
fn every_node_kind_the_builder_depends_on_is_produced_by_this_grammar() {
    let tree = parse_python(CORPUS);
    let present = kinds(&tree);

    let expected = [
        // units
        "module",
        "function_definition",
        "lambda",
        "class_definition",
        "block",
        "parameters",
        "decorated_definition",
        // branches
        "if_statement",
        "elif_clause",
        "else_clause",
        // loops
        "while_statement",
        "for_statement",
        // exceptions
        "try_statement",
        "except_clause",
        "finally_clause",
        "raise_statement",
        // the `as` binding, shared by `except` and `with`
        "as_pattern",
        "as_pattern_target",
        // context managers
        "with_statement",
        "with_clause",
        "with_item",
        // binding and unbinding
        "assignment",
        "augmented_assignment",
        "delete_statement",
        "named_expression",
        "global_statement",
        "nonlocal_statement",
        // structural pattern matching
        "match_statement",
        "case_clause",
        // comprehensions
        "list_comprehension",
        "generator_expression",
        "dictionary_comprehension",
        "set_comprehension",
        "for_in_clause",
        // jumps
        "return_statement",
        "break_statement",
        "continue_statement",
        "pass_statement",
        // suspensions
        "yield",
        "await",
        // misc the builder walks through
        "expression_statement",
        "call",
        "identifier",
        "attribute",
        "assert_statement",
        "import_statement",
        "import_from_statement",
    ];

    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|k| !present.contains(*k))
        .collect();

    assert!(
        missing.is_empty(),
        "the pinned grammar no longer produces these kinds, which the CFG builder depends on: {missing:?}"
    );
}

#[test]
fn try_with_and_delete_hide_their_parts_from_child_by_field_name() {
    // ⚠ ADR-0002 does not state this, and it decides how the builder is written.
    // `try_statement` exposes only `body` as a named field; `with_statement`
    // only `body`; `delete_statement` exposes nothing at all. So handlers,
    // `else`, `finally`, the `with` items and the `del` targets must be found
    // by iterating children and matching on *kind*. A builder reaching for
    // `child_by_field_name("handlers")` gets `None` and silently drops every
    // exception edge — a whole analysis quietly turning into a no-op.
    let tree = parse_python(CORPUS);
    let root = tree.root_node();

    let try_stmt = find(root, "try_statement").expect("corpus has a try");
    assert_eq!(
        field_names(
            try_stmt,
            &[
                "body",
                "handlers",
                "alternative",
                "finalizer",
                "else",
                "finally"
            ]
        ),
        vec!["body".to_string()],
        "try_statement exposes a field the builder could have used"
    );
    // ...but the parts are all reachable by kind.
    assert!(find(try_stmt, "except_clause").is_some());
    assert!(find(try_stmt, "finally_clause").is_some());
    assert!(find(try_stmt, "else_clause").is_some());

    let with_stmt = find(root, "with_statement").expect("corpus has a with");
    assert_eq!(
        field_names(with_stmt, &["body", "items", "clause", "alias"]),
        vec!["body".to_string()],
        "with_statement exposes a field the builder could have used"
    );
    assert!(find(with_stmt, "with_item").is_some());

    let del = find(root, "delete_statement").expect("corpus has a del");
    assert_eq!(
        field_names(del, &["target", "targets", "left", "value", "body"]),
        Vec::<String>::new(),
        "delete_statement exposes a field the builder could have used"
    );
    assert!(
        del.named_child_count() >= 1,
        "the del target must still be reachable positionally"
    );
}

#[test]
fn while_else_exists_even_though_the_adr_table_omits_it() {
    // ⚠ Gap-fill. ADR-0002's table gives `for/else` and is silent on
    // `while/else`. The grammar has it, so the builder must mirror the
    // `for/else` edge shape or the else-block becomes unreachable in the CFG
    // and every store in it reads as dead.
    let tree = parse_python(CORPUS);
    let while_stmt = find(tree.root_node(), "while_statement").expect("corpus has a while/else");

    assert!(
        while_stmt.child_by_field_name("alternative").is_some(),
        "while_statement lost its `alternative` field; while/else edges need rewriting"
    );
    assert_eq!(
        field_names(while_stmt, &["condition", "body", "alternative"]),
        vec![
            "condition".to_string(),
            "body".to_string(),
            "alternative".to_string()
        ]
    );
}

#[test]
fn a_bare_annotation_is_an_assignment_that_binds_nothing() {
    // ⚠ Gap-fill, and a false-negative source if missed. `x: int` parses as an
    // `assignment` carrying `left` and `type` but **no** `right`. It declares a
    // type; it does not bind a value. Counting it as a definition would make
    // `read-before-init` silently miss every read of an annotated-but-unassigned
    // local — the exact bug the rule exists to find.
    let tree = parse_python(CORPUS);
    let root = tree.root_node();

    let bare = find_where(root, &|n: Node<'_>| {
        n.kind() == "assignment"
            && n.child_by_field_name("left")
                .and_then(|l| l.utf8_text(CORPUS.as_bytes()).ok())
                == Some("annotated_only")
    })
    .expect("corpus has a bare annotation");

    assert!(
        bare.child_by_field_name("type").is_some(),
        "the bare annotation lost its `type` field"
    );
    assert!(
        bare.child_by_field_name("right").is_none(),
        "a bare annotation must not carry `right`, or it would read as a binding"
    );

    // Paired control: the annotated *assignment* two lines above does bind, and
    // carries all three. Without this leg the assertion above would pass just as
    // happily on a grammar that had dropped `right` everywhere.
    let bound = find_where(root, &|n: Node<'_>| {
        n.kind() == "assignment"
            && n.child_by_field_name("left")
                .and_then(|l| l.utf8_text(CORPUS.as_bytes()).ok())
                == Some("total")
            && n.child_by_field_name("type").is_some()
    })
    .expect("corpus has an annotated assignment that binds");

    assert!(
        bound.child_by_field_name("right").is_some(),
        "`total: int = 0` must carry `right` — it binds"
    );
}

#[test]
fn except_star_parses_clean_so_has_error_cannot_be_its_opaque_trigger() {
    // ⚠⚠ MEASURED, AND IT CONTRADICTS THE PLAN. ADR-0002's implementation plan
    // assumed PEP 654 `except*` is unrepresentable in the pinned grammar and so
    // would land in an ERROR node, caught by the `has_error()` OPAQUE trigger.
    // It does not. `tree-sitter-python` 0.25.0 accepts `except*` and demotes the
    // `*` to an anonymous child, leaving a *named* tree shape indistinguishable
    // from plain `except`. `has_error()` is false.
    //
    // That makes this the dangerous case rather than the safe one: a group
    // handler has genuinely different control flow from an ordinary one, so a
    // builder that cannot tell them apart emits a confident, plausible, wrong
    // CFG instead of refusing the function. Hence the dedicated structural
    // detector, asserted here against both arms.
    let starred = parse_python("try:\n    pass\nexcept* ValueError:\n    pass\n");
    assert!(
        !starred.root_node().has_error(),
        "`except*` now produces an error node — the has_error() trigger would cover it, \
         and is_except_group() may be redundant; re-check before deleting it"
    );
    assert!(
        !kinds(&starred).contains("except_group_clause"),
        "the grammar gained a dedicated `except_group_clause`; the detector should use it"
    );

    let starred_clause =
        find(starred.root_node(), "except_clause").expect("parsed as except_clause");
    assert!(
        is_except_group(starred_clause),
        "the `*` must remain detectable as an anonymous child, or `except*` becomes \
         invisible and gets analysed as an ordinary handler"
    );

    // Paired control: an ordinary handler must NOT trip the detector, or every
    // try/except in the corpus would be forced opaque and both T1 rules would
    // go vacuously silent while still passing their own tests.
    let plain = parse_python("try:\n    pass\nexcept ValueError:\n    pass\n");
    let plain_clause = find(plain.root_node(), "except_clause").expect("parsed as except_clause");
    assert!(
        !is_except_group(plain_clause),
        "an ordinary `except` must not read as a group handler"
    );
    for handler in handlers_of_the_corpus_try(&parse_python(CORPUS)) {
        assert!(
            !is_except_group(handler),
            "no handler in the probe corpus is a group handler"
        );
    }
}

#[test]
fn the_except_as_binding_is_nested_in_an_as_pattern_not_a_field_of_the_clause() {
    // ⚠⚠ MEASURED, AND `node-types.json` IS MISLEADING HERE. The generated
    // schema advertises an `alias` field on `except_clause`, so a builder
    // written from the schema would call
    // `except_clause.child_by_field_name("alias")`, get `None`, and silently
    // never bind `e` at handler entry — turning every `except E as e` into a
    // `read-before-init` false positive on `e`.
    //
    // What the parser actually emits is
    //   (except_clause value: (as_pattern (identifier)
    //                                     alias: (as_pattern_target (identifier))))
    // so the binding is one level down, inside `value`.
    let tree = parse_python(CORPUS);
    let handlers = handlers_of_the_corpus_try(&tree);
    assert_eq!(
        handlers.len(),
        2,
        "corpus has one aliased and one bare handler"
    );

    let aliased = handlers[0];
    assert!(
        aliased.child_by_field_name("alias").is_none(),
        "the schema's `alias` field is not what the parser emits here; if this starts \
         passing, simplify the binding lookup below"
    );

    let value = aliased
        .child_by_field_name("value")
        .expect("an aliased handler exposes `value`");
    assert_eq!(
        value.kind(),
        "as_pattern",
        "`except E as e` wraps its exception type in an as_pattern"
    );
    let target = value
        .child_by_field_name("alias")
        .expect("the as_pattern carries the binding in `alias`");
    assert_eq!(target.kind(), "as_pattern_target");
    assert_eq!(target.utf8_text(CORPUS.as_bytes()).unwrap(), "exc");

    // Paired control: a bare handler binds nothing, and its `value` is the
    // exception tuple itself rather than an as_pattern.
    let bare = handlers[1];
    let bare_value = bare
        .child_by_field_name("value")
        .expect("a bare handler still exposes `value`");
    assert_ne!(
        bare_value.kind(),
        "as_pattern",
        "`except (TypeError, KeyError):` binds no name"
    );
}

#[test]
fn the_fields_the_builder_will_use_are_all_present() {
    let tree = parse_python(CORPUS);
    let root = tree.root_node();

    let cases: &[(&str, &[&str])] = &[
        ("for_statement", &["left", "right", "body", "alternative"]),
        ("if_statement", &["condition", "consequence", "alternative"]),
        ("function_definition", &["name", "parameters", "body"]),
        ("lambda", &["parameters", "body"]),
        ("named_expression", &["name", "value"]),
        ("for_in_clause", &["left", "right"]),
        ("case_clause", &["consequence"]),
        ("match_statement", &["subject", "body"]),
        ("augmented_assignment", &["left", "operator", "right"]),
        ("elif_clause", &["condition", "consequence"]),
        ("else_clause", &["body"]),
        ("with_item", &["value"]),
        ("decorated_definition", &["definition"]),
        ("attribute", &["object"]),
        ("keyword_argument", &["value"]),
        ("default_parameter", &["value"]),
    ];

    for (kind, fields) in cases {
        let node = find(root, kind).unwrap_or_else(|| panic!("corpus has no {kind}"));
        let present = field_names(node, fields);
        assert_eq!(
            present.len(),
            fields.len(),
            "{kind} is missing fields the builder uses: expected {fields:?}, found {present:?}"
        );
    }
}

/// The fields above sit on the first node of their kind. These three sit only
/// on a later one — a guarded `case`, a subclass, the `with … as` binding —
/// so each is found by shape rather than by first occurrence.
/// What is sought, the field it must expose, and how to recognise it.
type ShapedCase<'a> = (&'a str, &'a str, &'a dyn Fn(Node<'_>) -> bool);

#[test]
fn the_fields_only_some_nodes_of_a_kind_carry_are_present() {
    let tree = parse_python(CORPUS);
    let root = tree.root_node();

    let cases: &[ShapedCase] = &[
        ("guarded case_clause", "guard", &|n| {
            n.kind() == "case_clause" && n.utf8_text(CORPUS.as_bytes()).unwrap().contains(" if ")
        }),
        ("subclass class_definition", "superclasses", &|n| {
            n.kind() == "class_definition"
                && n.utf8_text(CORPUS.as_bytes())
                    .unwrap()
                    .starts_with("class Sub")
        }),
        ("with_item as_pattern", "alias", &|n| {
            n.kind() == "as_pattern" && n.parent().is_some_and(|p| p.kind() == "with_item")
        }),
    ];

    for (what, field, pred) in cases {
        let node = find_where(root, *pred).unwrap_or_else(|| panic!("corpus has no {what}"));
        assert!(
            node.child_by_field_name(field).is_some(),
            "{what} does not expose `{field}`, which the builder reads"
        );
    }
}
