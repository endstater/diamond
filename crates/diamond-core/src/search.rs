//! Shared bounded regex compilation; no UI or filesystem policy.
use regex::{NoExpand, Regex, RegexBuilder};
use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub regex: bool,
}

#[derive(Clone, Debug)]
pub struct SearchPattern {
    expression: Regex,
    regex: bool,
}

impl SearchPattern {
    pub fn compile(query: &str, options: &SearchOptions) -> Result<Self, String> {
        if query.len() > 16 * 1024 {
            return Err("Слишком длинный поисковый запрос".into());
        }
        let source = if options.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        let expression = RegexBuilder::new(&source)
            .case_insensitive(!options.case_sensitive)
            .multi_line(true)
            .size_limit(2 * 1024 * 1024)
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            expression,
            regex: options.regex,
        })
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.expression.is_match(text)
    }

    pub fn byte_ranges(&self, text: &str) -> Vec<Range<usize>> {
        self.expression
            .find_iter(text)
            .take(10_000)
            .map(|found| found.range())
            .collect()
    }

    pub fn scalar_ranges(&self, text: &str) -> Vec<Range<usize>> {
        let mut boundaries: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
        boundaries.push(text.len());
        self.byte_ranges(text)
            .into_iter()
            .map(|range| {
                boundaries
                    .binary_search(&range.start)
                    .expect("regex UTF-8 boundary")
                    ..boundaries
                        .binary_search(&range.end)
                        .expect("regex UTF-8 boundary")
            })
            .collect()
    }

    pub fn replace_all(&self, text: &str, replacement: &str) -> String {
        if self.regex {
            self.expression.replace_all(text, replacement).into_owned()
        } else {
            self.expression
                .replace_all(text, NoExpand(replacement))
                .into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_case_and_scalar_positions() {
        let pattern = SearchPattern::compile("Я", &SearchOptions::default()).unwrap();
        assert_eq!(pattern.scalar_ranges("🙂яЯ"), vec![1..2, 2..3]);
        let options = SearchOptions {
            case_sensitive: true,
            regex: false,
        };
        assert_eq!(
            SearchPattern::compile("Я", &options)
                .unwrap()
                .scalar_ranges("🙂яЯ"),
            vec![2..3]
        );
    }
    #[test]
    fn regex_captures_and_literal_replacements_are_distinct() {
        let pattern = SearchPattern::compile(
            r"(\d+)",
            &SearchOptions {
                case_sensitive: true,
                regex: true,
            },
        )
        .unwrap();
        assert_eq!(pattern.replace_all("a12", "[$1]"), "a[12]");
        let literal = SearchPattern::compile("a", &SearchOptions::default()).unwrap();
        assert_eq!(literal.replace_all("a", "$1"), "$1");
        assert!(
            SearchPattern::compile(
                "[",
                &SearchOptions {
                    case_sensitive: false,
                    regex: true
                }
            )
            .is_err()
        );
    }
}
