// SPDX-License-Identifier: MPL-2.0

use tree_sitter::{Parser, Tree};

use crate::source::SourceFile;

/// Parse a [`SourceFile`] with the tree-sitter grammar matching its
/// [`crate::lang::Lang`].
pub fn parse(source: &SourceFile) -> anyhow::Result<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&source.lang.tree_sitter_language())?;
    let tree = parser
        .parse(&source.text, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter produced no tree for {:?}", source.path))?;
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
