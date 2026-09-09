// SPDX-License-Identifier: MPL-2.0
use crate::{Lang, Location};
use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub struct Source {
    pub path: PathBuf,
    pub relative: String,
    pub text: String,
    pub lang: Option<Lang>,
}
impl Source {
    pub fn read(path: &Path, root: &Path) -> Result<Self> {
        let metadata = path
            .symlink_metadata()
            .with_context(|| format!("stat {}", path.display()))?;
        if !metadata.is_file() {
            bail!("not a regular file: {}", path.display());
        }
        if metadata.len() > MAX_FILE_BYTES {
            bail!(
                "{} exceeds the {} byte input limit",
                path.display(),
                MAX_FILE_BYTES
            );
        }
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            bail!("file grew beyond input limit: {}", path.display());
        }
        // Byte offsets must identify the original file: never silently transcode.
        let text = String::from_utf8(bytes).with_context(|| {
            format!(
                "{} is not UTF-8; convert explicitly before scanning",
                path.display()
            )
        })?;
        if text.contains('\0') {
            bail!("binary/NUL-containing input: {}", path.display());
        }
        Ok(Self {
            path: path.to_path_buf(),
            relative: path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/"),
            text,
            lang: path
                .extension()
                .and_then(|x| x.to_str())
                .and_then(Lang::from_extension),
        })
    }
    pub fn location(&self, start: usize, end: usize) -> Location {
        let point = |offset: usize| {
            let prefix = &self.text[..offset];
            let line = prefix.bytes().filter(|x| *x == b'\n').count() + 1;
            let col = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            (line, col)
        };
        let (line_start, col_start) = point(start);
        let (line_end, col_end) = point(end);
        Location {
            file: self.relative.clone(),
            byte_start: start,
            byte_end: end,
            line_start,
            col_start,
            line_end,
            col_end,
        }
    }
}

/// Enumerate files, honouring ignore files even outside a Git checkout.
pub fn discover(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        bail!("scan path does not exist: {}", root.display());
    }
    if root.symlink_metadata()?.file_type().is_symlink() {
        bail!("scan root must not be a symlink");
    }
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .follow_links(false)
        .require_git(false)
        .add_custom_ignore_filename(".ponsignore");
    builder.filter_entry(|e| {
        !matches!(
            e.file_name().to_str(),
            Some(
                ".git"
                    | "target"
                    | "node_modules"
                    | "external_corpora"
                    | "third_party"
                    | "corpus"
                    | ".venv"
                    | "_build"
                    | ".zig-cache"
            )
        )
    });
    let mut paths = Vec::new();
    for item in builder.build() {
        let entry = item.context("enumerating scan input")?;
        if entry.file_type().is_some_and(|t| t.is_file()) {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    Ok(paths)
}
