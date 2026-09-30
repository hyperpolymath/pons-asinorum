// SPDX-License-Identifier: MPL-2.0

//! Each ADR-0002 corpus case, asserted on the analysis directly. Every "no
//! finding" case sits next to a case that does report, so a solver that
//! reports nothing cannot pass.

use std::path::Path;

use tree_sitter::Parser;

use super::{dead_stores, unbound_reads};
use crate::cfg::{build_units, FunctionCfg, FunctionUnit};

fn cfg(src: &str) -> FunctionCfg {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(src, None).unwrap();
    match build_units(Path::new("t.py"), src, &tree).remove(0) {
        FunctionUnit::Analysable(cfg) => cfg,
        FunctionUnit::Opaque { reason, .. } => panic!("opaque: {reason}"),
    }
}

/// `name@line` for each dead store.
fn dead(src: &str) -> Vec<String> {
    let cfg = cfg(src);
    dead_stores(&cfg)
        .iter()
        .map(|(e, _)| format!("{}@{}", cfg.locals()[e.local], e.loc.line_start))
        .collect()
}

fn unbound(src: &str) -> Vec<String> {
    let cfg = cfg(src);
    unbound_reads(&cfg)
        .iter()
        .map(|e| format!("{}@{}", cfg.locals()[e.local], e.loc.line_start))
        .collect()
}

// ---- dead-store ---------------------------------------------------------------

#[test]
fn an_overwritten_store_is_dead_and_the_read_one_is_not() {
    assert_eq!(
        dead("def f():\n    x = 1\n    x = 2\n    return x\n"),
        ["x@2"]
    );
}

#[test]
fn a_store_never_read_is_dead() {
    assert_eq!(dead("def f():\n    x = 1\n"), ["x@2"]);
}

#[test]
fn a_store_read_round_a_loop_back_edge_is_live() {
    assert_eq!(
        dead("def f(c):\n    n = 0\n    while c():\n        n = n + 1\n    return n\n"),
        Vec::<String>::new()
    );
    assert_eq!(
        dead("def f(c):\n    n = 0\n    while c():\n        n = 1\n    return 0\n"),
        ["n@2", "n@4"]
    );
}

#[test]
fn a_store_in_try_read_only_by_the_handler_is_live() {
    // The try-body-tail → handler edge.
    let src = "def f():\n    s = 'start'\n    try:\n        s = 'mid'\n        g()\n    except E:\n        log(s)\n";
    assert_eq!(dead(src), Vec::<String>::new());
}

/// ADR-0002's body-tail → handler edge, pinned. Every statement in a `try`
/// already raises from its own block, so this edge matters only for the state
/// after the *last* statement, where it is conservative: the handler cannot
/// in fact see `s = 'end'`. The ADR mandates the edge; this keeps the store
/// silent rather than reported, which is the direction T1 errs in.
#[test]
fn the_last_store_in_try_is_live_if_a_handler_reads_it() {
    let src = "def f():\n    try:\n        g()\n        s = 'end'\n    except E:\n        log(s)\n";
    assert_eq!(dead(src), Vec::<String>::new());
    assert_eq!(unbound(src), ["s@6"]);
}

#[test]
fn a_store_read_only_in_finally_on_the_return_path_is_live() {
    // The gap-fill: `return` runs the finally, which reads `y`.
    let src = "def f():\n    try:\n        y = 1\n        return 0\n    finally:\n        log(y)\n";
    assert_eq!(dead(src), Vec::<String>::new());
}

#[test]
fn a_store_in_finally_is_dead_only_if_dead_in_every_copy() {
    // Dead in the exceptional copy (it re-raises), live in the normal one.
    let src = "def f():\n    try:\n        g()\n    finally:\n        y = 1\n    return y\n";
    assert_eq!(dead(src), Vec::<String>::new());
    let src = "def f():\n    try:\n        g()\n    finally:\n        y = 1\n    return 0\n";
    assert_eq!(dead(src), ["y@5"]);
}

#[test]
fn a_captured_local_is_never_a_dead_store() {
    // The late-binding closure: `h` reads `x` when called, after the rebind.
    let src = "def f():\n    x = 1\n    h = lambda: x\n    x = 2\n    return h\n";
    assert_eq!(dead(src), Vec::<String>::new());
}

#[test]
fn a_store_in_unreachable_code_is_not_reported() {
    assert_eq!(
        dead("def f():\n    return 0\n    x = 1\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_walrus_does_not_end_a_live_range() {
    let src = "def f(c):\n    n = 0\n    if c and (n := g()):\n        pass\n    return n\n";
    assert_eq!(dead(src), Vec::<String>::new());
}

// ---- read-before-init ------------------------------------------------------------

#[test]
fn a_read_after_a_one_armed_if_is_may_unbound() {
    assert_eq!(
        unbound("def f(c):\n    if c:\n        y = 1\n    return y\n"),
        ["y@4"]
    );
    assert_eq!(
        unbound("def f(c):\n    if c:\n        y = 1\n    else:\n        y = 2\n    return y\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_parameter_is_bound_on_entry() {
    assert_eq!(
        unbound("def f(a, *b, c=1, **d):\n    return a, b, c, d\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_handler_read_of_a_name_bound_in_try_is_may_unbound() {
    // The try_entry → handler edge.
    let src = "def f():\n    try:\n        a = g()\n    except E:\n        log(a)\n";
    assert_eq!(unbound(src), ["a@5"]);
}

#[test]
fn del_makes_a_name_unbound_again_and_reaches_the_handler() {
    assert_eq!(
        unbound("def f():\n    x = 1\n    del x\n    return x\n"),
        ["x@4"]
    );
    let src = "def f():\n    x = 1\n    try:\n        del x\n        g()\n    except E:\n        log(x)\n";
    assert_eq!(unbound(src), ["x@7"]);
}

#[test]
fn the_except_alias_is_unbound_after_its_handler() {
    let src = "def f():\n    try:\n        g()\n    except E as e:\n        pass\n    return e\n";
    assert_eq!(unbound(src), ["e@6"]);
}

#[test]
fn the_loop_variable_after_a_loop_that_may_not_run_is_may_unbound() {
    assert_eq!(
        unbound("def f(xs):\n    for x in xs:\n        pass\n    return x\n"),
        ["x@4"]
    );
}

#[test]
fn a_while_true_loop_exits_only_through_break() {
    let src = "def f():\n    while True:\n        v = g()\n        if v:\n            break\n    return v\n";
    assert_eq!(unbound(src), Vec::<String>::new());
}

#[test]
fn a_walrus_binds_for_may_unbound() {
    assert_eq!(
        unbound("def f():\n    if (n := g()):\n        pass\n    return n\n"),
        Vec::<String>::new()
    );
    // A known false negative, chosen: a walrus inside a comprehension runs
    // only if the iterable is non-empty, so `n` really may be unbound here.
    // Reporting it would also fire on `if any((m := p(s)) for s in xs):
    // use(m)`, where `any()` being true guarantees the binding — a false
    // positive on a common idiom. T1 errs silent.
    let src = "def f(xs):\n    if any((n := x) for x in xs):\n        pass\n    return n\n";
    assert_eq!(unbound(src), Vec::<String>::new());
}

#[test]
fn a_read_in_a_nested_function_is_not_judged_here() {
    let src = "def f():\n    def g():\n        return y\n    y = 1\n    return g\n";
    assert_eq!(unbound(src), Vec::<String>::new());
}
