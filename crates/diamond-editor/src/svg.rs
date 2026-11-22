//! Local SVG rasterization and per-context texture caching. No network loader.
use egui::{Color32,Rect,Id,Painter,TextureHandle};

pub fn rasterize(bytes:&[u8],edge:u32)->Option<egui::ColorImage> {
    let tree=resvg::usvg::Tree::from_data(bytes,&resvg::usvg::Options::default()).ok()?;
    let size=tree.size();
    let scale=edge as f32/size.width().max(size.height());
    let width=(size.width()*scale).ceil().max(1.0) as u32;
    let height=(size.height()*scale).ceil().max(1.0) as u32;
    let mut pixels=resvg::tiny_skia::Pixmap::new(width,height)?;
    resvg::render(&tree,resvg::tiny_skia::Transform::from_scale(scale,scale),&mut pixels.as_mut());
    Some(egui::ColorImage::from_rgba_premultiplied([width as usize,height as usize],pixels.data()))
}

pub fn paint(painter:&Painter,id:Id,bytes:&[u8],rect:Rect,tint:Color32) {
    let cached=painter.ctx().data(|data|data.get_temp::<TextureHandle>(id));
    let cached=cached.or_else(|| {
        let mut image=rasterize(bytes,48)?;
        for pixel in &mut image.pixels {
            *pixel=Color32::from_rgba_unmultiplied(255,255,255,pixel.a());
        }
        let texture=painter.ctx().load_texture(format!("local-icon-{id:?}"),image,egui::TextureOptions::LINEAR);
        painter.ctx().data_mut(|data|data.insert_temp(id,texture.clone()));Some(texture)
    });
    if let Some(texture)=cached {
        painter.image(texture.id(),rect,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),tint);
    }
}

pub fn button(ui:&mut egui::Ui,name:&str,bytes:&[u8])->egui::Response {
    let (rect,response)=ui.allocate_exact_size(egui::vec2(32.0,36.0),egui::Sense::click());
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,ui.is_enabled(),name));
    let visuals=ui.style().interact(&response);
    if response.hovered() || response.has_focus(){ui.painter().rect_filled(rect,8.0,visuals.bg_fill);}
    paint(ui.painter(),Id::new(("editor-icon",name)),bytes,
        Rect::from_center_size(rect.center(),egui::vec2(18.0,18.0)),visuals.fg_stroke.color);
    response.on_hover_text(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_lucide_rasterizes_and_bad_svg_is_rejected(){
        let image=rasterize(include_bytes!("../../../assets/lucide/search.svg"),48).unwrap();
        assert_eq!(image.size,[48,48]);
        assert!(image.pixels.iter().any(|pixel|pixel.a()>0));
        assert!(rasterize(b"not svg",48).is_none());
    }
}
