// SPDX-License-Identifier: GPL-3.0-only
//! Optional frame timings and repaint sources. No idle clock or note contents.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub struct FrameProfile {
    since: Instant,
    frames: u64,
    updates: u64,
    focused_frames: u64,
    input_events: u64,
    pointer_events: u64,
    total: Duration,
    max: Duration,
    causes: BTreeMap<(&'static str, u32), u64>,
}

impl FrameProfile {
    pub fn from_env() -> Option<Self> {
        (std::env::var_os("DIAMOND_PROFILE").as_deref() == Some(std::ffi::OsStr::new("1"))).then(
            || Self {
                since: Instant::now(),
                frames: 0,
                updates: 0,
                focused_frames: 0,
                input_events: 0,
                pointer_events: 0,
                total: Duration::ZERO,
                max: Duration::ZERO,
                causes: BTreeMap::new(),
            },
        )
    }

    pub fn begin_frame(&mut self, ctx: &egui::Context, input: &egui::RawInput) {
        self.frames += 1;
        self.focused_frames += u64::from(input.focused);
        self.input_events += input.events.len() as u64;
        self.pointer_events += input
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_)
                )
            })
            .count() as u64;
        for cause in ctx.repaint_causes() {
            let key = (cause.file, cause.line);
            if self.causes.contains_key(&key) || self.causes.len() < 128 {
                *self.causes.entry(key).or_default() += 1;
            }
        }
    }

    pub fn record(&mut self, duration: Duration) {
        self.updates += 1;
        self.total += duration;
        self.max = self.max.max(duration);
        let elapsed = self.since.elapsed();
        if elapsed < Duration::from_secs(5) {
            return;
        }
        eprintln!(
            "[diamond profile] frames={} frames_per_sec={:.1} updates={} updates_per_sec={:.1} focused_frames={} input_events={} pointer_events={} gui_mean_ms={:.3} gui_max_ms={:.3}",
            self.frames,
            self.frames as f64 / elapsed.as_secs_f64(),
            self.updates,
            self.updates as f64 / elapsed.as_secs_f64(),
            self.focused_frames,
            self.input_events,
            self.pointer_events,
            self.total.as_secs_f64() * 1000.0 / self.updates as f64,
            self.max.as_secs_f64() * 1000.0,
        );
        let mut causes: Vec<_> = self.causes.iter().collect();
        causes.sort_unstable_by(|a, b| b.1.cmp(a.1));
        for ((file, line), count) in causes.into_iter().take(8) {
            eprintln!("[diamond repaint] count={count} source={file}:{line}");
        }
        self.since = Instant::now();
        self.frames = 0;
        self.updates = 0;
        self.focused_frames = 0;
        self.input_events = 0;
        self.pointer_events = 0;
        self.total = Duration::ZERO;
        self.max = Duration::ZERO;
        self.causes.clear();
    }
}
