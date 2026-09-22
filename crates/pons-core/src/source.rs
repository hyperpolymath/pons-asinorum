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

/// A source file located by [`discover`] but **not yet read**.
///
/// Discovery is deliberately separated from reading. Slurping an entire tree
/// into memory before a single rule runs is wasteful, but the real defect it
/// caused was worse: one unreadable file aborted the whole scan. Reading now
/// happens per file inside [`Engine::scan_files`][crate::engine::Engine::scan_files],
/// where a failure is recorded as a skip and the scan carries on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub lang: Lang,
}

pub struct SourceFile {
    pub path: PathBuf,
    pub lang: Lang,
    pub text: String,
}

/// Recursively discover source files under `root`, honouring `.gitignore`
/// and a custom `.ponsignore` file, and skipping [`SKIP_DIRS`].
///
/// Returns paths and languages only — see [`read`] for the contents.
///
/// A walk error is **fatal by design**, and that is not merely the cheaper
/// option: the walker reports "root does not exist" as an error entry, and
/// ADR-0005 requires that case to exit 2. Failing to enumerate the tree is an
/// operational error about the scan itself; failing to read one file inside a
/// tree that enumerated fine is not, and is handled as a skip instead.
pub fn discover(root: &Path) -> anyhow::Result<Vec<DiscoveredFile>> {
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

        files.push(DiscoveredFile {
            path: path.to_path_buf(),
            lang,
        });
    }

    Ok(files)
}

/// Read one [`DiscoveredFile`] into a [`SourceFile`].
///
/// Fallible on purpose, and called per file: this is the boundary at which a
/// single unreadable file stops being able to end the whole scan.
pub fn read(file: &DiscoveredFile) -> anyhow::Result<SourceFile> {
    let bytes = std::fs::read(&file.path)?;
    let text = String::from_utf8(bytes).unwrap_or_else(|e| {
        // Latin-1 fallback: every byte is a valid Unicode scalar value
        // in 0..=255, so this never fails, unlike UTF-8 decoding.
        e.into_bytes().into_iter().map(char::from).collect()
    });

    Ok(SourceFile {
        path: file.path.clone(),
        lang: file.lang,
        text,
    })
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

    #[test]
    fn reading_a_file_that_vanished_after_discovery_is_an_error_not_a_panic() {
        let dir = tempfile_dir();
        std::fs::write(dir.join("gone.py"), "x = 1").unwrap();

        let found = discover(&dir).unwrap();
        assert_eq!(found.len(), 1);

        // The TOCTOU window discovery/read separation exists to survive.
        std::fs::remove_file(dir.join("gone.py")).unwrap();
        assert!(read(&found[0]).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_applies_the_latin1_fallback_for_non_utf8_bytes() {
        let dir = tempfile_dir();
        // 0xFF is never valid UTF-8; it must survive as U+00FF, not panic.
        std::fs::write(dir.join("latin.py"), [b'x', b' ', b'=', b' ', 0xFF]).unwrap();

        let found = discover(&dir).unwrap();
        let src = read(&found[0]).unwrap();
        assert!(src.text.ends_with('\u{00FF}'));

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
