// SPDX-License-Identifier: MPL-2.0
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Parser, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Python,
    Javascript,
    Typescript,
    Tsx,
    Rust,
}

impl Lang {
    pub const ALL: [Self; 5] = [
        Self::Python,
        Self::Javascript,
        Self::Typescript,
        Self::Tsx,
        Self::Rust,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Javascript => "javascript",
            Self::Typescript => "typescript",
            Self::Tsx => "tsx",
            Self::Rust => "rust",
        }
    }
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "py" => Some(Self::Python),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::Javascript),
            "ts" => Some(Self::Typescript),
            "tsx" => Some(Self::Tsx),
            "rs" => Some(Self::Rust),
            _ => None,
        }
    }
    pub fn grammar(self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

pub struct Parsed {
    pub lang: Lang,
    pub tree: Tree,
}
impl Parsed {
    pub fn new(lang: Lang, text: &str) -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&lang.grammar())
            .context("loading pinned grammar")?;
        let tree = parser.parse(text, None).context("parser cancelled")?;
        if tree.root_node().has_error() {
            bail!(
                "{} parse contains errors; syntax rules were not run",
                lang.name()
            );
        }
        Ok(Self { lang, tree })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Query, QueryCursor, StreamingIterator};
    #[test]
    fn all_grammars_parse_and_query_real_constructs() {
        for (lang, text) in [
            (Lang::Python, "def f(x):\n    return x + 1\n"),
            (Lang::Javascript, "function f(x) { return x + 1; }"),
            (
                Lang::Typescript,
                "function f(x: number): number { return x + 1; }",
            ),
            (Lang::Tsx, "const f = (x: number) => <div>{x}</div>;"),
            (Lang::Rust, "fn f(x: i32) -> i32 { x + 1 }"),
        ] {
            let parsed = Parsed::new(lang, text).unwrap();
            let query = Query::new(&lang.grammar(), "(identifier) @id").unwrap();
            let mut cursor = QueryCursor::new();
            assert!(
                cursor
                    .matches(&query, parsed.tree.root_node(), text.as_bytes())
                    .next()
                    .is_some(),
                "{}",
                lang.name()
            );
        }
    }
}
