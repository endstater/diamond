//! Native bounded image decoding. Files/network are supplied by the desktop host.
use std::{collections::{HashMap,HashSet}, io::Cursor, sync::{Arc,atomic::{AtomicUsize,Ordering},mpsc::{self,Receiver}}};
use egui::{Color32,ColorImage,Rect,TextureHandle,Ui};
use diamond_document::Image;
pub type Loader=Arc<dyn Fn(&str)->Result<Vec<u8>,String>+Send+Sync>;
const MAX_BYTES:usize=16*1024*1024;
static RUNNING:AtomicUsize=AtomicUsize::new(0);
struct Permit;
impl Drop for Permit {fn drop(&mut self){RUNNING.fetch_sub(1,Ordering::AcqRel);}}
enum Slot {Loading(Receiver<Result<ColorImage,String>>),Ready(TextureHandle),Error(String)}
#[derive(Default)]
pub struct Images {loader:Option<Loader>,slots:HashMap<String,Slot>,visible:HashSet<String>,requested:HashSet<String>}

impl Images {
    pub fn set_loader(&mut self,loader:Loader){self.loader=Some(loader);self.slots.clear();self.requested.clear();}
    pub fn begin_frame(&mut self,ctx:&egui::Context){
        self.visible.clear();
        for (target,slot) in &mut self.slots {
            let result=if let Slot::Loading(receiver)=slot {
                match receiver.try_recv(){Ok(result)=>Some(result),Err(mpsc::TryRecvError::Disconnected)=>Some(Err("Image worker stopped".into())),Err(mpsc::TryRecvError::Empty)=>None}
            } else {None};
            if let Some(result)=result {*slot=match result {
                Ok(image)=>Slot::Ready(ctx.load_texture(format!("note-image:{target}"),image,egui::TextureOptions::LINEAR)),
                Err(error)=>Slot::Error(error),
            };}
        }
    }
    pub fn height(&self,target:&str,width:f32)->f32 {
        if let Some(Slot::Ready(texture))=self.slots.get(target) {
            let [w,h]=texture.size();return (width.min(w as f32)*h as f32/w as f32).min(360.0);
        }
        88.0
    }
    fn start(&mut self,ctx:&egui::Context,target:&str){
        if self.slots.contains_key(target){return;}
        let Some(loader)=self.loader.clone() else{return;};
        if self.slots.len()>=12 {
            let evict=self.slots.iter().find(|(key,slot)|!self.visible.contains(*key)&&!matches!(slot,Slot::Loading(_))).map(|(key,_)|key.clone());
            if let Some(evict)=evict {self.slots.remove(&evict);}else{return;}
        }
        if RUNNING.fetch_update(Ordering::AcqRel,Ordering::Acquire,|count|(count<2).then_some(count+1)).is_err(){return;}
        let (sender,receiver)=mpsc::channel();let target_owned=target.to_owned();let repaint=ctx.clone();
        self.slots.insert(target.into(),Slot::Loading(receiver));
        std::thread::spawn(move||{
            let _permit=Permit;
            let result=loader(&target_owned).and_then(|bytes|decode(&bytes));
            let _=sender.send(result);drop(_permit);repaint.request_repaint();
        });
    }
    pub fn show(&mut self,ui:&mut Ui,rect:Rect,image:&Image,language:diamond_core::Language){
        if !ui.is_rect_visible(rect){return;}
        self.visible.insert(image.target.clone());
        let remote=image.target.starts_with("https://")||image.target.starts_with("http://");
        if !remote || self.requested.contains(&image.target) {self.start(ui.ctx(),&image.target);}
        let response=ui.interact(rect,ui.id().with(("image",&image.target)),egui::Sense::click());
        response.clone().on_hover_text(format!("{}\n{}",image.title,image.target));
        match self.slots.get(&image.target) {
            Some(Slot::Ready(texture))=>{
                let [w,h]=texture.size();let scale=(rect.width()/w as f32).min(rect.height()/h as f32);
                let fitted=Rect::from_center_size(rect.center(),egui::vec2(w as f32*scale,h as f32*scale));
                ui.painter().image(texture.id(),fitted,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
            },
            status=>{
                ui.painter().rect_filled(rect,10.0,ui.visuals().faint_bg_color);
                let label=match status {Some(Slot::Loading(_))=>language.text("Loading image…"),Some(Slot::Error(_))=>language.text("Image unavailable"),_=>language.text("Load image")};
                ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,label,egui::TextStyle::Button.resolve(ui.style()),ui.visuals().text_color());
                if let Some(Slot::Error(error))=status {response.clone().on_hover_text(error);}
                if response.clicked(){response.request_focus();self.requested.insert(image.target.clone());self.slots.remove(&image.target);self.start(ui.ctx(),&image.target);}
            }
        }
    }
}
fn decode(bytes:&[u8])->Result<ColorImage,String>{
    if bytes.len()>MAX_BYTES{return Err("Image exceeds 16 MiB".into());}
    let mut reader=image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|error|error.to_string())?;
    let mut limits=image::Limits::default();limits.max_image_width=Some(4096);limits.max_image_height=Some(4096);limits.max_alloc=Some(96*1024*1024);reader.limits(limits);
    let image=reader.decode().map_err(|error|error.to_string())?.thumbnail(1200,1200).to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()))
}
#[cfg(test)]
mod tests {
    #[test]fn invalid_images_fail_without_lossy_text_conversion(){assert!(super::decode(b"not an image").is_err());}
}
