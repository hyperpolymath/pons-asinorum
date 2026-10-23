// SPDX-License-Identifier: MPL-2.0

//! Inline, source-level suppression.
//!
//! Only actual comment nodes can carry an allowance. A string that happens to
//! contain `pons:allow` cannot silence a finding, because strings are data and
//! the scanner must not let scanned code edit its own verdict.

use crate::engine::RuleCtx;
use crate::finding::Finding;

/// True when a comment on the finding's line, or the immediately preceding
/// line, names the finding's rule in a `pons:allow` directive.
///
/// The syntax is `pons:allow <rule-id>[, <rule-id>...] -- <reason>`. The reason
/// is not machine-enforced for inline comments (project-scoped allowances in
/// `pons.toml` do require one), but the `--` separator is accepted so the same
/// wording works in both places.
pub fn inline_allowed(ctx: &RuleCtx<'_>, finding: &Finding) -> bool {
    let target_line = finding.location().line_start;
    let mut stack = vec![ctx.tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind().contains("comment") {
            let row = node.start_position().row + 1;
            let adjacent = row == target_line || row + 1 == target_line;
            if adjacent {
                let comment = &ctx.text[node.byte_range()];
                if let Some((_, rest)) = comment.split_once("pons:allow ") {
                    let ids = rest.split_once(" -- ").map_or(rest, |x| x.0);
                    if ids
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .any(|id| id == finding.rule_id())
                    {
                        return true;
                    }
                }
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Location, Severity, Tier};
    use crate::lang::Lang;
    use crate::parse;
    use crate::source::SourceFile;
    use std::path::PathBuf;

    fn finding(line: usize) -> Finding {
        Finding::new(
            "self-assignment",
            Tier::T0,
            Severity::Warn,
            Location {
                file: "x.py".to_string(),
                byte_start: 0,
                byte_end: 1,
                line_start: line,
                col_start: 1,
                line_end: line,
                col_end: 2,
            },
            "x",
            "y",
            None,
        )
    }

    #[test]
    fn a_preceding_comment_can_allow_a_rule_but_a_string_cannot() {
        let text = "# pons:allow self-assignment -- deliberate fixture\nx = x\n";
        let source = SourceFile {
            path: PathBuf::from("x.py"),
            lang: Lang::Python,
            text: text.to_string(),
            transcoded: false,
        };
        let tree = parse::parse(&source).unwrap();
        let ctx = RuleCtx::new(&source.path, source.lang, &source.text, &tree);
        assert!(inline_allowed(&ctx, &finding(2)));
        assert!(!inline_allowed(&ctx, &finding(1)));
    }
}
