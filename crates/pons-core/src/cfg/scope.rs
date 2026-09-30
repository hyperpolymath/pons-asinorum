// SPDX-License-Identifier: MPL-2.0

//! The locals pre-pass: which names a function binds, before any edge exists.
//!
//! Python decides scope statically. A name is local to a function if the
//! function binds it *anywhere* in its body, whatever the order. So the set of
//! locals has to be complete before the builder emits its first event. The
//! builder only records events for names in this set. Everything else is a
//! global or a builtin, and neither T1 rule has anything to say about those.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use super::LocalId;

pub(super) struct Scope {
    pub locals: Vec<String>,
    pub index: HashMap<String, LocalId>,
    pub params: Vec<LocalId>,
    /// Locals that some nested `def`, `lambda` or `class` mentions. A closure
    /// reads a variable when it is *called*, not when it is defined, so no
    /// store to one of these can be shown dead. See `Builder::def`.
    pub captured: HashSet<LocalId>,
}

/// Node kinds that open a new scope whose body is not this unit's code.
pub(super) fn is_nested_scope(kind: &str) -> bool {
    matches!(kind, "function_definition" | "class_definition" | "lambda")
}

pub(super) fn is_comprehension(kind: &str) -> bool {
    matches!(
        kind,
        "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression"
    )
}

pub(super) fn analyse(func: Node<'_>, text: &str) -> Scope {
    let body = func
        .child_by_field_name("body")
        .expect("function_definition always has a body");

    let mut declared = HashSet::new();
    collect_declared(body, text, &mut declared);

    let mut names = Vec::new();
    if let Some(params) = func.child_by_field_name("parameters") {
        collect_params(params, text, &mut names);
    }
    let n_params = names.len();
    collect_bound(body, text, &mut names);

    let mut locals = Vec::new();
    let mut index = HashMap::new();
    let mut params = Vec::new();
    for (i, name) in names.into_iter().enumerate() {
        // `global x` / `nonlocal x` make `x` a name this function writes
        // *through*, not one it owns.
        if declared.contains(&name) {
            continue;
        }
        let id = *index.entry(name.clone()).or_insert_with(|| {
            locals.push(name);
            locals.len() - 1
        });
        if i < n_params && !params.contains(&id) {
            params.push(id);
        }
    }

    let mut mentioned = HashSet::new();
    collect_nested_mentions(body, text, &mut mentioned);
    let captured = mentioned
        .iter()
        .filter_map(|name| index.get(name.as_str()).copied())
        .collect();

    Scope {
        locals,
        index,
        params,
        captured,
    }
}

fn text_of<'t>(node: Node<'_>, text: &'t str) -> &'t str {
    &text[node.byte_range()]
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn collect_declared(node: Node<'_>, text: &str, out: &mut HashSet<String>) {
    match node.kind() {
        "global_statement" | "nonlocal_statement" => {
            for child in named_children(node) {
                if child.kind() == "identifier" {
                    out.insert(text_of(child, text).to_string());
                }
            }
        }
        kind if is_nested_scope(kind) => {}
        _ => {
            for child in named_children(node) {
                collect_declared(child, text, out);
            }
        }
    }
}

/// Every name a `parameters` node binds: plain, defaulted, typed, and the
/// identifiers inside `*args` / `**kwargs`. Separators bind nothing.
fn collect_params(params: Node<'_>, text: &str, out: &mut Vec<String>) {
    for p in named_children(params) {
        let name = match p.kind() {
            "identifier" => Some(p),
            "default_parameter" | "typed_default_parameter" => p.child_by_field_name("name"),
            "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => {
                first_identifier(p)
            }
            _ => None,
        };
        if let Some(name) = name {
            if name.kind() == "identifier" {
                out.push(text_of(name, text).to_string());
            } else if let Some(inner) = first_identifier(name) {
                // `typed_parameter` wrapping a splat: `*args: int`.
                out.push(text_of(inner, text).to_string());
            }
        }
    }
}

fn first_identifier(node: Node<'_>) -> Option<Node<'_>> {
    for child in named_children(node) {
        if child.kind() == "identifier" {
            return Some(child);
        }
        if matches!(
            child.kind(),
            "list_splat_pattern" | "dictionary_splat_pattern"
        ) {
            return first_identifier(child);
        }
    }
    None
}

/// The names a binding target binds. Attribute and subscript targets bind
/// nothing; they *read* their object.
pub(super) fn target_identifiers<'t>(target: Node<'t>, out: &mut Vec<Node<'t>>) {
    match target.kind() {
        "identifier" => out.push(target),
        "pattern_list"
        | "tuple_pattern"
        | "list_pattern"
        | "list_splat_pattern"
        | "parenthesized_expression"
        | "expression_list"
        | "tuple"
        | "list" => {
            for child in named_children(target) {
                target_identifiers(child, out);
            }
        }
        _ => {}
    }
}

/// The name an import binds: `import a.b` binds `a`; `import a.b as c` and
/// `from m import a as c` bind `c`; `from m import a` binds `a`.
pub(super) fn import_bindings<'t>(stmt: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = stmt.walk();
    let mut out = Vec::new();
    for name in stmt.children_by_field_name("name", &mut cursor) {
        let bound = match name.kind() {
            "aliased_import" => name.child_by_field_name("alias"),
            "dotted_name" => name.named_child(0),
            _ => None,
        };
        if let Some(bound) = bound.filter(|b| b.kind() == "identifier") {
            out.push(bound);
        }
    }
    out
}

/// Is this identifier, found inside a `case_pattern`, a *capture*?
///
/// Measured against the pinned grammar: a bare name is a `dotted_name` with a
/// single identifier; `Color.RED` is a `dotted_name` with two and is a value,
/// not a capture; the class in `Point(x=0)` is the `dotted_name` directly under
/// `class_pattern`; `*rest`, `**kw` and `as p` put the identifier directly
/// under `splat_pattern` / `as_pattern`. The `x` in `Point(x=0)` is an
/// attribute name under `keyword_pattern` and binds nothing.
pub(super) fn is_capture(ident: Node<'_>) -> bool {
    let Some(parent) = ident.parent() else {
        return false;
    };
    match parent.kind() {
        "splat_pattern" => true,
        "as_pattern" => parent.named_child(0) != Some(ident),
        "dotted_name" => {
            parent.named_child_count() == 1
                && parent.parent().is_some_and(|g| g.kind() != "class_pattern")
        }
        _ => false,
    }
}

/// Is this identifier the attribute name in `Point(x=0)`? It is neither a
/// read nor a binding.
pub(super) fn is_keyword_pattern_name(ident: Node<'_>) -> bool {
    ident
        .parent()
        .is_some_and(|p| p.kind() == "keyword_pattern" && p.named_child(0) == Some(ident))
}

pub(super) fn identifiers_in<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    if node.kind() == "identifier" {
        out.push(node);
        return;
    }
    for child in named_children(node) {
        identifiers_in(child, out);
    }
}

fn push_all(nodes: &[Node<'_>], text: &str, out: &mut Vec<String>) {
    out.extend(nodes.iter().map(|n| text_of(*n, text).to_string()));
}

fn collect_bound(node: Node<'_>, text: &str, out: &mut Vec<String>) {
    let mut targets = Vec::new();
    match node.kind() {
        "function_definition" | "class_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.push(text_of(name, text).to_string());
            }
            // Default values and decorators run in *this* scope, and a walrus
            // there would bind here. Both are vanishingly rare; the body is
            // another scope entirely and is not entered.
            return;
        }
        "lambda" => return,
        "global_statement" | "nonlocal_statement" => return,
        kind if is_comprehension(kind) => {
            // A comprehension's own `for` targets do not leak (PEP 709 kept
            // that), but a walrus inside it binds in *this* scope (PEP 572).
            collect_walrus(node, text, out);
            return;
        }
        "assignment" | "augmented_assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                target_identifiers(left, &mut targets);
            }
            // A bare annotation `x: int` has no `right`. It still makes `x`
            // local (reading it before assignment is UnboundLocalError), so it
            // is collected here. It binds nothing; the builder emits no def.
            if let Some(right) = node.child_by_field_name("right") {
                collect_bound(right, text, out);
            }
        }
        "named_expression" => {
            if let Some(name) = node.child_by_field_name("name") {
                targets.push(name);
            }
            if let Some(value) = node.child_by_field_name("value") {
                collect_bound(value, text, out);
            }
        }
        "for_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                target_identifiers(left, &mut targets);
            }
            for child in named_children(node) {
                if Some(child) != node.child_by_field_name("left") {
                    collect_bound(child, text, out);
                }
            }
        }
        "as_pattern_target" => {
            for child in named_children(node) {
                target_identifiers(child, &mut targets);
            }
        }
        "import_statement" | "import_from_statement" => targets = import_bindings(node),
        "delete_statement" => {
            for child in named_children(node) {
                target_identifiers(child, &mut targets);
            }
        }
        "case_pattern" => {
            let mut idents = Vec::new();
            identifiers_in(node, &mut idents);
            targets.extend(idents.into_iter().filter(|i| is_capture(*i)));
        }
        _ => {
            for child in named_children(node) {
                collect_bound(child, text, out);
            }
        }
    }
    push_all(&targets, text, out);
}

fn collect_walrus(node: Node<'_>, text: &str, out: &mut Vec<String>) {
    match node.kind() {
        kind if is_nested_scope(kind) => {}
        "named_expression" => {
            if let Some(name) = node.child_by_field_name("name") {
                out.push(text_of(name, text).to_string());
            }
            if let Some(value) = node.child_by_field_name("value") {
                collect_walrus(value, text, out);
            }
        }
        _ => {
            for child in named_children(node) {
                collect_walrus(child, text, out);
            }
        }
    }
}

fn collect_nested_mentions(node: Node<'_>, text: &str, out: &mut HashSet<String>) {
    if is_nested_scope(node.kind()) {
        let mut idents = Vec::new();
        identifiers_in(node, &mut idents);
        out.extend(idents.iter().map(|i| text_of(*i, text).to_string()));
        return;
    }
    for child in named_children(node) {
        collect_nested_mentions(child, text, out);
    }
}
