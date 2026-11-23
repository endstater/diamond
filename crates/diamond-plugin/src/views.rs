// SPDX-License-Identifier: GPL-3.0-only
//! Protocol v2: host-rendered document tabs, split previews and source anchors.
use crate::{PluginError, valid_id, valid_title};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDescriptor {
    pub id: String,
    pub title: String,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub editable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDocument {
    pub view: String,
    pub document_id: String,
    pub revision: u64,
    /// Only the explicitly opened file is disclosed, never the vault root.
    pub file_path: String,
    /// Present only with document_read. Positions count Unicode scalar values.
    pub text: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewPage {
    pub id: String,
    pub png_base64: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAnchor {
    pub offset: usize,
    pub page: usize,
    pub x: f32,
    pub y: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub message: String,
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewFrame {
    pub revision: u64,
    #[serde(default)]
    pub first_page: usize,
    #[serde(default)]
    pub page_count: Option<usize>,
    #[serde(default)]
    pub pages: Vec<PreviewPage>,
    #[serde(default)]
    pub anchors: Vec<SourceAnchor>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    /// A plugin may request navigation; it cannot mutate the editor this way.
    #[serde(default)]
    pub cursor: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ViewEvent {
    Click {
        page: usize,
        x: f32,
        y: f32,
    },
    Scroll {
        page: usize,
        y: f32,
    },
    Cursor {
        offset: usize,
    },
    Viewport {
        width: u32,
        height: u32,
    },
    RequestPages {
        start: usize,
        count: usize,
        scale: f32,
    },
    Close,
}
impl ViewDescriptor {
    pub fn validate(&self) -> Result<(), PluginError> {
        if !valid_id(&self.id)
            || !valid_title(&self.title)
            || self.extensions.is_empty()
            || self.extensions.len() > 32
            || self.extensions.iter().any(|extension| {
                extension.len() > 24
                    || extension.is_empty()
                    || !extension
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'+')
            })
        {
            return Err(PluginError::Protocol("Invalid view descriptor".into()));
        }
        Ok(())
    }
}
impl ViewFrame {
    pub fn validate(&self) -> Result<(), PluginError> {
        let page_count = self.page_count.unwrap_or(self.pages.len());
        if page_count > 100000
            || self
                .first_page
                .checked_add(self.pages.len())
                .is_none_or(|end| end > page_count)
            || self.pages.len() > 16
            || self.anchors.len() > 8192
            || self.diagnostics.len() > 256
            || self
                .pages
                .iter()
                .any(|page| !valid_id(&page.id) || page.png_base64.len() > 768 * 1024)
            || self.anchors.iter().any(|anchor| {
                anchor.page >= page_count
                    || !anchor.x.is_finite()
                    || !anchor.y.is_finite()
                    || !(0.0..=1.0).contains(&anchor.x)
                    || !(0.0..=1.0).contains(&anchor.y)
            })
            || self
                .diagnostics
                .iter()
                .any(|item| item.start > item.end || item.message.len() > 4096)
        {
            return Err(PluginError::Protocol(
                "Invalid or excessive preview data".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_page_indexes_and_nonfinite_anchors_are_rejected() {
        let mut frame = ViewFrame {
            revision: 1,
            page_count: Some(1),
            anchors: vec![SourceAnchor {
                offset: 0,
                page: 0,
                x: f32::NAN,
                y: 0.0,
            }],
            ..Default::default()
        };
        assert!(frame.validate().is_err());
        frame.anchors[0].x = 0.5;
        assert!(frame.validate().is_ok());
        frame.anchors[0].page = 1;
        assert!(frame.validate().is_err());
        frame.anchors.clear();
        frame.first_page = usize::MAX;
        frame.pages.push(PreviewPage {
            id: "p".into(),
            png_base64: String::new(),
        });
        assert!(frame.validate().is_err());
    }
}
