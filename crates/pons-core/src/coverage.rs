// SPDX-License-Identifier: MPL-2.0
//! Inventory outcomes describe scope and execution, never assurance of correctness.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Analysed,
    Unsupported,
    ExcludedByPolicy,
    ParseError,
    Timeout,
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryEntry {
    pub path: String,
    pub kind: String,
    pub language: String,
    pub outcome: Outcome,
    pub reason: String,
    pub parser: Option<String>,
    pub artifact_digest: Option<String>,
    pub transcoded: bool,
    pub analysis_notes: Vec<String>,
    pub analysis_complete: bool,
}
impl InventoryEntry {
    /// Enumeration failures may name a whole subtree, not a source file.
    /// If metadata is inaccessible too, preserve that uncertainty explicitly.
    pub fn unreadable_path(path: &Path, reason: impl Into<String>) -> Self {
        let mut item = Self::new(path, Outcome::Unreadable, reason);
        item.kind = match std::fs::symlink_metadata(path) {
            Ok(m) if m.is_dir() => "directory",
            Ok(m) if m.is_file() => "file",
            _ => "unknown",
        }
        .into();
        if item.kind != "file" {
            item.language = "unknown".into();
            item.parser = None;
            item.analysis_notes
                .push("contents/descendants not enumerated".into());
        }
        item
    }
    pub fn with_language(mut self, lang: crate::lang::Lang) -> Self {
        self.language = lang.name().into();
        self.parser = Some(format!(
            "tree-sitter 0.27.0 / {} {}",
            lang.name(),
            lang.parser_version()
        ));
        self
    }
    pub fn new(path: &Path, outcome: Outcome, reason: impl Into<String>) -> Self {
        Self {
            path: path.display().to_string(),
            kind: "file".into(),
            language: language(path),
            outcome,
            reason: reason.into(),
            parser: path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(crate::lang::Lang::from_extension)
                .map(|l| format!("tree-sitter 0.27.0 / {} {}", l.name(), l.parser_version())),
            artifact_digest: None,
            transcoded: false,
            analysis_notes: Vec::new(),
            analysis_complete: false,
        }
    }
}
/// Known source/config extensions; unknown file types are explicitly out of scope.
pub fn language(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if let Some(lang) = crate::lang::Lang::from_extension(&ext) {
        return lang.name().into();
    }
    match ext.as_str() {
        "php" | "phtml" | "php5" | "inc" => "php",
        "hs" | "lhs" => "haskell",
        "ex" | "exs" => "elixir",
        "lock" | "json" | "jsonl" | "toml" | "yaml" | "yml" | "kyaml" | "xml" | "ini" | "conf"
        | "ncl" | "k9" | "deed" | "a2ml" => "configuration",
        "sh" | "bash" | "zsh" => "shell",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "java" => "java",
        "go" => "go",
        "rb" => "ruby",
        "res" | "resi" => "rescript",
        "idr" | "lidr" => "idris",
        "zig" => "zig",
        "sql" => "sql",
        "html" | "htm" | "twig" => "template",
        "css" | "scss" => "stylesheet",
        "nix" => "nix",
        "scm" | "ss" => "scheme",
        "erl" | "hrl" => "erlang",
        "gleam" => "gleam",
        "lua" => "lua",
        "r" => "r",
        "jl" => "julia",
        _ => match path.file_name().and_then(|s| s.to_str()).unwrap_or("") {
            "Dockerfile" | "Containerfile" | "Makefile" | "justfile" | "Gemfile" | "dune" => {
                "configuration"
            }
            _ => "out-of-scope",
        },
    }
    .into()
}
