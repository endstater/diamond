// SPDX-License-Identifier: GPL-3.0-only
//! Accept the common MathJax shorthand without rewriting the stored Markdown.
pub fn normalize(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(at) = rest.find(r"\array{") {
        output.push_str(&rest[..at]);
        let start = at + 7;
        let Some(end) = group_end(rest, start) else {
            output.push_str(&rest[at..]);
            return output;
        };
        let body = &rest[start..end];
        let mut consumed = end + 1;
        let (alignment, body) = if !body.is_empty()
            && body
                .chars()
                .all(|c| matches!(c, 'l' | 'c' | 'r' | '|' | ' '))
            && rest
                .get(consumed..)
                .is_some_and(|tail| tail.starts_with('{'))
        {
            let next = consumed + 1;
            if let Some(last) = group_end(rest, next) {
                consumed = last + 1;
                (body.to_owned(), &rest[next..last])
            } else {
                output.push_str(&rest[at..]);
                return output;
            }
        } else {
            let columns = body
                .split(r"\\")
                .map(|row| {
                    let mut depth = 0usize;
                    let mut columns = 1usize;
                    for c in row.chars() {
                        match c {
                            '{' => depth += 1,
                            '}' => depth = depth.saturating_sub(1),
                            '&' if depth == 0 => columns += 1,
                            _ => {}
                        }
                    }
                    columns
                })
                .max()
                .unwrap_or(1);
            ("c".repeat(columns.min(64)), body)
        };
        output.push_str(r"\begin{array}{");
        output.push_str(&alignment);
        output.push('}');
        output.push_str(&normalize(body));
        output.push_str(r"\end{array}");
        rest = &rest[consumed..];
    }
    output.push_str(rest);
    output
}
fn group_end(input: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut escaped = false;
    for (at, c) in input[start..].char_indices() {
        if !escaped {
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => return Some(start + at),
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        escaped = c == '\\' && !escaped;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrays_keep_groups_unicode_and_explicit_alignment() {
        assert_eq!(
            normalize(r"\array{a & \frac{я}{2} \\ b & c}"),
            r"\begin{array}{cc}a & \frac{я}{2} \\ b & c\end{array}"
        );
        assert_eq!(
            normalize(r"\array{lr}{я & b}"),
            r"\begin{array}{lr}я & b\end{array}"
        );
        assert_eq!(normalize(r"\array{unfinished"), r"\array{unfinished");
    }
}
