// SPDX-License-Identifier: MPL-2.0
//! Tests report transaction behaviour using explicit fake deployment commands.
//! Does not claim firewalld or SELinux enforcement on the test host.
#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn executable(dir: &Path, name: &str, text: &str) {
    let p = dir.join(name);
    fs::write(&p, text).unwrap();
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
fn only_complete_valid_reports_replace_the_previous_report() {
    let t = tempfile::tempdir().unwrap();
    let commands = t.path().join("commands");
    let input = t.path().join("input");
    fs::create_dir(&commands).unwrap();
    fs::create_dir(&input).unwrap();
    executable(&commands, "firewall-cmd", "#!/bin/sh\nexit 0\n");
    executable(
        &commands,
        "getenforce",
        "#!/bin/sh\nprintf '%s\\n' \"${PONS_TEST_SELINUX:-Enforcing}\"\n",
    );
    executable(
        &commands,
        "podman",
        r#"#!/bin/sh
printf '%s\n' "$@" > "$PONS_TEST_ARGS"
case "$PONS_TEST_MODE" in
  failed) echo 'scanner failed' >&2; exit 2 ;;
  malformed) echo 'not json' ;;
  incomplete) echo '{"tool":{"name":"pons"},"scanned":{"complete":false},"findings":[]}' ;;
  *) echo '{"tool":{"name":"pons"},"scanned":{"complete":true},"findings":[]}' ;;
esac
"#,
    );
    let report = t.path().join("report.json");
    let args = t.path().join("args.txt");
    let wrapper =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/container/transactional-scan.sh");
    let mut path = commands.into_os_string();
    path.push(":/usr/bin:/bin");
    for (mode, selinux, success) in [
        ("failed", "Enforcing", false),
        ("malformed", "Enforcing", false),
        ("incomplete", "Enforcing", false),
        ("complete", "Permissive", false),
        ("complete", "Enforcing", true),
    ] {
        fs::write(&report, "previous report").unwrap();
        let out = Command::new("bash")
            .arg(&wrapper)
            .arg(&input)
            .arg(&report)
            .env("PATH", &path)
            .env("PONS_TEST_MODE", mode)
            .env("PONS_TEST_SELINUX", selinux)
            .env("PONS_TEST_ARGS", &args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = fs::read_to_string(&report).unwrap();
        if success {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap()["scanned"]["complete"],
                true
            );
        } else {
            assert_eq!(text, "previous report");
        }
        assert!(!fs::read_dir(t.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("report.json.")
        }));
    }
    let args = fs::read_to_string(args).unwrap();
    for flag in [
        "--network=none",
        "--read-only",
        "--cap-drop=all",
        "--security-opt=no-new-privileges",
    ] {
        assert!(args.lines().any(|a| a == flag));
    }
}
