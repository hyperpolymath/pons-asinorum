// SPDX-License-Identifier: MPL-2.0
use pons_core::{
    engine::{Rule, RuleCtx},
    finding::{Location, RawFinding, Severity, Tier},
    lang::Lang,
};
use tree_sitter::Node;
fn nodes(root: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(n) = pending.pop() {
        out.push(n);
        let mut c = n.walk();
        pending.extend(n.named_children(&mut c));
    }
    out
}
pub struct PhpContextFlow;
impl Rule for PhpContextFlow {
    fn id(&self) -> &'static str {
        "php-context-flow"
    }
    fn description(&self) -> &'static str {
        "request data may reach an output context without a matching escape contract"
    }
    fn languages(&self) -> &'static [Lang] {
        &[Lang::Php]
    }
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        ctx.php().flows.iter().filter_map(|f|{
            let node=ctx.tree.root_node().descendant_for_byte_range(f.start,f.end)?;
            Some(RawFinding::new(Tier::T1,Severity::Warn,Location::from_node(ctx.path.display().to_string(),&node,ctx.text),
                format!("request data may reach {} output",f.context),
                format!("bounded intraprocedural CFG may-flow: request source lines {:?} -> sink line {}; context={}; branch joins use union",f.sources,f.line,f.context),
                Some("Assumes standard unshadowed builtins/WordPress escaping, request superglobals not rebound, HTML text context for echo, and execution of this function. Parameters/callers, path feasibility, aliases, authorization and runtime behavior are not established. Unknown scopes emit no flow verdict.".into())))
        }).collect()
    }
}
pub struct PhpEmptySecurityFunction;
impl Rule for PhpEmptySecurityFunction {
    fn id(&self) -> &'static str {
        "php-empty-security-function"
    }
    fn description(&self) -> &'static str {
        "a security-named PHP function has an empty executable body"
    }
    fn languages(&self) -> &'static [Lang] {
        &[Lang::Php]
    }
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        nodes(ctx.tree.root_node()).into_iter().filter_map(|n|{
            if !matches!(n.kind(),"function_definition"|"method_declaration"){return None;}
            let name=n.child_by_field_name("name")?.utf8_text(ctx.text.as_bytes()).ok()?.to_ascii_lowercase();
            if !["encrypt","decrypt","verify","sign","hash","auth","secure","random","validate"].iter().any(|s|name.contains(s)){return None;}
            let body=n.child_by_field_name("body")?;
            let mut c=body.walk();if body.named_children(&mut c).any(|n|n.kind()!="comment"){return None;}
            Some(RawFinding::new(Tier::T0,Severity::Warn,Location::from_node(ctx.path.display().to_string(),&n,ctx.text),
                "empty security-named function", "native PHP AST: declared body contains no executable statements; ordinary PHP completion returns null", Some("Name is a heuristic: an intentional no-op hook/test double may be legitimate; reachability and caller expectations are unknown.".into())))
        }).collect()
    }
}
/// Literal dependency resolution only; no include execution, include_path, or Composer autoload claims.
pub struct PhpUnresolvedInclude;
fn literal(n: Node<'_>, src: &str, dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let text = n.utf8_text(src.as_bytes()).ok()?;
    if n.kind() == "string" && !text.contains('\\') && !text.contains('$') {
        return Some(text.get(1..text.len().checked_sub(1)?)?.into());
    }
    if text == "__DIR__" {
        return Some(if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(dir)
        });
    }
    if n.kind() == "binary_expression"
        && n.child_by_field_name("operator")?
            .utf8_text(src.as_bytes())
            .ok()?
            == "."
    {
        let left = literal(n.child_by_field_name("left")?, src, dir)?;
        let right = literal(n.child_by_field_name("right")?, src, dir)?;
        return Some(format!("{}{}", left.display(), right.display()).into());
    }
    None
}
impl Rule for PhpUnresolvedInclude {
    fn id(&self) -> &'static str {
        "php-unresolved-include"
    }
    fn description(&self) -> &'static str {
        "a literal PHP include is empty or absent relative to the source directory"
    }
    fn languages(&self) -> &'static [Lang] {
        &[Lang::Php]
    }
    fn check(&self, ctx: &RuleCtx) -> Vec<RawFinding> {
        let dir = ctx.path.parent().unwrap_or(std::path::Path::new("."));
        nodes(ctx.tree.root_node()).into_iter().filter_map(|n|{
            if !matches!(n.kind(),"include_expression"|"include_once_expression"|"require_expression"|"require_once_expression"){return None;}
            let path=literal(n.named_child(0)?,ctx.text,dir)?;
            if !path.as_os_str().is_empty() && dir.join(&path).is_file(){return None;}
            Some(RawFinding::new(Tier::T0,Severity::Warn,Location::from_node(ctx.path.display().to_string(),&n,ctx.text),
                "literal include dependency is unresolved",format!("literal path {:?}; absent relative to source directory or empty; no code executed",path),
                Some("PHP cwd/include_path, generated dependencies or deployment packaging may resolve it differently. Dynamic includes, Composer class resolution and WordPress bootstrap ordering remain unknown.".into())))
        }).collect()
    }
}
