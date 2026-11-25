// SPDX-License-Identifier: GPL-3.0-only
//! Explicit source/semantic coordinates for a caret-aware editor projection.

use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::{Block, BlockKind, Document, parser};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    /// Exact Markdown snapshot to which every coordinate in this map refers.
    pub source: String,
    pub source_scalar_len: usize,
    /// Same order and length as `Document::blocks()`.
    pub blocks: Vec<BlockSourceMap>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSourceMap {
    /// Global Unicode scalar positions in `SourceMap::source`. Includes the
    /// block's own final newline, but excludes following blank separators.
    pub source_range: Range<usize>,
    /// The same source span in bytes, suitable for slicing UTF-8.
    pub source_byte_range: Range<usize>,
    /// Global Unicode scalar positions in `Document::text()`.
    pub semantic_range: Range<usize>,
    /// Local semantic caret -> global source scalar, with left affinity.
    /// At an interior boundary this is the end of the preceding visible atom.
    pub semantic_to_source_before: Vec<usize>,
    /// Local semantic caret -> global source scalar, with right affinity.
    /// At an interior boundary this is the start of the following visible atom.
    pub semantic_to_source_after: Vec<usize>,
    /// False when a source atom expands to multiple visible scalars or parser
    /// normalization cannot be mapped unambiguously. Editors must retain the
    /// full source representation for such a block instead of using an
    /// approximate mapping for edits.
    pub is_exact: bool,
}

pub(super) fn build(document: &Document) -> SourceMap {
    let (source, ranges) = document.serialize_with_ranges();
    let mut byte_boundaries = source
        .char_indices()
        .map(|(byte, _)| byte)
        .collect::<Vec<_>>();
    byte_boundaries.push(source.len());
    let to_scalar = |byte: usize| {
        byte_boundaries
            .binary_search(&byte)
            .expect("UTF-8 boundary")
    };
    let mut blocks = Vec::with_capacity(ranges.len());
    let mut semantic_start = 0;
    for (block, bytes) in document.blocks().iter().zip(ranges) {
        let fragment = &source[bytes.clone()];
        let source_start = to_scalar(bytes.start);
        let source_end = to_scalar(bytes.end);
        let semantic_len = block.text.chars().count();
        let content_end = if block.kind == BlockKind::Raw {
            fragment.len()
        } else {
            fragment.trim_end_matches(['\r', '\n']).len()
        };
        let mut visible = visible_mapping(fragment, block);
        if visible.text != block.text || visible.spans.len() != semantic_len {
            visible.exact = false;
            visible.spans = vec![0..content_end; semantic_len];
        }
        let to_global = |byte: usize| to_scalar(bytes.start + byte);
        let mut before = Vec::with_capacity(semantic_len + 1);
        let mut after = Vec::with_capacity(semantic_len + 1);
        if semantic_len == 0 {
            let anchor = visible
                .empty_anchor
                .unwrap_or(content_end)
                .min(fragment.len());
            before.push(to_global(anchor));
            after.push(to_global(anchor));
        } else {
            before.push(source_start);
            after.push(to_global(visible.spans[0].start));
            for pair in visible.spans.windows(2) {
                let left = pair[0].end;
                let right = pair[1].start;
                if left > right {
                    visible.exact = false;
                }
                before.push(to_global(left.min(right)));
                after.push(to_global(left.max(right)));
            }
            let end = visible.spans[semantic_len - 1].end;
            before.push(to_global(end));
            after.push(to_global(content_end.max(end)));
        }
        blocks.push(BlockSourceMap {
            source_range: source_start..source_end,
            source_byte_range: bytes,
            semantic_range: semantic_start..semantic_start + semantic_len,
            semantic_to_source_before: before,
            semantic_to_source_after: after,
            is_exact: visible.exact,
        });
        semantic_start += semantic_len + 1;
    }
    SourceMap {
        source_scalar_len: byte_boundaries.len() - 1,
        source,
        blocks,
    }
}

struct VisibleMapping {
    text: String,
    spans: Vec<Range<usize>>,
    exact: bool,
    empty_anchor: Option<usize>,
}

fn visible_mapping(source: &str, block: &Block) -> VisibleMapping {
    if block.kind == BlockKind::Raw {
        return VisibleMapping {
            text: source.to_owned(),
            spans: scalar_spans(source, 0),
            exact: source == block.text,
            empty_anchor: Some(0),
        };
    }
    let mut mapping = VisibleMapping {
        text: String::new(),
        spans: Vec::new(),
        exact: true,
        empty_anchor: None,
    };
    let base = if block.content_prefix.is_some() {
        crate::containers::prefix(source).bytes
    } else {
        0
    };
    let source = if block.content_prefix.is_some() {
        source[base..].trim_end_matches(['\r', '\n'])
    } else {
        source
    };
    let mut code_block = false;
    if block.kind == BlockKind::Image
        && block
            .image
            .as_ref()
            .is_some_and(|image| image.alt.is_empty())
    {
        return VisibleMapping {
            text: block.text.clone(),
            spans: vec![0..0; block.text.chars().count()],
            exact: true,
            empty_anchor: Some(0),
        };
    }
    for (event, span) in parser(source).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_block = true,
            Event::End(TagEnd::CodeBlock) => code_block = false,
            Event::InlineMath(_) | Event::DisplayMath(_) => {
                mapping.text.push('◇');
                mapping.spans.push(span);
            }
            Event::FootnoteReference(_) => {
                mapping.text.push('⁎');
                mapping.spans.push(span);
            }
            Event::End(TagEnd::Paragraph) if block.kind == BlockKind::Footnote => {
                let anchor = mapping.spans.last().map_or(span.end, |last| last.end);
                mapping.text.push('\n');
                mapping.spans.push(anchor..anchor);
            }
            Event::End(TagEnd::FootnoteDefinition) if block.kind == BlockKind::Footnote => {
                if mapping.text.ends_with('\n') {
                    mapping.text.pop();
                    mapping.spans.pop();
                }
            }
            Event::Text(value) => {
                let span = if code_block {
                    span
                } else {
                    include_backslash_escape(source, span, &value)
                };
                let (spans, exact) = text_spans(source, span, &value, !code_block);
                mapping.empty_anchor = mapping
                    .empty_anchor
                    .or_else(|| spans.first().map(|s| s.start));
                mapping.text.push_str(&value);
                mapping.spans.extend(spans);
                mapping.exact &= exact;
            }
            Event::Code(value) => {
                let (spans, exact) = inline_code_spans(source, span, &value);
                mapping.empty_anchor = mapping
                    .empty_anchor
                    .or_else(|| spans.first().map(|s| s.start));
                mapping.text.push_str(&value);
                mapping.spans.extend(spans);
                mapping.exact &= exact;
            }
            Event::End(TagEnd::TableCell) if block.kind == BlockKind::Table => {
                let anchor = mapping.spans.last().map_or(span.start, |last| last.end);
                mapping.text.push('\t');
                mapping.spans.push(anchor..anchor);
            }
            Event::End(TagEnd::TableHead | TagEnd::TableRow) if block.kind == BlockKind::Table => {
                if mapping.text.ends_with('\t') {
                    mapping.text.pop();
                    mapping.text.push('\n');
                }
            }
            Event::End(TagEnd::Table) if block.kind == BlockKind::Table => {
                if mapping.text.ends_with('\n') {
                    mapping.text.pop();
                    mapping.spans.pop();
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                mapping.text.push('\n');
                mapping.spans.push(span);
            }
            _ => {}
        }
    }
    if block.kind == BlockKind::Code && mapping.text.ends_with('\n') {
        mapping.text.pop();
        mapping.spans.pop();
        if mapping.text.ends_with('\r') {
            mapping.text.pop();
            mapping.spans.pop();
        }
    }
    if block.kind == BlockKind::Code && mapping.empty_anchor.is_none() {
        mapping.empty_anchor = source.find('\n').map(|newline| newline + 1);
    }
    for span in &mut mapping.spans {
        span.start += base;
        span.end += base;
    }
    mapping.empty_anchor = mapping
        .empty_anchor
        .map(|anchor| anchor + base)
        .or(Some(base));
    mapping
}

fn scalar_spans(source: &str, base: usize) -> Vec<Range<usize>> {
    source
        .char_indices()
        .map(|(start, character)| base + start..base + start + character.len_utf8())
        .collect()
}

fn include_backslash_escape(source: &str, mut span: Range<usize>, value: &str) -> Range<usize> {
    if span.start == 0
        || !value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_punctuation())
        || source[span.clone()].starts_with('\\')
    {
        return span;
    }
    let preceding = source[..span.start]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count();
    if preceding % 2 == 1 {
        span.start -= 1;
    }
    span
}

fn text_spans(
    source: &str,
    span: Range<usize>,
    value: &str,
    decode_markdown: bool,
) -> (Vec<Range<usize>>, bool) {
    let raw = &source[span.clone()];
    if raw == value {
        return (scalar_spans(raw, span.start), true);
    }
    let (decoded, spans, exact) = decode_atoms(raw, span.start, decode_markdown, false);
    if decoded == value {
        return (spans, exact);
    }
    // Keep coordinate arrays well-formed, but require the editor to show source
    // whenever parser normalization cannot be attributed to exact source atoms.
    (vec![span; value.chars().count()], false)
}

fn inline_code_spans(source: &str, span: Range<usize>, value: &str) -> (Vec<Range<usize>>, bool) {
    let raw = &source[span.clone()];
    let fence = raw.bytes().take_while(|byte| *byte == b'`').count();
    if fence == 0 || raw.len() < fence * 2 || !raw.ends_with(&"`".repeat(fence)) {
        return (vec![span; value.chars().count()], false);
    }
    let body = &raw[fence..raw.len() - fence];
    let (mut decoded, mut spans, exact) = decode_atoms(body, span.start + fence, false, true);
    if decoded.starts_with(' ')
        && decoded.ends_with(' ')
        && decoded.chars().any(|character| character != ' ')
    {
        decoded.remove(0);
        decoded.pop();
        spans.remove(0);
        spans.pop();
    }
    if decoded == value {
        (spans, exact)
    } else {
        (vec![span; value.chars().count()], false)
    }
}

fn decode_atoms(
    source: &str,
    base: usize,
    decode_markdown: bool,
    inline_code: bool,
) -> (String, Vec<Range<usize>>, bool) {
    let mut decoded = String::new();
    let mut spans = Vec::new();
    let mut cursor = 0;
    let mut exact = true;
    while cursor < source.len() {
        let start = cursor;
        let character = source[cursor..]
            .chars()
            .next()
            .expect("cursor is in source");
        cursor += character.len_utf8();
        let atom = if character == '\r' || character == '\n' {
            if character == '\r' && source[cursor..].starts_with('\n') {
                cursor += 1;
            }
            if inline_code {
                " ".to_owned()
            } else {
                "\n".to_owned()
            }
        } else if decode_markdown && character == '\\' {
            if let Some(next) = source[cursor..]
                .chars()
                .next()
                .filter(|next| next.is_ascii_punctuation())
            {
                cursor += next.len_utf8();
                next.to_string()
            } else {
                character.to_string()
            }
        } else if decode_markdown && character == '&' {
            let entity_end = source[cursor..].find(';').map(|end| cursor + end + 1);
            if let Some((end, value)) = entity_end
                .and_then(|end| decode_entity(&source[start..end]).map(|value| (end, value)))
            {
                cursor = end;
                value
            } else {
                character.to_string()
            }
        } else if character == '\0' {
            "\u{fffd}".to_owned()
        } else {
            character.to_string()
        };
        if atom.chars().count() > 1 {
            exact = false;
        }
        spans.extend(std::iter::repeat_n(
            base + start..base + cursor,
            atom.chars().count(),
        ));
        decoded.push_str(&atom);
    }
    (decoded, spans, exact)
}

fn decode_entity(source: &str) -> Option<String> {
    let mut decoded = String::new();
    for event in parser(source) {
        match event {
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => {}
            Event::Text(value) => decoded.push_str(&value),
            _ => return None,
        }
    }
    (decoded != source).then_some(decoded)
}

pub(super) fn tag_source_starts(source: &str, block: &Block) -> Vec<usize> {
    let mapping = visible_mapping(source, block);
    if mapping.text != block.text {
        return Vec::new();
    }
    mapping.spans.iter().map(|span| span.start).collect()
}
