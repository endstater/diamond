use std::ops::Range;
use diamond_core::search::{SearchOptions,SearchPattern};

#[derive(Default)]
pub(super) struct SearchState {
    pub options:SearchOptions,
    pub focus:bool,
    pub matches:Vec<Range<usize>>,
    pub current:usize,
    pub jump:Option<Range<usize>>,
    pub error:Option<String>,
    key:Option<(String,SearchOptions,u64)>,
    compiled:Option<SearchPattern>,
}

impl SearchState {
    pub fn needs_refresh(&self,query:&str,generation:u64)->bool {
        self.key.as_ref().is_none_or(|key|key.0!=query || key.1!=self.options || key.2!=generation)
    }
    pub fn refresh(&mut self,query:&str,text:&str,generation:u64) {
        if !self.needs_refresh(query,generation){return}
        let recompile=self.key.as_ref().is_none_or(|key|key.0!=query || key.1!=self.options);
        self.key=Some((query.to_owned(),self.options.clone(),generation));
        if query.is_empty(){self.matches.clear();self.compiled=None;self.error=None;self.jump=None;return}
        if recompile {
            match SearchPattern::compile(query,&self.options){
                Ok(pattern)=>{self.compiled=Some(pattern);self.error=None;}
                Err(error)=>{self.compiled=None;self.error=Some(error);}
            }
        }
        self.matches=self.compiled.as_ref().map_or_else(Vec::new,|pattern|pattern.scalar_ranges(text));
        if recompile {
            self.current=0;self.jump=self.matches.first().cloned();
        }else{
            self.current=self.current.min(self.matches.len().saturating_sub(1));
        }
    }
    pub fn navigate(&mut self,forward:bool) {
        let count=self.matches.len();if count==0{return}
        self.current=if forward{(self.current+1)%count}else{(self.current+count-1)%count};
        self.jump=Some(self.matches[self.current].clone());
    }
    pub fn replace(&self,text:&str,replacement:&str)->Option<String> {
        self.compiled.as_ref().map(|pattern|pattern.replace_all(text,replacement))
    }
}

pub(super) fn highlight(mut job:egui::text::LayoutJob,matches:&[Range<usize>],active:Option<&Range<usize>>,color:egui::Color32)->egui::text::LayoutJob {
    if matches.is_empty(){return job}
    let boundaries:Vec<usize>=job.text.char_indices().map(|(i,_)|i).chain(std::iter::once(job.text.len())).collect();
    let ranges:Vec<Range<usize>>=matches.iter().filter_map(|r|Some(*boundaries.get(r.start)?..*boundaries.get(r.end)?)).collect();
    let active=active.and_then(|r|Some(*boundaries.get(r.start)?..*boundaries.get(r.end)?));
    let mut sections=Vec::new();
    for section in &job.sections {
        let mut cuts=vec![section.byte_range.start,section.byte_range.end];
        let first=ranges.partition_point(|r|r.end<=section.byte_range.start);
        for range in &ranges[first..] {
            if range.start>=section.byte_range.end{break}
            cuts.push(range.start.max(section.byte_range.start));cuts.push(range.end.min(section.byte_range.end));
        }
        cuts.sort_unstable();cuts.dedup();
        for pair in cuts.windows(2){
            if pair[0]==pair[1]{continue}
            let mut part=section.clone();part.byte_range=pair[0]..pair[1];part.leading_space=if pair[0]==section.byte_range.start{section.leading_space}else{0.0};
            let index=ranges.partition_point(|r|r.end<=pair[0]);
            if ranges.get(index).is_some_and(|r|r.start<=pair[0] && pair[0]<r.end){
                let selected=active.as_ref().is_some_and(|r|r.start<=pair[0]&&pair[0]<r.end);
                part.format.background=color.gamma_multiply(if selected{0.48}else{0.23});
            }
            sections.push(part);
        }
    }
    job.sections=sections;job
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlighting_preserves_text_and_handles_unicode(){
        let text="Я🙂world";
        let mut job=egui::text::LayoutJob::default();
        job.append(text,0.0,egui::TextFormat::default());
        let highlighted=highlight(job,&[1..2],Some(&(1..2)),egui::Color32::WHITE);
        assert_eq!(highlighted.text,text);
        assert_eq!(highlighted.sections.len(),3);
        assert_eq!(&highlighted.text[highlighted.sections[1].byte_range.clone()],"🙂");
    }
    #[test]
    fn editing_with_search_open_does_not_move_the_caret_to_the_first_match(){
        let mut state=SearchState::default();state.refresh("x","x x",0);
        assert_eq!(state.jump.take(),Some(0..1));
        state.refresh("x","x xx",1);assert!(state.jump.is_none());
        state.navigate(true);assert_eq!(state.jump.take(),Some(2..3));
    }
}
