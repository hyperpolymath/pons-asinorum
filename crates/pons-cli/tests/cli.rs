// SPDX-License-Identifier: MPL-2.0
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn pons(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pons"))
        .args(args)
        .output()
        .unwrap()
}
fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn scan(root: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["scan", root.to_str().unwrap(), "--format", "json"];
    args.extend(extra);
    pons(&args)
}
fn report(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}; stderr={}", String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn help_and_bad_arity_are_real() {
    for command in [
        "scan",
        "catalogue",
        "man",
        "doctor",
        "extract",
        "spell",
        "search",
        "trace",
        "challenge",
    ] {
        let out = pons(&[command, "--help"]);
        assert!(out.status.success(), "{command}");
    }
    assert_eq!(pons(&["scan"]).status.code(), Some(2));
    assert_eq!(
        pons(&["scan", ".", "--does-nothing"]).status.code(),
        Some(2)
    );
    assert_eq!(
        pons(&["extract", "x", "--via", "imaginary"]).status.code(),
        Some(2)
    );
    assert!(pons(&["--version"]).status.success());
    assert!(String::from_utf8_lossy(&pons(&["man"]).stdout).contains("pons"));
}
#[test]
fn findings_are_advisory_unless_a_threshold_is_selected() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "app.py", "value = value\n");
    let out = scan(t.path(), &[]);
    assert!(out.status.success());
    assert_eq!(report(&out)["counts"]["total"], 1);
    assert_eq!(
        scan(t.path(), &["--fail-on", "warn"]).status.code(),
        Some(1)
    );
    assert_eq!(scan(t.path(), &["--rule", "typo"]).status.code(), Some(2));
}
#[test]
fn parse_and_configuration_errors_cannot_pass_as_clean_scans() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "app.py", "def broken(\n");
    let out = scan(t.path(), &[]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(report(&out)["scanned"]["complete"], false);
    put(t.path(), "pons.toml", "[supress]\nrules=[]\n");
    assert_eq!(scan(t.path(), &[]).status.code(), Some(2));
}
#[test]
fn strings_cannot_suppress_but_comments_can() {
    let t = tempfile::tempdir().unwrap();
    put(
        t.path(),
        "app.py",
        "value = value # pons:allow self-assignment -- deliberate\n",
    );
    let out = scan(t.path(), &[]);
    assert_eq!(report(&out)["counts"]["total"], 0);
    assert_eq!(report(&out)["scanned"]["suppressed_findings"], 1);
    put(
        t.path(),
        "app.py",
        "text = 'pons:allow self-assignment'; value = value\n",
    );
    assert_eq!(report(&scan(t.path(), &[]))["counts"]["total"], 1);
}
#[test]
fn scoped_overrides_require_valid_ids_and_reasons() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "one.py", "x = x\n");
    put(t.path(), "two.py", "x = x\n");
    put(
        t.path(),
        "pons.toml",
        "[[suppress.allow]]\nrule='self-assignment'\npath='one.py'\nreason='binding demonstration'\n",
    );
    let out = scan(t.path(), &[]);
    assert!(out.status.success());
    assert_eq!(report(&out)["counts"]["total"], 1);
    put(
        t.path(),
        "pons.toml",
        "[suppress]\nrules=['self-assigment']\n",
    );
    assert_eq!(scan(t.path(), &[]).status.code(), Some(2));
}
#[test]
fn ignore_files_hidden_files_and_spaces_work() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), ".ponsignore", "ignore me.py\n");
    put(t.path(), "ignore me.py", "x=x\n");
    put(t.path(), ".hidden.py", "x=x\n");
    let r = report(&scan(t.path(), &[]));
    assert_eq!(r["counts"]["total"], 1);
    assert_eq!(r["findings"][0]["location"]["file"], ".hidden.py");
}
#[test]
fn binary_and_oversized_source_are_reported_as_incomplete() {
    let t = tempfile::tempdir().unwrap();
    fs::write(t.path().join("app.py"), [0xff, 0xfe]).unwrap();
    assert_eq!(scan(t.path(), &[]).status.code(), Some(2));
    fs::write(t.path().join("app.py"), vec![b' '; 2 * 1024 * 1024 + 1]).unwrap();
    assert_eq!(scan(t.path(), &[]).status.code(), Some(2));
}
#[test]
fn sarif_has_valid_locations_and_speculation_is_demoted() {
    let t = tempfile::tempdir().unwrap();
    put(
        t.path(),
        "README with spaces.adoc",
        "= Benchmark\n\nIt is 8933 times faster.\n",
    );
    let out = pons(&["scan", t.path().to_str().unwrap(), "--format", "sarif"]);
    assert!(out.status.success());
    let r = report(&out);
    let finding = &r["runs"][0]["results"][0];
    assert_eq!(finding["level"], "note");
    assert_eq!(finding["properties"]["evidenceClass"], "SPECULATIVE");
    assert_eq!(finding["rank"], 20);
    assert!(
        finding["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .unwrap()
            .contains("%20")
    );
    assert!(
        finding["message"]["text"]
            .as_str()
            .unwrap()
            .contains("not a verdict")
    );
    assert_eq!(
        report(&scan(t.path(), &["--no-speculative"]))["counts"]["total"],
        0
    );
}
#[test]
fn custom_patterns_run_and_unknown_fields_fail() {
    let t = tempfile::tempdir().unwrap();
    put(
        t.path(),
        "README.adoc",
        "Juice is always a tropical type.\n",
    );
    put(
        t.path(),
        "pons.toml",
        "[[pattern]]\nid='user-juice-type'\npath='*.adoc'\nregex='Juice is always a tropical type'\nmessage='Check the intended type meaning'\ncounter_condition='A stated project convention may define this'\n",
    );
    let out = scan(t.path(), &[]);
    assert!(out.status.success());
    assert_eq!(report(&out)["findings"][0]["rule_id"], "user-juice-type");
}
#[test]
fn resource_budget_reports_relational_arithmetic() {
    let t = tempfile::tempdir().unwrap();
    put(
        t.path(),
        "breakfast.py",
        "for _ in range(2245):\n    eat(egg)\n",
    );
    put(
        t.path(),
        "pons.toml",
        "[[budget]]\npath='breakfast.py'\nresource='eggs'\navailable=4\nconsume='eat'\nunits=1\n",
    );
    let out = scan(t.path(), &[]);
    assert!(out.status.success());
    let r = report(&out);
    assert_eq!(r["findings"][0]["evidence"], "PROTOCOL");
    assert!(
        r["findings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("2241 short")
    );
    put(
        t.path(),
        "breakfast.py",
        "for _ in range(2245):\n    if enough():\n        eat(egg)\n",
    );
    assert_eq!(report(&scan(t.path(), &[]))["counts"]["total"], 0);
}
#[test]
fn catalogue_is_generated_from_the_registry() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(
        fs::read_to_string(root.join("docs/catalogue.adoc")).unwrap(),
        String::from_utf8(pons(&["catalogue"]).stdout).unwrap()
    );
}

#[test]
fn local_document_paths_preserve_spaces_and_decode_url_escapes() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "images/me and molly.svg", "<svg/>\n");
    put(
        t.path(),
        "README.md",
        "# Example\n![Figure 1](<images/me and molly.svg>)\n![Figure 2](images/me%20and%20molly.svg)\n",
    );
    assert_eq!(
        report(&scan(t.path(), &["--rule", "document-reference-missing"]))["counts"]["total"],
        0
    );
    put(
        t.path(),
        "README.md",
        "# Example\n![Figure 1](<images/not here.svg>)\n",
    );
    assert_eq!(
        report(&scan(t.path(), &["--rule", "document-reference-missing"]))["counts"]["total"],
        1
    );
}

#[test]
fn unsupported_explicit_input_and_forced_tree_grammar_do_not_fake_success() {
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "photo.png", "not image data");
    assert_eq!(
        pons(&["scan", t.path().join("photo.png").to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(scan(t.path(), &["--lang", "python"]).status.code(), Some(2));
    assert_eq!(report(&scan(t.path(), &[]))["scanned"]["skipped_files"], 1);
}

#[test]
fn custom_source_patterns_obey_real_inline_comments() {
    let t = tempfile::tempdir().unwrap();
    put(
        t.path(),
        "pons.toml",
        "[[pattern]]\nid='user-constant'\npath='*.py'\nregex='pi = 3'\nmessage='Check the value'\ncounter_condition='Integer approximation may be intentional'\n",
    );
    put(
        t.path(),
        "app.py",
        "pi = 3 # pons:allow user-constant -- approximation example\n",
    );
    assert_eq!(report(&scan(t.path(), &[]))["counts"]["total"], 0);
    put(t.path(), "app.py", "pi = 3\n");
    assert_eq!(report(&scan(t.path(), &[]))["counts"]["total"], 1);
}
