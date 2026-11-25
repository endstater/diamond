// SPDX-License-Identifier: GPL-3.0-only
//! Read-only Markdown fragments for selections in semantic text.
use crate::{BlockKind, Document, Mark, serialize_inline};
use std::ops::Range;
impl Document {
    /// A balanced inline fragment; positions are local semantic scalars.
    pub fn markdown_fragment(&self, index: usize, range: Range<usize>) -> Option<String> {
        let block = self.blocks().get(index)?;
        let chars: Vec<char> = block.text.chars().collect();
        if range.start > range.end || range.end > chars.len() {
            return None;
        }
        if range.is_empty() {
            return Some(String::new());
        }
        let inline = |range: Range<usize>| {
            let text: String = chars[range.clone()].iter().collect();
            let marks: Vec<Mark> = block
                .marks
                .iter()
                .filter_map(|mark| {
                    let start = mark.range.start.max(range.start);
                    let end = mark.range.end.min(range.end);
                    (start < end).then(|| Mark {
                        range: start - range.start..end - range.start,
                        kind: mark.kind,
                    })
                })
                .collect();
            serialize_inline(&text, &marks)
        };
        if block.kind == BlockKind::Code {
            let mut fragment = block.clone();
            fragment.original = None;
            fragment.trailing.clear();
            fragment.ending.clear();
            fragment.text = chars[range].iter().collect();
            fragment.marks.clear();
            return Some(fragment.serialize("\n"));
        }
        let map = self.source_map_shared();
        let span = map.blocks.get(index)?;
        let source: Vec<char> = map.source.chars().collect();
        let mut output = String::new();
        let mut position = range.start;
        while position < range.end {
            if let Some(link) = block.links.iter().find(|link| {
                link.range.contains(&position) && !link.target.starts_with(crate::tags::LINK_PREFIX)
            }) {
                let end = range.end.min(link.range.end);
                let label = inline(position..end);
                let target = link
                    .target
                    .replace('\\', "\\\\")
                    .replace('(', "\\(")
                    .replace(')', "\\)")
                    .replace(' ', "%20");
                output.push_str(&format!("[{label}]({target})"));
                position = end;
                continue;
            }
            let atom = block
                .math
                .iter()
                .map(|m| &m.range)
                .chain(block.footnotes.iter().map(|f| &f.range))
                .find(|atom| atom.start == position && atom.end <= range.end);
            if let Some(atom) = atom {
                let start = *span.semantic_to_source_after.get(atom.start)?;
                let end = *span.semantic_to_source_before.get(atom.end)?;
                output.extend(source[start..end].iter());
                position = atom.end;
                continue;
            }
            let next = block
                .links
                .iter()
                .filter(|l| !l.target.starts_with(crate::tags::LINK_PREFIX))
                .map(|l| l.range.start)
                .chain(block.math.iter().map(|m| m.range.start))
                .chain(block.footnotes.iter().map(|f| f.range.start))
                .filter(|start| *start > position)
                .min()
                .unwrap_or(range.end)
                .min(range.end);
            output.push_str(&inline(position..next));
            position = next;
        }
        if range.start == 0 {
            let prefix = match block.kind {
                BlockKind::Heading(level) => format!("{} ", "#".repeat(usize::from(level))),
                BlockKind::Bullet => format!("{}- ", " ".repeat(block.indent)),
                BlockKind::Ordered => format!("{}{}. ", " ".repeat(block.indent), block.order),
                BlockKind::Task(done) => format!(
                    "{}- [{}] ",
                    " ".repeat(block.indent),
                    if done { "x" } else { " " }
                ),
                _ => String::new(),
            };
            output = format!("{prefix}{output}");
            if block.quote_depth > 0 || block.kind == BlockKind::Quote {
                let prefix = "> ".repeat(block.quote_depth.max(1));
                output = output
                    .split('\n')
                    .map(|line| format!("{prefix}{line}"))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_unicode_emphasis_and_links_remain_markdown_without_mutating_source() {
        let source = "plain **你好🙂** and [link](https://example.com)\n";
        let document = Document::parse(source);
        assert_eq!(document.markdown_fragment(0, 7..9).unwrap(), "**好🙂**");
        let block = &document.blocks()[0];
        let link = &block.links[0];
        assert_eq!(
            document.markdown_fragment(0, link.range.clone()).unwrap(),
            "[link](https://example.com)"
        );
        assert_eq!(document.to_markdown(), source);
    }
}
