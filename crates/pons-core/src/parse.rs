// SPDX-License-Identifier: MPL-2.0

use tree_sitter::{Parser, Tree};

use crate::source::SourceFile;

/// Parse a [`SourceFile`] with the tree-sitter grammar matching its
/// [`crate::lang::Lang`].
pub fn parse(source: &SourceFile) -> anyhow::Result<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&source.lang.tree_sitter_language())?;
    parser
        .parse(&source.text, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter produced no tree for {:?}", source.path))
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
        };

        let tree = parse(&source).unwrap();
        assert_eq!(tree.root_node().kind(), "module");
        assert!(!tree.root_node().has_error());
    }
}
