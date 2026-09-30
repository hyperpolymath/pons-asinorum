// SPDX-License-Identifier: MPL-2.0

//! Lowers one `function_definition` body to a [`FunctionCfg`].
//!
//! ADR-0002 is the authority. Where it is silent the choice made here is
//! marked **gap-fill** and pinned by a snapshot test in `cfg::tests`.

use tree_sitter::Node;

use super::scope::{self, Scope};
use super::{BasicBlock, BlockId, DefReport, Event, EventKind, FunctionCfg, LocalId, Stmt};
use crate::finding::Location;

/// Something a `break`, `continue` or `return` has to pass on its way out.
#[derive(Clone)]
enum Frame<'t> {
    Loop {
        brk: BlockId,
        cont: BlockId,
    },
    /// A `finally` still to run. `outer_raise` is where an exception goes once
    /// it is outside this `try`.
    Finally {
        body: Node<'t>,
        outer_raise: Vec<BlockId>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Jump {
    Return,
    Break,
    Continue,
}

pub(super) struct Builder<'t> {
    text: &'t str,
    file: String,
    scope: &'t Scope,
    blocks: Vec<BasicBlock>,
    cur: BlockId,
    frames: Vec<Frame<'t>>,
    /// Where an exception raised at the current point goes: the handlers of
    /// the enclosing `try`, the exceptional copy of its `finally`, or `EXIT`.
    raise_to: Vec<BlockId>,
    /// A comprehension's own iteration variables, innermost last. They
    /// shadow a same-named local inside the comprehension.
    shadow: Vec<Vec<String>>,
}

impl<'t> Builder<'t> {
    pub(super) fn build(
        func: Node<'t>,
        name: String,
        span: Location,
        text: &'t str,
        file: String,
        scope: &'t Scope,
    ) -> FunctionCfg {
        let mut b = Builder {
            text,
            file,
            scope,
            blocks: vec![BasicBlock::default(), BasicBlock::default()],
            cur: FunctionCfg::ENTRY,
            frames: Vec::new(),
            raise_to: vec![FunctionCfg::EXIT],
            shadow: Vec::new(),
        };
        let body = func
            .child_by_field_name("body")
            .expect("function_definition always has a body");
        b.block(body);
        b.edge(b.cur, FunctionCfg::EXIT);
        FunctionCfg {
            name,
            span,
            locals: scope.locals.clone(),
            params: scope.params.clone(),
            blocks: b.blocks,
        }
    }

    // ---- graph plumbing -------------------------------------------------

    fn new_block(&mut self) -> BlockId {
        self.blocks.push(BasicBlock::default());
        self.blocks.len() - 1
    }

    fn edge(&mut self, from: BlockId, to: BlockId) {
        let succs = &mut self.blocks[from].succs;
        if !succs.contains(&to) {
            succs.push(to);
        }
    }

    fn edges_to_raise(&mut self, from: BlockId) {
        for t in self.raise_to.clone() {
            self.edge(from, t);
        }
    }

    /// Inside any `try` region an exception can leave between two statements,
    /// so each statement gets its own block (see [`Builder::stmt`]).
    fn in_try_region(&self) -> bool {
        self.raise_to != [FunctionCfg::EXIT]
    }

    fn tail_raise(&mut self) {
        if self.in_try_region() {
            self.edges_to_raise(self.cur);
        }
    }

    fn emit(&mut self, start: usize, end: usize, events: Vec<Event>) {
        self.blocks[self.cur].stmts.push(Stmt {
            byte_start: start,
            byte_end: end,
            events,
        });
    }

    fn emit_node(&mut self, node: Node<'_>, events: Vec<Event>) {
        self.emit(node.start_byte(), node.end_byte(), events);
    }

    // ---- events ---------------------------------------------------------

    fn lookup(&self, ident: Node<'_>) -> Option<LocalId> {
        let name = &self.text[ident.byte_range()];
        if self.shadow.iter().any(|s| s.iter().any(|n| n == name)) {
            return None;
        }
        self.scope.index.get(name).copied()
    }

    fn event(&self, ident: Node<'_>, kind: EventKind, ev: &mut Vec<Event>) {
        if let Some(local) = self.lookup(ident) {
            ev.push(Event {
                local,
                kind,
                loc: Location::from_node(self.file.clone(), &ident, self.text),
            });
        }
    }

    fn use_(&self, ident: Node<'_>, checkable: bool, ev: &mut Vec<Event>) {
        self.event(ident, EventKind::Use { checkable }, ev);
    }

    /// A strong definition. `report` is dropped for a captured local: a
    /// closure reads it when *called*, so the store is never provably dead
    /// (the late-binding closure false positive).
    fn def(&self, ident: Node<'_>, report: Option<DefReport>, ev: &mut Vec<Event>) {
        let report = report.filter(|_| {
            self.lookup(ident)
                .is_some_and(|id| !self.scope.captured.contains(&id))
        });
        self.event(ident, EventKind::Def { report }, ev);
    }

    // ---- expressions ----------------------------------------------------

    fn expr(&mut self, node: Node<'t>, ev: &mut Vec<Event>, checkable: bool) {
        match node.kind() {
            "identifier" => self.use_(node, checkable, ev),
            // `obj.name`: only `obj` is a read of a variable.
            "attribute" => {
                if let Some(obj) = node.child_by_field_name("object") {
                    self.expr(obj, ev, checkable);
                }
            }
            // `f(name=value)`: `name` is a parameter name, not a variable.
            "keyword_argument" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.expr(value, ev, checkable);
                }
            }
            // Annotations are not evaluated as reads of locals.
            "type" => {}
            "lambda" => self.capture_uses(node, ev),
            kind if scope::is_comprehension(kind) => self.comprehension(node, ev, checkable),
            // PEP 572: the value first, then the name. A walrus is a *weak*
            // def: it binds, so it clears may-unbound, but it never kills
            // liveness and is never reported as a dead store. It usually sits
            // inside a condition whose branches this CFG does not split.
            "named_expression" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.expr(value, ev, checkable);
                }
                if let Some(name) = node.child_by_field_name("name") {
                    self.event(name, EventKind::WeakDef, ev);
                }
            }
            "assignment" => self.assignment(node, ev),
            "augmented_assignment" => self.augmented(node, ev),
            _ => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                for child in children {
                    self.expr(child, ev, checkable);
                }
            }
        }
    }

    /// Every outer local a nested `def`, `lambda` or `class` mentions is read,
    /// uncheckably, at the point the nested scope is created.
    fn capture_uses(&self, node: Node<'_>, ev: &mut Vec<Event>) {
        let mut idents = Vec::new();
        scope::identifiers_in(node, &mut idents);
        for ident in idents {
            self.use_(ident, false, ev);
        }
    }

    /// Comprehensions are evaluated inline. The first iterable runs in this
    /// scope; the rest runs in the comprehension's own, where its `for`
    /// targets shadow ours. A generator's body runs later, lazily, so its
    /// reads cannot be checked for may-unbound here.
    fn comprehension(&mut self, node: Node<'t>, ev: &mut Vec<Event>, checkable: bool) {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        let clauses: Vec<_> = children
            .iter()
            .copied()
            .filter(|c| c.kind() == "for_in_clause")
            .collect();

        let mut own = Vec::new();
        for clause in &clauses {
            if let Some(left) = clause.child_by_field_name("left") {
                let mut ids = Vec::new();
                scope::target_identifiers(left, &mut ids);
                own.extend(ids.iter().map(|i| self.text[i.byte_range()].to_string()));
            }
        }

        if let Some(right) = clauses.first().and_then(|c| c.child_by_field_name("right")) {
            self.expr(right, ev, checkable);
        }
        let inner = checkable && node.kind() != "generator_expression";
        self.shadow.push(own);
        for child in children {
            if child.kind() == "for_in_clause" {
                if Some(child) != clauses.first().copied() {
                    if let Some(right) = child.child_by_field_name("right") {
                        self.expr(right, ev, inner);
                    }
                }
            } else {
                self.expr(child, ev, inner);
            }
        }
        self.shadow.pop();
    }

    fn has_effect(node: Node<'_>) -> bool {
        match node.kind() {
            "call" | "await" | "yield" | "named_expression" => true,
            "lambda" => false,
            _ => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                children.into_iter().any(Self::has_effect)
            }
        }
    }

    /// Binds a target. Only a bare identifier target of a plain or augmented
    /// assignment carries a `report`; everything else is a def that the
    /// dead-store rule does not speak about.
    fn bind_target(&mut self, target: Node<'t>, report: Option<DefReport>, ev: &mut Vec<Event>) {
        match target.kind() {
            "identifier" => self.def(target, report, ev),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "list_splat_pattern"
            | "parenthesized_expression"
            | "expression_list"
            | "tuple"
            | "list" => {
                let mut cursor = target.walk();
                let children: Vec<_> = target.named_children(&mut cursor).collect();
                for child in children {
                    self.bind_target(child, None, ev);
                }
            }
            // `obj.attr = v`, `xs[i] = v`: reads of `obj`, `xs`, `i`.
            _ => self.expr(target, ev, true),
        }
    }

    fn assignment(&mut self, node: Node<'t>, ev: &mut Vec<Event>) {
        // `a = b = rhs` nests: assignment{left: a, right: assignment{left: b,
        // right: rhs}}. Collect the chain; evaluate `rhs` once; then bind.
        let mut targets = Vec::new();
        let mut cur = node;
        let rhs = loop {
            if let Some(left) = cur.child_by_field_name("left") {
                targets.push(left);
            }
            match cur.child_by_field_name("right") {
                Some(r) if r.kind() == "assignment" => cur = r,
                other => break other,
            }
        };
        // A bare annotation `x: int` evaluates nothing and binds nothing.
        let Some(rhs) = rhs else { return };
        let report = Some(DefReport {
            rhs_has_effect: Self::has_effect(rhs),
        });
        self.expr(rhs, ev, true);
        for target in targets {
            self.bind_target(target, report, ev);
        }
    }

    fn augmented(&mut self, node: Node<'t>, ev: &mut Vec<Event>) {
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return;
        };
        self.expr(right, ev, true);
        if left.kind() == "identifier" {
            self.use_(left, true, ev);
            let report = Some(DefReport {
                rhs_has_effect: Self::has_effect(right),
            });
            self.def(left, report, ev);
        } else {
            self.expr(left, ev, true);
        }
    }

    // ---- statements -----------------------------------------------------

    fn block(&mut self, block: Node<'t>) {
        let mut cursor = block.walk();
        let stmts: Vec<_> = block.named_children(&mut cursor).collect();
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, node: Node<'t>) {
        if node.kind() == "comment" {
            return;
        }
        if self.in_try_region() {
            // The block *before* each statement can raise to the handlers.
            // For the first statement of a `try` body that block is
            // `try_entry`, which is what delivers the pre-try state to
            // may-unbound (ADR-0002).
            let prev = self.cur;
            self.edges_to_raise(prev);
            let next = self.new_block();
            self.edge(prev, next);
            self.cur = next;
        }
        let mut ev = Vec::new();
        match node.kind() {
            "return_statement" => {
                self.walk_children(node, &mut ev);
                self.emit_node(node, ev);
                self.jump(Jump::Return);
            }
            "break_statement" => {
                self.emit_node(node, ev);
                self.jump(Jump::Break);
            }
            "continue_statement" => {
                self.emit_node(node, ev);
                self.jump(Jump::Continue);
            }
            "raise_statement" => {
                self.walk_children(node, &mut ev);
                self.emit_node(node, ev);
                let from = self.cur;
                self.edges_to_raise(from);
                self.cur = self.new_block();
            }
            "pass_statement"
            | "global_statement"
            | "nonlocal_statement"
            | "future_import_statement" => self.emit_node(node, ev),
            "import_statement" | "import_from_statement" => {
                for bound in scope::import_bindings(node) {
                    self.def(bound, None, &mut ev);
                }
                self.emit_node(node, ev);
            }
            "delete_statement" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                for child in children {
                    self.del_target(child, &mut ev);
                }
                self.emit_node(node, ev);
            }
            "if_statement" => self.if_(node),
            "while_statement" => self.while_(node),
            "for_statement" => self.for_(node),
            "try_statement" => self.try_(node),
            "with_statement" => self.with_(node),
            "match_statement" => self.match_(node),
            "function_definition" | "class_definition" => {
                self.nested_def(node, &mut ev);
                self.emit_node(node, ev);
            }
            "decorated_definition" => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                for child in children {
                    if child.kind() == "decorator" {
                        self.expr(child, &mut ev, true);
                    }
                }
                if let Some(def) = node.child_by_field_name("definition") {
                    self.nested_def(def, &mut ev);
                }
                self.emit_node(node, ev);
            }
            // expression_statement (which carries assignments), assert, and
            // anything this grammar adds later: read every name in it.
            _ => {
                self.walk_children(node, &mut ev);
                self.emit_node(node, ev);
            }
        }
    }

    fn walk_children(&mut self, node: Node<'t>, ev: &mut Vec<Event>) {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children {
            self.expr(child, ev, true);
        }
    }

    fn del_target(&mut self, target: Node<'t>, ev: &mut Vec<Event>) {
        match target.kind() {
            "identifier" => self.event(target, EventKind::Kill, ev),
            "expression_list" | "tuple" | "list" | "parenthesized_expression" => {
                let mut cursor = target.walk();
                let children: Vec<_> = target.named_children(&mut cursor).collect();
                for child in children {
                    self.del_target(child, ev);
                }
            }
            _ => self.expr(target, ev, true),
        }
    }

    /// `def` and `class` inside the unit: defaults, decorators and bases are
    /// read now; the body's mentions of our locals are captures; then the
    /// name is bound.
    fn nested_def(&mut self, node: Node<'t>, ev: &mut Vec<Event>) {
        if node.kind() == "function_definition" {
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                let ps: Vec<_> = params.named_children(&mut cursor).collect();
                for p in ps {
                    if let Some(v) = p.child_by_field_name("value") {
                        self.expr(v, ev, true);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                self.capture_uses(body, ev);
            }
        } else {
            if let Some(bases) = node.child_by_field_name("superclasses") {
                self.expr(bases, ev, true);
            }
            if let Some(body) = node.child_by_field_name("body") {
                self.capture_uses(body, ev);
            }
        }
        if let Some(name) = node.child_by_field_name("name") {
            self.def(name, None, ev);
        }
    }

    /// `return` / `break` / `continue`: run every `finally` between here and
    /// the destination, innermost first, each as its own copy of the
    /// `finally` body (**gap-fill**, see [`Builder::try_`]).
    fn jump(&mut self, kind: Jump) {
        let mut i = self.frames.len();
        let dest = loop {
            if i == 0 {
                // `break` outside a loop is a SyntaxError; send it to EXIT.
                break FunctionCfg::EXIT;
            }
            i -= 1;
            match self.frames[i].clone() {
                Frame::Loop { brk, cont } if kind != Jump::Return => {
                    break if kind == Jump::Break { brk } else { cont };
                }
                Frame::Loop { .. } => {}
                Frame::Finally { body, outer_raise } => {
                    let tail = self.frames.split_off(i);
                    let saved = std::mem::replace(&mut self.raise_to, outer_raise);
                    let copy = self.new_block();
                    self.edge(self.cur, copy);
                    self.cur = copy;
                    self.block(body);
                    self.raise_to = saved;
                    self.frames.extend(tail);
                }
            }
        };
        self.edge(self.cur, dest);
        // Anything after a jump is unreachable; it still gets a block.
        self.cur = self.new_block();
    }

    fn if_(&mut self, node: Node<'t>) {
        let after = self.new_block();
        let mut fork = self.cur;
        if let Some(cond) = node.child_by_field_name("condition") {
            let mut ev = Vec::new();
            self.expr(cond, &mut ev, true);
            self.emit_node(cond, ev);
        }
        if let Some(cons) = node.child_by_field_name("consequence") {
            let b = self.new_block();
            self.edge(fork, b);
            self.cur = b;
            self.block(cons);
            self.edge(self.cur, after);
        }
        let mut cursor = node.walk();
        let alts: Vec<_> = node
            .children_by_field_name("alternative", &mut cursor)
            .collect();
        let mut has_else = false;
        for alt in alts {
            let head = self.new_block();
            self.edge(fork, head);
            self.cur = head;
            if alt.kind() == "elif_clause" {
                if let Some(cond) = alt.child_by_field_name("condition") {
                    let mut ev = Vec::new();
                    self.expr(cond, &mut ev, true);
                    self.emit_node(cond, ev);
                }
                let b = self.new_block();
                self.edge(head, b);
                self.cur = b;
                if let Some(cons) = alt.child_by_field_name("consequence") {
                    self.block(cons);
                }
                self.edge(self.cur, after);
                fork = head;
            } else {
                if let Some(body) = alt.child_by_field_name("body") {
                    self.block(body);
                }
                self.edge(self.cur, after);
                has_else = true;
            }
        }
        if !has_else {
            self.edge(fork, after);
        }
        self.cur = after;
    }

    /// `while`, including `while … else` (**gap-fill**: the grammar has the
    /// field, ADR-0002's table does not). A literal `while True:` has no false
    /// edge (**gap-fill**): without this, code after an infinite loop that is
    /// left only by `break` would look reachable with the loop's pre-state.
    fn while_(&mut self, node: Node<'t>) {
        let header = self.new_block();
        self.edge(self.cur, header);
        self.cur = header;
        let cond = node.child_by_field_name("condition");
        if let Some(cond) = cond {
            let mut ev = Vec::new();
            self.expr(cond, &mut ev, true);
            self.emit_node(cond, ev);
        }
        let after = self.new_block();
        self.loop_body(node, header, after);
        if cond.is_some_and(|c| c.kind() == "true") {
            if node.child_by_field_name("alternative").is_some() {
                // `while True: … else:` — the else can never run.
                let dead = self.new_block();
                self.cur = dead;
                self.else_body(node, after);
            }
        } else {
            let e = self.new_block();
            self.edge(header, e);
            self.cur = e;
            self.else_body(node, after);
        }
        self.cur = after;
    }

    /// `for`: the iterable is evaluated once, before the loop. The target is
    /// defined on the iterate edge, in a synthetic block of its own, so that a
    /// read of the loop variable after a loop that never ran is may-unbound.
    fn for_(&mut self, node: Node<'t>) {
        if let Some(right) = node.child_by_field_name("right") {
            let mut ev = Vec::new();
            self.expr(right, &mut ev, true);
            self.emit_node(right, ev);
        }
        let header = self.new_block();
        self.edge(self.cur, header);
        let iterate = self.new_block();
        self.edge(header, iterate);
        self.cur = iterate;
        if let Some(left) = node.child_by_field_name("left") {
            let mut ev = Vec::new();
            self.bind_target(left, None, &mut ev);
            self.emit_node(left, ev);
        }
        let after = self.new_block();
        self.loop_body(node, header, after);
        let exhausted = self.new_block();
        self.edge(header, exhausted);
        self.cur = exhausted;
        self.else_body(node, after);
        self.cur = after;
    }

    fn loop_body(&mut self, node: Node<'t>, header: BlockId, after: BlockId) {
        let body_entry = self.new_block();
        self.edge(self.cur, body_entry);
        self.cur = body_entry;
        self.frames.push(Frame::Loop {
            brk: after,
            cont: header,
        });
        if let Some(body) = node.child_by_field_name("body") {
            self.block(body);
        }
        self.frames.pop();
        self.edge(self.cur, header);
    }

    /// The `else` of a loop runs when the loop ends without `break`.
    fn else_body(&mut self, node: Node<'t>, after: BlockId) {
        if let Some(alt) = node.child_by_field_name("alternative") {
            if let Some(body) = alt.child_by_field_name("body") {
                self.block(body);
            }
        }
        self.edge(self.cur, after);
    }

    fn with_(&mut self, node: Node<'t>) {
        let mut ev = Vec::new();
        let mut header_end = node.start_byte();
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for clause in children.iter().filter(|c| c.kind() == "with_clause") {
            header_end = clause.end_byte();
            let mut cc = clause.walk();
            let items: Vec<_> = clause.named_children(&mut cc).collect();
            for item in items.into_iter().filter(|i| i.kind() == "with_item") {
                let Some(value) = item.child_by_field_name("value") else {
                    continue;
                };
                if value.kind() == "as_pattern" {
                    if let Some(expr) = value.named_child(0) {
                        self.expr(expr, &mut ev, true);
                    }
                    if let Some(alias) = value.child_by_field_name("alias") {
                        let mut ac = alias.walk();
                        let targets: Vec<_> = alias.named_children(&mut ac).collect();
                        for t in targets {
                            self.bind_target(t, None, &mut ev);
                        }
                    }
                } else {
                    self.expr(value, &mut ev, true);
                }
            }
        }
        self.emit(node.start_byte(), header_end, ev);
        if let Some(body) = node.child_by_field_name("body") {
            self.block(body);
        }
    }

    /// `match`: the subject is read, then each arm is a successor of the
    /// header. Every identifier in a pattern is first an uncheckable read
    /// (value patterns read names), then each capture is a weak def. Unless
    /// the last arm is irrefutable (`case _`, or a bare capture, with no
    /// guard), the header also falls through to `after`.
    fn match_(&mut self, node: Node<'t>) {
        if let Some(subject) = node.child_by_field_name("subject") {
            let mut ev = Vec::new();
            self.expr(subject, &mut ev, true);
            self.emit_node(subject, ev);
        }
        let header = self.cur;
        let after = self.new_block();
        let mut cases = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            cases.extend(
                body.named_children(&mut cursor)
                    .filter(|c| c.kind() == "case_clause"),
            );
        }
        let irrefutable = cases.last().is_some_and(|c| Self::irrefutable(*c));
        for case in cases {
            let arm = self.new_block();
            self.edge(header, arm);
            self.cur = arm;
            let mut ev = Vec::new();
            let mut cursor = case.walk();
            let patterns: Vec<_> = case
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "case_pattern")
                .collect();
            let mut idents = Vec::new();
            for p in &patterns {
                scope::identifiers_in(*p, &mut idents);
            }
            idents.retain(|i| !scope::is_keyword_pattern_name(*i));
            for i in &idents {
                self.use_(*i, false, &mut ev);
            }
            for i in idents.iter().filter(|i| scope::is_capture(**i)) {
                self.event(*i, EventKind::WeakDef, &mut ev);
            }
            if let Some(guard) = case.child_by_field_name("guard") {
                self.expr(guard, &mut ev, true);
            }
            let header_end = case
                .child_by_field_name("consequence")
                .map_or(case.end_byte(), |c| c.start_byte());
            self.emit(case.start_byte(), header_end, ev);
            if let Some(cons) = case.child_by_field_name("consequence") {
                self.block(cons);
            }
            self.edge(self.cur, after);
        }
        if !irrefutable {
            self.edge(header, after);
        }
        self.cur = after;
    }

    fn irrefutable(case: Node<'_>) -> bool {
        if case.child_by_field_name("guard").is_some() {
            return false;
        }
        let mut cursor = case.walk();
        let patterns: Vec<_> = case
            .named_children(&mut cursor)
            .filter(|c| c.kind() == "case_pattern")
            .collect();
        let [pattern] = patterns.as_slice() else {
            return false;
        };
        match pattern.named_child_count() {
            0 => true, // `case _`
            1 => {
                let only = pattern.named_child(0).expect("count is 1");
                only.kind() == "dotted_name" && only.named_child_count() == 1
            }
            _ => false,
        }
    }

    /// `try`. Two load-bearing raise edges per handler (ADR-0002): one from
    /// the block before each statement of the body (the first of which is the
    /// empty `try_entry`, carrying the pre-try state), and one from the body's
    /// tail. Unhandled exceptions also leave to the enclosing raise targets.
    ///
    /// **Gap-fill — `finally` is built once per exit destination**, not once
    /// as a join: a normal copy (to `after`), an exceptional copy (re-raise to
    /// the enclosing targets), and one per `return`/`break`/`continue` that
    /// crosses it (see [`Builder::jump`]). A single shared `finally` would
    /// merge those paths, and a read in `finally` of a name bound only on the
    /// normal path would look may-unbound on every path. The rules must
    /// therefore combine per source location: a store is dead only if it is
    /// dead in every copy; a read is may-unbound if it is in any copy.
    fn try_(&mut self, node: Node<'t>) {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        let handlers: Vec<_> = children
            .iter()
            .copied()
            .filter(|c| c.kind() == "except_clause")
            .collect();
        let else_clause = children.iter().copied().find(|c| c.kind() == "else_clause");
        let finally_body = children
            .iter()
            .copied()
            .find(|c| c.kind() == "finally_clause")
            .and_then(Self::clause_block);

        let outer_raise = self.raise_to.clone();
        let exc_finally = finally_body.map(|_| self.new_block());
        let past_try: Vec<BlockId> = match exc_finally {
            Some(f) => vec![f],
            None => outer_raise.clone(),
        };
        let handler_entries: Vec<BlockId> = handlers.iter().map(|_| self.new_block()).collect();

        if let Some(body) = finally_body {
            self.frames.push(Frame::Finally {
                body,
                outer_raise: outer_raise.clone(),
            });
        }

        // Body.
        self.raise_to = handler_entries
            .iter()
            .copied()
            .chain(past_try.clone())
            .collect();
        let try_entry = self.new_block();
        self.edge(self.cur, try_entry);
        self.cur = try_entry;
        if let Some(body) = node.child_by_field_name("body") {
            self.block(body);
        }
        let tail = self.cur;
        self.edges_to_raise(tail);

        // `else` runs after a body that raised nothing; handlers don't cover it.
        self.raise_to = past_try.clone();
        if let Some(else_body) = else_clause.and_then(|e| e.child_by_field_name("body")) {
            self.block(else_body);
        }
        // `stmt` adds a raise edge *before* each statement; the state after the
        // last one needs its own, or the final statement cannot raise.
        self.tail_raise();
        let mut normal_ends = vec![self.cur];

        for (clause, entry) in handlers.iter().zip(handler_entries) {
            self.cur = entry;
            let mut ev = Vec::new();
            let mut alias_targets = Vec::new();
            let value = clause.child_by_field_name("value");
            if let Some(value) = value {
                if value.kind() == "as_pattern" {
                    if let Some(ty) = value.named_child(0) {
                        self.expr(ty, &mut ev, true);
                    }
                    if let Some(alias) = value.child_by_field_name("alias") {
                        let mut ac = alias.walk();
                        let ts: Vec<_> = alias.named_children(&mut ac).collect();
                        for t in ts {
                            scope::target_identifiers(t, &mut alias_targets);
                        }
                    }
                    for t in &alias_targets {
                        self.def(*t, None, &mut ev);
                    }
                } else {
                    self.expr(value, &mut ev, true);
                }
            }
            let block = Self::clause_block(*clause);
            let header_end = block.map_or(clause.end_byte(), |b| b.start_byte());
            self.emit(clause.start_byte(), header_end, ev);
            if let Some(block) = block {
                self.block(block);
            }
            // Python deletes the `as` name when the handler exits.
            if !alias_targets.is_empty() {
                let mut ev = Vec::new();
                for t in &alias_targets {
                    self.event(*t, EventKind::Kill, &mut ev);
                }
                let (s, e) = (alias_targets[0].start_byte(), alias_targets[0].end_byte());
                self.emit(s, e, ev);
            }
            // After the kill: the implicit `del` runs on the exceptional exit
            // too.
            self.tail_raise();
            normal_ends.push(self.cur);
        }

        if finally_body.is_some() {
            self.frames.pop();
        }
        self.raise_to = outer_raise.clone();

        let join = self.new_block();
        for end in normal_ends {
            self.edge(end, join);
        }
        self.cur = join;
        if let Some(body) = finally_body {
            self.block(body);
            let after_normal = self.cur;

            self.cur = exc_finally.expect("allocated with finally_body");
            self.block(body);
            let from = self.cur;
            self.edges_to_raise(from);

            self.cur = after_normal;
        }
    }

    /// The unfielded `block` child of `except_clause` / `finally_clause`.
    fn clause_block(clause: Node<'t>) -> Option<Node<'t>> {
        let mut cursor = clause.walk();
        let found = clause
            .named_children(&mut cursor)
            .find(|c| c.kind() == "block");
        found
    }
}
