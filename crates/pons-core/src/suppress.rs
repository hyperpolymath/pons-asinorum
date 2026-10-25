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
/// is mandatory, followed by `| owner=NAME | expires=UNIX_SECONDS`. Expired
/// or malformed directives never suppress a finding. Scope is the rule and
/// adjacent source line; coverage accounting never consults suppression.
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
                    let Some((ids, metadata)) = rest.split_once(" -- ") else {
                        continue;
                    };
                    let parts: Vec<_> = metadata
                        .trim_end_matches("*/")
                        .split('|')
                        .map(str::trim)
                        .collect();
                    let reason = parts.first().is_some_and(|s| !s.is_empty());
                    let owner = parts
                        .iter()
                        .find_map(|s| s.strip_prefix("owner="))
                        .is_some_and(|s| !s.trim().is_empty());
                    let expiry = parts
                        .iter()
                        .find_map(|s| s.strip_prefix("expires="))
                        .and_then(|s| s.trim().parse::<u64>().ok());
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(u64::MAX);
                    if !reason || !owner || !expiry.is_some_and(|e| e > now) {
                        continue;
                    }
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
        let text = concat!(
            "# pons:allow self-assignment -- deliberate fixture | owner=test | expires=4102444800\n",
            "x = x\n",
            "s = \"# pons:allow self-assignment\"\n",
            "y = y\n",
        );
        let source = SourceFile {
            path: PathBuf::from("x.py"),
            lang: Lang::Python,
            text: text.to_string(),
            transcoded: false,
        };
        let tree = parse::parse(&source).unwrap();
        let ctx = RuleCtx::new(&source.path, source.lang, &source.text, &tree);
        // The comment allows its own line and the line that follows it.
        assert!(inline_allowed(&ctx, &finding(1)));
        assert!(inline_allowed(&ctx, &finding(2)));
        // A string literal is data, not a comment: it allows nothing.
        assert!(!inline_allowed(&ctx, &finding(3)));
        assert!(!inline_allowed(&ctx, &finding(4)));
    }
}
