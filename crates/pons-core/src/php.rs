// SPDX-License-Identifier: MPL-2.0
//! Bounded intraprocedural PHP may-flow analysis. This is not PHP execution.
//! Unsupported side effects make a whole function opaque, never clean.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tree_sitter::{Node, Tree};

pub const HTML: u8 = 1;
pub const SQL: u8 = 2;
pub const SHELL: u8 = 4;
pub const HEADER: u8 = 8;
pub const URL: u8 = 16;
pub const JAVASCRIPT: u8 = 32;
const ALL: u8 = HTML | SQL | SHELL | HEADER | URL | JAVASCRIPT;
#[derive(Debug, Default)]
pub struct Analysis {
    pub flows: Vec<Flow>,
    pub unknowns: Vec<String>,
    pub blocks: usize,
}
#[derive(Debug)]
pub struct Flow {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub sources: Vec<usize>,
    pub context: &'static str,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Fact(BTreeMap<usize, u8>);
impl Fact {
    fn join(&mut self, other: &Self) {
        for (&line, &bits) in &other.0 {
            *self.0.entry(line).or_default() |= bits;
        }
    }
    fn clear(&mut self, bits: u8) {
        for b in self.0.values_mut() {
            *b &= !bits;
        }
        self.0.retain(|_, b| *b != 0);
    }
}
type State = BTreeMap<String, Fact>;
#[derive(Clone)]
enum Expr {
    Clean,
    Source(usize),
    Var(String),
    Join(Vec<Expr>),
    Sanitise(Box<Expr>, u8),
}
impl Expr {
    fn eval(&self, state: &State) -> Fact {
        match self {
            Self::Clean => Fact::default(),
            Self::Source(line) => Fact(BTreeMap::from([(*line, ALL)])),
            Self::Var(name) => state.get(name).cloned().unwrap_or_default(),
            Self::Join(exprs) => {
                let mut f = Fact::default();
                for e in exprs {
                    f.join(&e.eval(state));
                }
                f
            }
            Self::Sanitise(e, bits) => {
                let mut f = e.eval(state);
                f.clear(*bits);
                f
            }
        }
    }
}
#[derive(Clone)]
enum Op {
    Nop,
    Assign(String, Expr),
    Sink(Expr, u8, &'static str, usize, usize, usize),
}
struct Block {
    op: Op,
    next: Vec<usize>,
}
struct Builder {
    blocks: Vec<Block>,
}
fn children(n: Node<'_>) -> Vec<Node<'_>> {
    let mut c = n.walk();
    n.named_children(&mut c).collect()
}
fn text<'a>(n: Node<'_>, src: &'a str) -> &'a str {
    n.utf8_text(src.as_bytes()).unwrap_or("")
}
fn field<'a>(n: Node<'a>, name: &str) -> Result<Node<'a>, String> {
    n.child_by_field_name(name)
        .ok_or_else(|| format!("missing {name} at line {}", n.start_position().row + 1))
}
fn scope(n: Node<'_>) -> bool {
    matches!(
        n.kind(),
        "function_definition" | "method_declaration" | "anonymous_function" | "arrow_function"
    )
}
fn arguments<'a>(n: Node<'a>) -> Vec<Node<'a>> {
    n.child_by_field_name("arguments")
        .map(children)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.kind() != "comment")
        .filter_map(|a| a.named_child(0))
        .collect()
}
fn expression(n: Node<'_>, src: &str) -> Result<Expr, String> {
    let cs = children(n);
    Ok(match n.kind() {
        "integer" | "float" | "boolean" | "null" | "string" => Expr::Clean,
        "variable_name" => {
            if ["$_GET", "$_POST", "$_REQUEST", "$_COOKIE"].contains(&text(n, src)) {
                Expr::Source(n.start_position().row + 1)
            } else {
                Expr::Var(text(n, src).into())
            }
        }
        "subscript_expression" => {
            let base = n.named_child(0).ok_or("empty subscript")?;
            // Arrays/aliases other than request sources are outside this slice.
            if base.kind() == "variable_name"
                && ["$_GET", "$_POST", "$_REQUEST", "$_COOKIE"].contains(&text(base, src))
            {
                Expr::Source(n.start_position().row + 1)
            } else {
                return Err("array/alias value not modelled".into());
            }
        }
        "parenthesized_expression" | "argument" | "sequence_expression" | "encapsed_string" => {
            Expr::Join(
                cs.into_iter()
                    .map(|c| expression(c, src))
                    .collect::<Result<_, _>>()?,
            )
        }
        "string_content" | "escape_sequence" => Expr::Clean,
        "binary_expression" => {
            let op = text(field(n, "operator")?, src);
            // Concatenation propagates taint; comparisons and boolean results
            // are values, not copies of attacker-controlled strings. Validate
            // both operands to reject hidden assignments/calls either way.
            let a = expression(field(n, "left")?, src)?;
            let b = expression(field(n, "right")?, src)?;
            if op == "." || op == "??" {
                Expr::Join(vec![a, b])
            } else {
                Expr::Clean
            }
        }
        "unary_op_expression" => {
            for c in cs {
                expression(c, src)?;
            }
            Expr::Clean
        }
        "function_call_expression" => {
            let name = text(field(n, "function")?, src)
                .trim_start_matches('\\')
                .to_ascii_lowercase();
            let args = arguments(n);
            let mut exprs = args
                .into_iter()
                .map(|c| expression(c, src))
                .collect::<Result<Vec<_>, _>>()?;
            match name.as_str() {
                "htmlspecialchars" | "htmlentities" | "esc_html" => Expr::Sanitise(
                    Box::new(exprs.drain(..).next().ok_or("missing sanitizer argument")?),
                    HTML,
                ),
                // These operations do not establish a general output contract.
                "strip_tags" | "addslashes" | "esc_js" | "esc_url" | "esc_sql"
                | "escapeshellarg" | "json_encode" => Expr::Join(exprs),
                _ => return Err(format!("call summary unavailable: {name}")),
            }
        }
        _ => return Err(format!("expression {} not modelled", n.kind())),
    })
}
impl Builder {
    fn add(&mut self, op: Op, next: Vec<usize>) -> Result<usize, String> {
        if self.blocks.len() >= 1024 {
            return Err("CFG budget exhausted (1024 blocks)".into());
        }
        let id = self.blocks.len();
        self.blocks.push(Block { op, next });
        Ok(id)
    }
    fn stmt(
        &mut self,
        n: Node<'_>,
        src: &str,
        next: usize,
        loop_edges: Option<(usize, usize)>,
    ) -> Result<usize, String> {
        match n.kind() {
            "program" | "compound_statement" | "colon_block" => {
                let mut head = next;
                for c in children(n).into_iter().rev() {
                    head = self.stmt(c, src, head, loop_edges)?;
                }
                Ok(head)
            }
            "comment" | "php_tag" | "text" | "text_interpolation" | "empty_statement" => Ok(next),
            _ if scope(n) => Ok(next),
            "class_declaration" | "interface_declaration" | "trait_declaration" => Ok(next),
            "if_statement" => {
                expression(field(n, "condition")?, src)?;
                let yes = self.stmt(field(n, "body")?, src, next, loop_edges)?;
                let mut no = next;
                for a in children(n)
                    .into_iter()
                    .filter(|c| matches!(c.kind(), "else_clause" | "else_if_clause"))
                    .rev()
                {
                    let body = self.stmt(field(a, "body")?, src, next, loop_edges)?;
                    if a.kind() == "else_if_clause" {
                        expression(field(a, "condition")?, src)?;
                        no = self.add(Op::Nop, vec![body, no])?;
                    } else {
                        no = body;
                    }
                }
                self.add(Op::Nop, vec![yes, no])
            }
            "while_statement" => {
                expression(field(n, "condition")?, src)?;
                let cond = self.add(Op::Nop, vec![])?;
                let body = self.stmt(field(n, "body")?, src, cond, Some((next, cond)))?;
                self.blocks[cond].next = vec![body, next];
                Ok(cond)
            }
            "break_statement" | "continue_statement" => {
                if n.named_child_count() != 0 {
                    return Err("numbered loop jump not modelled".into());
                }
                let (br, co) = loop_edges.ok_or("jump outside supported loop")?;
                self.add(
                    Op::Nop,
                    vec![if n.kind() == "break_statement" {
                        br
                    } else {
                        co
                    }],
                )
            }
            "return_statement" | "throw_expression" => {
                for c in children(n) {
                    expression(c, src)?;
                }
                self.add(Op::Nop, vec![])
            }
            "echo_statement" => {
                let expr = Expr::Join(
                    children(n)
                        .into_iter()
                        .map(|c| expression(c, src))
                        .collect::<Result<_, _>>()?,
                );
                self.add(
                    Op::Sink(
                        expr,
                        HTML,
                        "html-text",
                        n.start_byte(),
                        n.end_byte(),
                        n.start_position().row + 1,
                    ),
                    vec![next],
                )
            }
            "expression_statement" => {
                let c = n.named_child(0).ok_or("empty expression statement")?;
                if c.kind() == "assignment_expression" {
                    let lhs = field(c, "left")?;
                    if lhs.kind() != "variable_name" {
                        return Err("assignment alias not modelled".into());
                    }
                    self.add(
                        Op::Assign(text(lhs, src).into(), expression(field(c, "right")?, src)?),
                        vec![next],
                    )
                } else if c.kind() == "function_call_expression" {
                    let name = text(field(c, "function")?, src)
                        .trim_start_matches('\\')
                        .to_ascii_lowercase();
                    let (bits, context, index) = match name.as_str() {
                        "mysqli_query" => (SQL, "sql-query", 1),
                        "exec" | "shell_exec" | "system" | "passthru" => {
                            (SHELL, "shell-command", 0)
                        }
                        "header" => (HEADER, "header", 0),
                        "wp_redirect" => (URL, "redirect-url", 0),
                        _ => return Err(format!("statement call summary unavailable: {name}")),
                    };
                    let args = arguments(c);
                    let arg = *args.get(index).ok_or("missing sink argument")?;
                    self.add(
                        Op::Sink(
                            expression(arg, src)?,
                            bits,
                            context,
                            c.start_byte(),
                            c.end_byte(),
                            c.start_position().row + 1,
                        ),
                        vec![next],
                    )
                } else if c.kind() == "throw_expression" {
                    self.add(Op::Nop, vec![])
                } else {
                    expression(c, src)?;
                    self.add(Op::Nop, vec![next])
                }
            }
            _ => Err(format!(
                "statement {} at line {} not modelled",
                n.kind(),
                n.start_position().row + 1
            )),
        }
    }
}
fn solve(b: &Builder, entry: usize) -> Result<Vec<State>, String> {
    let mut states = vec![State::new(); b.blocks.len()];
    let mut queue = VecDeque::from([entry]);
    let mut reached = BTreeSet::from([entry]);
    let mut queued = BTreeSet::from([entry]);
    let mut steps = 0;
    while let Some(id) = queue.pop_front() {
        queued.remove(&id);
        steps += 1;
        if steps > 50_000 {
            return Err("dataflow budget exhausted (50000 transfers)".into());
        }
        let mut state = states[id].clone();
        if let Op::Assign(name, expr) = &b.blocks[id].op {
            state.insert(name.clone(), expr.eval(&state));
        }
        if state.len() > 512 {
            return Err("local-variable budget exhausted (512)".into());
        }
        for &next in &b.blocks[id].next {
            let mut changed = reached.insert(next);
            for (name, fact) in &state {
                let prior = states[next].entry(name.clone()).or_default();
                let old = prior.clone();
                prior.join(fact);
                changed |= *prior != old;
            }
            if changed && queued.insert(next) {
                queue.push_back(next);
            }
        }
    }
    // Unreachable blocks must not emit direct-source findings.
    for (id, state) in states.iter_mut().enumerate() {
        if !reached.contains(&id) {
            state.insert("<unreachable>".into(), Fact::default());
        }
    }
    Ok(states)
}
pub fn analyse(tree: &Tree, src: &str) -> Analysis {
    let mut result = Analysis::default();
    let root = tree.root_node();
    let mut units = vec![root];
    let mut pending = vec![root];
    // Namespaces, shadowed sanitizers and PHP template contexts need resolution
    // beyond this slice. Parsing them is supported; flow evidence is withheld.
    let mut ambiguous = false;
    while let Some(n) = pending.pop() {
        if n.kind() == "namespace_definition" {
            ambiguous = true;
        }
        if n.kind() == "text" && !text(n, src).trim().is_empty() {
            ambiguous = true;
        }
        if scope(n) {
            if n.kind() == "anonymous_function" || n.kind() == "arrow_function" {
                result.unknowns.push(format!(
                    "callback captures/invocation unresolved at line {}",
                    n.start_position().row + 1
                ));
            }
            if let Some(body) = n.child_by_field_name("body") {
                units.push(body);
            }
            if let Some(name) = n.child_by_field_name("name") {
                if ["htmlspecialchars", "htmlentities", "esc_html"]
                    .contains(&text(name, src).to_ascii_lowercase().as_str())
                {
                    ambiguous = true;
                }
            }
        }
        pending.extend(children(n));
    }
    if ambiguous {
        result.unknowns.push(
            "namespace/shadowing or template output context unresolved; PHP flow withheld".into(),
        );
        return result;
    }
    for unit in units {
        let mut b = Builder {
            blocks: vec![Block {
                op: Op::Nop,
                next: vec![],
            }],
        };
        let analysis = b
            .stmt(unit, src, 0, None)
            .and_then(|entry| solve(&b, entry));
        result.blocks += b.blocks.len();
        match analysis {
            Err(e) => result
                .unknowns
                .push(format!("scope line {}: {e}", unit.start_position().row + 1)),
            Ok(states) => {
                for (id, block) in b.blocks.iter().enumerate() {
                    if states[id].contains_key("<unreachable>") {
                        continue;
                    }
                    if let Op::Sink(expr, bits, context, start, end, line) = &block.op {
                        let sources: Vec<_> = expr
                            .eval(&states[id])
                            .0
                            .into_iter()
                            .filter(|(_, b)| b & bits != 0)
                            .map(|(l, _)| l)
                            .collect();
                        if !sources.is_empty() {
                            result.flows.push(Flow {
                                start: *start,
                                end: *end,
                                line: *line,
                                sources,
                                context,
                            });
                        }
                    }
                }
            }
        }
    }
    result.unknowns.sort();
    result.unknowns.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lang::Lang, parse, source::SourceFile};
    fn analyse_text(text: &str) -> Analysis {
        let src = SourceFile {
            path: "case.php".into(),
            lang: Lang::Php,
            text: text.into(),
            transcoded: false,
        };
        analyse(&parse::parse(&src).unwrap(), text)
    }
    #[test]
    fn branch_join_early_exit_and_loop_fixed_point() {
        assert_eq!(
            analyse_text("<?php function f($b) { $x=''; while ($b) { $x=$_GET['x']; } echo $x; }")
                .flows
                .len(),
            1
        );
        assert!(analyse_text("<?php function f($b) { $x=$_GET['x']; if ($b) { $x=htmlspecialchars($x); } else { return; } echo $x; }").flows.is_empty());
        assert_eq!(analyse_text("<?php function f($b) { $x=$_GET['x']; if ($b) { $x=htmlspecialchars($x); } echo $x; }").flows.len(),1);
    }
    #[test]
    fn unknown_is_not_safe_and_scopes_do_not_leak() {
        let a =
            analyse_text("<?php function f() { try { echo $_GET['x']; } finally { cleanup(); } }");
        assert!(a.flows.is_empty());
        assert!(a.unknowns.iter().any(|s| s.contains("try_statement")));
        let a = analyse_text("<?php function f() { $x=&$y; echo $x; }");
        assert!(!a.unknowns.is_empty());
        let a = analyse_text("<?php function a() { $x=$_GET['x']; } function b() { echo $x; }");
        assert!(a.flows.is_empty());
    }
    #[test]
    fn output_contexts_are_distinct() {
        let a=analyse_text("<?php function f($db) { $x=htmlspecialchars($_GET['x']); echo $x; mysqli_query($db,$x); header($x); exec($x); wp_redirect($x); }");
        assert_eq!(a.flows.len(), 4);
        assert!(!a.flows.iter().any(|f| f.context == "html-text"));
    }
    #[test]
    fn native_php_syntax_and_template_unknowns() {
        for text in [
            "<?php namespace A; trait T { function verify() {} } class C { use T; }",
            "<div><?php echo $_GET['x']; ?></div>",
            "<?php $f = function($x) use ($y) { return $x; }; add_action('init', $f);",
        ] {
            let a = analyse_text(text);
            assert!(!a.unknowns.is_empty());
        }
    }
    #[test]
    fn bounded_cfg_records_exhaustion() {
        let text = format!("<?php function f() {{ {} }}", "$x=1;".repeat(1100));
        let a = analyse_text(&text);
        assert!(a.unknowns.iter().any(|s| s.contains("budget exhausted")));
    }
}
