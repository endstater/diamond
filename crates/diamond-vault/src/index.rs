use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingMetadata {
    pub level: u8,
    pub text: String,
    /// One-based source line.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMetadata {
    pub text: String,
    pub checked: bool,
    /// One-based source line.
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct NoteMetadata {
    pub path: PathBuf,
    pub title: String,
    pub headings: Vec<HeadingMetadata>,
    pub tags: Vec<String>,
    /// Wikilink targets without aliases, block references or heading fragments.
    pub links: Vec<String>,
    pub tasks: Vec<TaskMetadata>,
    /// Original UTF-8 source, used by the initial full-text search.
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct VaultIndex {
    pub notes: Vec<NoteMetadata>,
    /// Non-UTF-8 or oversized Markdown files excluded from the index.
    pub skipped: Vec<PathBuf>,
}

impl VaultIndex {
    /// Case-insensitive AND search across paths, titles, tags and note source.
    /// Empty queries return all notes in deterministic vault order.
    pub fn search(&self, query: &str) -> Vec<&NoteMetadata> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if terms.is_empty() {
            return self.notes.iter().collect();
        }
        self.notes
            .iter()
            .filter(|note| {
                let haystack = format!(
                    "{}\n{}\n{}\n{}",
                    note.path.to_string_lossy(),
                    note.title,
                    note.tags.join(" "),
                    note.text
                )
                .to_lowercase();
                terms.iter().all(|term| haystack.contains(term))
            })
            .collect()
    }

    pub fn note(&self, path: &Path) -> Option<&NoteMetadata> {
        self.notes.iter().find(|note| note.path == path)
    }

    /// Return referring notes, resolving ambiguous bare stems only when unique.
    pub fn backlinks(&self, target: &Path) -> Vec<&NoteMetadata> {
        self.notes
            .iter()
            .filter(|note| {
                note.links.iter().any(|link| {
                    self.resolve_link(&note.path, link)
                        .is_some_and(|p| p == target)
                })
            })
            .collect()
    }

    pub fn resolve_link(&self, source: &Path, link: &str) -> Option<PathBuf> {
        let link = link.trim();
        if link.is_empty() || link.contains(':') || link.starts_with('/') {
            return None;
        }
        let mut candidate = PathBuf::from(link);
        if candidate.extension().is_none() {
            candidate.set_extension("md");
        }
        let sibling = source.parent().unwrap_or_else(|| Path::new(""));
        for candidate in [sibling.join(&candidate), candidate.clone()] {
            if let Some(candidate) = normalize_link_path(&candidate)
                && self.note(&candidate).is_some()
            {
                return Some(candidate);
            }
        }
        if Path::new(link).components().count() == 1 {
            let target_stem = Path::new(link).file_stem()?;
            let mut matching = self.notes.iter().filter(|note| {
                note.path
                    .file_stem()
                    .is_some_and(|stem| stem == target_stem)
            });
            let first = matching.next()?;
            if matching.next().is_none() {
                return Some(first.path.clone());
            }
        }
        None
    }
}

fn normalize_link_path(path: &Path) -> Option<PathBuf> {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => normal.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normal.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(normal)
}

impl NoteMetadata {
    pub(crate) fn parse(path: PathBuf, text: String) -> Self {
        let mut note = Self {
            title: path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_owned()),
            path,
            headings: Vec::new(),
            tags: Vec::new(),
            links: Vec::new(),
            tasks: Vec::new(),
            text,
        };
        let lines: Vec<&str> = note.text.lines().collect();
        let frontmatter_end = if lines.first().is_some_and(|line| line.trim() == "---") {
            lines
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, line)| matches!(line.trim(), "---" | "..."))
                .map(|(i, _)| i)
        } else {
            None
        };
        let mut fence: Option<(char, usize)> = None;
        let mut tags = BTreeSet::new();
        let mut links = BTreeSet::new();
        for (index, line) in lines.iter().enumerate() {
            if frontmatter_end.is_some_and(|end| index <= end) {
                continue;
            }
            let trimmed = line.trim_start_matches(' ');
            let indentation = line.len() - trimmed.len();
            if let Some((marker, count)) = fence {
                let closing_count = trimmed.chars().take_while(|c| *c == marker).count();
                if indentation <= 3
                    && closing_count >= count
                    && trimmed[closing_count..].trim().is_empty()
                {
                    fence = None;
                }
                continue;
            }
            if indentation >= 4 || line.starts_with('\t') {
                continue;
            }
            if let Some(marker) = trimmed.chars().next().filter(|c| matches!(c, '`' | '~')) {
                let count = trimmed.chars().take_while(|c| *c == marker).count();
                if count >= 3 {
                    fence = Some((marker, count));
                    continue;
                }
            }
            let heading_count = trimmed.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&heading_count)
                && trimmed[heading_count..].starts_with(char::is_whitespace)
            {
                let text = trimmed[heading_count..].trim();
                let text = strip_heading_closing(text);
                note.headings.push(HeadingMetadata {
                    level: heading_count as u8,
                    text: text.to_owned(),
                    line: index + 1,
                });
            }
            if let Some((checked, text)) = parse_task(trimmed) {
                note.tasks.push(TaskMetadata {
                    text: text.to_owned(),
                    checked,
                    line: index + 1,
                });
            }
            extract_inline_metadata(trimmed, &mut tags, &mut links);
        }
        if let Some(heading) = note
            .headings
            .iter()
            .find(|heading| heading.level == 1)
            .or_else(|| note.headings.first())
        {
            note.title.clone_from(&heading.text);
        }
        note.tags = tags.into_iter().collect();
        note.links = links.into_iter().collect();
        note
    }
}

fn strip_heading_closing(text: &str) -> &str {
    let stripped = text.trim_end_matches('#');
    if stripped.len() != text.len() && stripped.ends_with(char::is_whitespace) {
        stripped.trim_end()
    } else {
        text
    }
}

fn parse_task(line: &str) -> Option<(bool, &str)> {
    let after_marker = if let Some(after) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        after.trim_start()
    } else {
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        line[digits..]
            .strip_prefix(". ")
            .or_else(|| line[digits..].strip_prefix(") "))?
            .trim_start()
    };
    let checked = match after_marker.get(..3)? {
        "[ ]" => false,
        "[x]" | "[X]" => true,
        _ => return None,
    };
    let after = after_marker.get(3..)?;
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some((checked, after.trim_start()))
}

fn extract_inline_metadata(line: &str, tags: &mut BTreeSet<String>, links: &mut BTreeSet<String>) {
    let mut offset = 0;
    let mut previous: Option<char> = None;
    while offset < line.len() {
        let rest = &line[offset..];
        let character = rest.chars().next().expect("nonempty character boundary");
        if character == '\\' {
            offset += character.len_utf8();
            if let Some(escaped) = line[offset..].chars().next() {
                offset += escaped.len_utf8();
                previous = Some(escaped);
            }
            continue;
        }
        if character == '`' {
            let count = rest.chars().take_while(|c| *c == '`').count();
            let closing = "`".repeat(count);
            if let Some(end) = rest[count..].find(&closing) {
                offset += count + end + count;
                previous = Some('`');
                continue;
            }
            offset += count;
            previous = Some('`');
            continue;
        }
        if rest.starts_with("[[")
            && let Some(end) = rest[2..].find("]]")
        {
            let target = rest[2..2 + end]
                .split(['|', '#', '^'])
                .next()
                .unwrap_or_default()
                .trim();
            if !target.is_empty() {
                links.insert(target.to_owned());
            }
            offset += 2 + end + 2;
            previous = Some(']');
            continue;
        }
        if character == '#'
            && previous.is_none_or(|c| !c.is_alphanumeric() && !matches!(c, '_' | '/' | '#'))
        {
            let tag: String = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .collect();
            if tag
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            {
                let tag = tag.trim_end_matches('/');
                if !tag.is_empty() {
                    tags.insert(tag.to_owned());
                }
            }
        }
        offset += character.len_utf8();
        previous = Some(character);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_preserves_unicode_and_excludes_code_and_frontmatter() {
        let note = NoteMetadata::parse(
            PathBuf::from("notes/заметка.md"),
            "---\ntitle: #ignored [[Ignored]]\n---\n# Привет ###\n\n#проект #work/linux\n- [ ] Сделать\n1. [X] Готово\n[[Other#Section|label]] `#code [[Code]]`\n```rust\n# Not a heading\n- [x] Code task\n[[No]] #no\n```\n    # Also code\n\\#escaped \\[[Escaped]]\n".to_owned(),
        );
        assert_eq!(note.title, "Привет");
        assert_eq!(note.headings.len(), 1);
        assert_eq!(note.headings[0].line, 4);
        assert_eq!(note.tags, ["work/linux", "проект"]);
        assert_eq!(note.links, ["Other"]);
        assert_eq!(note.tasks.len(), 2);
        assert!(!note.tasks[0].checked);
        assert!(note.tasks[1].checked);
        assert_eq!(note.tasks[1].text, "Готово");
    }

    #[test]
    fn search_and_backlinks_resolve_relative_and_unique_targets() {
        let index = VaultIndex {
            notes: vec![
                NoteMetadata::parse(PathBuf::from("notes/a.md"), "# Alpha\n[[../b]]".into()),
                NoteMetadata::parse(PathBuf::from("b.md"), "# Бета\n#проект".into()),
                NoteMetadata::parse(PathBuf::from("other/c.md"), "[[Бета]] [[b|alias]]".into()),
            ],
            skipped: Vec::new(),
        };
        assert_eq!(index.search("БЕТА проект").len(), 1);
        assert_eq!(index.search("alpha missing").len(), 0);
        assert_eq!(index.backlinks(Path::new("b.md")).len(), 2);
        assert_eq!(index.resolve_link(Path::new("notes/a.md"), "../../b"), None);
    }

    #[test]
    fn ambiguous_bare_stems_are_not_arbitrarily_linked() {
        let index = VaultIndex {
            notes: vec![
                NoteMetadata::parse(PathBuf::from("one/a.md"), String::new()),
                NoteMetadata::parse(PathBuf::from("two/a.md"), String::new()),
                NoteMetadata::parse(PathBuf::from("origin.md"), "[[a]]".into()),
            ],
            skipped: Vec::new(),
        };
        assert_eq!(index.resolve_link(Path::new("origin.md"), "a"), None);
        assert!(index.backlinks(Path::new("one/a.md")).is_empty());
    }
}
