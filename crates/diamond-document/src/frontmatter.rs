// SPDX-License-Identifier: GPL-3.0-only
//! Cached YAML metadata; editing uses ordinary lossless source transactions.
use crate::Document;
use serde_yaml_ng::{Mapping, Value};
use std::ops::Range;
use std::sync::Arc;

const MAX_YAML_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Text(String),
    Number(String),
    Date(String),
    Bool(bool),
    List(Vec<String>),
    Other(String),
}
impl PropertyValue {
    pub fn text(&self) -> String {
        match self {
            Self::Text(v) | Self::Number(v) | Self::Date(v) | Self::Other(v) => v.clone(),
            Self::Bool(v) => v.to_string(),
            Self::List(v) => v.join(", "),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: PropertyValue,
}
#[derive(Clone, Debug)]
pub struct Frontmatter {
    pub source_range: Range<usize>,
    pub properties: Vec<Property>,
    pub error: Option<String>,
    original: String,
}
impl Frontmatter {
    pub fn tags(&self) -> Vec<String> {
        self.properties
            .iter()
            .filter(|p| p.name == "tags")
            .flat_map(|p| match &p.value {
                PropertyValue::List(values) => values.clone(),
                PropertyValue::Text(value) => value.split([',', ' ']).map(str::to_owned).collect(),
                _ => Vec::new(),
            })
            .filter_map(|tag| crate::tags::normalize(&tag))
            .collect()
    }
}

pub fn valid_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
    {
        return false;
    }
    let year = text[..4].parse::<u32>().unwrap_or(0);
    let month = text[5..7].parse::<usize>().unwrap_or(0);
    let day = text[8..].parse::<u32>().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        0,
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year > 0 && month > 0 && month <= 12 && day > 0 && day <= days[month]
}

fn property_value(value: &Value) -> PropertyValue {
    match value {
        Value::Null => PropertyValue::Text(String::new()),
        Value::String(v) if valid_date(v) => PropertyValue::Date(v.clone()),
        Value::String(v) => PropertyValue::Text(v.clone()),
        Value::Bool(v) => PropertyValue::Bool(*v),
        Value::Number(v) => PropertyValue::Number(v.to_string()),
        Value::Sequence(v) if v.iter().all(|v| v.as_str().is_some()) => PropertyValue::List(
            v.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
        ),
        _ => PropertyValue::Other(serde_yaml_ng::to_string(value).unwrap_or_default()),
    }
}

pub(super) fn parse(source: &str) -> Option<Frontmatter> {
    let start = usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    let first = source[start..].split_inclusive('\n').next()?;
    if first.trim_end_matches(['\r', '\n']) != "---" || !first.ends_with('\n') {
        return None;
    }
    let yaml_start = start + first.len();
    let mut cursor = yaml_start;
    let mut closing = None;
    for line in source[yaml_start..].split_inclusive('\n') {
        if cursor - yaml_start > MAX_YAML_BYTES {
            break;
        }
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            closing = Some((cursor, cursor + line.len()));
            break;
        }
        cursor += line.len();
    }
    let mut result = Frontmatter {
        source_range: 0..0,
        properties: Vec::new(),
        error: None,
        original: String::new(),
    };
    let Some((yaml_end, end)) = closing else {
        result.error = Some("Unclosed or oversized YAML frontmatter; edit it in Source.".into());
        return Some(result);
    };
    result.original = source[..end].to_owned();
    result.source_range = source[..start].chars().count()..source[..end].chars().count();
    let mapping = match serde_yaml_ng::from_str::<Value>(&source[yaml_start..yaml_end]) {
        Ok(Value::Mapping(map)) => map,
        Ok(Value::Null) => Mapping::new(),
        Ok(_) => {
            result.error = Some("Frontmatter must contain a YAML mapping.".into());
            Mapping::new()
        }
        Err(error) => {
            result.error = Some(error.to_string());
            Mapping::new()
        }
    };
    for (key, value) in &mapping {
        let Some(name) = key.as_str() else {
            result.error = Some("Property names must be strings; edit this YAML in Source.".into());
            break;
        };
        result.properties.push(Property {
            name: name.into(),
            value: property_value(value),
        });
    }
    Some(result)
}

impl Document {
    pub fn frontmatter(&self) -> Option<Arc<Frontmatter>> {
        if let Some((revision, value)) = self.cached_frontmatter.borrow().as_ref() {
            if *revision == self.revision() {
                return value.clone();
            }
        }
        let source = self.source_map_shared();
        let previous = self
            .cached_frontmatter
            .borrow()
            .as_ref()
            .and_then(|(_, header)| header.clone());
        let value = if let Some(header) = previous
            .filter(|header| header.error.is_none() && source.source.starts_with(&header.original))
        {
            Some(header)
        } else {
            parse(&source.source).map(Arc::new)
        };
        *self.cached_frontmatter.borrow_mut() = Some((self.revision(), value.clone()));
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn replace(doc: &mut Document, old: &str, new: &str) {
        let source = doc.source_map_shared();
        let byte = source.source.find(old).unwrap();
        let start = source.source[..byte].chars().count();
        doc.replace_source_range(start..start + old.chars().count(), new)
            .unwrap();
    }
    #[test]
    fn typed_yaml_edits_keep_comments_crlf_and_undo() {
        let input = "---\r\n# keep this comment\r\ntags: [матан, двойной-интеграл]\r\nвариант: 19 # keep this too\r\nдедлайн: 2026-11-16\r\nстатус: решение готово\r\nstatus: Todo\r\n---\r\n\r\n# Тело\r\n";
        let mut doc = Document::parse(input);
        let header = doc.frontmatter().unwrap();
        assert_eq!(header.tags(), ["матан", "двойной-интеграл"]);
        assert_eq!(header.properties.len(), 5);
        replace(&mut doc, "19", "20");
        assert_eq!(doc.to_markdown(), input.replacen("19", "20", 1));
        assert!(doc.undo());
        assert_eq!(doc.to_markdown(), input);
        assert!(doc.redo());
        assert_eq!(doc.to_markdown(), input.replacen("19", "20", 1));
    }
    #[test]
    fn typed_header_creation_keeps_bom() {
        let mut doc = Document::parse("\u{feff}# Body\n");
        doc.replace_source_range(1..1, "---\nstatus: 'true: #tag'\n---\n\n")
            .unwrap();
        assert_eq!(
            doc.frontmatter().unwrap().properties[0].value,
            PropertyValue::Text("true: #tag".into())
        );
        assert!(doc.to_markdown().starts_with("\u{feff}---\n"));
        doc.undo();
        assert_eq!(doc.to_markdown(), "\u{feff}# Body\n");
    }
    #[test]
    fn malformed_and_complex_yaml_remain_original_source() {
        for input in [
            "---\nx: [\n---\nBody",
            "---\nx: 1\nBody",
            "---\n- item\n---\n",
        ] {
            let doc = Document::parse(input);
            assert!(doc.frontmatter().unwrap().error.is_some());
            assert_eq!(doc.to_markdown(), input);
        }
        let input = "---\n# comment\ncomplex: {a: 1}\nstatus: Todo\n---\nBody";
        let mut doc = Document::parse(input);
        replace(&mut doc, "Todo", "Done");
        assert_eq!(doc.to_markdown(), input.replace("Todo", "Done"));
        assert!(matches!(
            doc.frontmatter().unwrap().properties[0].value,
            PropertyValue::Other(_)
        ));
    }
    #[test]
    fn body_edits_reuse_the_same_yaml_snapshot() {
        let mut doc = Document::parse("---\ntags: [math]\n---\nBody\n");
        let header = doc.frontmatter().unwrap();
        let end = doc.source_map_shared().source_scalar_len;
        doc.replace_source_range(end..end, "more\n").unwrap();
        assert!(Arc::ptr_eq(&header, &doc.frontmatter().unwrap()));
    }
    #[test]
    fn date_validation_handles_leap_years() {
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2026-02-29"));
        assert!(!valid_date("2026-13-01"));
        assert!(!valid_date("２０２６-11-16"));
    }
}
