// SPDX-License-Identifier: MPL-2.0
//! Input accounting is independent of findings and suppressions.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputStatus {
    Analyzed,
    Unsupported,
    Unreadable,
    ParseError,
    NoRules,
    Excluded,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputCoverage {
    pub path: String,
    pub status: InputStatus,
    pub language: Option<String>,
    pub analyzer: Option<String>,
    pub rules: Vec<String>,
    pub reason: Option<String>,
}

impl InputCoverage {
    pub fn new(path: &std::path::Path, status: InputStatus, reason: &str) -> Self {
        Self {
            path: path.display().to_string(),
            status,
            language: None,
            analyzer: None,
            rules: Vec::new(),
            reason: (!reason.is_empty()).then(|| reason.to_string()),
        }
    }
}

/// Recognised source/config inputs without a native grammar are explicit gaps.
/// Other extensions are listed as outside this declared scope, not analyzed.
pub fn is_candidate(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "php"
            | "php3"
            | "php4"
            | "php5"
            | "php7"
            | "php8"
            | "phtml"
            | "inc"
            | "py"
            | "pyw"
            | "js"
            | "mjs"
            | "cjs"
            | "jsx"
            | "ts"
            | "tsx"
            | "rs"
            | "hs"
            | "lhs"
            | "ex"
            | "exs"
            | "erl"
            | "hrl"
            | "gleam"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "cxx"
            | "hpp"
            | "go"
            | "java"
            | "kt"
            | "rb"
            | "swift"
            | "cs"
            | "fs"
            | "ml"
            | "mli"
            | "res"
            | "resi"
            | "scala"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "lua"
            | "pl"
            | "r"
            | "jl"
            | "zig"
            | "nix"
            | "ncl"
            | "sql"
            | "yaml"
            | "yml"
            | "toml"
            | "json"
            | "xml"
            | "scm"
            | "idr"
            | "lean"
            | "agda"
            | "v"
            | "chpl"
    ) || matches!(
        path.file_name().and_then(|s| s.to_str()),
        Some("Dockerfile" | "Containerfile" | "Makefile" | "Justfile" | "justfile")
    )
}

#[derive(Serialize)]
pub struct Coverage<'a> {
    pub status: &'static str,
    pub scope: &'static str,
    pub candidate_files: usize,
    pub analyzed_files: usize,
    pub inputs: &'a [InputCoverage],
}

pub const SCOPE: &str = "Enumerated regular files after ignore/hidden/build-directory policy; unsupported recognised source/config inputs are gaps; other extensions are explicitly excluded. Pruned directories are not enumerated. PHP is T0 syntax only; Python alone has CFG/dataflow.";
