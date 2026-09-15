// SPDX-License-Identifier: MPL-2.0

use tree_sitter::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Rust,
}

impl Lang {
    /// Per `docs/PLAN.adoc` Appendix F: `.jsx` maps to `JavaScript` — the
    /// `tree-sitter-javascript` grammar parses JSX natively, and TSX is
    /// reserved for files with actual TypeScript type annotations.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "py" => Some(Lang::Python),
            "js" | "mjs" | "cjs" | "jsx" => Some(Lang::JavaScript),
            "ts" => Some(Lang::TypeScript),
            "tsx" => Some(Lang::Tsx),
            "rs" => Some(Lang::Rust),
            _ => None,
        }
    }

    pub fn tree_sitter_language(&self) -> Language {
        match self {
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsx_maps_to_javascript_not_tsx() {
        assert_eq!(Lang::from_extension("jsx"), Some(Lang::JavaScript));
    }

    #[test]
    fn unknown_extension_maps_to_none() {
        assert_eq!(Lang::from_extension("txt"), None);
    }

    #[test]
    fn every_lang_variant_produces_a_loadable_grammar() {
        for lang in [
            Lang::Python,
            Lang::JavaScript,
            Lang::TypeScript,
            Lang::Tsx,
            Lang::Rust,
        ] {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&lang.tree_sitter_language())
                .unwrap_or_else(|e| panic!("{lang:?} grammar failed to load: {e}"));
        }
    }
}
