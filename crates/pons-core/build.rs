// SPDX-License-Identifier: MPL-2.0
use std::process::Command;
fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=../../crates");
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();
    let sha = output
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| s.len() == 40 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=PONS_SOURCE_COMMIT={sha}");
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty());
    println!(
        "cargo:rustc-env=PONS_SOURCE_DIRTY={}",
        dirty
            .map(|d| d.to_string())
            .unwrap_or_else(|| "unknown".into())
    );
}
