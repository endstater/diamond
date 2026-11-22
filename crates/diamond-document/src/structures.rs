//! Structural table/image data. Their original bytes remain authoritative;
//! edits use source transactions rather than a lossy Markdown reserializer.
use std::ops::Range;
use pulldown_cmark::{Alignment, Event, Tag, TagEnd};
use crate::{Block, BlockKind, Link, Mark, MarkKind, parser, ending};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellAlignment { Left, Center, Right }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableCell { pub text:String, pub range: Range<usize>, pub row:usize, pub column:usize }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {pub columns:Vec<CellAlignment>, pub cells:Vec<TableCell>}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {pub target:String, pub title:String, pub alt:String}

pub(super) fn table(source:&str,text:&str)->Table {
    let columns=parser(source).find_map(|event| if let Event::Start(Tag::Table(columns))=event {
        Some(columns.into_iter().map(|alignment|match alignment {
            Alignment::Center=>CellAlignment::Center,Alignment::Right=>CellAlignment::Right,_=>CellAlignment::Left,
        }).collect())
    } else {None}).unwrap_or_default();
    let mut cells=Vec::new();let mut offset=0;
    for (row,line) in text.split('\n').enumerate() {
        for (column,value) in line.split('\t').enumerate() {
            let count=value.chars().count();cells.push(TableCell{text:value.into(),range:offset..offset+count,row,column});offset+=count+1;
        }
    }
    Table{columns,cells}
}

pub(super) fn parse_image(source:&str)->Option<Block> {
    let mut image=None;let mut inside=false;let mut alt=String::new();
    for event in parser(source) {
        match event {
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => {},
            Event::Start(Tag::Image{dest_url,title,..}) if image.is_none()=>{
                image=Some(Image{target:dest_url.into_string(),title:title.into_string(),alt:String::new()});inside=true;
            },
            Event::End(TagEnd::Image) if inside=>inside=false,
            Event::Text(value)|Event::Code(value) if inside=>alt.push_str(&value),
            Event::Start(Tag::Strong|Tag::Emphasis|Tag::Strikethrough) if inside=>{},
            Event::End(TagEnd::Strong|TagEnd::Emphasis|TagEnd::Strikethrough) if inside=>{},
            Event::SoftBreak if inside=>alt.push(' '),
            Event::Text(value) if value.trim().is_empty()=>{},
            _=>return None,
        }
    }
    let mut image=image?;image.alt=alt.clone();
    Some(Block{kind:BlockKind::Image,text:if alt.is_empty(){"Image".into()}else{alt},
        image:Some(image),original:Some(source.into()),ending:ending(source).into(),..Block::empty()})
}

pub(super) fn bare_links(text:&str, marks:&[Mark], links:&mut Vec<Link>) {
    // Existing explicit links and inline code always take precedence.
    for (byte,_) in text.match_indices("http") {
        let tail=&text[byte..];
        if !(tail.starts_with("https://")||tail.starts_with("http://")){continue;}
        let length=tail.find(|c:char|c.is_whitespace()||matches!(c,'<'|'>'|'"'|'\'')).unwrap_or(tail.len());
        let url=tail[..length].trim_end_matches(['.',',',';',':','!','?',')',']']);
        if url.ends_with("://"){continue;}
        let start=text[..byte].chars().count();let end=start+url.chars().count();
        if links.iter().any(|link|link.range.start<end&&start<link.range.end)
            || marks.iter().any(|mark|mark.kind==MarkKind::Code && mark.range.start<end&&start<mark.range.end){continue;}
        links.push(Link{range:start..end,target:url.into()});
    }
    links.sort_by_key(|link|link.range.start);
}
