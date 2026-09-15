// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};

use crate::lang::Lang;

/// Directories skipped unconditionally, mirroring panic-attack's house
/// behaviour — these are never source we want to analyse, `.ponsignore` or
/// not.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "external_corpora",
    "third_party",
    "corpus",
];

pub struct SourceFile {
    pub path: PathBuf,
    pub lang: Lang,
    pub text: String,
}

/// Recursively discover source files under `root`, honouring `.gitignore`
/// and a custom `.ponsignore` file, and skipping [`SKIP_DIRS`].
pub fn discover(root: &Path) -> anyhow::Result<Vec<SourceFile>> {
    let mut files = Vec::new();

    let walker = ignore::WalkBuilder::new(root)
        .add_custom_ignore_filename(".ponsignore")
        .filter_entry(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| !SKIP_DIRS.contains(&name))
                .unwrap_or(true)
        })
        .build();

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(lang) = path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Lang::from_extension)
        else {
            continue;
        };

        let bytes = std::fs::read(path)?;
        let text = String::from_utf8(bytes).unwrap_or_else(|e| {
            // Latin-1 fallback: every byte is a valid Unicode scalar value
            // in 0..=255, so this never fails, unlike UTF-8 decoding.
            e.into_bytes().into_iter().map(char::from).collect()
        });

        files.push(SourceFile {
            path: path.to_path_buf(),
            lang,
            text,
        });
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_configured_directories() {
        let dir = tempfile_dir();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/ignored.py"), "x = 1").unwrap();
        std::fs::write(dir.join("kept.py"), "x = 1").unwrap();

        let found = discover(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("kept.py"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_extensions_are_skipped() {
        let dir = tempfile_dir();
        std::fs::write(dir.join("notes.txt"), "hello").unwrap();

        let found = discover(&dir).unwrap();
        assert!(found.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pons-source-test-{}", uuid_ish()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn uuid_ish() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
