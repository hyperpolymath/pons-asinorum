// SPDX-License-Identifier: MPL-2.0
use std::process::Command;
fn main() {
    // Ask Git for metadata paths: .git may be a worktree pointer file.
    for name in ["HEAD", "refs/heads", "index", "packed-refs"] {
        if let Ok(output) = Command::new("git")
            .args(["rev-parse", "--git-path", name])
            .output()
        {
            if output.status.success() {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    println!("cargo:rerun-if-changed={}", path.trim());
                }
            }
        }
    }
    println!("cargo:rerun-if-changed=../../crates");
    println!("cargo:rerun-if-changed=../../schemas");
    println!("cargo:rerun-if-changed=../../protocols");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
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
