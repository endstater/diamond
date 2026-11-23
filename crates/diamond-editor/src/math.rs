// SPDX-License-Identifier: GPL-3.0-only
//! Cached native mathematical LaTeX. Only the embedded STIX math font is read.
use egui::{Color32, Context, TextureHandle, Vec2};
use latex_rust::{Dim, MathFont, SvgOptions};
use std::sync::OnceLock;
static FONT: OnceLock<Result<MathFont, String>> = OnceLock::new();
#[derive(Clone)]
pub struct Formula {
    pub texture: Option<TextureHandle>,
    pub size: Vec2,
    pub error: Option<String>,
}
#[derive(Clone)]
struct Entry {
    latex: String,
    display: bool,
    formula: Formula,
}
#[derive(Clone, Default)]
struct Cache {
    entries: Vec<Entry>,
    bytes: usize,
}

pub fn formula(ctx: &Context, latex: &str, display: bool) -> Formula {
    let id = egui::Id::new("diamond-math-cache");
    if let Some(hit) = ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<Cache>(id);
        let at = cache
            .entries
            .iter()
            .position(|entry| entry.latex == latex && entry.display == display)?;
        let hit = cache.entries.remove(at);
        let formula = hit.formula.clone();
        cache.entries.push(hit);
        Some(formula)
    }) {
        return hit;
    }
    let result = render(ctx, latex, display).unwrap_or_else(|error| Formula {
        texture: None,
        size: egui::vec2(22.0, 20.0),
        error: Some(error),
    });
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<Cache>(id);
        let cost = formula_bytes(&result) + latex.len();
        while !cache.entries.is_empty()
            && (cache.entries.len() >= 128 || cache.bytes + cost > 64 * 1024 * 1024)
        {
            let removed = cache.entries.remove(0);
            cache.bytes = cache
                .bytes
                .saturating_sub(formula_bytes(&removed.formula) + removed.latex.len());
        }
        cache.bytes += cost;
        cache.entries.push(Entry {
            latex: latex.into(),
            display,
            formula: result.clone(),
        });
    });
    result
}
fn formula_bytes(formula: &Formula) -> usize {
    formula.texture.as_ref().map_or(0, |texture| {
        let [w, h] = texture.size();
        w * h * 4
    })
}
fn render(ctx: &Context, latex: &str, display: bool) -> Result<Formula, String> {
    if latex.len() > 8192 {
        return Err("Formula exceeds the 8192-byte limit".into());
    }
    let mut depth = 0usize;
    for byte in latex.bytes() {
        if byte == b'{' {
            depth += 1;
            if depth > 64 {
                return Err("Formula nesting exceeds 64 groups".into());
            }
        } else if byte == b'}' {
            depth = depth.saturating_sub(1);
        }
    }
    let font = FONT
        .get_or_init(|| MathFont::stix_two_math().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(Clone::clone)?;
    let options = SvgOptions {
        font_size_pt: Dim::from_i64(12),
        color: latex_rust::Color::rgb(255, 255, 255),
        display,
    };
    let normalized = super::math_normalize::normalize(latex);
    let inputs = if let Some((left, right)) = system_parts(&normalized) {
        vec![left, right]
    } else {
        vec![normalized.as_str()]
    };
    let mut trees = Vec::new();
    for input in inputs {
        let svg =
            latex_rust::latex_to_svg(input, font, &options).map_err(|error| error.to_string())?;
        trees.push(
            resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
                .map_err(|error| error.to_string())?,
        );
    }
    let gap = if trees.len() > 1 { 6.0 } else { 0.0 };
    let size = egui::vec2(
        trees.iter().map(|tree| tree.size().width()).sum::<f32>() + gap,
        trees
            .iter()
            .map(|tree| tree.size().height())
            .fold(0.0_f32, f32::max),
    );
    if !(size.x > 0.0 && size.y > 0.0) || size.x > 4096.0 || size.y > 2048.0 {
        return Err("Formula is too large to render".into());
    }
    let scale = 2.0_f32.min(2048.0 / size.x).min(1024.0 / size.y);
    let width = (size.x * scale).ceil().max(1.0) as u32;
    let height = (size.y * scale).ceil().max(1.0) as u32;
    let image = if trees.len() == 1 {
        let mut pixels =
            resvg::tiny_skia::Pixmap::new(width, height).ok_or("Cannot allocate formula image")?;
        resvg::render(
            &trees[0],
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixels.as_mut(),
        );
        egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], pixels.data())
    } else {
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        let mut left = 0.0_f32;
        for tree in &trees {
            let w = (tree.size().width() * scale).ceil().max(1.0) as u32;
            let h = (tree.size().height() * scale).ceil().max(1.0) as u32;
            let mut pixels =
                resvg::tiny_skia::Pixmap::new(w, h).ok_or("Cannot allocate formula image")?;
            resvg::render(
                tree,
                resvg::tiny_skia::Transform::from_scale(scale, scale),
                &mut pixels.as_mut(),
            );
            let x = (left * scale).round() as usize;
            if x >= width as usize {
                return Err("Invalid formula box position".into());
            }
            let y = (height as usize).saturating_sub(h as usize) / 2;
            let copy_width = (w as usize).min((width as usize).saturating_sub(x));
            for row in 0..h as usize {
                if y + row >= height as usize {
                    break;
                }
                let destination = ((y + row) * width as usize + x) * 4;
                let source = row * w as usize * 4;
                rgba[destination..destination + copy_width * 4]
                    .copy_from_slice(&pixels.data()[source..source + copy_width * 4]);
            }
            left += tree.size().width() + gap;
        }
        egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], &rgba)
    };
    let texture = ctx.load_texture("latex-math", image, egui::TextureOptions::LINEAR);
    Ok(Formula {
        texture: Some(texture),
        size,
        error: None,
    })
}
// Center a complete braced array and its following top-level relation as
// separate boxes. This avoids the engine's first-row baseline for this form;
// the source and ordinary inline expressions are left unchanged.
fn system_parts(latex: &str) -> Option<(&str, &str)> {
    let text = latex.trim();
    if !text.starts_with(r"\left.\begin{array}") {
        return None;
    }
    let end = text.rfind(r"\end{array}")? + r"\end{array}".len();
    let tail = text[end..].trim_start();
    let tail = tail.strip_prefix(r"\right\}")?;
    let right = tail.trim_start();
    if ![r"\Rightarrow", r"\Longrightarrow", r"\implies", "="]
        .iter()
        .any(|relation| right.starts_with(relation))
    {
        return None;
    }
    let split = text.len() - tail.len();
    Some((&text[..split], right))
}

pub fn paint(ui: &mut egui::Ui, formula: &Formula, rect: egui::Rect, color: Color32) {
    if let Some(texture) = &formula.texture {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            color,
        );
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "◇",
            egui::FontId::proportional(18.0),
            color,
        );
    }
    if let Some(error) = &formula.error {
        ui.interact(
            rect,
            ui.id().with((
                "math-error",
                rect.left().to_bits(),
                rect.top().to_bits(),
                error,
            )),
            egui::Sense::hover(),
        )
        .on_hover_text(error);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractions_roots_and_scripts_produce_native_textures() {
        let ctx = Context::default();
        for latex in [
            r"\frac{1}{2}",
            r"\sqrt{x^2+y^2}",
            r"\sum_{n=1}^{\infty}\frac{1}{n^2}",
        ] {
            let result = formula(&ctx, latex, true);
            assert!(result.error.is_none(), "{:?}", result.error);
            assert!(result.texture.is_some());
            assert!(result.size.x > 0.0 && result.size.y > 0.0);
        }
        assert!(formula(&ctx, r"\unknownCommand{x}", false).error.is_some());
    }
}

#[cfg(test)]
mod cache_regression {
    use super::*;
    #[test]
    fn more_than_twenty_four_formulas_keep_their_native_textures() {
        let ctx = Context::default();
        let inputs: Vec<_> = (0..40).map(|index| format!("x_{{{index}}}+1")).collect();
        let ids: Vec<_> = inputs
            .iter()
            .map(|latex| formula(&ctx, latex, false).texture.unwrap().id())
            .collect();
        for (latex, id) in inputs.iter().zip(ids) {
            assert_eq!(formula(&ctx, latex, false).texture.unwrap().id(), id);
        }
    }
}

#[cfg(test)]
mod system_regression {
    use super::*;
    #[test]
    fn braced_array_relation_uses_centered_boxes_without_rewriting_source() {
        let source = r"\left.\begin{array}{l}a_x=|a|\cdot\cos\alpha\\a_y=|a|\cdot\cos\beta\\a_z=|a|\cdot\cos\gamma\end{array}\right\} \Rightarrow \cos^2\alpha+\cos^2\beta+\cos^2\gamma=1";
        let (left, right) = system_parts(source).unwrap();
        assert!(left.ends_with(r"\right\}"));
        assert!(right.starts_with(r"\Rightarrow"));
        assert!(system_parts(r"x+\frac{1}{2}").is_none());
        let ctx = Context::default();
        assert!(formula(&ctx, source, true).error.is_none());
    }
}
