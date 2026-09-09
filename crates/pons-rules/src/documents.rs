// SPDX-License-Identifier: MPL-2.0
use anyhow::Result;
use pons_core::{Finding, RuleMeta, Source, config::Identity};
use regex::Regex;

pub fn metadata() -> Vec<RuleMeta> {
    [
        ("document-reference-missing","A local document, figure or included file is missing","Generated assets may be absent in a source checkout; use a scoped allowance for that build contract."),
        ("document-anchor-missing","An explicit figure/table anchor is not defined in this document","An included document can supply the anchor; cross-include anchor resolution is not implemented."),
        ("document-reference-placeholder","A figure or table reference still names a placeholder","An intentional template example should be fenced as code or explicitly allowed."),
        ("project-identity-mismatch","A project entry document names a different project","Use an explicit alias for a CLI name or former project name; this is a title check, not semantic description matching."),
    ].into_iter().map(|(id,m,c)|crate::meta(id,m,c,vec!["documentation".into()])).collect()
}
pub fn check(sources: &[Source], identity: Option<&Identity>) -> Result<Vec<Finding>> {
    let metas = metadata();
    let mut out = vec![];
    let references = Regex::new(
        r#"(?:link|image|include)::?([^\n\[]+)\[|!?\[[^\]\n]*\]\((<[^>\n]+>|[^\s)]+)(?:\s+"[^"\n]*")?\)"#,
    )?;
    let anchor = Regex::new(r"<<([A-Za-z0-9_-]+)(?:,[^>]*)?>>")?;
    let placeholder =
        Regex::new(r"(?i)(?:^|[/_.-])(?:placeholder|todo|tbd|replace-me)(?:[_.-]|$)")?;
    for source in sources {
        if !source
            .path
            .extension()
            .is_some_and(|e| matches!(e.to_str(), Some("md" | "adoc" | "asciidoc")))
        {
            continue;
        }
        // Mask fenced code without changing byte offsets. Example references
        // are not claims that a file must exist in this repository.
        let mut prose = String::with_capacity(source.text.len());
        let mut fence: Option<&str> = None;
        for line in source.text.split_inclusive('\n') {
            let trimmed = line.trim();
            let delimiter = if trimmed.starts_with("```") {
                Some("```")
            } else if trimmed.starts_with("~~~") {
                Some("~~~")
            } else if trimmed == "----" {
                Some("----")
            } else {
                None
            };
            let code = fence.is_some() || delimiter.is_some();
            if let Some(d) = delimiter {
                if fence == Some(d) {
                    fence = None;
                } else if fence.is_none() {
                    fence = Some(d);
                }
            }
            if code {
                prose.extend(line.bytes().map(|b| if b == b'\n' { '\n' } else { ' ' }));
            } else {
                prose.push_str(line);
            }
        }
        for captures in references.captures_iter(&prose) {
            let m = captures
                .get(1)
                .or_else(|| captures.get(2))
                .expect("reference target");
            let target = m.as_str().trim_matches('<').trim_matches('>');
            if target.starts_with('#')
                || target.contains("://")
                || target.starts_with("mailto:")
                || target.starts_with("data:")
                || target.starts_with('/')
                || target.contains('{')
            {
                continue;
            }
            let target = target.split(['#', '?']).next().unwrap_or(target);
            if target.is_empty() {
                continue;
            }
            if placeholder.is_match(target) {
                out.push(crate::review(
                    &metas[2],
                    source.location(m.start(), m.end()),
                    format!("Reference target `{target}` contains a placeholder marker."),
                ));
            }
            let path = source
                .path
                .parent()
                .expect("source parent")
                .join(decode_target(target));
            if !path.exists() {
                out.push(crate::review(
                    &metas[0],
                    source.location(m.start(), m.end()),
                    format!("`{target}` does not exist relative to {}.", source.relative),
                ));
            }
        }
        for captures in anchor.captures_iter(&prose) {
            let m = captures.get(1).expect("anchor id");
            let id = m.as_str();
            if !prose.contains(&format!("[[{id}]]"))
                && !prose.contains(&format!("[[{id},"))
                && !prose.contains(&format!("[#{id}]"))
            {
                out.push(crate::review(
                    &metas[1],
                    source.location(m.start(), m.end()),
                    format!(
                        "No explicit `[[{id}]]` or `[#{id}]` definition occurs in this document."
                    ),
                ));
            }
        }
        if let Some(identity) = identity {
            let name = source
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !source.relative.contains('/')
                && (name.starts_with("README.") || name.starts_with("EXPLAINME."))
            {
                let normalise = |s: &str| {
                    s.chars()
                        .filter(|c| c.is_alphanumeric())
                        .flat_map(|c| c.to_lowercase())
                        .collect::<String>()
                };
                if let Some(title) = prose
                    .lines()
                    .find_map(|l| l.strip_prefix("= ").or_else(|| l.strip_prefix("# ")))
                {
                    let title_key = normalise(title);
                    let matches = std::iter::once(&identity.name)
                        .chain(&identity.aliases)
                        .any(|n| {
                            let n = normalise(n);
                            !n.is_empty() && title_key.contains(&n)
                        });
                    if !matches {
                        out.push(crate::review(&metas[3],source.location(0,0),format!("Title `{title}` does not name `{}` or an explicitly allowed alias.",identity.name)));
                    }
                }
            }
        }
    }
    Ok(out)
}

pub(crate) fn decode_target(target: &str) -> String {
    let bytes = target.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            decoded.push((a * 16 + b) as u8);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| target.to_string())
}
