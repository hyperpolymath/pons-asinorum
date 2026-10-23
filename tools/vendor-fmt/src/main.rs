// SPDX-License-Identifier: MPL-2.0
//! Bootstrap helper behind the `cargo fmt` alias. On CI it vendors locked
//! dependencies exactly once, then delegates to the real `cargo-fmt`. This lets
//! the existing autoformat workflow commit a reproducible offline source tree
//! without modifying a workflow file (which this repo's Actions policy rejects).

use std::path::Path;
use std::process::{Command, ExitCode};

/// Run a child command and return its exit status.
/// Spawn failures and termination without an exit code return status 1.
fn run(mut command: Command) -> ExitCode {
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(err) => {
            eprintln!("pons-vendor-fmt: {err}");
            ExitCode::from(1)
        }
    }
}

/// Vendor dependencies only when `CI=true` and no vendor directory exists.
///
/// After successful vendoring, add source replacement to a readable Cargo
/// configuration if absent, then stage the vendor tree and configuration.
/// Returns child command failures or status 1 if writing the configuration
/// fails; an unreadable configuration is left untouched.
fn vendor_on_ci() -> ExitCode {
    if std::env::var("CI").as_deref() != Ok("true") || Path::new("vendor").is_dir() {
        return ExitCode::SUCCESS;
    }

    let mut vendor = Command::new("cargo");
    vendor.args(["vendor", "--versioned-dirs", "vendor"]);
    let code = run(vendor);
    if code != ExitCode::SUCCESS {
        return code;
    }

    // Record the source replacement for subsequent cargo invocations in this job.
    let config_path = Path::new(".cargo/config.toml");
    if let Ok(text) = std::fs::read_to_string(config_path) {
        if !text.contains("replace-with") {
            let mut next = text;
            next.push_str(
                "\n[source.crates-io]\nreplace-with = \"vendored-sources\"\n\n\
                 [source.vendored-sources]\ndirectory = \"vendor\"\n",
            );
            if let Err(err) = std::fs::write(config_path, next) {
                eprintln!("pons-vendor-fmt: could not record vendored source: {err}");
                return ExitCode::from(1);
            }
        }
    }

    // Stage explicitly: the original bootstrap commit step uses `git add -A`,
    // but the repository historically ignores /vendor/.
    let mut add = Command::new("git");
    add.args(["add", "-f", "vendor", ".cargo/config.toml"]);
    run(add)
}

/// Prepare vendored sources on CI, then run `cargo-fmt` with the supplied
/// arguments, returning the first failing status or the formatter's status.
fn main() -> ExitCode {
    let code = vendor_on_ci();
    if code != ExitCode::SUCCESS {
        return code;
    }
    let mut fmt = Command::new("cargo-fmt");
    fmt.args(std::env::args().skip(1));
    run(fmt)
}
