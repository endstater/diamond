// SPDX-License-Identifier: GPL-3.0-only
//! Source-preserving per-line containers and GUI-independent Enter transactions.
use crate::{Block, BlockKind, Document, ending, parse_inline};
use std::ops::Range;

#[derive(Clone, Debug)]
pub(crate) struct Prefix {
    pub bytes: usize,
    pub indent: usize,
    pub quotes: usize,
    pub marker_start: usize,
    pub list: Option<Marker>,
    pub quote_shell: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Marker {
    pub bullet: char,
    pub number: Option<u64>,
    pub task: Option<bool>,
    pub content_indent: usize,
}
impl Marker {
    fn next(&self) -> String {
        let marker = if let Some(number) = self.number {
            format!("{}{} ", number.saturating_add(1), self.bullet)
        } else {
            format!("{} ", self.bullet)
        };
        if self.task.is_some() {
            format!("{marker}[ ] ")
        } else {
            marker
        }
    }
}
fn whitespace(source: &str, cursor: &mut usize) -> usize {
    let mut columns = 0;
    while let Some(c) = source[*cursor..]
        .chars()
        .next()
        .filter(|c| matches!(c, ' ' | '\t'))
    {
        columns += if c == '\t' { 4 - columns % 4 } else { 1 };
        *cursor += c.len_utf8();
    }
    columns
}
pub(crate) fn prefix(source: &str) -> Prefix {
    let mut cursor = 0;
    let mut indent = whitespace(source, &mut cursor);
    let mut quotes = 0;
    while source[cursor..].starts_with('>') {
        quotes += 1;
        cursor += 1;
        if source[cursor..].starts_with(' ') {
            cursor += 1;
        }
        indent += whitespace(source, &mut cursor);
    }
    let marker_start = cursor;
    let rest = &source[cursor..];
    let mut list = None;
    if let Some(c) = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '+')) {
        if rest.len() == 1 || rest[1..].starts_with([' ', '\t']) {
            cursor += 1;
            whitespace(source, &mut cursor);
            list = Some(Marker {
                bullet: c,
                number: None,
                task: None,
                content_indent: indent + cursor - marker_start,
            });
        }
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            let delimiter = rest[digits..].chars().next();
            if let Some(c) = delimiter.filter(|c| matches!(c, '.' | ')')) {
                let after = digits + 1;
                if rest[after..].is_empty() || rest[after..].starts_with([' ', '\t']) {
                    if let Ok(number) = rest[..digits].parse::<u64>() {
                        cursor += after;
                        whitespace(source, &mut cursor);
                        list = Some(Marker {
                            bullet: c,
                            number: Some(number),
                            task: None,
                            content_indent: indent + cursor - marker_start,
                        });
                    }
                }
            }
        }
    }
    if let Some(marker) = &mut list {
        let rest = &source[cursor..];
        if (rest.starts_with("[ ]") || rest.starts_with("[x]") || rest.starts_with("[X]"))
            && (rest.len() == 3 || rest[3..].starts_with([' ', '\t']))
        {
            marker.task = Some(!rest.starts_with("[ ]"));
            cursor += 3;
            whitespace(source, &mut cursor);
        }
    }
    Prefix {
        bytes: cursor,
        indent,
        quotes,
        marker_start,
        list,
        quote_shell: if quotes == 0 {
            String::new()
        } else {
            "> ".repeat(quotes)
        },
    }
}

pub(crate) fn lines(source: &str, quoted: bool) -> Vec<Block> {
    let mut result: Vec<Block> = Vec::new();
    let mut inherited_quotes = 0;
    let mut parents: Vec<(usize, usize, usize)> = Vec::new();
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let mut info = prefix(content);
        if content.trim().is_empty() {
            if let Some(last) = result.last_mut() {
                last.trailing.push_str(line);
            } else {
                result.push(Block::raw(line));
            }
            continue;
        }
        if quoted && info.quotes == 0 {
            info.quotes = inherited_quotes;
        }
        inherited_quotes = info.quotes;
        if let Some(marker) = &info.list {
            parents.retain(|(quotes, indent, _)| *quotes == info.quotes && *indent < info.indent);
            parents.push((info.quotes, info.indent, marker.content_indent));
        } else if let Some((_, parent, content)) = parents
            .iter()
            .rev()
            .find(|(quotes, _, content)| *quotes == info.quotes && info.indent >= *content)
        {
            info.indent = parent + (info.indent - content);
        }
        let body = &content[info.bytes..];
        let kind = if let Some(marker) = &info.list {
            match marker.task {
                Some(done) => BlockKind::Task(done),
                None => {
                    if marker.number.is_some() {
                        BlockKind::Ordered
                    } else {
                        BlockKind::Bullet
                    }
                }
            }
        } else if info.quotes > 0 {
            BlockKind::Quote
        } else {
            BlockKind::Paragraph
        };
        // Complex embedded constructs stay local raw lines rather than demoting siblings.
        let Some(inline) = parse_inline(body, &BlockKind::Paragraph) else {
            result.push(Block::raw(line));
            continue;
        };
        let mut block = Block {
            kind,
            indent: info.indent,
            quote_depth: info.quotes,
            content_prefix: Some(info.bytes),
            order: info
                .list
                .as_ref()
                .and_then(|marker| marker.number)
                .unwrap_or(1),
            original: Some(line.into()),
            ending: ending(line).into(),
            ..Block::empty()
        };
        inline.apply(&mut block);
        result.push(block);
    }
    result
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Continuation {
    pub range: Range<usize>,
    pub text: String,
}
impl Document {
    /// An atomic source transaction for Enter. Source positions count scalars.
    pub fn line_continuation(&self, caret: usize) -> Option<Continuation> {
        let map = self.source_map_shared();
        let byte = map
            .source
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(map.source.len()))
            .nth(caret)?;
        if map.blocks.iter().zip(self.blocks()).any(|(span, block)| {
            matches!(block.kind, BlockKind::Code | BlockKind::Math)
                && span.source_range.start <= caret
                && caret <= span.source_range.end
        }) {
            return None;
        }
        let start = map.source[..byte].rfind('\n').map_or(0, |at| at + 1);
        let end = map.source[byte..]
            .find('\n')
            .map_or(map.source.len(), |at| byte + at);
        let line = map.source[start..end].trim_end_matches('\r');
        let info = prefix(line);
        if byte < start + info.bytes {
            return None;
        }
        let newline = if map.source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let source_start = map.source[..start].chars().count();
        if let Some(marker) = &info.list {
            if line[info.bytes..].trim().is_empty() {
                let parent = map.source[..start]
                    .lines()
                    .rev()
                    .map(prefix)
                    .take_while(|p| p.quotes == info.quotes)
                    .find(|p| p.list.is_some() && p.indent < info.indent);
                let replacement = parent
                    .map(|parent| {
                        format!(
                            "{}{}{}",
                            parent.quote_shell,
                            " ".repeat(parent.indent),
                            parent.list.as_ref().unwrap().next()
                        )
                    })
                    .unwrap_or_else(|| info.quote_shell.clone());
                return Some(Continuation {
                    range: source_start..caret,
                    text: replacement,
                });
            }
            let shell = &line[..info.marker_start];
            return Some(Continuation {
                range: caret..caret,
                text: format!("{newline}{shell}{}", marker.next()),
            });
        }
        if info.quotes > 0 {
            if line[info.bytes..].trim().is_empty() {
                return Some(Continuation {
                    range: source_start..caret,
                    text: "> ".repeat(info.quotes - 1),
                });
            }
            return Some(Continuation {
                range: caret..caret,
                text: format!("{newline}{}", &line[..info.bytes]),
            });
        }
        None
    }
}
