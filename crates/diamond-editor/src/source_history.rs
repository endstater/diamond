// SPDX-License-Identifier: GPL-3.0-only
//! Source Undo stores changed fragments, never a document per keystroke.
use egui::TextBuffer;
use std::{collections::VecDeque, ops::Range};
const MAX_TRANSACTIONS: usize = 256;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}
impl Edit {
    fn cost(&self) -> usize {
        self.removed.len() + self.inserted.len()
    }
}
struct Transaction {
    edits: Vec<Edit>,
    before: Range<usize>,
    after: Range<usize>,
}
impl Transaction {
    fn cost(&self) -> usize {
        self.edits.iter().map(Edit::cost).sum()
    }
}
#[derive(Default)]
pub(super) struct History {
    undo: VecDeque<Transaction>,
    redo: Vec<Transaction>,
    bytes: usize,
    composition: Option<(Range<usize>, Vec<Edit>)>,
}
impl History {
    pub fn begin_composition(&mut self, selection: Range<usize>) {
        self.composition
            .get_or_insert_with(|| (selection, Vec::new()));
    }
    pub fn cancel_composition(&mut self) {
        self.composition = None;
    }
    pub fn commit_composition(&mut self, selection: Range<usize>) {
        if let Some((before, edits)) = self.composition.take() {
            self.record(edits, before, selection);
        }
    }
    pub fn record(&mut self, edits: Vec<Edit>, before: Range<usize>, after: Range<usize>) {
        if edits.is_empty() {
            return;
        }
        if let Some((_, pending)) = &mut self.composition {
            pending.extend(edits);
            return;
        }
        self.bytes -= self.redo.iter().map(Transaction::cost).sum::<usize>();
        self.redo.clear();
        let transaction = Transaction {
            edits,
            before,
            after,
        };
        self.bytes += transaction.cost();
        self.undo.push_back(transaction);
        while self.undo.len() > MAX_TRANSACTIONS || self.bytes > MAX_BYTES {
            if let Some(old) = self.undo.pop_front() {
                self.bytes -= old.cost();
            } else {
                break;
            }
        }
    }
    pub fn undo(&mut self, source: &mut String) -> Option<Range<usize>> {
        let transaction = self.undo.pop_back()?;
        // Validate in a temporary only on explicit Undo, never on native typing.
        let mut next = source.clone();
        for edit in transaction.edits.iter().rev() {
            if !apply(&mut next, edit.at, &edit.inserted, &edit.removed) {
                *self = Self::default();
                return None;
            }
        }
        *source = next;
        let selection = transaction.before.clone();
        self.redo.push(transaction);
        Some(selection)
    }
    pub fn redo(&mut self, source: &mut String) -> Option<Range<usize>> {
        let transaction = self.redo.pop()?;
        let mut next = source.clone();
        for edit in &transaction.edits {
            if !apply(&mut next, edit.at, &edit.removed, &edit.inserted) {
                *self = Self::default();
                return None;
            }
        }
        *source = next;
        let selection = transaction.after.clone();
        self.undo.push_back(transaction);
        Some(selection)
    }
}
fn apply(source: &mut String, at: usize, expected: &str, replacement: &str) -> bool {
    let Some(end) = at.checked_add(expected.len()) else {
        return false;
    };
    if source.get(at..end) != Some(expected) {
        return false;
    }
    source.replace_range(at..end, replacement);
    true
}
fn byte(text: &str, scalar: usize) -> usize {
    text.char_indices()
        .nth(scalar)
        .map_or(text.len(), |(at, _)| at)
}
pub(super) fn replace(source: &mut String, range: Range<usize>, text: &str, edits: &mut Vec<Edit>) {
    if source.get(range.clone()) == Some(text) {
        return;
    }
    let removed = source[range.clone()].to_owned();
    edits.push(Edit {
        at: range.start,
        removed,
        inserted: text.into(),
    });
    source.replace_range(range, text);
}
pub(super) fn difference(before: &str, after: &str) -> Vec<Edit> {
    if before == after {
        return Vec::new();
    }
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    vec![Edit {
        at: prefix,
        removed: before[prefix..before.len() - suffix].into(),
        inserted: after[prefix..after.len() - suffix].into(),
    }]
}
pub(super) struct Buffer<'a> {
    pub source: &'a mut String,
    pub edits: &'a mut Vec<Edit>,
}
impl TextBuffer for Buffer<'_> {
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Buffer<'static>>()
    }
    fn as_str(&self) -> &str {
        self.source
    }
    fn is_mutable(&self) -> bool {
        true
    }
    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let at = byte(self.source, char_index);
        replace(self.source, at..at, text, self.edits);
        text.chars().count()
    }
    fn delete_char_range(&mut self, range: Range<usize>) {
        let start = byte(self.source, range.start);
        let end = byte(self.source, range.end);
        if start <= end {
            replace(self.source, start..end, "", self.edits);
        }
    }
    fn replace_with(&mut self, text: &str) {
        self.edits.extend(difference(self.source, text));
        self.source.clear();
        self.source.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_replacement_undo_redo_and_branching_preserve_bytes() {
        let mut source = "я🙂\r\n你好".to_owned();
        let original = source.clone();
        let mut history = History::default();
        let mut edits = Vec::new();
        let mut buffer = Buffer {
            source: &mut source,
            edits: &mut edits,
        };
        buffer.delete_char_range(1..2);
        buffer.insert_text("мир", 1);
        history.record(edits, 1..2, 4..4);
        let changed = source.clone();
        assert_eq!(history.undo(&mut source), Some(1..2));
        assert_eq!(source, original);
        assert_eq!(history.redo(&mut source), Some(4..4));
        assert_eq!(source, changed);
        history.undo(&mut source);
        let mut edits = Vec::new();
        replace(&mut source, 0..0, "!", &mut edits);
        history.record(edits, 0..0, 1..1);
        assert!(history.redo(&mut source).is_none());
    }
    #[test]
    fn typing_history_cost_depends_on_edits_not_nine_hundred_line_source() {
        let mut source = "let x=1;\n".repeat(900);
        let mut history = History::default();
        for _ in 0..200 {
            let at = source.len();
            let mut edits = Vec::new();
            replace(&mut source, at..at, "a", &mut edits);
            history.record(edits, at..at, at + 1..at + 1);
        }
        assert_eq!(history.bytes, 200);
        assert_eq!(history.undo.len(), 200);
        for _ in 0..200 {
            assert!(history.undo(&mut source).is_some());
        }
        assert_eq!(source, "let x=1;\n".repeat(900));
    }
    #[test]
    fn ime_is_one_transaction_and_cancel_preserves_earlier_undo() {
        let mut source = "a".to_owned();
        let mut history = History::default();
        history.begin_composition(1..1);
        for next in ["a你", "a你好"] {
            let edits = difference(&source, next);
            source = next.into();
            history.record(edits, 1..1, source.chars().count()..source.chars().count());
        }
        history.commit_composition(3..3);
        assert_eq!(history.undo.len(), 1);
        history.undo(&mut source);
        assert_eq!(source, "a");
        history.redo(&mut source);
        assert_eq!(source, "a你好");
        history.begin_composition(3..3);
        history.cancel_composition();
        history.undo(&mut source);
        assert_eq!(source, "a");
    }
    #[test]
    fn memory_and_transaction_limits_include_redo_and_refuse_stale_edits() {
        let mut source = String::new();
        let mut history = History::default();
        for _ in 0..300 {
            let at = source.len();
            let mut edits = Vec::new();
            replace(&mut source, at..at, "x", &mut edits);
            history.record(edits, at..at, at + 1..at + 1);
        }
        assert_eq!(history.undo.len(), 256);
        assert_eq!(history.bytes, 256);
        source.clear();
        assert!(history.undo(&mut source).is_none());
        assert_eq!(source, "");
    }
}
