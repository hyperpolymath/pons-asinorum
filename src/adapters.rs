// SPDX-License-Identifier: MPL-2.0
//! Explicit, bounded subprocess adapters. No shell and no auto-executed plug-ins.
use crate::{Extractor, Speller};
use anyhow::{Context, Result, bail};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
fn executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|p| p.join(name))
            .find(|p| p.is_file())
    })
}
pub fn doctor() -> String {
    let mut out =
        "Optional tools (presence only; availability is not an integration test):\n".to_string();
    for name in [
        "timeout",
        "agrep",
        "pandoc",
        "aspell",
        "hunspell",
        "tesseract",
        "podman",
        "firewall-cmd",
        "getenforce",
        "invariant-path",
        "invariant-path-cli",
    ] {
        out.push_str(&format!(
            "{name}: {}\n",
            executable(name).map_or_else(|| "not installed".into(), |p| p.display().to_string())
        ));
    }
    out.push_str("Core scans need none of these tools. Adapters require GNU timeout and run only on explicit commands.\n");
    out
}
fn run(
    tool: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    allow_no_match: bool,
) -> Result<String> {
    let binary = executable(tool)
        .with_context(|| format!("optional tool {tool} is not installed; no substitute was run"))?;
    let timeout = executable("timeout")
        .context("external adapters require GNU timeout to bound subprocess execution")?;
    let mut child = Command::new(timeout)
        .args(["--kill-after=1s", "15s"])
        .arg(binary)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {tool}"))?;
    let stdout = child.stdout.take().context("stdout pipe")?;
    let stderr = child.stderr.take().context("stderr pipe")?;
    let reader = |pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let out = reader(Box::new(stdout));
    let err = reader(Box::new(stderr));
    let writer = input.map(|data| {
        let mut stdin = child.stdin.take().expect("piped stdin");
        thread::spawn(move || stdin.write_all(&data))
    });
    let status = child.wait()?;
    let out = out
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
    let err = err
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
    let write_result = writer
        .map(|w| {
            w.join()
                .map_err(|_| anyhow::anyhow!("stdin writer panicked"))
        })
        .transpose()?;
    if out.len() as u64 > OUTPUT_LIMIT || err.len() as u64 > OUTPUT_LIMIT {
        bail!("{tool} exceeded the output limit");
    }
    if !status.success() && !(allow_no_match && status.code() == Some(1)) {
        bail!(
            "{tool} failed ({status}): {}",
            String::from_utf8_lossy(&err)
        );
    }
    if let Some(result) = write_result {
        result?;
    }
    if !err.is_empty() {
        eprint!("{tool}: {}", String::from_utf8_lossy(&err));
    }
    String::from_utf8(out).with_context(|| format!("{tool} output is not UTF-8"))
}
pub fn extract(path: &Path, via: Extractor, language: &str) -> Result<String> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        bail!("extraction input must be a regular file");
    }
    if path.metadata()?.len() > 32 * 1024 * 1024 {
        bail!("extraction input exceeds 32 MiB");
    }
    match via {
        Extractor::Pandoc => {
            let format = match path.extension().and_then(|e| e.to_str()) {
                Some("md") => "markdown",
                Some("html" | "htm") => "html",
                Some("docx") => "docx",
                Some("odt") => "odt",
                Some("rst") => "rst",
                _ => bail!(
                    "Pandoc adapter accepts .md, .html, .docx, .odt and .rst; choose OCR for supported images"
                ),
            };
            run(
                "pandoc",
                &[
                    "--sandbox".into(),
                    "--from".into(),
                    format.into(),
                    "--to".into(),
                    "plain".into(),
                    path.display().to_string(),
                ],
                None,
                false,
            )
        }
        Extractor::Ocr => {
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp")
            ) {
                bail!("OCR adapter accepts images, not PDF; render pages explicitly first");
            }
            if !language
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
                || language.starts_with('-')
            {
                bail!("invalid OCR language name");
            }
            run(
                "tesseract",
                &[
                    path.display().to_string(),
                    "stdout".into(),
                    "-l".into(),
                    language.into(),
                ],
                None,
                false,
            )
        }
    }
}
pub fn spell(path: &Path, via: Speller, dictionary: &str) -> Result<String> {
    if dictionary.is_empty()
        || !dictionary
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        || dictionary.starts_with('-')
    {
        bail!("dictionary must be a language identifier");
    }
    let input = crate::read_text(path)?.into_bytes();
    match via {
        Speller::Aspell => run(
            "aspell",
            &[
                "list".into(),
                format!("--lang={dictionary}"),
                "--encoding=utf-8".into(),
            ],
            Some(input),
            false,
        ),
        Speller::Hunspell => run(
            "hunspell",
            &[
                "-l".into(),
                "-i".into(),
                "UTF-8".into(),
                "-d".into(),
                dictionary.into(),
            ],
            Some(input),
            false,
        ),
    }
}
pub fn search(pattern: &str, path: &Path, errors: u8) -> Result<String> {
    if pattern.is_empty() || pattern.len() > 256 {
        bail!("agrep pattern must contain 1 to 256 bytes");
    }
    let path = path.canonicalize()?;
    // Validate the file and input bound before handing it to the tool.
    crate::read_text(&path)?;
    run(
        "agrep",
        &[
            format!("-{errors}"),
            "-n".into(),
            "-e".into(),
            pattern.into(),
            path.display().to_string(),
        ],
        None,
        true,
    )
}

pub fn challenge(path: &Path) -> Result<serde_json::Value> {
    let path = path.canonicalize()?;
    crate::read_text(&path)?;
    // The inspected Cargo package emits invariant-path-cli; packaged installs
    // may expose the documented invariant-path command instead.
    let tool = if executable("invariant-path").is_some() {
        "invariant-path"
    } else {
        "invariant-path-cli"
    };
    let output = run(
        tool,
        &[
            "scan".into(),
            "--file".into(),
            path.display().to_string(),
            "--json".into(),
        ],
        None,
        false,
    )?;
    let value: serde_json::Value =
        serde_json::from_str(&output).context("Invariant Path did not return JSON")?;
    if !value.is_array() {
        bail!("Invariant Path scan output must be an annotation array");
    }
    Ok(value)
}
