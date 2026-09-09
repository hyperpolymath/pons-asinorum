// SPDX-License-Identifier: MPL-2.0
//! Cross-file contradictions beyond a compiler's remit. Never executes project code.
use anyhow::{Result, bail};
use pons_core::{Finding, RuleMeta, Source, config::I18n};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn metadata() -> Vec<RuleMeta> {
    [
        ("conventional-name-typo", "A conventional repository filename appears misspelt or mis-cased", "A deliberately different file is valid; allow this path with a reason."),
        ("template-placeholder", "An entry-point document contains an unfilled template token", "A template repository or an intentional template example should allow this rule for the relevant document."),
        ("readme-language-contradiction", "One README makes conflicting primary implementation-language claims", "Separate components may use different languages; explain their roles or allow the document."),
        ("manifest-license-contradiction", "A source SPDX identifier disagrees with its Cargo package licence", "Separately licensed files may be intentional. This checks single identifiers, not legal compatibility or compound expressions."),
        ("just-toolchain-missing", "A Justfile is present but the mise tool manifest does not provide just", "A documented system package or external development environment may provide just instead."),
        ("shell-quoted-expansion", "A shell assignment single-quotes a variable expansion", "Literal dollar syntax for another interpreter can be intentional; allow it explicitly."),
        ("localisation-key-drift", "A translation has missing or unexpected message keys", "A configured fallback or an intentional partial translation should be documented and allowed."),
        ("localisation-placeholder-drift", "A translation changes the set of named interpolation placeholders", "Different interpolation syntax requires an adapter; repeated occurrences and their ordering are deliberately ignored."),
        ("implausible-speedup-claim", "A very large performance multiplier needs its benchmark and baseline checked", "An algorithmic change or pathological baseline can produce a real large speedup. This is a review question, not a falsehood verdict."),
        ("ambiguous-speedup-claim", "A sub-unit multiplier is described as faster", "0.3 times the original throughput is slower; a 0.3-times increase means 30% faster. State which quantity and baseline you mean."),
        ("license-identifier-typo", "A likely licence-name transposition needs checking", "This suggests a known spelling correction; it does not determine legal compatibility or prohibit a custom licence."),
    ].into_iter().map(|(id,m,c)| {
        let mut meta=crate::meta(id,m,c,vec!["repository".into()]);
        if id=="implausible-speedup-claim" {meta.evidence=pons_core::EvidenceClass::Speculative;meta.severity=pons_core::Severity::Info;}
        if id=="ambiguous-speedup-claim" {meta.severity=pons_core::Severity::Info;}
        meta
    }).collect()
}

pub fn check(sources: &[Source], i18n: Option<&I18n>) -> Result<Vec<Finding>> {
    let metas = metadata();
    let mut out = Vec::new();
    let by_path: BTreeMap<&str, &Source> =
        sources.iter().map(|s| (s.relative.as_str(), s)).collect();
    let placeholder = Regex::new(
        r"\{\{(?:PROJECT(?:_NAME)?|AUTHOR|OWNER|REPO(?:SITORY)?|DESCRIPTION|YEAR)\}\}|<(?:YOUR[-_ ](?:NAME|PROJECT|REPO|EMAIL))>",
    )?;
    let language_claim = Regex::new(
        r"(?i)(?:written|implemented) in (rust|python|rescript|javascript|typescript|zig|idris2|elixir|haskell)\b",
    )?;
    let quoted_expansion = Regex::new(
        r"(?m)^\s*(?:export\s+)?[A-Za-z_][A-Za-z0-9_]*='[^'\n]*\$(?:\{[A-Za-z_][A-Za-z0-9_]*\}|[A-Za-z_][A-Za-z0-9_]*)[^'\n]*'\s*(?:#.*)?$",
    )?;
    let speedup =
        Regex::new(r"(?i)\b([0-9]+(?:\.[0-9]+)?)\s*(?:x|×|times)\s+(?:faster|speedup|as fast)\b")?;
    let misspelt_license = Regex::new(r"(?i)\bMLP(?:-2\.0|\s+licen[cs]e)\b")?;
    for source in sources {
        let basename = source
            .path
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or("");
        if source.path.parent().is_some()
            && (source.relative.split('/').count() == 1 || source.relative.starts_with(".github/"))
        {
            for expected in ["CODEOWNERS", "FUNDING.yml", "SECURITY.md", "Justfile"] {
                let applies = match expected {
                    "FUNDING.yml" => source.relative.starts_with(".github/"),
                    "Justfile" => source.relative.split('/').count() == 1 && basename != "justfile",
                    _ => true,
                };
                if applies && basename != expected && close_name(basename, expected) {
                    out.push(crate::review(
                        &metas[0],
                        source.location(0, 0),
                        format!(
                            "Found `{}`; consumers discover `{expected}` by its conventional name.",
                            source.relative
                        ),
                    ));
                }
            }
        }
        let entry_doc = source.relative.split('/').count() == 1
            && (basename.starts_with("README.")
                || basename.starts_with("CONTRIBUTING.")
                || basename.starts_with("EXPLAINME."));
        if entry_doc {
            for m in placeholder.find_iter(&source.text) {
                out.push(crate::review(
                    &metas[1],
                    source.location(m.start(), m.end()),
                    format!(
                        "Unsubstituted token `{}` in a repository entry-point document.",
                        m.as_str()
                    ),
                ));
            }
        }
        if basename.starts_with("README.") {
            let claims: BTreeSet<_> = language_claim
                .captures_iter(&source.text)
                .map(|c| c[1].to_lowercase())
                .collect();
            if claims.len() > 1 {
                out.push(crate::review(
                    &metas[2],
                    source.location(0, 0),
                    format!(
                        "Explicit 'written/implemented in' claims name: {}.",
                        claims.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                ));
            }
        }
        if source
            .path
            .extension()
            .is_some_and(|e| matches!(e.to_str(), Some("adoc" | "asciidoc" | "md" | "txt" | "rst")))
        {
            for c in speedup.captures_iter(&source.text) {
                let value: f64 = c[1].parse()?;
                let matched = c.get(0).expect("whole regex match");
                if value >= 1000.0 {
                    let mut finding = crate::review(
                        &metas[8],
                        source.location(matched.start(), matched.end()),
                        format!(
                            "Claimed multiplier {value}; review threshold is 1000. Could a decimal shift (for example {}) be intended? No replacement is assumed correct.",
                            value / 10.0
                        ),
                    );
                    finding.severity = pons_core::Severity::Info;
                    out.push(finding);
                } else if value < 1.0 {
                    let mut finding = crate::review(
                        &metas[9],
                        source.location(matched.start(), matched.end()),
                        format!(
                            "A {value}× ratio is below the baseline; distinguish a ratio from an increase of {}%.",
                            value * 100.0
                        ),
                    );
                    finding.severity = pons_core::Severity::Info;
                    out.push(finding);
                }
            }
        }
        if entry_doc || basename.starts_with("LICENSE") || basename == "Cargo.toml" {
            for m in misspelt_license.find_iter(&source.text) {
                out.push(crate::review(&metas[10],source.location(m.start(),m.end()),"Did you mean MPL / MPL-2.0 (Mozilla Public License)? MLP is a likely transposition in this licence context."));
            }
        }
        if source.path.extension().is_some_and(|e| e == "sh") {
            for m in quoted_expansion.find_iter(&source.text) {
                out.push(crate::review(&metas[5],source.location(m.start(),m.end()),"Single quotes preserve the dollar expression literally in a shell assignment; shell expansion does not occur."));
            }
        }
    }
    if let Some(mise) = by_path
        .get("mise.toml")
        .or_else(|| by_path.get(".mise.toml"))
    {
        let value: toml::Value = toml::from_str(&mise.text)?;
        if let Some(justfile) = by_path.get("Justfile").or_else(|| by_path.get("justfile")) {
            let has_just = value
                .get("tools")
                .and_then(|v| v.as_table())
                .is_some_and(|t| t.keys().any(|k| k == "just" || k.ends_with(":just")));
            if !has_just {
                out.push(crate::review(
                    &metas[4],
                    justfile.location(0, 0),
                    "The repository's mise.toml [tools] has no just entry.",
                ));
            }
        }
    }
    // Resolve each Rust source against its nearest package manifest. A virtual
    // workspace's licence applies only through explicit package inheritance.
    let mut manifests = BTreeMap::new();
    for s in sources
        .iter()
        .filter(|s| s.path.file_name().is_some_and(|n| n == "Cargo.toml"))
    {
        manifests.insert(
            s.relative.trim_end_matches("Cargo.toml").to_string(),
            toml::from_str::<toml::Value>(&s.text)?,
        );
    }
    let root_license = manifests
        .get("")
        .and_then(|v| v.get("workspace"))
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("license"))
        .and_then(|v| v.as_str());
    for s in sources
        .iter()
        .filter(|s| s.lang == Some(pons_core::Lang::Rust))
    {
        let nearest = manifests
            .iter()
            .filter(|(p, _)| s.relative.starts_with(p.as_str()))
            .max_by_key(|(p, _)| p.len());
        let license = nearest
            .and_then(|(_, v)| v.get("package"))
            .and_then(|v| v.get("license"))
            .and_then(|v| {
                v.as_str().or_else(|| {
                    if v.get("workspace").and_then(|x| x.as_bool()) == Some(true) {
                        root_license
                    } else {
                        None
                    }
                })
            });
        let header = s
            .text
            .lines()
            .take(5)
            .find_map(|l| l.trim().strip_prefix("// SPDX-License-Identifier: "));
        if let Some((license, header)) = license.zip(header) {
            let simple = |s: &str| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '+'))
            };
            if simple(license) && simple(header) && license != header {
                out.push(crate::review(&metas[3],s.location(0,0),format!("Nearest Cargo package declares `{license}`; source header declares `{header}`.")));
            }
        }
    }
    if let Some(config) = i18n {
        check_i18n(&by_path, config, &metas, &mut out)?;
    }
    Ok(out)
}

fn close_name(a: &str, b: &str) -> bool {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    if a == b {
        return true;
    }
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.bytes().enumerate() {
        let mut next = vec![i + 1];
        for (j, y) in b.bytes().enumerate() {
            next.push(
                (prev[j + 1] + 1)
                    .min(next[j] + 1)
                    .min(prev[j] + usize::from(x != y)),
            );
        }
        prev = next;
    }
    prev[b.len()] <= 1
}

fn messages(value: &Value, prefix: &str, out: &mut BTreeMap<String, String>) -> Result<()> {
    match value {
        Value::String(s) => {
            out.insert(prefix.into(), s.clone());
        }
        Value::Object(map) => {
            for (key, value) in map {
                // JSON Pointer paths avoid collisions between "a.b" and {"a":{"b":…}}.
                messages(
                    value,
                    &format!("{prefix}/{}", key.replace('~', "~0").replace('/', "~1")),
                    out,
                )?;
            }
        }
        _ => bail!("translation value at {prefix} must be a string or an object"),
    }
    Ok(())
}
fn check_i18n(
    sources: &BTreeMap<&str, &Source>,
    config: &I18n,
    metas: &[RuleMeta],
    out: &mut Vec<Finding>,
) -> Result<()> {
    let read = |name: &str| -> Result<(&Source, BTreeMap<String, String>)> {
        let source = *sources.get(name).ok_or_else(|| {
            anyhow::anyhow!(
                "configured localisation file {name} is missing, ignored, or unreadable"
            )
        })?;
        let value: Value = serde_json::from_str(&source.text)?;
        if !value.is_object() {
            bail!("localisation root in {name} must be an object");
        }
        let mut map = BTreeMap::new();
        messages(&value, "", &mut map)?;
        Ok((source, map))
    };
    let (_, base) = read(&config.base)?;
    let marker = Regex::new(
        r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}|\{([A-Za-z_][A-Za-z0-9_]*)\}|%\(([A-Za-z_][A-Za-z0-9_]*)\)[sdf]",
    )?;
    let markers = |text: &str| -> BTreeSet<String> {
        marker
            .captures_iter(text)
            .filter_map(|c| (1..=4).find_map(|i| c.get(i).map(|m| m.as_str().to_string())))
            .collect()
    };
    for file in &config.translations {
        let (source, translation) = read(file)?;
        let base_keys: BTreeSet<_> = base.keys().collect();
        let translated_keys: BTreeSet<_> = translation.keys().collect();
        if base_keys != translated_keys {
            out.push(crate::review(
                &metas[6],
                source.location(0, 0),
                format!(
                    "Compared with {}: missing {:?}; extra {:?}.",
                    config.base,
                    base_keys.difference(&translated_keys).collect::<Vec<_>>(),
                    translated_keys.difference(&base_keys).collect::<Vec<_>>()
                ),
            ));
        }
        for (key, text) in &translation {
            if let Some(original) = base.get(key)
                && markers(text) != markers(original)
            {
                out.push(crate::review(
                    &metas[7],
                    source.location(0, 0),
                    format!(
                        "Message {key}: base placeholders {:?}; translation placeholders {:?}.",
                        markers(original),
                        markers(text)
                    ),
                ));
            }
        }
    }
    if let Some(file) = &config.lol_catalogue {
        let source = sources.get(file.as_str()).ok_or_else(|| {
            anyhow::anyhow!("LOL catalogue {file} is missing, ignored, or unreadable")
        })?;
        let data: Value = serde_json::from_str(&source.text)?;
        let rows = data.as_array().ok_or_else(|| {
            anyhow::anyhow!("LOL catalogue must be a local JSON array of language records")
        })?;
        let mut codes = BTreeSet::new();
        for row in rows {
            let code = row
                .get("code")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow::anyhow!("LOL language record needs a code"))?;
            if code.len() != 3
                || !code.bytes().all(|c| c.is_ascii_lowercase())
                || !codes.insert(code)
            {
                bail!("LOL language code must be unique lowercase ISO 639-3: {code}");
            }
            if row
                .get("name")
                .and_then(|x| x.as_str())
                .is_none_or(|n| n.trim().is_empty())
            {
                bail!("LOL language record {code} needs a name");
            }
        }
        for file in std::iter::once(&config.base).chain(&config.translations) {
            let code = std::path::Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if !codes.contains(code) {
                bail!("locale {code} is not in the supplied LOL catalogue");
            }
        }
    }
    Ok(())
}
