// SPDX-License-Identifier: MPL-2.0

//! Control-flow graphs for the T1 dataflow analyses (ADR-0002).
//!
//! The analysis unit is a `function_definition` body, at any depth. Module
//! top level and class bodies are **not** units: a module-level name can be
//! imported by another module, so a store to it is never provably dead.
//! Lambdas are not units either; they contain no statements.
//!
//! A unit is either [`FunctionUnit::Analysable`] or [`FunctionUnit::Opaque`].
//! A T1 rule cannot get a CFG for an opaque function, so it cannot forget to
//! check whether it should have asked. That mirrors the way `finding.rs`
//! enforces the evidence invariant in the type system rather than in review.
//!
//! This module also holds the grammar probe, which proves that the vocabulary
//! the builder is written against is the vocabulary the *pinned parser*
//! produces — see [`grammar_probe`]. That probe overturned two claims this
//! module would otherwise have been built on; [`is_except_group`] exists
//! because of one of them.

use std::fmt;
use std::path::Path;

use tree_sitter::{Node, Tree};

use crate::finding::Location;

mod builder;
#[cfg(test)]
mod grammar_probe;
mod scope;
#[cfg(test)]
mod tests;

pub type BlockId = usize;
/// Index into [`FunctionCfg::locals`].
pub type LocalId = usize;

/// ADR-0002's runaway guard: a function with more locals than this is
/// generated code, and is reported opaque rather than analysed.
pub const MAX_LOCALS: usize = 4096;

/// What one statement does to one local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// A read. `checkable` is false for reads the may-unbound analysis cannot
    /// judge from here: a closure capture (read when the closure runs), a
    /// name in a `match` pattern, a lazily-run generator body.
    Use { checkable: bool },
    /// A binding that overwrites the old value. `report` is `Some` only for a
    /// store the dead-store rule may speak about: a bare-name target of a
    /// plain or augmented assignment, and not captured by a nested scope.
    Def { report: Option<DefReport> },
    /// A binding that clears may-unbound but does not end a live range: a
    /// walrus target or a `match` capture. Both usually sit on a branch this
    /// CFG does not split, so treating them as a kill would invent dead stores.
    WeakDef,
    /// `del x`, and the `except … as e` name at the end of its handler.
    Kill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefReport {
    /// The right-hand side contains a call, `await`, `yield` or walrus.
    pub rhs_has_effect: bool,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub local: LocalId,
    pub kind: EventKind,
    /// The identifier the event is about.
    pub loc: Location,
}

/// One statement, or one statement's header (`if` condition, `for` target,
/// `except` clause). The byte range is kept, not just the events, because
/// M5's typestate walks this same CFG and needs to see the calls in it.
#[derive(Debug, Clone)]
pub struct Stmt {
    pub byte_start: usize,
    pub byte_end: usize,
    /// In evaluation order.
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    pub stmts: Vec<Stmt>,
    pub succs: Vec<BlockId>,
}

/// The CFG of one analysable function. Only the builder can make one.
#[derive(Debug)]
pub struct FunctionCfg {
    name: String,
    span: Location,
    locals: Vec<String>,
    params: Vec<LocalId>,
    blocks: Vec<BasicBlock>,
}

impl FunctionCfg {
    pub const ENTRY: BlockId = 0;
    pub const EXIT: BlockId = 1;

    pub fn name(&self) -> &str {
        &self.name
    }
    /// The source span of the whole function definition, including its header.
    pub fn span(&self) -> &Location {
        &self.span
    }
    /// Local names indexed by [`LocalId`], including parameters but excluding
    /// names declared `global` or `nonlocal`.
    pub fn locals(&self) -> &[String] {
        &self.locals
    }
    /// Local IDs bound by the function's parameters on entry.
    pub fn params(&self) -> &[LocalId] {
        &self.params
    }
    /// Blocks indexed by [`BlockId`], including entry, exit and unreachable code.
    pub fn blocks(&self) -> &[BasicBlock] {
        &self.blocks
    }

    /// Predecessors indexed by destination block ID, including unreachable edges.
    pub fn preds(&self) -> Vec<Vec<BlockId>> {
        let mut preds = vec![Vec::new(); self.blocks.len()];
        for (b, block) in self.blocks.iter().enumerate() {
            for &s in &block.succs {
                preds[s].push(b);
            }
        }
        preds
    }

    /// Blocks reachable from [`FunctionCfg::ENTRY`]. Code after a `return`
    /// still gets blocks; findings in it belong to `unreachable-after-jump`,
    /// not to the dataflow rules.
    pub fn reachable(&self) -> Vec<bool> {
        let mut seen = vec![false; self.blocks.len()];
        let mut stack = vec![Self::ENTRY];
        while let Some(b) = stack.pop() {
            if std::mem::replace(&mut seen[b], true) {
                continue;
            }
            stack.extend(&self.blocks[b].succs);
        }
        seen
    }

    /// A stable, human-diffable rendering for the snapshot tests.
    /// `text` must be the complete source used to build this CFG.
    ///
    /// # Panics
    /// Panics if a stored statement byte range is out of bounds or does not
    /// lie on UTF-8 boundaries in `text`.
    pub fn render(&self, text: &str) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let names = |ids: &[LocalId]| {
            ids.iter()
                .map(|&i| self.locals[i].as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let all: Vec<LocalId> = (0..self.locals.len()).collect();
        writeln!(out, "fn {}", self.name).unwrap();
        writeln!(out, "locals: {}", names(&all)).unwrap();
        writeln!(out, "params: {}", names(&self.params)).unwrap();
        let reachable = self.reachable();
        for (b, block) in self.blocks.iter().enumerate() {
            let label = match b {
                Self::ENTRY => " (entry)",
                Self::EXIT => " (exit)",
                _ if !reachable[b] => " (unreachable)",
                _ => "",
            };
            let succs: Vec<String> = block.succs.iter().map(|s| format!("b{s}")).collect();
            writeln!(out, "b{b}{label} -> [{}]", succs.join(" ")).unwrap();
            for stmt in &block.stmts {
                let line = text[..stmt.byte_start].matches('\n').count() + 1;
                let src = text[stmt.byte_start..stmt.byte_end]
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim();
                let events: Vec<String> = stmt
                    .events
                    .iter()
                    .map(|e| {
                        let n = &self.locals[e.local];
                        match e.kind {
                            EventKind::Use { checkable: true } => format!("use {n}"),
                            EventKind::Use { checkable: false } => format!("use~ {n}"),
                            EventKind::Def { report: None } => format!("def {n}"),
                            EventKind::Def {
                                report:
                                    Some(DefReport {
                                        rhs_has_effect: false,
                                    }),
                            } => format!("def! {n}"),
                            EventKind::Def {
                                report:
                                    Some(DefReport {
                                        rhs_has_effect: true,
                                    }),
                            } => format!("def!* {n}"),
                            EventKind::WeakDef => format!("weak {n}"),
                            EventKind::Kill => format!("kill {n}"),
                        }
                    })
                    .collect();
                writeln!(out, "  L{line} `{src}` {{{}}}", events.join(", ")).unwrap();
            }
        }
        out
    }
}

/// Why a function was not analysed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpaqueReason {
    /// A call to `exec`, `eval`, `locals`, `globals` or `vars`: the set of
    /// names, or their values, is no longer visible in the source.
    DynamicScope(&'static str),
    /// `from m import *` anywhere in the module.
    WildcardImport,
    /// `sys._getframe` / `inspect.currentframe`.
    FrameIntrospection,
    /// A PEP 654 `except*` handler (see [`is_except_group`]).
    ExceptGroup,
    /// The function's subtree contains a parse error.
    ParseError,
    /// More than [`MAX_LOCALS`] locals.
    TooManyLocals(usize),
}

impl fmt::Display for OpaqueReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DynamicScope(name) => write!(f, "calls `{name}`"),
            Self::WildcardImport => f.write_str("the module has a wildcard import"),
            Self::FrameIntrospection => f.write_str("inspects stack frames"),
            Self::ExceptGroup => f.write_str("has an `except*` handler"),
            Self::ParseError => f.write_str("does not parse cleanly"),
            Self::TooManyLocals(n) => write!(f, "has {n} locals (limit {MAX_LOCALS})"),
        }
    }
}

#[derive(Debug)]
pub enum FunctionUnit {
    Analysable(FunctionCfg),
    Opaque {
        name: String,
        reason: OpaqueReason,
        span: Location,
    },
}

/// Every function unit in a Python module, in source order.
/// Includes nested functions and methods; module and class bodies and lambdas
/// are not units. Unsupported functions are returned as opaque units with a
/// reason, including functions with parse errors or more than [`MAX_LOCALS`].
///
/// `tree` must be the Python parse of `text`. `path` labels source locations;
/// no file is read.
///
/// # Panics
/// Panics if a node's byte range cannot be sliced from `text`, for example
/// because the tree and source do not match.
pub fn build_units(path: &Path, text: &str, tree: &Tree) -> Vec<FunctionUnit> {
    let file = path.display().to_string();
    let root = tree.root_node();
    let wildcard = contains_kind(root, "wildcard_import");
    let mut funcs = Vec::new();
    collect_functions(root, &mut funcs);
    funcs
        .into_iter()
        .map(|func| {
            let name = func
                .child_by_field_name("name")
                .map_or("<anonymous>", |n| &text[n.byte_range()])
                .to_string();
            let span = Location::from_node(file.clone(), &func, text);
            let opaque = |reason| FunctionUnit::Opaque {
                name: name.clone(),
                reason,
                span: span.clone(),
            };
            if let Some(reason) = opaque_reason(func, text, wildcard) {
                return opaque(reason);
            }
            let scope = scope::analyse(func, text);
            if scope.locals.len() > MAX_LOCALS {
                return opaque(OpaqueReason::TooManyLocals(scope.locals.len()));
            }
            FunctionUnit::Analysable(builder::Builder::build(
                func,
                name.clone(),
                span.clone(),
                text,
                file.clone(),
                &scope,
            ))
        })
        .collect()
}

/// Appends function definitions in source order, descending into nested scopes.
fn collect_functions<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    if node.kind() == "function_definition" {
        out.push(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_functions(child, out);
    }
}

fn contains_kind(node: Node<'_>, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|c| contains_kind(c, kind));
    found
}

const DYNAMIC_SCOPE: &[&str] = &["exec", "eval", "locals", "globals", "vars"];

/// Returns the first opacity trigger, before the separate local-count check.
/// Frame-introspection names are matched textually anywhere in the body.
fn opaque_reason(func: Node<'_>, text: &str, module_has_wildcard: bool) -> Option<OpaqueReason> {
    if func.has_error() {
        return Some(OpaqueReason::ParseError);
    }
    let body = func.child_by_field_name("body")?;
    if let Some(reason) = own_body_trigger(body, text) {
        return Some(reason);
    }
    if module_has_wildcard {
        return Some(OpaqueReason::WildcardImport);
    }
    // Textual, and deliberately broad: `from sys import _getframe` then a bare
    // `_getframe()` is caught too. A false OPAQUE costs findings, never
    // correctness.
    let src = &text[body.byte_range()];
    if src.contains("_getframe") || src.contains("currentframe") {
        return Some(OpaqueReason::FrameIntrospection);
    }
    None
}

/// Triggers that belong to this function's own code, not to a nested `def`
/// or `lambda` (whose `locals()` is its own).
fn own_body_trigger(node: Node<'_>, text: &str) -> Option<OpaqueReason> {
    match node.kind() {
        "function_definition" | "lambda" => return None,
        "exec_statement" => return Some(OpaqueReason::DynamicScope("exec")),
        "except_clause" if is_except_group(node) => return Some(OpaqueReason::ExceptGroup),
        "call" => {
            if let Some(f) = node.child_by_field_name("function") {
                if f.kind() == "identifier" {
                    let name = &text[f.byte_range()];
                    if let Some(hit) = DYNAMIC_SCOPE.iter().find(|d| **d == name) {
                        return Some(OpaqueReason::DynamicScope(hit));
                    }
                }
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    children.into_iter().find_map(|c| own_body_trigger(c, text))
}

/// Is this `except_clause` a PEP 654 exception *group* handler (`except*`)?
///
/// **This cannot be left to the `has_error()` OPAQUE trigger.** ADR-0002's
/// implementation plan assumed `except*` is unparseable by the pinned grammar
/// and so would be caught as an error node. Measured, it is not:
/// `tree-sitter-python` 0.25.0 parses `except* ValueError:` to an ordinary
/// `except_clause` with the `*` demoted to an *anonymous* child, producing a
/// named-tree shape **identical** to plain `except ValueError:`.
/// `has_error()` returns `false`.
///
/// Silently misreading a group handler as an ordinary one is worse than
/// refusing to analyse it: the two have genuinely different control flow (a
/// group handler runs for the matching *part* of a raised group, and several
/// sibling handlers can each run for a single raise), so the builder would emit
/// a confident, plausible, wrong CFG and both T1 rules would report against it.
///
/// The `*` token does survive as an anonymous child, so the discrimination is
/// structural rather than textual.
///
/// # Panics
/// Panics in debug builds if the node is not an `except_clause`.
pub(crate) fn is_except_group(except_clause: Node<'_>) -> bool {
    debug_assert_eq!(except_clause.kind(), "except_clause");
    let mut cursor = except_clause.walk();
    for child in except_clause.children(&mut cursor) {
        if !child.is_named() && child.kind() == "*" {
            return true;
        }
    }
    false
}
