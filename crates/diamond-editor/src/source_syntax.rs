// SPDX-License-Identifier: GPL-3.0-only
//! One sleeping worker for source grammars. Parser states never cross threads.
use super::syntax::{self, Span};
use egui::{Color32, Context};
use std::{
    collections::VecDeque,
    ops::Range,
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, HighlightState},
    parsing::ParseState,
};

const MAX_BYTES: usize = 512 * 1024;
const MAX_LINES: usize = 8192;
const CHECKPOINT: usize = 32;
const MAX_BUFFERS: usize = 8;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
type ParserState = (HighlightState, ParseState);

struct Line {
    text: String,
    spans: Vec<Span>,
    state: Option<ParserState>,
}
#[derive(Default)]
struct Incremental {
    language: String,
    dark: bool,
    lines: Vec<Rc<Line>>,
}
impl Incremental {
    // Restart at the preceding checkpoint, stop when the unchanged suffix and
    // both grammar states converge. Newline insertions may shift line numbers.
    fn highlight(
        &mut self,
        text: &str,
        language: &str,
        dark: bool,
        cancelled: impl Fn() -> bool,
    ) -> Option<(Arc<Vec<Span>>, usize)> {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let compatible = self.language == language && self.dark == dark;
        let old = if compatible {
            self.lines.as_slice()
        } else {
            &[]
        };
        let prefix = lines
            .iter()
            .zip(old)
            .take_while(|(a, b)| **a == b.text)
            .count();
        let suffix = lines[prefix..]
            .iter()
            .rev()
            .zip(old[prefix..].iter().rev())
            .take_while(|(a, b)| **a == b.text)
            .count();
        let restart = (0..prefix)
            .rev()
            .find(|&i| old[i].state.is_some())
            .map_or(0, |i| i + 1);
        let (syntaxes, theme, syntax) = syntax::grammar(language, dark);
        let mut highlighter = if restart == 0 {
            HighlightLines::new(syntax, theme)
        } else {
            let state = old[restart - 1].state.as_ref().unwrap();
            HighlightLines::from_state(theme, state.0.clone(), state.1.clone())
        };
        let mut next: Vec<Rc<Line>> = old[..restart].to_vec();
        let mut parsed = 0;
        for (i, line) in lines.iter().enumerate().skip(restart) {
            if cancelled() {
                return None;
            }
            let mut spans = Vec::new();
            let mut byte = 0;
            let mut scalar = 0;
            match highlighter.highlight_line(line, syntaxes) {
                Ok(runs) => {
                    for (style, fragment) in runs {
                        let count = fragment.chars().count();
                        spans.push(Span {
                            bytes: byte..byte + fragment.len(),
                            scalars: scalar..scalar + count,
                            color: Color32::from_rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            ),
                            italic: style.font_style.contains(FontStyle::ITALIC),
                            bold: style.font_style.contains(FontStyle::BOLD),
                            underline: style.font_style.contains(FontStyle::UNDERLINE),
                        });
                        byte += fragment.len();
                        scalar += count;
                    }
                }
                Err(_) => spans = syntax::plain(line, dark),
            }
            parsed += 1;
            let old_index = (i >= lines.len() - suffix).then(|| old.len() - (lines.len() - i));
            let old_state = old_index
                .and_then(|index| old.get(index))
                .and_then(|line| line.state.as_ref());
            let checkpoint =
                (i + 1) % CHECKPOINT == 0 || i + 1 == lines.len() || old_state.is_some();
            let state = if checkpoint {
                let state = highlighter.state();
                highlighter = HighlightLines::from_state(theme, state.0.clone(), state.1.clone());
                Some(state)
            } else {
                None
            };
            let converged =
                i + 1 >= lines.len() - suffix && old_state.is_some() && state.as_ref() == old_state;
            next.push(Rc::new(Line {
                text: (*line).into(),
                spans,
                state,
            }));
            if converged {
                next.extend_from_slice(&old[old_index.unwrap() + 1..]);
                break;
            }
        }
        if cancelled() {
            return None;
        }
        let mut spans = Vec::new();
        let mut byte = 0;
        let mut scalar = 0;
        for line in &next {
            for span in &line.spans {
                let mut span = span.clone();
                span.bytes.start += byte;
                span.bytes.end += byte;
                span.scalars.start += scalar;
                span.scalars.end += scalar;
                syntax::push_span(&mut spans, span);
            }
            byte += line.text.len();
            scalar += line.text.chars().count();
        }
        self.lines = next;
        self.language = language.into();
        self.dark = dark;
        Some((Arc::new(spans), parsed))
    }
}

#[derive(Clone)]
struct Ready {
    revision: u64,
    source: Arc<str>,
    language: String,
    dark: bool,
    spans: Arc<Vec<Span>>,
}
#[derive(Default)]
struct Mailbox {
    requested: AtomicU64,
    ready: Mutex<Option<Ready>>,
}
struct Job {
    id: u64,
    revision: u64,
    source: Arc<str>,
    language: String,
    dark: bool,
    mailbox: Weak<Mailbox>,
    ctx: Context,
}
#[derive(Default)]
struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    wake: Condvar,
}
fn queue() -> &'static Arc<Queue> {
    static QUEUE: OnceLock<Arc<Queue>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let queue = Arc::new(Queue::default()); let worker = Arc::clone(&queue);
        std::thread::Builder::new().name("diamond-syntax".into()).spawn(move || {
            // Rc/parser checkpoints are created and retained on this thread only.
            let mut buffers: VecDeque<(u64, Weak<Mailbox>, Incremental)> = VecDeque::new();
            loop {
                let job = {
                    let mut jobs = worker.jobs.lock().unwrap_or_else(|e|e.into_inner());
                    while jobs.is_empty() {jobs = worker.wake.wait(jobs).unwrap_or_else(|e|e.into_inner());}
                    jobs.pop_front().unwrap()
                };
                let Some(mailbox) = job.mailbox.upgrade() else {continue;};
                let cancelled = || mailbox.requested.load(Ordering::Relaxed) != job.revision;
                if cancelled() {continue;}
                buffers.retain(|(_, owner, _)| owner.strong_count() > 0);
                let mut buffer = buffers.iter().position(|(id,_,_)| *id == job.id)
                    .and_then(|index| buffers.remove(index)).map_or_else(Incremental::default, |(_,_,buffer)|buffer);
                let started=std::time::Instant::now();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                    buffer.highlight(&job.source, &job.language, job.dark, cancelled)));
                if !cancelled() {
                    let spans = match result {
                        Ok(Some((spans, parsed))) => {
                            if syntax::profiling(){eprintln!("[diamond source] syntax_ms={:.3} bytes={} lines_reparsed={parsed}",started.elapsed().as_secs_f64()*1000.0,job.source.len());}
                            spans
                        },
                        Ok(None) => continue,
                        Err(_) => {buffer = Incremental::default(); Arc::new(syntax::plain(&job.source, job.dark))},
                    };
                    *mailbox.ready.lock().unwrap_or_else(|e|e.into_inner()) = Some(Ready {
                        revision: job.revision, source: Arc::clone(&job.source), language: job.language,
                        dark: job.dark, spans,
                    });
                    job.ctx.request_repaint();
                }
                buffers.push_back((job.id, job.mailbox, buffer));
                while buffers.len() > MAX_BUFFERS {buffers.pop_front();}
            }
        }).expect("cannot start syntax worker");
        queue
    })
}

pub(super) struct Client {
    id: u64,
    mailbox: Arc<Mailbox>,
    request: Option<(Arc<str>, String, bool)>,
    revision: u64,
    provisional: Option<Arc<Vec<Span>>>,
}
impl Default for Client {
    fn default() -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            mailbox: Arc::default(),
            request: None,
            revision: 0,
            provisional: None,
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.mailbox.requested.fetch_add(1, Ordering::Relaxed);
    }
}
impl Client {
    pub fn spans(
        &mut self,
        ctx: &Context,
        text: &str,
        language: &str,
        dark: bool,
    ) -> (u64, Arc<Vec<Span>>) {
        let changed = self.request.as_ref().is_none_or(|(source, lang, theme)| {
            source.as_ref() != text || lang != language || *theme != dark
        });
        if changed {
            self.revision = self.revision.wrapping_add(1);
            self.provisional = None;
            self.mailbox
                .requested
                .store(self.revision, Ordering::Relaxed);
            let source: Arc<str> = text.into();
            self.request = Some((Arc::clone(&source), language.into(), dark));
            // Replacing a pending request coalesces bursts; no per-key task queue.
            let supported = !matches!(language, "txt" | "text" | "plaintext")
                && !syntax::disabled()
                && text.len() <= MAX_BYTES
                && text.lines().take(MAX_LINES + 1).count() <= MAX_LINES
                && !text.lines().any(|line| line.len() > 16384);
            if supported {
                let queue = queue();
                let mut jobs = queue.jobs.lock().unwrap_or_else(|e| e.into_inner());
                jobs.retain(|job| job.id != self.id);
                jobs.push_back(Job {
                    id: self.id,
                    revision: self.revision,
                    source,
                    language: language.into(),
                    dark,
                    mailbox: Arc::downgrade(&self.mailbox),
                    ctx: ctx.clone(),
                });
                queue.wake.notify_one();
            } else {
                *self.mailbox.ready.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ready {
                    revision: self.revision,
                    source,
                    language: language.into(),
                    dark,
                    spans: Arc::new(syntax::plain(text, dark)),
                });
            }
        }
        let ready = self
            .mailbox
            .ready
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(ready) = ready {
            if ready.revision == self.revision {
                return (self.revision * 2 + 1, ready.spans);
            }
            if ready.language == language && ready.dark == dark {
                let spans = self.provisional.get_or_insert_with(|| {
                    Arc::new(remap(&ready.source, text, &ready.spans, dark))
                });
                return (self.revision * 2, Arc::clone(spans));
            }
        }
        let spans = self
            .provisional
            .get_or_insert_with(|| Arc::new(syntax::plain(text, dark)));
        (self.revision * 2, Arc::clone(spans))
    }
}

// Retain valid colors around the changed region until the worker completes.
// Always slice on Unicode boundaries, never apply stale absolute token offsets.
fn remap(old: &str, new: &str, spans: &[Span], dark: bool) -> Vec<Span> {
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let mut result = Vec::new();
    let mut append = |range: Range<usize>, old_range: Option<Range<usize>>| {
        if range.is_empty() {
            return;
        }
        let mut cursor = range.start;
        if let Some(old_range) = old_range {
            for span in spans
                .iter()
                .filter(|s| s.bytes.end > old_range.start && s.bytes.start < old_range.end)
            {
                let start = span.bytes.start.max(old_range.start) - old_range.start + range.start;
                let end = span.bytes.end.min(old_range.end) - old_range.start + range.start;
                if start > cursor {
                    let mut p = syntax::plain(&new[cursor..start], dark).remove(0);
                    p.bytes = cursor..start;
                    result.push(p);
                }
                let mut part = span.clone();
                part.bytes = start..end;
                result.push(part);
                cursor = end;
            }
        }
        if cursor < range.end {
            let mut p = syntax::plain(&new[cursor..range.end], dark).remove(0);
            p.bytes = cursor..range.end;
            result.push(p);
        }
    };
    append(0..prefix, Some(0..prefix));
    append(prefix..new_end, None);
    append(new_end..new.len(), Some(old_end..old.len()));
    let mut scalar = 0;
    for span in &mut result {
        let end = scalar + new[span.bytes.clone()].chars().count();
        span.scalars = scalar..end;
        scalar = end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(cache: &mut Incremental, text: &str, language: &str) -> usize {
        let (actual, parsed) = cache.highlight(text, language, true, || false).unwrap();
        let (expected, _) = Incremental::default()
            .highlight(text, language, true, || false)
            .unwrap();
        assert_eq!(actual.as_ref(), expected.as_ref());
        assert_eq!(
            actual
                .iter()
                .map(|s| &text[s.bytes.clone()])
                .collect::<String>(),
            text
        );
        parsed
    }
    #[test]
    fn middle_edits_and_newlines_converge_without_parsing_nine_hundred_lines() {
        let base = "let привет = \"你好🙂\";\r\n".repeat(900);
        let mut cache = Incremental::default();
        assert_eq!(check(&mut cache, &base, "rs"), 900);
        let split = base.find('\n').unwrap() + 1;
        let text = format!("{}\n\n{}", &base[..split], &base[split..]);
        assert!(check(&mut cache, &text, "rs") < 70);
        let text = text.replacen("let привет", "let мир", 1);
        assert!(check(&mut cache, &text, "rs") < 70);
        check(&mut cache, &format!("/*\n{text}"), "rs");
        check(&mut cache, &text, "rs");
        check(&mut cache, "```rust\n/* hi\n*/\n```\n# Title\n", "md");
        check(
            &mut cache,
            "```rust\n/* hi\nthere\n*/\n```\n# Title\n",
            "md",
        );
        check(&mut cache, "", "md");
    }
    #[test]
    fn cancelled_work_does_not_replace_checkpoints() {
        let mut cache = Incremental::default();
        check(&mut cache, "a\nb\n", "rs");
        assert!(
            cache
                .highlight("changed\nb\n", "rs", true, || true)
                .is_none()
        );
        check(&mut cache, "a\nb\n", "rs");
    }
    #[test]
    fn provisional_colors_preserve_every_unicode_byte() {
        for (old, new) in [
            ("я🙂\r\na", "я!🙂\r\na"),
            ("я🙂", "🙂"),
            ("abc", ""),
            ("", "你好"),
            ("aa\nbb", "aa\n\nbb"),
        ] {
            let spans = remap(old, new, &syntax::plain(old, true), true);
            assert_eq!(
                spans
                    .iter()
                    .map(|s| &new[s.bytes.clone()])
                    .collect::<String>(),
                new
            );
            assert_eq!(
                spans.last().map_or(0, |s| s.scalars.end),
                new.chars().count()
            );
        }
    }
}
