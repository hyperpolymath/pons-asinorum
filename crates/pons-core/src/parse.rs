// SPDX-License-Identifier: MPL-2.0

use tree_sitter::{Parser, Tree};

use crate::source::SourceFile;

/// Parse a [`SourceFile`] with the tree-sitter grammar matching its
/// [`crate::lang::Lang`].
///
/// Returns an error if the grammar cannot be loaded, no tree is produced,
/// or the tree contains syntax errors; recovered trees are not returned.
pub fn parse(source: &SourceFile) -> anyhow::Result<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&source.lang.tree_sitter_language())?;
    let started = std::time::Instant::now();
    let mut progress = |_: &tree_sitter::ParseState| {
        if started.elapsed() >= std::time::Duration::from_millis(250) {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    };
    let text = source.text.as_bytes();
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &text[offset..],
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut progress)),
        )
        .ok_or_else(|| anyhow::anyhow!("parser budget exhausted (250 ms)"))?;
    // Protect recursive analysis consumers from adversarial tree depth/size.
    let mut pending = vec![(tree.root_node(), 0usize)];
    let mut count = 0;
    while let Some((node, depth)) = pending.pop() {
        count += 1;
        anyhow::ensure!(
            count <= 100_000 && depth <= 256,
            "AST budget exhausted (100000 nodes / depth 256)"
        );
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor).map(|n| (n, depth + 1)));
    }
    if tree.root_node().has_error() {
        anyhow::bail!(
            "tree-sitter recovered from a syntax error in {}",
            source.path.display()
        );
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::lang::Lang;

    #[test]
    fn parses_a_small_python_snippet() {
        let source = SourceFile {
            path: PathBuf::from("example.py"),
            lang: Lang::Python,
            text: "x = 1\n".to_string(),
            transcoded: false,
        };

        let tree = parse(&source).unwrap();
        assert_eq!(tree.root_node().kind(), "module");
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn an_error_recovered_tree_is_rejected() {
        let source = SourceFile {
            path: PathBuf::from("broken.py"),
            lang: Lang::Python,
            text: "def f(:\n".to_string(),
            transcoded: false,
        };
        let err = parse(&source).unwrap_err();
        assert!(err.to_string().contains("syntax error"));
    }
}
