// SPDX-License-Identifier: GPL-3.0-only
//! Optional frame timings. No timer, polling loop, or document content logging.
use std::time::{Duration, Instant};
pub struct FrameProfile {
    since: Instant,
    frames: u64,
    total: Duration,
    max: Duration,
}
impl FrameProfile {
    pub fn from_env() -> Option<Self> {
        (std::env::var_os("DIAMOND_PROFILE").as_deref() == Some(std::ffi::OsStr::new("1"))).then(
            || Self {
                since: Instant::now(),
                frames: 0,
                total: Duration::ZERO,
                max: Duration::ZERO,
            },
        )
    }
    pub fn record(&mut self, duration: Duration) {
        self.frames += 1;
        self.total += duration;
        self.max = self.max.max(duration);
        let elapsed = self.since.elapsed();
        if elapsed >= Duration::from_secs(5) {
            eprintln!(
                "[diamond profile] updates={} updates_per_sec={:.1} gui_mean_ms={:.3} gui_max_ms={:.3}",
                self.frames,
                self.frames as f64 / elapsed.as_secs_f64(),
                self.total.as_secs_f64() * 1000.0 / self.frames as f64,
                self.max.as_secs_f64() * 1000.0
            );
            self.since = Instant::now();
            self.frames = 0;
            self.total = Duration::ZERO;
            self.max = Duration::ZERO;
        }
    }
}
