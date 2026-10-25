// SPDX-License-Identifier: MPL-2.0
//! Language-neutral policy oracle for resolved statement summaries supplied by
//! an adapter. No SQL parsing or Elixir/Haskell source analysis is implied.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
pub enum Statement {
    Read,
    Write { table: String },
    Unknown,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Unknown,
}
pub fn decide(
    statements: &[Statement],
    allow: &BTreeSet<String>,
    hybrid_resolved: bool,
) -> Decision {
    if !hybrid_resolved || statements.is_empty() {
        return Decision::Unknown;
    }
    let mut unknown = false;
    for s in statements {
        match s {
            Statement::Write { table } if !allow.contains(table) => return Decision::Deny,
            Statement::Unknown => unknown = true,
            _ => {}
        }
    }
    if unknown {
        Decision::Unknown
    } else {
        Decision::Allow
    }
}
#[derive(Default)]
pub struct Prepared {
    statements: BTreeMap<String, Vec<Statement>>,
    bound: BTreeSet<String>,
}
impl Prepared {
    pub fn prepare(&mut self, id: String, statements: Vec<Statement>) {
        self.bound.remove(&id);
        self.statements.insert(id, statements);
    }
    pub fn bind(&mut self, id: &str) -> bool {
        if self.statements.contains_key(id) {
            self.bound.insert(id.into());
            true
        } else {
            false
        }
    }
    pub fn execute(&self, id: &str, allow: &BTreeSet<String>) -> Decision {
        if !self.bound.contains(id) {
            return Decision::Unknown;
        }
        self.statements
            .get(id)
            .map_or(Decision::Unknown, |s| decide(s, allow, true))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn write(s: &str) -> Statement {
        Statement::Write { table: s.into() }
    }
    #[test]
    fn all_writes_exact_tables_empty_allowlist_and_hybrid() {
        let allow = BTreeSet::from(["posts".into()]);
        assert_eq!(
            decide(&[write("posts"), write("users")], &allow, true),
            Decision::Deny
        );
        assert_eq!(
            decide(&[write("posts_archive")], &allow, true),
            Decision::Deny
        );
        assert_eq!(
            decide(&[write("posts")], &BTreeSet::new(), true),
            Decision::Deny
        );
        assert_eq!(decide(&[write("posts")], &allow, false), Decision::Unknown);
        assert_eq!(
            decide(&[Statement::Read, write("posts")], &allow, true),
            Decision::Allow
        );
    }
    #[test]
    fn prepared_statements_keep_policy_across_bind_and_execute() {
        let mut p = Prepared::default();
        let allow = BTreeSet::from(["posts".into()]);
        assert_eq!(p.execute("q", &allow), Decision::Unknown);
        p.prepare("q".into(), vec![write("posts"), write("users")]);
        assert!(p.bind("q"));
        assert_eq!(p.execute("q", &allow), Decision::Deny);
        p.prepare("q".into(), vec![write("posts")]);
        assert_eq!(p.execute("q", &allow), Decision::Unknown);
        assert!(p.bind("q"));
        assert_eq!(p.execute("q", &allow), Decision::Allow);
    }
}
