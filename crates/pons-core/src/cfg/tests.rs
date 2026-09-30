// SPDX-License-Identifier: MPL-2.0

//! CFG snapshots and OPAQUE-hatch controls.
//!
//! Inputs are embedded raw strings: Python is banned outside `fixtures/`.
//! Snapshots use `report::assert_golden`; bless with `PONS_BLESS=1`, and only
//! after reading the diff.

use std::path::Path;

use tree_sitter::Parser;

use super::{build_units, EventKind, FunctionCfg, FunctionUnit, OpaqueReason};
use crate::report::assert_golden;

fn units(src: &str) -> Vec<FunctionUnit> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(src, None).unwrap();
    build_units(Path::new("t.py"), src, &tree)
}

fn only_cfg(src: &str) -> FunctionCfg {
    let mut us = units(src);
    assert_eq!(us.len(), 1, "expected exactly one unit");
    match us.remove(0) {
        FunctionUnit::Analysable(cfg) => cfg,
        FunctionUnit::Opaque { reason, .. } => panic!("unexpectedly opaque: {reason}"),
    }
}

fn opaque_reason(src: &str) -> Option<OpaqueReason> {
    match units(src).remove(0) {
        FunctionUnit::Analysable(_) => None,
        FunctionUnit::Opaque { reason, .. } => Some(reason),
    }
}

fn snapshot(name: &str, src: &str, expected: &str) {
    let cfg = only_cfg(src);
    assert_golden(name, &cfg.render(src), expected);
}

// ---- snapshots ADR-0002 requires ------------------------------------------

#[test]
fn cfg_for_else() {
    snapshot(
        "cfg_for_else.txt",
        r#"def f(xs):
    for x in xs:
        if x:
            break
        y = x
    else:
        y = 0
    return x
"#,
        include_str!("../../tests/golden/cfg_for_else.txt"),
    );
}

#[test]
fn cfg_try_except_finally() {
    snapshot(
        "cfg_try_except_finally.txt",
        r#"def f():
    try:
        a = g()
        b = a
    except E as e:
        print(e, a)
    else:
        c = 1
    finally:
        cleanup(a)
    return a
"#,
        include_str!("../../tests/golden/cfg_try_except_finally.txt"),
    );
}

#[test]
fn cfg_comprehension_walrus() {
    snapshot(
        "cfg_comprehension_walrus.txt",
        r#"def f(xs):
    x = 1
    ys = [x for x in xs if (n := x)]
    return n, x, ys
"#,
        include_str!("../../tests/golden/cfg_comprehension_walrus.txt"),
    );
}

// ---- gap-fills --------------------------------------------------------------

#[test]
fn cfg_while_true_has_no_false_edge() {
    snapshot(
        "cfg_while_true.txt",
        r#"def f():
    while True:
        v = g()
        if v:
            break
    else:
        v = 0
    return v
"#,
        include_str!("../../tests/golden/cfg_while_true.txt"),
    );
}

#[test]
fn cfg_jumps_run_a_copy_of_finally() {
    snapshot(
        "cfg_finally_jumps.txt",
        r#"def f(xs):
    for x in xs:
        try:
            if x:
                continue
            return x
        finally:
            done(x)
    return None
"#,
        include_str!("../../tests/golden/cfg_finally_jumps.txt"),
    );
}

#[test]
fn cfg_match_arms_and_captures() {
    snapshot(
        "cfg_match.txt",
        r#"def f(s):
    match s:
        case Point(x=0, y=b) as p:
            r = b
        case [a, *rest] if a:
            r = a
        case _:
            r = None
    return r
"#,
        include_str!("../../tests/golden/cfg_match.txt"),
    );
}

// ---- the try edges, asserted directly --------------------------------------

/// Both raise edges into a handler must exist: from `try_entry` (the pre-try
/// state, for may-unbound) and from the body's tail (so a store in the body
/// stays live if the handler reads it). Either alone breaks an ADR-0002
/// corpus case, so each is asserted here as well as in the snapshot.
#[test]
fn a_handler_is_reached_from_try_entry_and_from_the_body_tail() {
    let src = r#"def f():
    try:
        a = 1
    except E:
        use(a)
"#;
    let cfg = only_cfg(src);
    let preds = cfg.preds();
    let handler = cfg
        .blocks()
        .iter()
        .position(|b| {
            b.stmts
                .first()
                .is_some_and(|s| src[s.byte_start..].starts_with("except"))
        })
        .expect("a block starting with the except clause");
    let def_block = cfg
        .blocks()
        .iter()
        .position(|b| {
            b.stmts.iter().any(|s| {
                s.events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::Def { .. }))
            })
        })
        .expect("the block holding `a = 1`");
    // The body tail (the block that defines `a`) reaches the handler...
    assert!(preds[handler].contains(&def_block), "no body-tail edge");
    // ...and so does a block where `a` is not yet defined.
    assert!(
        preds[handler].iter().any(|&p| p != def_block),
        "no try_entry edge"
    );
}

// ---- scope -------------------------------------------------------------------

fn locals_of(src: &str) -> Vec<String> {
    only_cfg(src).locals().to_vec()
}

#[test]
fn a_class_pattern_name_and_a_keyword_name_are_not_locals() {
    let locals = locals_of(
        r#"def f(s):
    match s:
        case Point(x=0, y=b) as p:
            pass
        case [a, *rest] | {"k": a, **rest}:
            pass
        case Color.RED:
            pass
"#,
    );
    assert_eq!(locals, ["s", "b", "p", "a", "rest"]);
}

#[test]
fn a_bare_annotation_is_local_and_global_names_are_not() {
    let locals = locals_of(
        r#"def f(a, b=1, *args, c: int = 2, **kw):
    global g
    x: int
    g = 1
    import os.path, sys as system
    from m import y, z as w
    with open() as (h, i):
        pass
    for j, *k in xs:
        pass
    del q
"#,
    );
    assert_eq!(
        locals,
        ["a", "b", "args", "c", "kw", "x", "os", "system", "y", "w", "h", "i", "j", "k", "q"]
    );
}

#[test]
fn a_comprehension_target_does_not_leak_but_its_walrus_does() {
    let locals = locals_of("def f(xs):\n    return [v for v in xs if (n := v)]\n");
    assert_eq!(locals, ["xs", "n"]);
}

#[test]
fn a_nested_def_and_class_bind_their_names_but_not_their_bodies() {
    let src = r#"def f():
    def g(p):
        inner = p
    class C:
        attr = 1
    lambda q: q
"#;
    let us = units(src);
    assert_eq!(
        us.len(),
        2,
        "f and g are units; C's body and the lambda are not"
    );
    let FunctionUnit::Analysable(f) = &us[0] else {
        panic!("f opaque")
    };
    assert_eq!(f.locals(), ["g", "C"]);
}

// ---- the OPAQUE hatch ------------------------------------------------------
//
// Every trigger has a known-answer control: the same function without the
// trigger must be analysable. Without the twin, "opaque" could be the builder
// failing for some unrelated reason.

#[test]
fn each_dynamic_scope_builtin_makes_a_function_opaque() {
    for name in ["exec", "eval", "locals", "globals", "vars"] {
        let src = format!("def f():\n    x = 1\n    {name}('x')\n");
        assert_eq!(
            opaque_reason(&src),
            Some(OpaqueReason::DynamicScope(match name {
                "exec" => "exec",
                "eval" => "eval",
                "locals" => "locals",
                "globals" => "globals",
                _ => "vars",
            })),
            "{name}"
        );
    }
    assert_eq!(opaque_reason("def f():\n    x = 1\n    run('x')\n"), None);
}

#[test]
fn a_dynamic_scope_call_in_a_nested_def_does_not_make_the_outer_opaque() {
    let us = units("def f():\n    def g():\n        return locals()\n    return g\n");
    assert!(matches!(us[0], FunctionUnit::Analysable(_)), "f");
    assert!(matches!(us[1], FunctionUnit::Opaque { .. }), "g");
}

#[test]
fn a_wildcard_import_anywhere_in_the_module_makes_every_function_opaque() {
    assert_eq!(
        opaque_reason("def f():\n    x = 1\n    return x\nfrom m import *\n"),
        Some(OpaqueReason::WildcardImport)
    );
    assert_eq!(
        opaque_reason("def f():\n    x = 1\n    return x\nfrom m import n\n"),
        None
    );
}

#[test]
fn frame_introspection_makes_a_function_opaque() {
    assert_eq!(
        opaque_reason("def f():\n    return sys._getframe(1)\n"),
        Some(OpaqueReason::FrameIntrospection)
    );
    assert_eq!(
        opaque_reason("def f():\n    return inspect.currentframe()\n"),
        Some(OpaqueReason::FrameIntrospection)
    );
    assert_eq!(opaque_reason("def f():\n    return sys.argv\n"), None);
}

#[test]
fn an_except_star_handler_makes_a_function_opaque() {
    assert_eq!(
        opaque_reason("def f():\n    try:\n        g()\n    except* ValueError:\n        pass\n"),
        Some(OpaqueReason::ExceptGroup)
    );
    assert_eq!(
        opaque_reason("def f():\n    try:\n        g()\n    except ValueError:\n        pass\n"),
        None
    );
}

/// A unit test, not a fixture: the falsifier panics on a fixture that does
/// not parse, by design.
#[test]
fn a_parse_error_in_the_function_makes_it_opaque() {
    assert_eq!(
        opaque_reason("def f():\n    x = = 1\n"),
        Some(OpaqueReason::ParseError)
    );
    assert_eq!(opaque_reason("def f():\n    x = 1\n"), None);
}

#[test]
fn more_than_max_locals_makes_a_function_opaque() {
    let body: String = (0..=super::MAX_LOCALS)
        .map(|i| format!("    v{i} = 0\n"))
        .collect();
    assert_eq!(
        opaque_reason(&format!("def f():\n{body}")),
        Some(OpaqueReason::TooManyLocals(super::MAX_LOCALS + 1))
    );
    let body: String = (0..super::MAX_LOCALS)
        .map(|i| format!("    v{i} = 0\n"))
        .collect();
    assert_eq!(opaque_reason(&format!("def f():\n{body}")), None);
}
