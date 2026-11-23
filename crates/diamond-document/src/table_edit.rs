// SPDX-License-Identifier: GPL-3.0-only
//! Table mutations preserve raw inline Markdown and replace only the target table.
use crate::{CellAlignment, Document};
#[derive(Clone, Copy, Debug)]
pub enum TableAction {
    InsertRowBefore,
    InsertRowAfter,
    DeleteRow,
    InsertColumnBefore,
    InsertColumnAfter,
    DeleteColumn,
    Align(CellAlignment),
}

fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let escaped_end = line
        .strip_suffix('|')
        .is_some_and(|body| body.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1);
    let line = if line.ends_with('|') && !escaped_end {
        &line[..line.len() - 1]
    } else {
        line
    };
    let mut result = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for c in line.chars() {
        if c == '|' && !escaped {
            result.push(cell.trim().into());
            cell.clear();
        } else {
            cell.push(c);
        }
        escaped = if c == '\\' { !escaped } else { false };
    }
    result.push(cell.trim().into());
    result
}

impl Document {
    pub fn edit_table(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
        action: TableAction,
    ) -> Result<(), String> {
        let block = self.blocks().get(index).ok_or("Table no longer exists")?;
        let table = block.table.as_ref().ok_or("Block is not a table")?;
        if table.columns.is_empty() || column >= table.columns.len() {
            return Err("Column no longer exists".into());
        }
        let map = self.source_map();
        let span = map
            .blocks
            .get(index)
            .ok_or("Table source is unavailable")?
            .source_range
            .clone();
        let raw: String = map
            .source
            .chars()
            .skip(span.start)
            .take(span.end - span.start)
            .collect();
        let ending = if raw.contains("\r\n") { "\r\n" } else { "\n" };
        let mut lines: Vec<Vec<String>> = raw.lines().map(cells).collect();
        if lines.len() < 2 {
            return Err("Table has no separator row".into());
        }
        let mut align = table.columns.clone();
        let count = align.len();
        for line in &mut lines {
            line.resize(count, String::new());
            line.truncate(count);
        }
        let actual = if row == 0 { 0 } else { row + 1 };
        if actual >= lines.len() {
            return Err("Row no longer exists".into());
        }
        match action {
            TableAction::InsertRowBefore => {
                lines.insert(actual.max(2), vec![String::new(); count]);
            }
            TableAction::InsertRowAfter => {
                lines.insert((actual + 1).max(2), vec![String::new(); count]);
            }
            TableAction::DeleteRow => {
                if row == 0 {
                    return Err("The header row cannot be removed".into());
                }
                lines.remove(actual);
            }
            TableAction::InsertColumnBefore | TableAction::InsertColumnAfter => {
                let at = column + usize::from(matches!(action, TableAction::InsertColumnAfter));
                for line in &mut lines {
                    line.insert(at, String::new());
                }
                align.insert(at, CellAlignment::Left);
            }
            TableAction::DeleteColumn => {
                if count == 1 {
                    return Err("A table needs at least one column".into());
                }
                for line in &mut lines {
                    line.remove(column);
                }
                align.remove(column);
            }
            TableAction::Align(value) => align[column] = value,
        }
        lines[1] = align
            .iter()
            .map(|alignment| {
                match alignment {
                    CellAlignment::Left => ":---",
                    CellAlignment::Center => ":---:",
                    CellAlignment::Right => "---:",
                }
                .into()
            })
            .collect();
        let mut replacement = lines
            .iter()
            .map(|line| format!("| {} |", line.join(" | ")))
            .collect::<Vec<_>>()
            .join(ending);
        if raw.ends_with('\n') {
            replacement.push_str(ending);
        }
        self.replace_source_range(span, &replacement)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_preserves_inline_source_outside_and_undo() {
        let input = "before **exact**\n\n| 名字 | Link |\n| --- | --- |\n| я | [a](https://example.com) \\| pipe |\n\nafter\n";
        let mut document = Document::parse(input);
        let index = document
            .blocks()
            .iter()
            .position(|block| block.table.is_some())
            .unwrap();
        document
            .edit_table(index, 1, 1, TableAction::Align(CellAlignment::Right))
            .unwrap();
        let output = document.to_markdown();
        assert!(output.starts_with("before **exact**\n\n"));
        assert!(output.ends_with("\nafter\n"));
        assert!(output.contains("[a](https://example.com) \\| pipe"));
        assert!(output.contains("---:"));
        assert!(document.undo());
        assert_eq!(document.to_markdown(), input);
        document
            .edit_table(index, 1, 0, TableAction::InsertRowAfter)
            .unwrap();
        assert_eq!(
            document.blocks()[index].table.as_ref().unwrap().cells.len(),
            6
        );
        document
            .edit_table(index, 1, 0, TableAction::DeleteColumn)
            .unwrap();
        assert_eq!(
            document.blocks()[index]
                .table
                .as_ref()
                .unwrap()
                .columns
                .len(),
            1
        );
    }
}
