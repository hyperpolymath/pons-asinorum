// SPDX-License-Identifier: MPL-2.0

use std::io;
use std::path::{Path, PathBuf};

use crate::engine::SkippedFile;
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

/// What [`discover`] found: the files to scan, and the parts of the tree it
/// could not enumerate.
///
/// The second field exists because the first is not the whole truth. A walk
/// that hit an unreadable subdirectory has *fewer* files than the tree holds,
/// and returning only `files` would present that partial enumeration as a
/// complete one — the same "a partial scan must never read as a clean one"
/// contract [`SkippedFile`] already carries for reads and parses.
#[derive(Debug, Default)]
pub struct Discovery {
    pub files: Vec<DiscoveredFile>,
    pub skipped: Vec<SkippedFile>,
}

/// The path an [`ignore::Error`] is about, if it names one.
///
/// `ignore` exposes `io_error()` but no path accessor, and `WithPath` — the
/// only variant carrying a path — is routinely nested inside `WithDepth`
/// during a walk, and may sit under `WithLineNumber` or a `Partial` when the
/// failure came from an ignore file. Matching only the outermost variant would
/// therefore drop the path for exactly the errors a walk produces, leaving
/// every skip named `<unknown path>`. The recursion mirrors `io_error`'s.
///
/// The outermost `WithPath` wins: `ignore` attaches the path at the point it
/// becomes known, so the outer one is the entry the walker was working on.
fn error_path(err: &ignore::Error) -> Option<&Path> {
    match err {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            error_path(err)
        }
        ignore::Error::Partial(errs) => errs.iter().find_map(error_path),
        // Not reachable without `follow_links`, but the child is the entry
        // that could not be visited, which is what a skip should name.
        ignore::Error::Loop { child, .. } => Some(child),
        _ => None,
    }
}

/// The bare OS cause of an I/O error, with every wrapper's prose stripped off.
///
/// Measured against `ignore` 0.4.33, not assumed. `Error::io_error()` recurses
/// to the innermost `Error::Io`, but for a *walk* error that `io::Error` was
/// built by `walkdir` as `io::Error::new(kind, walkdir_err)` — a custom
/// payload, so `raw_os_error()` on it is `None` and its `Display` re-states
/// the path. The real `io::Error` sits one link down `source()`:
///
/// ```text
/// io display : IO error for operation on /…/locked: Permission denied (os error 13)
/// io raw     : None
/// source[1]  : Permission denied (os error 13)   <- is io::Error, raw = Some(13)
/// ```
///
/// So: take the raw code if this error has one, else walk `source()` for the
/// first link that is an `io::Error` with one. Returns `None` for a genuinely
/// non-OS error, whose own display is then the best available.
fn bare_os_cause(io: &io::Error) -> Option<io::Error> {
    use std::error::Error as _;

    if let Some(code) = io.raw_os_error() {
        return Some(io::Error::from_raw_os_error(code));
    }
    let mut source = io.source();
    while let Some(err) = source {
        if let Some(code) = err
            .downcast_ref::<io::Error>()
            .and_then(io::Error::raw_os_error)
        {
            return Some(io::Error::from_raw_os_error(code));
        }
        source = err.source();
    }
    None
}

/// Turns a walk error into the skip that records it.
///
/// The reason must not repeat the path already held in `path` — see
/// [`bare_os_cause`] for why that takes two unwraps rather than one. A skip
/// reads `could not enumerate: Permission denied (os error 13)`, with the
/// directory said once, in `path`. A non-OS `io::Error` and a non-I/O error
/// (a bad glob in a `.ponsignore`, a symlink loop) keep their own display.
fn skip_for(err: &ignore::Error) -> SkippedFile {
    let path =
        error_path(err).map_or_else(|| "<unknown path>".to_string(), |p| p.display().to_string());
    let reason = match err.io_error() {
        Some(io) => match bare_os_cause(io) {
            Some(bare) => format!("could not enumerate: {bare}"),
            None => format!("could not enumerate: {io}"),
        },
        None => format!("could not enumerate: {err}"),
    };
    SkippedFile { path, reason }
}

/// Recursively discover source files under `root`, honouring `.gitignore`
/// and a custom `.ponsignore` file, and skipping [`SKIP_DIRS`].
///
/// Returns paths and languages only — see [`read`] for the contents.
///
/// **A walk error below the root is a skip, not a failure** (#29). One
/// unreadable subdirectory used to abort the entire scan, discarding every
/// finding from every tree that enumerated perfectly well; a scanner that
/// refuses to report what it *did* see because of one directory it did not is
/// not usable on a real checkout. The unreadable part is named in
/// [`Discovery::skipped`], so the partial enumeration is visible rather than
/// silent.
///
/// **The root itself is still fatal**, and is checked here rather than left to
/// the walker. ADR-0005 requires a nonexistent root to exit 2, but the walker
/// reports that as an ordinary error entry — indistinguishable from an
/// unreadable subdirectory once walk errors become skips. Inferring it from
/// the walk would make `pons scan /nonexistent` exit 0 having "scanned"
/// nothing, which is the worst possible answer: a clean bill of health for a
/// scan that never happened.
pub fn discover(root: &Path) -> anyhow::Result<Discovery> {
    use anyhow::Context as _;

    // `metadata` settles existence and permission on the path itself. It does
    // *not* prove a directory can be opened: a mode-000 directory stats fine
    // and fails on read. `read_dir` is therefore the real precondition — but
    // only for a directory, because `pons scan foo.py` is a valid invocation
    // and `read_dir` on a file would reject it.
    let meta =
        std::fs::metadata(root).with_context(|| format!("cannot scan {}", root.display()))?;
    if meta.is_dir() {
        std::fs::read_dir(root).with_context(|| format!("cannot scan {}", root.display()))?;
    }

    let mut files = Vec::new();
    let mut skipped = Vec::new();

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
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                skipped.push(skip_for(&e));
                continue;
            }
        };
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

    Ok(Discovery { files, skipped })
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
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn skips_configured_directories() {
        let dir = tempfile_dir();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/ignored.py"), "x = 1").unwrap();
        std::fs::write(dir.join("kept.py"), "x = 1").unwrap();

        let found = discover(&dir).unwrap().files;
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("kept.py"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_extensions_are_skipped() {
        let dir = tempfile_dir();
        std::fs::write(dir.join("notes.txt"), "hello").unwrap();

        let found = discover(&dir).unwrap().files;
        assert!(found.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reading_a_file_that_vanished_after_discovery_is_an_error_not_a_panic() {
        let dir = tempfile_dir();
        std::fs::write(dir.join("gone.py"), "x = 1").unwrap();

        let found = discover(&dir).unwrap().files;
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

        let found = discover(&dir).unwrap().files;
        let src = read(&found[0]).unwrap();
        assert!(src.text.ends_with('\u{00FF}'));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A directory whose mode is set so it cannot be opened, plus a guard.
    ///
    /// Root bypasses DAC permission checks entirely, so under `sudo` — or in a
    /// container that runs as uid 0 — the mode change has no effect and the
    /// walk succeeds. The tests below would then assert nothing while printing
    /// a green tick, which is the vacuous pass this codebase treats as worse
    /// than a red one. The guard makes that situation *loud*: CI here is
    /// `ubuntu-latest` with no container, i.e. non-root, so a failure means
    /// something about the environment changed and the coverage went away.
    fn unreadable_dir(parent: &Path, name: &str) -> PathBuf {
        let dir = parent.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hidden.py"), "x = 1\n").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            std::fs::read_dir(&dir).is_err(),
            "fixture is readable despite mode 000 — running as root? \
             this test would pass without exercising anything"
        );
        dir
    }

    /// Undo [`unreadable_dir`] so the temp tree can actually be removed.
    fn make_readable(dir: &Path) {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
    }

    #[test]
    fn an_unreadable_subdirectory_is_skipped_and_the_rest_of_the_tree_still_enumerates() {
        let dir = tempfile_dir();
        std::fs::write(dir.join("kept.py"), "x = 1\n").unwrap();
        let locked = unreadable_dir(&dir, "locked");

        // The whole point of #29: this used to return Err and throw away
        // `kept.py` along with every other finding in the tree.
        let found = discover(&dir).unwrap();

        assert_eq!(found.files.len(), 1, "the readable file was not enumerated");
        assert!(found.files[0].path.ends_with("kept.py"));
        assert_eq!(
            found.skipped.len(),
            1,
            "the unreadable directory was not recorded"
        );
        assert!(
            found.skipped[0].path.ends_with("locked"),
            "the skip did not name the directory: {:?}",
            found.skipped[0]
        );
        assert!(
            found.skipped[0].reason.starts_with("could not enumerate"),
            "the skip did not say why: {:?}",
            found.skipped[0]
        );

        // The reason must not say the directory again — `path` already holds
        // it. walkdir's `io::Error` payload re-states the path in its own
        // Display, so without the `raw_os_error` re-derivation in `skip_for`
        // this reads "could not enumerate: IO error for operation on
        // /…/locked: Permission denied". Assert the bare cause.
        assert!(
            !found.skipped[0].reason.contains("locked"),
            "the reason repeats the path already in `path`: {:?}",
            found.skipped[0]
        );
        assert!(
            found.skipped[0].reason.contains("Permission denied"),
            "the skip lost the actual cause: {:?}",
            found.skipped[0]
        );

        make_readable(&locked);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_nonexistent_root_is_an_error_not_an_empty_scan() {
        // ADR-0005: an operational error about the scan itself exits 2. Once
        // walk errors became skips this stopped being something the walker
        // could tell us, so it is checked before the walk — and this is the
        // control that proves the check is there.
        let missing = std::env::temp_dir().join(format!("pons-absent-{}", uuid_ish()));
        assert!(
            discover(&missing).is_err(),
            "a nonexistent root reported a clean scan"
        );
    }

    #[test]
    fn an_unreadable_root_is_an_error_even_though_an_unreadable_subdirectory_is_not() {
        let dir = tempfile_dir();
        let locked = unreadable_dir(&dir, "root");

        // `metadata` succeeds on a mode-000 directory — it stats the inode,
        // it does not open it — so the precondition has to be `read_dir`.
        assert!(
            std::fs::metadata(&locked).is_ok(),
            "premise of this test is wrong"
        );
        assert!(
            discover(&locked).is_err(),
            "an unopenable ROOT was treated as an empty scan rather than an error"
        );

        make_readable(&locked);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_single_file_root_is_scannable() {
        // `pons scan foo.py` is valid, so the root precondition must not
        // `read_dir` a file. Without the `is_dir` guard this returns Err.
        let dir = tempfile_dir();
        let file = dir.join("one.py");
        std::fs::write(&file, "x = 1\n").unwrap();

        let found = discover(&file).unwrap();
        assert_eq!(found.files.len(), 1);
        assert!(found.skipped.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_skip_names_its_path_through_the_wrappers_a_walk_actually_produces() {
        // A walk error is not a bare `Io`: `ignore` wraps it in `WithPath` and
        // then in `WithDepth`, and an ignore-file failure adds
        // `WithLineNumber` on top. Matching only the outermost variant would
        // leave every real skip named "<unknown path>" while a unit test on a
        // bare `WithPath` passed. These are constructed by hand precisely
        // because the nesting is the thing under test.
        let inner = ignore::Error::WithPath {
            path: PathBuf::from("/tmp/locked"),
            err: Box::new(ignore::Error::Io(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied,
            ))),
        };
        let nested = ignore::Error::WithDepth {
            depth: 3,
            err: Box::new(inner),
        };
        let skip = skip_for(&nested);
        assert_eq!(skip.path, "/tmp/locked");
        assert!(
            skip.reason.starts_with("could not enumerate"),
            "reason was {:?}",
            skip.reason
        );
        // The reason is the inner io error, not the wrapper's Display, which
        // would repeat the path already held in `path`.
        assert!(
            !skip.reason.contains("/tmp/locked"),
            "the path is said twice: {:?}",
            skip.reason
        );

        let deeper = ignore::Error::WithLineNumber {
            line: 7,
            err: Box::new(ignore::Error::Partial(vec![ignore::Error::WithDepth {
                depth: 1,
                err: Box::new(ignore::Error::WithPath {
                    path: PathBuf::from("/tmp/deep"),
                    err: Box::new(ignore::Error::InvalidDefinition),
                }),
            }])),
        };
        assert_eq!(skip_for(&deeper).path, "/tmp/deep");

        // No path anywhere: named rather than silently attributed to a file.
        assert_eq!(
            skip_for(&ignore::Error::InvalidDefinition).path,
            "<unknown path>"
        );
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
