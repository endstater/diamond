//! Cached native mathematical LaTeX. Only the embedded STIX math font is read.
use std::sync::OnceLock;
use egui::{Color32,Context,TextureHandle,Vec2};
use latex_rust::{MathFont,SvgOptions,Dim};
static FONT:OnceLock<Result<MathFont,String>>=OnceLock::new();
#[derive(Clone)]
pub struct Formula {pub texture:Option<TextureHandle>,pub size:Vec2,pub error:Option<String>}
#[derive(Clone)]
struct Entry {latex:String,display:bool,formula:Formula}
#[derive(Clone,Default)]
struct Cache(Vec<Entry>);

pub fn formula(ctx:&Context,latex:&str,display:bool)->Formula {
    let id=egui::Id::new("diamond-math-cache");
    if let Some(hit)=ctx.data_mut(|data|data.get_temp_mut_or_default::<Cache>(id).0.iter()
        .find(|entry|entry.latex==latex&&entry.display==display).map(|entry|entry.formula.clone())){return hit;}
    let result=render(ctx,latex,display).unwrap_or_else(|error|Formula{texture:None,size:egui::vec2(22.0,20.0),error:Some(error)});
    ctx.data_mut(|data|{let cache=data.get_temp_mut_or_default::<Cache>(id);
        if cache.0.len()>=24{cache.0.remove(0);}cache.0.push(Entry{latex:latex.into(),display,formula:result.clone()});});result
}
fn render(ctx:&Context,latex:&str,display:bool)->Result<Formula,String>{
    if latex.len()>8192{return Err("Formula exceeds the 8192-byte limit".into());}
    let mut depth=0usize;
    for byte in latex.bytes(){if byte==b'{' {depth+=1;if depth>64{return Err("Formula nesting exceeds 64 groups".into());}}else if byte==b'}'{depth=depth.saturating_sub(1);}}
    let font=FONT.get_or_init(||MathFont::stix_two_math().map_err(|error|error.to_string())).as_ref().map_err(Clone::clone)?;
    let options=SvgOptions{font_size_pt:Dim::from_i64(12),color:latex_rust::Color::rgb(255,255,255),display};
    let svg=latex_rust::latex_to_svg(latex,font,&options).map_err(|error|error.to_string())?;
    let tree=resvg::usvg::Tree::from_data(svg.as_bytes(),&resvg::usvg::Options::default()).map_err(|error|error.to_string())?;
    let size=egui::vec2(tree.size().width(),tree.size().height());
    if !(size.x>0.0&&size.y>0.0)||size.x>4096.0||size.y>2048.0{return Err("Formula is too large to render".into());}
    let scale=2.0_f32.min(2048.0/size.x).min(1024.0/size.y);
    let width=(size.x*scale).ceil().max(1.0) as u32;let height=(size.y*scale).ceil().max(1.0) as u32;
    let mut pixels=resvg::tiny_skia::Pixmap::new(width,height).ok_or("Cannot allocate formula image")?;
    resvg::render(&tree,resvg::tiny_skia::Transform::from_scale(scale,scale),&mut pixels.as_mut());
    let image=egui::ColorImage::from_rgba_premultiplied([width as usize,height as usize],pixels.data());
    let texture=ctx.load_texture("latex-math",image,egui::TextureOptions::LINEAR);
    Ok(Formula{texture:Some(texture),size,error:None})
}
pub fn paint(ui:&mut egui::Ui,formula:&Formula,rect:egui::Rect,color:Color32){
    if let Some(texture)=&formula.texture {ui.painter().image(texture.id(),rect,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),color);}
    else {ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"◇",egui::FontId::proportional(18.0),color);}
    if let Some(error)=&formula.error {
        ui.interact(rect,ui.id().with(("math-error",rect.left().to_bits(),rect.top().to_bits(),error)),egui::Sense::hover()).on_hover_text(error);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn fractions_roots_and_scripts_produce_native_textures(){
        let ctx=Context::default();
        for latex in [r"\frac{1}{2}",r"\sqrt{x^2+y^2}",r"\sum_{n=1}^{\infty}\frac{1}{n^2}"] {
            let result=formula(&ctx,latex,true);assert!(result.error.is_none(),"{:?}",result.error);
            assert!(result.texture.is_some());assert!(result.size.x>0.0&&result.size.y>0.0);
        }
        assert!(formula(&ctx,r"\unknownCommand{x}",false).error.is_some());
    }
}
