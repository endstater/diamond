// SPDX-License-Identifier: GPL-3.0-only
//! Shared Unicode tag syntax and exact/hierarchical search semantics.
use crate::{Block, BlockKind, Link, MarkKind};
use std::ops::Range;

pub const LINK_PREFIX: &str = "diamond-tag:";

pub fn normalize(tag: &str) -> Option<String> {
    let tag = tag.trim().trim_start_matches('#').trim_end_matches('/');
    if tag.is_empty()
        || !tag
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
        || !tag.chars().any(|c| c.is_alphabetic() || c == '_')
        || tag.split('/').any(str::is_empty)
    {
        return None;
    }
    Some(tag.to_lowercase())
}

pub fn matches(stored: &str, query: &str) -> bool {
    let stored = stored.to_lowercase();
    stored == query
        || stored
            .strip_prefix(query)
            .is_some_and(|tail| tail.starts_with('/'))
}

pub fn query(text: &str) -> Option<String> {
    if let Some(tag) = text.trim().strip_prefix("tag:") {
        normalize(tag)
    } else {
        text.trim().strip_prefix('#').and_then(normalize)
    }
}

pub(super) fn candidates(text: &str) -> Vec<(Range<usize>, String)> {
    let chars: Vec<_> = text.chars().collect();
    let mut result = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '#'
            || at.checked_sub(1).is_some_and(|i| {
                chars[i].is_alphanumeric() || matches!(chars[i], '_' | '/' | '#' | '\\')
            })
        {
            at += 1;
            continue;
        }
        let start = at;
        at += 1;
        while at < chars.len()
            && (chars[at].is_alphanumeric() || matches!(chars[at], '_' | '-' | '/'))
        {
            at += 1;
        }
        let value: String = chars[start + 1..at].iter().collect();
        if let Some(tag) = normalize(&value) {
            let end = at - value.chars().rev().take_while(|c| *c == '/').count();
            result.push((start..end, tag));
        }
    }
    result
}

pub(super) fn links(source: &str, block: &Block) -> Vec<Link> {
    if !block.text.contains('#')
        || matches!(
            block.kind,
            BlockKind::Raw | BlockKind::Code | BlockKind::Math | BlockKind::Image
        )
    {
        return Vec::new();
    }
    let candidates = candidates(&block.text);
    if candidates.is_empty() {
        return Vec::new();
    }
    let positions = crate::source_map::tag_source_starts(source, block);
    candidates.into_iter().filter(|(range, _)| {
        !block.marks.iter().any(|mark| mark.kind == MarkKind::Code && mark.range.start < range.end && range.start < mark.range.end)
        && !block.links.iter().any(|link| link.range.start < range.end && range.start < link.range.end)
        // Escape/entity expansions must not turn literal text into a tag.
        && positions.get(range.start).is_some_and(|byte| source[*byte..].starts_with('#'))
    }).map(|(range, tag)| Link { range, target: format!("{LINK_PREFIX}{tag}") }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    #[test]
    fn tags_are_unicode_links_outside_code_links_escapes_and_url_fragments() {
        let source = "# Heading\n\n#Матан #work/linux #матан-идз word#no `#code` \\#escaped [#label](https://example.org) https://example.org/#fragment\n\n```yaml\n#not-tag\n```\n";
        let doc = Document::parse(source);
        let tags: Vec<_> = doc
            .blocks()
            .iter()
            .flat_map(|b| &b.links)
            .filter_map(|l| l.target.strip_prefix(LINK_PREFIX))
            .collect();
        assert_eq!(tags, ["матан", "work/linux", "матан-идз"]);
        assert_eq!(doc.to_markdown(), source);
    }
    #[test]
    fn tag_queries_do_not_match_prefixes_but_include_children() {
        assert!(matches("МАТАН", "матан"));
        assert!(matches("матан/идз", "матан"));
        assert!(!matches("матанализ", "матан"));
        assert_eq!(query("tag:#Матан"), Some("матан".into()));
        assert!(query("#123").is_none());
        assert!(query("#a #b").is_none());
    }
}
