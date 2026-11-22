//! Semantic, source-preserving Markdown with Unicode scalar edit positions.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use thiserror::Error;

mod source_map;
mod structures;

pub use source_map::{BlockSourceMap, SourceMap};
pub use structures::{Table, TableCell, CellAlignment, Image};

const HISTORY_LIMIT: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Quote,
    Bullet,
    Ordered,
    Task(bool),
    Rule,
    Table,
    Image,
    Code,
    Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarkKind {
    Bold,
    Italic,
    Strike,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    /// Unicode scalar positions within this block's visible text.
    pub range: Range<usize>,
    pub kind: MarkKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub range: Range<usize>,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    pub marks: Vec<Mark>,
    pub links: Vec<Link>,
    pub table: Option<Table>,
    pub image: Option<Image>,
    original: Option<String>,
    ending: String,
    trailing: String,
    language: String,
    order: u64,
}

impl Block {
    pub fn kind(&self) -> &BlockKind {
        &self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    pub fn order(&self) -> u64 { self.order }

    pub fn language(&self) -> &str {
        &self.language
    }

    fn empty() -> Self {
        Self {
            kind: BlockKind::Paragraph,
            text: String::new(),
            marks: Vec::new(),
            links: Vec::new(),
            table: None,
            image: None,
            original: None,
            ending: String::new(),
            trailing: String::new(),
            language: String::new(),
            order: 1,
        }
    }

    fn raw(source: &str) -> Self {
        Self {
            kind: BlockKind::Raw,
            text: source.to_owned(),
            original: Some(source.to_owned()),
            ..Self::empty()
        }
    }

    fn serialize(&self, newline: &str) -> String {
        if let Some(original) = &self.original {
            let mut result = original.clone();
            result.push_str(&self.trailing);
            return result;
        }
        let inline = serialize_inline(&self.text, &self.marks);
        let body = match self.kind {
            BlockKind::Paragraph => inline,
            BlockKind::Heading(level) => format!("{} {inline}", "#".repeat(usize::from(level))),
            BlockKind::Quote => inline
                .split('\n')
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join(newline),
            BlockKind::Bullet => format!("- {inline}"),
            BlockKind::Ordered => format!("{}. {inline}", self.order),
            BlockKind::Task(checked) => {
                format!("- [{}] {inline}", if checked { "x" } else { " " })
            }
            BlockKind::Code => {
                let fence = "`".repeat(longest_run(&self.text, '`').max(2) + 1);
                let text = self.text.replace('\n', newline);
                format!("{fence}{}{newline}{text}{newline}{fence}", self.language)
            }
            BlockKind::Rule => "---".into(),
            BlockKind::Table | BlockKind::Image => self.text.clone(),
            BlockKind::Raw => self.text.clone(),
        };
        let mut result = body.replace("\r\n", "\n");
        if newline == "\r\n" {
            result = result.replace('\n', "\r\n");
        }
        result.push_str(&self.ending);
        result.push_str(&self.trailing);
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    blocks: Vec<Block>,
    leading: String,
    newline: String,
}

#[derive(Clone, Debug)]
pub struct Document {
    state: State,
    undo: Vec<State>,
    redo: Vec<State>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DocumentError {
    #[error("range is outside the document's Unicode scalar positions")]
    InvalidRange,
    #[error("this Markdown construct must be edited in source mode")]
    RawBlock,
    #[error("block does not exist")]
    InvalidBlock,
    #[error("heading level must be between 1 and 6")]
    InvalidHeading,
    #[error("this edit cannot be represented faithfully by the supported Markdown schema; use source mode")]
    UnrepresentableEdit,
}

impl Default for Document {
    fn default() -> Self {
        Self::parse("")
    }
}

impl Document {
    pub fn parse(source: &str) -> Self {
        let newline = if source.contains("\r\n") { "\r\n" } else { "\n" }.to_owned();
        let ranges = source_ranges(source);
        let mut blocks = Vec::new();
        let mut leading = String::new();
        let mut cursor = 0;
        for range in &ranges {
            preserve_gap(&source[cursor..range.start], &mut blocks, &mut leading);
            let fragment = &source[range.clone()];
            blocks.extend(parse_fragment(fragment));
            cursor = range.end;
        }
        preserve_gap(&source[cursor..], &mut blocks, &mut leading);
        if blocks.is_empty() {
            blocks.push(Block {
                original: Some(source.to_owned()),
                ..Block::empty()
            });
            leading.clear();
        }
        Self {
            state: State { blocks, leading, newline },
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.state.blocks
    }

    /// Visible content with a single scalar newline between blocks.
    pub fn text(&self) -> String {
        self.state
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn to_markdown(&self) -> String {
        self.serialize_with_ranges().0
    }

    /// A source snapshot and exact block/caret mappings for a live projection.
    pub fn source_map(&self) -> SourceMap {
        source_map::build(self)
    }

    /// Splice serialized Markdown at Unicode scalar positions and reparse it as
    /// one undoable transaction. Unlike semantic edits, source transactions may
    /// directly change Markdown delimiters and raw blocks. Replacement bytes,
    /// including line endings, are retained exactly.
    pub fn replace_source_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(), DocumentError> {
        let mut source = self.to_markdown();
        if range.start > range.end || range.end > source.chars().count() {
            return Err(DocumentError::InvalidRange);
        }
        let bytes = scalar_to_byte(&source, range.start)..scalar_to_byte(&source, range.end);
        if &source[bytes.clone()] == replacement {
            return Ok(());
        }
        source.replace_range(bytes, replacement);
        let parsed = Self::parse(&source);
        let before = std::mem::replace(&mut self.state, parsed.state);
        self.commit(before);
        Ok(())
    }

    fn serialize_with_ranges(&self) -> (String, Vec<Range<usize>>) {
        let mut result = self.state.leading.clone();
        let mut ranges = Vec::with_capacity(self.state.blocks.len());
        for (index, block) in self.state.blocks.iter().enumerate() {
            if index > 0 {
                let previous = &self.state.blocks[index - 1];
                if previous.original.is_none() || block.original.is_none() {
                    let required = if previous.kind == BlockKind::Paragraph
                        && block.kind == BlockKind::Paragraph
                    {
                        2
                    } else {
                        1
                    };
                    for _ in trailing_newlines(&result)..required {
                        result.push_str(&self.state.newline);
                    }
                }
            }
            let start = result.len();
            let serialized = block.serialize(&self.state.newline);
            let end = start + serialized.len() - block.trailing.len();
            result.push_str(&serialized);
            ranges.push(start..end);
        }
        (result, ranges)
    }

    pub fn replace_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(), DocumentError> {
        self.validate_range(&range)?;
        if range.is_empty() && replacement.is_empty() {
            return Ok(());
        }
        let (first, first_offset) = self.locate(range.start);
        let (last, last_offset) = self.locate(range.end);
        if self.state.blocks[first..=last]
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Raw | BlockKind::Table | BlockKind::Image) || !block.links.is_empty())
        {
            return Err(DocumentError::RawBlock);
        }
        let before = self.state.clone();
        let selected = &self.state.blocks[first..=last];
        let combined = selected
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let local_end = selected[..selected.len() - 1]
            .iter()
            .map(|block| block.text.chars().count() + 1)
            .sum::<usize>()
            + last_offset;
        let mut styles = Vec::new();
        for (index, block) in selected.iter().enumerate() {
            if index > 0 {
                styles.push(Vec::new());
            }
            styles.extend(styles_for(block));
        }
        let inherited = if first_offset > 0 {
            styles.get(first_offset - 1).cloned().unwrap_or_default()
        } else {
            styles.get(first_offset).cloned().unwrap_or_default()
        };
        let normalized = replacement.replace("\r\n", "\n");
        let mut edited = combined;
        edited.replace_range(
            scalar_to_byte(&edited, first_offset)..scalar_to_byte(&edited, local_end),
            &normalized,
        );
        styles.splice(
            first_offset..local_end,
            std::iter::repeat_n(inherited, normalized.chars().count()),
        );
        let keep_block = first == last
            && matches!(selected[0].kind, BlockKind::Code | BlockKind::Quote);
        let parts = if keep_block {
            vec![edited.as_str()]
        } else {
            edited.split('\n').collect::<Vec<_>>()
        };
        let mut updated = Vec::new();
        let mut offset = 0;
        for (index, part) in parts.iter().enumerate() {
            let mut block = selected[0].clone();
            block.original = None;
            block.text = (*part).to_owned();
            let length = part.chars().count();
            block.marks = marks_from_styles(&styles[offset..offset + length]);
            if index > 0 {
                block.kind = continuation_kind(&selected[0].kind);
                block.order = selected[0].order + index as u64;
            }
            if index + 1 == parts.len() {
                block.ending.clone_from(&selected[selected.len() - 1].ending);
                block.trailing.clone_from(&selected[selected.len() - 1].trailing);
            } else {
                block.ending.clone_from(&self.state.newline);
                block.trailing.clear();
            }
            updated.push(block);
            offset += length + usize::from(!keep_block);
        }
        self.state.blocks.splice(first..=last, updated);
        self.validate_serialization(&before)?;
        self.commit(before);
        Ok(())
    }

    pub fn toggle_mark(
        &mut self,
        range: Range<usize>,
        kind: MarkKind,
    ) -> Result<(), DocumentError> {
        self.validate_range(&range)?;
        if range.is_empty() {
            return Ok(());
        }
        let mut spans = Vec::new();
        let mut base = 0;
        for (index, block) in self.state.blocks.iter().enumerate() {
            let length = block.text.chars().count();
            let start = range.start.saturating_sub(base).min(length);
            let end = range.end.saturating_sub(base).min(length);
            if start < end {
                if matches!(block.kind, BlockKind::Raw | BlockKind::Code | BlockKind::Rule | BlockKind::Table | BlockKind::Image) || !block.links.is_empty() {
                    return Err(DocumentError::RawBlock);
                }
                let chars = block.text.chars().collect::<Vec<_>>();
                let trimmed_start = (start..end).find(|position| !chars[*position].is_whitespace());
                let trimmed_end = (start..end)
                    .rev()
                    .find(|position| !chars[*position].is_whitespace());
                if let (Some(start), Some(end)) = (trimmed_start, trimmed_end) {
                    spans.push((index, start..end + 1));
                }
            }
            base += length + 1;
        }
        if spans.is_empty() {
            return Ok(());
        }
        let remove = spans.iter().all(|(index, span)| {
            styles_for(&self.state.blocks[*index])[span.clone()]
                .iter()
                .all(|styles| styles.contains(&kind))
        });
        let before = self.state.clone();
        for (index, span) in spans {
            let block = &mut self.state.blocks[index];
            let mut styles = styles_for(block);
            for active in &mut styles[span] {
                if remove {
                    active.retain(|existing| *existing != kind);
                } else {
                    if kind == MarkKind::Code {
                        active.clear();
                    } else {
                        active.retain(|existing| *existing != MarkKind::Code);
                    }
                    if !active.contains(&kind) {
                        active.push(kind);
                        active.sort_unstable();
                    }
                }
            }
            block.marks = marks_from_styles(&styles);
            block.original = None;
        }
        self.validate_serialization(&before)?;
        self.commit(before);
        Ok(())
    }

    pub fn toggle_task(&mut self, index: usize) -> Result<(), DocumentError> {
        let block = self.blocks().get(index).ok_or(DocumentError::InvalidBlock)?;
        let BlockKind::Task(done) = block.kind else { return Err(DocumentError::InvalidBlock) };
        let map = self.source_map();
        let span = &map.blocks[index];
        let fragment = &map.source[span.source_byte_range.clone()];
        let marker = fragment.find('[').ok_or(DocumentError::InvalidBlock)?;
        let at = span.source_range.start + fragment[..marker].chars().count() + 1;
        self.replace_source_range(at..at + 1, if done { " " } else { "x" })
    }

    pub fn set_block_kind(
        &mut self,
        index: usize,
        kind: BlockKind,
    ) -> Result<(), DocumentError> {
        if matches!(kind, BlockKind::Heading(level) if !(1..=6).contains(&level)) {
            return Err(DocumentError::InvalidHeading);
        }
        let block = self.state.blocks.get(index).ok_or(DocumentError::InvalidBlock)?;
        if matches!(block.kind,BlockKind::Raw|BlockKind::Table|BlockKind::Image) || matches!(kind,BlockKind::Raw|BlockKind::Table|BlockKind::Image) || !block.links.is_empty() {
            return Err(DocumentError::RawBlock);
        }
        if block.kind == kind {
            return Ok(());
        }
        let before = self.state.clone();
        let block = &mut self.state.blocks[index];
        block.kind = kind;
        if block.kind == BlockKind::Code {
            block.marks.clear();
        }
        block.original = None;
        self.validate_serialization(&before)?;
        self.commit(before);
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.state, previous));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.state, next));
        true
    }

    fn validate_range(&self, range: &Range<usize>) -> Result<(), DocumentError> {
        if range.start > range.end || range.end > self.text().chars().count() {
            Err(DocumentError::InvalidRange)
        } else {
            Ok(())
        }
    }

    fn locate(&self, position: usize) -> (usize, usize) {
        let mut base = 0;
        for (index, block) in self.state.blocks.iter().enumerate() {
            let length = block.text.chars().count();
            if position <= base + length {
                return (index, position - base);
            }
            base += length + 1;
        }
        let last = self.state.blocks.len() - 1;
        (last, self.state.blocks[last].text.chars().count())
    }

    fn commit(&mut self, before: State) {
        if before == self.state {
            return;
        }
        if self.undo.len() == HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
    }

    fn validate_serialization(&mut self, before: &State) -> Result<(), DocumentError> {
        let round_trip = Self::parse(&self.to_markdown());
        if semantic_signature(&self.state) == semantic_signature(&round_trip.state) {
            Ok(())
        } else {
            self.state = before.clone();
            Err(DocumentError::UnrepresentableEdit)
        }
    }
}

fn preserve_gap(source: &str, blocks: &mut Vec<Block>, leading: &mut String) {
    if source.is_empty() {
        return;
    }
    if source.trim().is_empty() {
        if let Some(last) = blocks.last_mut() {
            last.trailing.push_str(source);
        } else {
            leading.push_str(source);
        }
    } else {
        // Reference definitions need not produce parser events. Treat any such
        // unclaimed source as an explicit raw block so cross-block edits cannot
        // silently discard it with an obsolete separator.
        blocks.push(Block::raw(source));
    }
}

type SemanticSignature = Vec<(BlockKind, String, Vec<Vec<MarkKind>>)>;

fn semantic_signature(state: &State) -> SemanticSignature {
    state
        .blocks
        .iter()
        .filter(|block| {
            // Markdown has no syntax for a separately identifiable empty paragraph.
            // Keep spacers while editing, and retain their blank lines on disk.
            !(block.kind == BlockKind::Paragraph && block.text.is_empty())
        })
        .map(|block| (block.kind.clone(), block.text.clone(), styles_for(block)))
        .collect()
}

fn parser(source: &str) -> Parser<'_> {
    Parser::new_ext(
        source,
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_WIKILINKS,
    )
}

fn source_ranges(source: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut end = 0;
    for (event, span) in parser(source).into_offset_iter() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    start = span.start;
                    end = span.end;
                }
                depth += 1;
                end = end.max(span.end);
            }
            Event::End(_) => {
                end = end.max(span.end);
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    ranges.push(start..end);
                }
            }
            _ if depth == 0 => ranges.push(span),
            _ => end = end.max(span.end),
        }
    }
    ranges
}

fn parse_fragment(source: &str) -> Vec<Block> {
    let first = parser(source).next();
    if let Some(image) = structures::parse_image(source) { return vec![image]; }
    if matches!(first, Some(Event::Start(Tag::List(_)))) {
        return parse_flat_list(source).unwrap_or_else(|| vec![Block::raw(source)]);
    }
    let kind = match first {
        Some(Event::Start(Tag::Paragraph)) => BlockKind::Paragraph,
        Some(Event::Start(Tag::Heading { level, .. })) => BlockKind::Heading(level as u8),
        Some(Event::Start(Tag::BlockQuote(_))) => BlockKind::Quote,
        Some(Event::Start(Tag::CodeBlock(_))) => BlockKind::Code,
        Some(Event::Rule) => BlockKind::Rule,
        Some(Event::Start(Tag::Table(_))) => BlockKind::Table,
        _ => return vec![Block::raw(source)],
    };
    let mut block = Block {
        kind,
        original: Some(source.to_owned()),
        ending: ending(source).to_owned(),
        ..Block::empty()
    };
    if block.kind == BlockKind::Code {
        for event in parser(source) {
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language))) => {
                    block.language = language.to_string();
                }
                Event::Text(text) => block.text.push_str(&text),
                _ => {}
            }
        }
        if block.text.ends_with('\n') {
            block.text.pop();
            if block.text.ends_with('\r') {
                block.text.pop();
            }
        }
    } else if block.kind == BlockKind::Rule {
        // A structural block has no editable semantic text.
    } else if let Some((text, marks, links)) = parse_inline(source, &block.kind) {
        block.text = text;
        block.marks = marks;
        block.links = links;
        if block.kind == BlockKind::Table {block.table=Some(structures::table(source, &block.text));}
    } else {
        return vec![Block::raw(source)];
    }
    vec![block]
}

fn parse_flat_list(source: &str) -> Option<Vec<Block>> {
    let mut blocks: Vec<Block> = Vec::new();
    for line in source.split_inclusive('\n') {
        if line.trim().is_empty() {
            if let Some(last) = blocks.last_mut() { last.trailing.push_str(line); }
            else { blocks.push(Block::raw(line)); }
            continue;
        }
        match parse_list_line(line) {
            Some(block) => blocks.push(block),
            None => blocks.push(Block::raw(line)),
        }
    }
    (!blocks.is_empty()).then_some(blocks)
}

fn parse_list_line(line: &str) -> Option<Block> {
        let content = line.trim_end_matches(['\r', '\n']);
        let (mut kind, order, mut body) = if let Some(body) = content
            .strip_prefix("- ")
            .or_else(|| content.strip_prefix("* "))
            .or_else(|| content.strip_prefix("+ "))
            .or_else(|| matches!(content, "-" | "*" | "+").then_some(""))
        {
            (BlockKind::Bullet, 1, body)
        } else {
            let digits = content.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            let remainder = &content[digits..];
            let body = remainder
                .strip_prefix(". ")
                .or_else(|| remainder.strip_prefix(") "))?;
            (BlockKind::Ordered, content[..digits].parse().ok()?, body)
        };
        {
            if let Some(rest) = body.strip_prefix("[ ] ").or_else(|| (body == "[ ]").then_some("")) {
                kind = BlockKind::Task(false);
                body = rest;
            } else if let Some(rest) = body
                .strip_prefix("[x] ")
                .or_else(|| body.strip_prefix("[X] "))
                .or_else(|| matches!(body, "[x]" | "[X]").then_some(""))
            {
                kind = BlockKind::Task(true);
                body = rest;
            }
        }
        let (text, marks, links) = parse_inline(body, &BlockKind::Paragraph)?;
        Some(Block {
            kind,
            text,
            marks,
            links,
            original: Some(line.to_owned()),
            ending: ending(line).to_owned(),
            order,
            ..Block::empty()
        })
}

fn parse_inline(source: &str, kind: &BlockKind) -> Option<(String, Vec<Mark>, Vec<Link>)> {
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut stack = Vec::new();
    let mut links = Vec::new();
    let mut link_stack = Vec::new();
    let mut paragraphs = 0;
    let mut headings = 0;
    let mut quotes = 0;
    let mut position: usize = 0;
    for event in parser(source) {
        match event {
            Event::Start(Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell) if *kind == BlockKind::Table => {},
            Event::End(TagEnd::TableCell) if *kind == BlockKind::Table => {text.push('\t');position+=1;},
            Event::End(TagEnd::TableHead | TagEnd::TableRow) if *kind == BlockKind::Table => {
                if text.ends_with('\t') {text.pop();text.push('\n');}
            },
            Event::End(TagEnd::Table) if *kind == BlockKind::Table => {
                if text.ends_with('\n') {text.pop();position=position.saturating_sub(1);}
            },
            Event::Start(Tag::Paragraph) => paragraphs += 1,
            Event::End(TagEnd::Paragraph) => {}
            Event::Start(Tag::Heading { .. }) => headings += 1,
            Event::End(TagEnd::Heading(_)) => {}
            Event::Start(Tag::BlockQuote(_)) => quotes += 1,
            Event::End(TagEnd::BlockQuote(_)) => {}
            Event::Start(Tag::Link { dest_url, .. }) => link_stack.push((position, dest_url.to_string())),
            Event::End(TagEnd::Link) => {
                let (start, target) = link_stack.pop()?;
                links.push(Link { range: start..position, target });
            }
            Event::Start(Tag::Strong) => stack.push((MarkKind::Bold, position)),
            Event::Start(Tag::Emphasis) => stack.push((MarkKind::Italic, position)),
            Event::Start(Tag::Strikethrough) => stack.push((MarkKind::Strike, position)),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                let (kind, start) = stack.pop()?;
                if start < position {
                    marks.push(Mark {
                        range: start..position,
                        kind,
                    });
                }
            }
            Event::Text(value) => {
                position += value.chars().count();
                text.push_str(&value);
            }
            Event::Code(value) => {
                let length = value.chars().count();
                marks.push(Mark {
                    range: position..position + length,
                    kind: MarkKind::Code,
                });
                position += length;
                text.push_str(&value);
            }
            Event::SoftBreak => {
                text.push('\n');
                position += 1;
            }
            _ => return None,
        }
    }
    let valid = match kind {
        BlockKind::Paragraph => paragraphs <= 1 && headings == 0 && quotes == 0,
        BlockKind::Heading(_) => paragraphs == 0 && headings == 1 && quotes == 0,
        BlockKind::Quote => paragraphs <= 1 && headings == 0 && quotes == 1,
        BlockKind::Table => true,
        _ => false,
    };
    structures::bare_links(&text, &marks, &mut links);
    valid.then_some((text, marks, links))
}

fn ending(source: &str) -> &str {
    if source.ends_with("\r\n") {
        "\r\n"
    } else if source.ends_with('\n') {
        "\n"
    } else {
        ""
    }
}

fn scalar_to_byte(text: &str, position: usize) -> usize {
    text.char_indices()
        .nth(position)
        .map_or(text.len(), |(byte, _)| byte)
}

fn styles_for(block: &Block) -> Vec<Vec<MarkKind>> {
    let mut styles = vec![Vec::new(); block.text.chars().count()];
    for mark in &block.marks {
        for active in &mut styles[mark.range.clone()] {
            if !active.contains(&mark.kind) {
                active.push(mark.kind);
                active.sort_unstable();
            }
        }
    }
    styles
}

fn marks_from_styles(styles: &[Vec<MarkKind>]) -> Vec<Mark> {
    let mut marks = Vec::new();
    for kind in [
        MarkKind::Bold,
        MarkKind::Italic,
        MarkKind::Strike,
        MarkKind::Code,
    ] {
        let mut start = None;
        for position in 0..=styles.len() {
            let active = styles
                .get(position)
                .is_some_and(|styles| styles.contains(&kind));
            match (start, active) {
                (None, true) => start = Some(position),
                (Some(begin), false) => {
                    marks.push(Mark {
                        range: begin..position,
                        kind,
                    });
                    start = None;
                }
                _ => {}
            }
        }
    }
    marks
}

fn continuation_kind(kind: &BlockKind) -> BlockKind {
    match kind {
        BlockKind::Bullet | BlockKind::Ordered | BlockKind::Quote => kind.clone(),
        BlockKind::Task(_) => BlockKind::Task(false),
        _ => BlockKind::Paragraph,
    }
}

fn marker(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Bold => "**",
        MarkKind::Italic => "*",
        MarkKind::Strike => "~~",
        MarkKind::Code => "`",
    }
}

fn serialize_inline(text: &str, marks: &[Mark]) -> String {
    let block = Block {
        text: text.to_owned(),
        marks: marks.to_vec(),
        ..Block::empty()
    };
    let styles = styles_for(&block);
    let chars = text.chars().collect::<Vec<_>>();
    let mut result = String::new();
    let mut open = Vec::new();
    let mut position = 0;
    while position < chars.len() {
        if styles[position].contains(&MarkKind::Code) {
            let desired = styles[position]
                .iter()
                .copied()
                .filter(|kind| *kind != MarkKind::Code)
                .collect::<Vec<_>>();
            let shared = open
                .iter()
                .zip(&desired)
                .take_while(|(a, b)| a == b)
                .count();
            for kind in open.drain(shared..).rev() {
                result.push_str(marker(kind));
            }
            for kind in &desired[shared..] {
                result.push_str(marker(*kind));
                open.push(*kind);
            }
            let end = (position..chars.len())
                .find(|index| styles[*index] != styles[position])
                .unwrap_or(chars.len());
            let value = chars[position..end].iter().collect::<String>();
            let fence = "`".repeat(longest_run(&value, '`') + 1);
            let pad = !value.chars().all(|character| character == ' ')
                && (value.starts_with(['`', ' ']) || value.ends_with(['`', ' ']));
            result.push_str(&fence);
            if pad {
                result.push(' ');
            }
            result.push_str(&value);
            if pad {
                result.push(' ');
            }
            result.push_str(&fence);
            position = end;
            continue;
        }
        let desired = &styles[position];
        let shared = open
            .iter()
            .zip(desired)
            .take_while(|(a, b)| a == b)
            .count();
        for kind in open.drain(shared..).rev() {
            result.push_str(marker(kind));
        }
        for kind in &desired[shared..] {
            result.push_str(marker(*kind));
            open.push(*kind);
        }
        match chars[position] {
            '&' => result.push_str("&amp;"),
            '\t' => result.push_str("&#9;"),
            '\r' => result.push_str("&#13;"),
            ' ' if position == 0
                || position + 1 == chars.len()
                || chars[position - 1] == '\n'
                || chars[position + 1] == '\n'
                || (!styles[position].is_empty()
                    && (styles[position - 1] != styles[position]
                        || styles[position + 1] != styles[position])) =>
            {
                result.push_str("&#32;");
            }
            character @ ('\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '+' | '-'
            | '!' | '|' | '~' | '.') => {
                result.push('\\');
                result.push(character);
            }
            character => result.push(character),
        }
        position += 1;
    }
    for kind in open.into_iter().rev() {
        result.push_str(marker(kind));
    }
    result
}

fn longest_run(text: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in text.chars() {
        if character == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn trailing_newlines(text: &str) -> usize {
    text.chars()
        .rev()
        .take_while(|character| matches!(character, '\r' | '\n'))
        .filter(|character| *character == '\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_and_loose_lists_do_not_demote_completed_items() {
        for prefix in ["- ", "1. "] {
            for tail in ["", "\n", "\n\n", "\n- [", "\n\n- [", "\n2. "] {
                let source=format!("{prefix}[label](https://example.org)\n{tail}");
                let document=Document::parse(&source);
                assert!(!matches!(document.blocks()[0].kind,BlockKind::Raw));
                assert_eq!(document.blocks()[0].links[0].target,"https://example.org");
                assert!(document.source_map().blocks[0].is_exact);
                assert_eq!(document.to_markdown(),source);
            }
        }
    }

    #[test]
    fn typing_task_sequence_preserves_previous_tasks_and_bytes() {
        let mut document=Document::parse("- [ ] task1\n");
        for suffix in ["-", " ", "[", " ", "]", " ", "task2", "\n", "\n"] {
            let end=document.to_markdown().chars().count();document.replace_source_range(end..end,suffix).unwrap();
            assert_eq!(document.blocks()[0].kind,BlockKind::Task(false));
            assert_eq!(document.blocks()[0].text,"task1");
        }
        assert_eq!(document.blocks()[1].kind,BlockKind::Task(false));
        assert_eq!(document.blocks()[1].text,"task2");
    }

    #[test]
    fn tables_keep_alignment_inline_links_and_exact_unicode_anchors() {
        let source="| Имя | Value |\r\n| :--- | ---: |\r\n| **你好** | [site](https://example.org) |\r\n| | 42 |\r\n";
        let mut document=Document::parse(source);
        let block=&document.blocks()[0];assert_eq!(block.kind,BlockKind::Table);
        let table=block.table.as_ref().unwrap();assert_eq!(table.columns,vec![CellAlignment::Left,CellAlignment::Right]);
        assert_eq!(table.cells.len(),6);assert_eq!(block.links[0].target,"https://example.org");
        let map=document.source_map();assert!(map.blocks[0].is_exact);
        assert!(map.blocks[0].semantic_to_source_before.windows(2).all(|pair|pair[0]<=pair[1]));
        assert_eq!(document.to_markdown(),source);
        let start=source[..source.find("42").unwrap()].chars().count();
        document.replace_source_range(start..start+2,"43").unwrap();assert!(document.undo());assert_eq!(document.to_markdown(),source);
    }

    #[test]
    fn standalone_images_keep_alt_title_and_original_source() {
        for source in ["![Alt текст](https://example.org/a.png \"Подпись\")\n", "![](assets/a.png)\r\n"] {
            let document=Document::parse(source);let block=&document.blocks()[0];
            assert_eq!(block.kind,BlockKind::Image);assert!(document.source_map().blocks[0].is_exact);
            assert!(block.image.as_ref().unwrap().target.ends_with("a.png"));assert_eq!(document.to_markdown(),source);
        }
    }

    #[test]
    fn links_lists_rules_and_tasks_preserve_source_and_history() {
        let source = "- [ ] [site](https://example.org) and [[Folder/Note|alias]]\r\n\r\n---\r\n";
        let mut doc = Document::parse(source);
        assert_eq!(doc.blocks()[0].kind, BlockKind::Task(false));
        assert_eq!(doc.blocks()[0].text, "site and alias");
        assert_eq!(doc.blocks()[0].links.len(), 2);
        assert_eq!(doc.blocks()[1].kind, BlockKind::Rule);
        assert_eq!(doc.to_markdown(), source);
        assert!(doc.source_map().blocks[0].is_exact);
        doc.toggle_task(0).unwrap();
        assert_eq!(doc.to_markdown(), source.replacen("[ ]", "[x]", 1));
        assert!(doc.undo()); assert_eq!(doc.to_markdown(), source);
        assert!(doc.redo()); assert_eq!(doc.blocks()[0].kind, BlockKind::Task(true));
    }

    #[test]
    fn source_marker_edits_reparse_and_share_undo_history() {
        let mut document = Document::parse("Я🙂\r\n\r\n[link](x)\r\n");
        document.replace_source_range(0..0, "# ").unwrap();
        assert_eq!(document.blocks()[0].kind, BlockKind::Heading(1));
        assert_eq!(document.to_markdown(), "# Я🙂\r\n\r\n[link](x)\r\n");
        document.replace_source_range(2..4, "**мир**").unwrap();
        assert_eq!(document.blocks()[0].text, "мир");
        assert!(document.blocks()[0].marks.iter().any(|mark| mark.kind == MarkKind::Bold));
        assert!(document.undo());
        assert_eq!(document.to_markdown(), "# Я🙂\r\n\r\n[link](x)\r\n");
        assert!(document.undo());
        assert_eq!(document.to_markdown(), "Я🙂\r\n\r\n[link](x)\r\n");
        assert!(document.redo());
        assert_eq!(document.blocks()[0].kind, BlockKind::Heading(1));
    }

    #[test]
    fn source_map_resolves_unicode_and_markup_boundaries() {
        let document = Document::parse("# Я🙂\r\n\r\n**мир**\n");
        let map = document.source_map();
        assert_eq!(map.blocks.len(), document.blocks().len());
        assert_eq!(map.source, document.to_markdown());
        let heading = &map.blocks[0];
        assert!(heading.is_exact);
        assert_eq!(&map.source[heading.source_byte_range.clone()], "# Я🙂\r\n");
        assert_eq!(heading.semantic_to_source_after[0], 2);
        assert_eq!(heading.semantic_to_source_before[2], 4);
        for block in &map.blocks {
            let len = block.semantic_range.len() + 1;
            assert_eq!(block.semantic_to_source_before.len(), len);
            assert_eq!(block.semantic_to_source_after.len(), len);
            assert!(block.semantic_to_source_before.windows(2).all(|pair| pair[0] <= pair[1]));
            assert!(block.semantic_to_source_after.windows(2).all(|pair| pair[0] <= pair[1]));
        }
    }

    #[test]
    fn invalid_source_ranges_leave_bytes_and_history_intact() {
        let mut document = Document::parse("🙂");
        assert_eq!(document.replace_source_range(0..2, "x"), Err(DocumentError::InvalidRange));
        assert_eq!(document.to_markdown(), "🙂");
        assert!(!document.undo());
    }

    #[test]
    fn unchanged_source_is_byte_identical() {
        for source in [
            "",
            " \r\n\t\r\n",
            "# Heading  \r\n\r\nA **bold** paragraph.\r\n",
            "| a | b |\n| - | - |\n| 1 | 2 |\n\n[ref]: https://example.com\n",
            "---\ntitle: YAML\n---\n\n<div>custom</div>\n",
            "- nested\n  - inner\n\n![image](asset.png)\n",
            "```rust\nfn main() {}\n```\n\n[[Wikilink]]\n",
        ] {
            assert_eq!(Document::parse(source).to_markdown().as_bytes(), source.as_bytes());
        }
    }

    #[test]
    fn projection_removes_supported_markdown_syntax() {
        let document = Document::parse("# Привет **мир**\n\n- [x] Сделано\n- Второе\n");
        assert_eq!(document.text(), "Привет мир\nСделано\nВторое");
        assert_eq!(document.blocks()[0].kind, BlockKind::Heading(1));
        assert_eq!(document.blocks()[1].kind, BlockKind::Task(true));
        assert_eq!(document.blocks()[0].marks[0].range, 7..10);
    }

    #[test]
    fn unicode_edits_count_scalars_and_undo_restores_source() {
        let source = "# Привет 🦀\r\n\r\nНетронуто  \r\n";
        let mut document = Document::parse(source);
        let untouched_projection = document.blocks()[1].text.clone();
        document.replace_range(7..8, "世界").unwrap();
        assert_eq!(document.text(), format!("Привет 世界\n{untouched_projection}"));
        assert!(document.to_markdown().ends_with("Нетронуто  \r\n"));
        assert!(document.undo());
        assert_eq!(document.to_markdown(), source);
        assert!(document.redo());
        assert!(document.text().starts_with("Привет 世界"));
    }

    #[test]
    fn formatting_and_block_structure_share_history() {
        let mut document = Document::parse("Hello world\n");
        document.toggle_mark(6..11, MarkKind::Bold).unwrap();
        assert_eq!(document.to_markdown(), "Hello **world**\n");
        document.set_block_kind(0, BlockKind::Heading(2)).unwrap();
        assert_eq!(document.to_markdown(), "## Hello **world**\n");
        assert!(document.undo());
        assert_eq!(document.to_markdown(), "Hello **world**\n");
        assert!(document.undo());
        assert_eq!(document.to_markdown(), "Hello world\n");
        assert!(document.redo());
        assert_eq!(document.to_markdown(), "Hello **world**\n");
    }

    #[test]
    fn deleting_separator_merges_blocks_atomically() {
        let mut document = Document::parse("One\n\nTwo\n");
        document.replace_range(3..4, " ").unwrap();
        assert_eq!(document.text(), "One Two");
        assert_eq!(document.blocks().len(), 1);
        assert!(document.undo());
        assert_eq!(document.to_markdown(), "One\n\nTwo\n");
    }

    #[test]
    fn inserted_newline_creates_semantic_paragraphs() {
        let mut document = Document::parse("AlphaBeta");
        document.replace_range(5..5, "\n").unwrap();
        assert_eq!(document.blocks().len(), 2);
        assert_eq!(document.to_markdown(), "Alpha\n\nBeta");
        assert_eq!(Document::parse(&document.to_markdown()).text(), document.text());
    }

    #[test]
    fn unsupported_blocks_reject_edits_without_partial_changes() {
        let source = "Before\n\n[link](https://example.com)\n";
        let mut document = Document::parse(source);
        assert_eq!(document.blocks()[1].kind, BlockKind::Paragraph);
        assert_eq!(document.blocks()[1].links[0].target, "https://example.com");
        assert_eq!(document.replace_range(0..document.text().chars().count(), "x"), Err(DocumentError::RawBlock));
        assert_eq!(document.to_markdown(), source);
        assert!(!document.undo());
    }

    #[test]
    fn parser_gaps_with_reference_definitions_are_protected_raw_source() {
        let source = "Before\n\n[ref]: https://example.com\n\nAfter\n";
        let mut document = Document::parse(source);
        assert!(document.blocks().iter().any(|block| block.kind == BlockKind::Raw));
        assert_eq!(document.replace_range(0..document.text().chars().count(), "replacement"), Err(DocumentError::RawBlock));
        assert_eq!(document.to_markdown(), source);
        let mut definition_only = Document::parse("[ref]: https://example.com\n");
        assert_eq!(definition_only.blocks()[0].kind, BlockKind::Raw);
        assert_eq!(definition_only.replace_range(0..0, "x"), Err(DocumentError::RawBlock));
    }

    #[test]
    fn code_fence_grows_to_protect_inserted_fences() {
        let mut document = Document::parse("```rust\nx\n```\n");
        document.replace_range(0..1, "```\n🦀").unwrap();
        let serialized = document.to_markdown();
        assert!(serialized.starts_with("````rust\n```\n🦀\n````"));
        assert_eq!(Document::parse(&serialized).text(), "```\n🦀");
    }

    #[test]
    fn unsupported_crossing_marks_are_rejected_atomically() {
        let mut document = Document::parse("abcdef");
        document.toggle_mark(0..4, MarkKind::Bold).unwrap();
        let before = document.to_markdown();
        assert_eq!(document.toggle_mark(2..6, MarkKind::Italic), Err(DocumentError::UnrepresentableEdit));
        assert_eq!(document.to_markdown(), before);
        assert_eq!(Document::parse(&document.to_markdown()).text(), "abcdef");
        document.toggle_mark(0..4, MarkKind::Bold).unwrap();
        assert!(!document.blocks()[0].marks.iter().any(|mark| mark.kind == MarkKind::Bold));
    }

    #[test]
    fn intraword_italic_preserves_text_and_semantic_mark() {
        let mut document = Document::parse("abcdef");
        document.toggle_mark(2..4, MarkKind::Italic).unwrap();
        let reparsed = Document::parse(&document.to_markdown());
        assert_eq!(reparsed.text(), "abcdef");
        assert_eq!(reparsed.blocks()[0].marks, vec![Mark { range: 2..4, kind: MarkKind::Italic }]);
    }

    #[test]
    fn nested_and_adjacent_marks_round_trip_semantically() {
        let mut nested = Document::parse("abcdef");
        nested.toggle_mark(0..6, MarkKind::Bold).unwrap();
        nested.toggle_mark(2..4, MarkKind::Italic).unwrap();
        assert_eq!(semantic_signature(&Document::parse(&nested.to_markdown()).state), semantic_signature(&nested.state));
        let mut adjacent = Document::parse("abcd");
        adjacent.toggle_mark(0..2, MarkKind::Bold).unwrap();
        adjacent.toggle_mark(2..4, MarkKind::Italic).unwrap();
        assert_eq!(semantic_signature(&Document::parse(&adjacent.to_markdown()).state), semantic_signature(&adjacent.state));
    }

    #[test]
    fn empty_paragraphs_remain_editable_spacers() {
        let mut document = Document::parse("");
        document.replace_range(0..0, "\n").unwrap();
        assert_eq!(document.text(), "\n");
        document.replace_range(1..1, "Next").unwrap();
        assert_eq!(document.text(), "\nNext");
        assert!(document.to_markdown().ends_with("Next"));
    }

    #[test]
    fn incremental_typing_preserves_edge_spaces_and_indentation() {
        let mut document = Document::parse("Hello");
        document.replace_range(5..5, " ").unwrap();
        assert_eq!(document.text(), "Hello ");
        document.replace_range(6..6, "world").unwrap();
        assert_eq!(document.text(), "Hello world");
        document.replace_range(0..0, "  \t").unwrap();
        document.replace_range(document.text().chars().count()..document.text().chars().count(), "  ").unwrap();
        assert_eq!(Document::parse(&document.to_markdown()).text(), "  \tHello world  ");
        let mut whitespace = Document::parse("");
        whitespace.replace_range(0..0, " ").unwrap();
        whitespace.replace_range(1..1, " ").unwrap();
        assert_eq!(Document::parse(&whitespace.to_markdown()).text(), "  ");
    }

    #[test]
    fn editing_bold_inline_code_preserves_both_marks() {
        let mut document = Document::parse("**A `code` example**");
        let end = document.text().chars().count();
        document.replace_range(end..end, "!").unwrap();
        assert_eq!(Document::parse(&document.to_markdown()).text(), "A code example!");
        assert!(document.blocks()[0].marks.iter().any(|mark| mark.kind == MarkKind::Code));
    }

    #[test]
    fn typing_after_bold_can_insert_spaces_and_continue_the_mark() {
        let mut document = Document::parse("**Bold**");
        document.replace_range(4..4, " ").unwrap();
        document.replace_range(5..5, "word").unwrap();
        assert_eq!(document.text(), "Bold word");
        let reparsed = Document::parse(&document.to_markdown());
        assert_eq!(semantic_signature(&reparsed.state), semantic_signature(&document.state));
        assert_eq!(document.blocks()[0].marks[0].range, 0..9);
    }

    #[test]
    fn newline_typing_continues_quotes_and_flat_tasks() {
        let mut quote = Document::parse("> Alpha");
        quote.replace_range(5..5, "\nBeta").unwrap();
        assert_eq!(quote.text(), "Alpha\nBeta");
        assert_eq!(quote.blocks().len(), 1);
        assert_eq!(Document::parse(&quote.to_markdown()).text(), quote.text());
        let mut task = Document::parse("- [x] Alpha");
        task.replace_range(5..5, "\nBeta").unwrap();
        assert_eq!(task.blocks()[0].kind, BlockKind::Task(true));
        assert_eq!(task.blocks()[1].kind, BlockKind::Task(false));
        assert_eq!(Document::parse(&task.to_markdown()).text(), task.text());
    }

    #[test]
    fn crlf_paste_normalizes_projection_without_changing_saved_newline_style() {
        let mut document = Document::parse("# Existing\r\n");
        document.replace_range(8..8, "\r\n🦀").unwrap();
        assert_eq!(document.text(), "Existing\n🦀");
        let source = document.to_markdown();
        assert!(source.contains("\r\n"));
        assert!(!source.replace("\r\n", "").contains('\n'));
        assert_eq!(Document::parse(&source).text(), document.text());
    }

    #[test]
    fn invalid_ranges_leave_document_unchanged() {
        let mut document = Document::parse("🦀");
        assert_eq!(document.replace_range(0..2, "x"), Err(DocumentError::InvalidRange));
        assert_eq!(document.text(), "🦀");
        assert_eq!(document.set_block_kind(0, BlockKind::Heading(0)), Err(DocumentError::InvalidHeading));
    }
}
