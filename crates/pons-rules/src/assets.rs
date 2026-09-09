// SPDX-License-Identifier: MPL-2.0
//! Deliberately narrow markup and author-declaration checks, not image forensics.
use anyhow::Result;
use pons_core::{Finding, RuleMeta, Severity, Source, config::Asset};
use regex::Regex;
use std::path::{Component, Path};

pub fn metadata() -> Vec<RuleMeta> {
    [
        ("asset-alt-missing", "An image has no explicit text alternative", "An empty alt is correct for decorative or already-described images. This check covers literal HTML img elements, not generated DOM or complete accessibility conformance."),
        ("asset-decorative-conflict", "A decorative declaration disagrees with image markup", "Decorative intent is per use. Use separate declarations/pages if an asset is informative in another context; empty alt is explicit, absent alt is not."),
        ("asset-declaration-incomplete", "An asset declaration is missing information requested by this project", "Requirements come from the supplied asset declaration. Recorded text is not proof of rights, attribution display, disclosure, accessibility or legal compliance."),
    ].into_iter().map(|(id,m,c)| {
        let mut meta = crate::meta(id,m,c,vec!["html".into(), "documentation".into()]);
        meta.severity = Severity::Info;
        meta
    }).collect()
}

pub fn check(sources: &[Source], declarations: &[Asset]) -> Result<Vec<Finding>> {
    let metas = metadata();
    let mut out = vec![];
    let img = Regex::new(r#"(?is)<img\b(?:[^>"']|"[^"]*"|'[^']*')*>"#)?;
    let attr = Regex::new(r#"(?is)\s([a-z][a-z0-9_-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)?;
    let ignored =
        Regex::new(r"(?is)<!--.*?-->|<script\b[^>]*>.*?</script>|<style\b[^>]*>.*?</style>")?;
    for source in sources {
        if !source
            .path
            .extension()
            .is_some_and(|e| e == "html" || e == "htm")
        {
            continue;
        }
        // Preserve offsets while excluding comment and script examples.
        let mut prose = source.text.as_bytes().to_vec();
        for m in ignored.find_iter(&source.text) {
            for b in &mut prose[m.range()] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
        }
        let prose = String::from_utf8(prose)?;
        for m in img.find_iter(&prose) {
            let attrs: std::collections::BTreeMap<_, _> = attr
                .captures_iter(m.as_str())
                .map(|c| {
                    (
                        c[1].to_ascii_lowercase(),
                        c.get(2)
                            .or_else(|| c.get(3))
                            .or_else(|| c.get(4))
                            .unwrap()
                            .as_str()
                            .to_string(),
                    )
                })
                .collect();
            if !attrs.contains_key("alt") {
                out.push(crate::review(&metas[0],source.location(m.start(),m.end()),"Add an appropriate alt attribute, or explicitly use alt=\"\" when this image is decorative or already described. A title alone does not supply alt text."));
            }
            if let Some(src) = attrs.get("src") {
                let relative = Path::new(&source.relative)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .join(crate::documents::decode_target(src));
                let key = normalise(&relative);
                if declarations
                    .iter()
                    .any(|a| a.decorative && normalise(Path::new(&a.path)) == key)
                    && attrs.get("alt").is_none_or(|s| !s.is_empty())
                {
                    out.push(crate::review(&metas[1],source.location(m.start(),m.end()),format!("`{src}` is declared decorative, but this use does not have alt=\"\". Review the intent of this occurrence.")));
                }
            }
        }
    }
    for a in declarations {
        let mut missing = vec![];
        for (needed, value, label) in [
            (a.rights_required, &a.rights, "rights basis"),
            (a.attribution_required, &a.attribution, "attribution"),
            (a.ai_generated, &a.ai_disclosure, "AI disclosure"),
            (a.description_required, &a.description, "description"),
            (a.title_required, &a.title, "title"),
        ] {
            if needed && value.trim().is_empty() {
                missing.push(label);
            }
        }
        if !missing.is_empty() {
            // Config might be external to the scanned tree. No invented asset location.
            if let Some(source) = sources
                .iter()
                .find(|s| s.relative == "pons.toml")
                .or_else(|| sources.first())
            {
                out.push(crate::review(&metas[2],source.location(0,0),format!("The declaration for `{}` requests {} but records none. Check the project requirement before publishing this asset.",a.path,missing.join(", "))));
            }
        }
    }
    Ok(out)
}
fn normalise(path: &Path) -> String {
    let mut parts = vec![];
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(p) => parts.push(p.to_string_lossy()),
            _ => (),
        }
    }
    parts.join("/")
}
