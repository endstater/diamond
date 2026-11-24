// SPDX-License-Identifier: GPL-3.0-only
//! Frame ceiling and finite task-progress transitions; no idle polling clock.
use std::time::{Duration, Instant};

const FRAME_INTERVAL: Duration = Duration::from_millis(5);
const TRANSITION_SECONDS: f64 = 0.25;
const TRANSITION_STEP: Duration = Duration::from_millis(16);

#[derive(Default)]
pub struct FramePacer {
    last_frame: Option<Instant>,
}

impl FramePacer {
    fn remaining(&self, now: Instant) -> Duration {
        self.last_frame.map_or(Duration::ZERO, |last| {
            FRAME_INTERVAL.saturating_sub(now.saturating_duration_since(last))
        })
    }

    pub fn wait(&mut self) {
        let remaining = self.remaining(Instant::now());
        if !remaining.is_zero() {
            // Only an already requested frame can reach here. The wait is at
            // most 5 ms; it neither requests another frame nor busy-spins.
            std::thread::sleep(remaining);
        }
        // Use the actual start, so a delayed frame cannot create catch-up bursts.
        self.last_frame = Some(Instant::now());
    }
}

#[derive(Clone, Copy)]
struct ProgressTransition {
    from: f32,
    target: f32,
    started: f64,
}

impl ProgressTransition {
    fn value(self, now: f64) -> f32 {
        let elapsed = now - self.started;
        if self.from == self.target || elapsed >= TRANSITION_SECONDS {
            return self.target;
        }
        let t = (elapsed / TRANSITION_SECONDS).clamp(0.0, 1.0) as f32;
        self.from + (self.target - self.from) * t
    }

    fn running(self, now: f64) -> bool {
        self.from != self.target && now - self.started < TRANSITION_SECONDS
    }
}

/// egui 0.33 subtracts predicted_dt from repaint delays. Compensate for
/// one-shot deadlines so the UI does not wake early and spin until they expire.
pub fn after(ctx: &egui::Context, delay: Duration) {
    let predicted = ctx.input(|input| input.predicted_dt);
    let compensation = Duration::try_from_secs_f32(predicted).unwrap_or(Duration::ZERO);
    ctx.request_repaint_after(delay.saturating_add(compensation));
}

pub fn progress_value(ctx: &egui::Context, id: egui::Id, target: f32, animate: bool) -> f32 {
    let now = ctx.input(|input| input.time);
    let mut transition = ctx
        .data_mut(|data| data.get_temp::<ProgressTransition>(id))
        .unwrap_or(ProgressTransition {
            from: target,
            target,
            started: now,
        });
    if !animate {
        transition = ProgressTransition {
            from: target,
            target,
            started: now,
        };
    } else if transition.target != target {
        transition = ProgressTransition {
            from: transition.value(now),
            target,
            started: now,
        };
    }
    let value = transition.value(now);
    if transition.running(now) {
        let left =
            Duration::from_secs_f64((TRANSITION_SECONDS - (now - transition.started)).max(0.0));
        after(ctx, TRANSITION_STEP.min(left));
    } else {
        // Exact endpoint, even if floating-point interpolation would round away
        // from the target. Completed progress schedules no more frames.
        transition.from = transition.target;
    }
    ctx.data_mut(|data| data.insert_temp(id, transition));
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_ceiling_has_no_idle_tick_or_catch_up() {
        let now = Instant::now();
        let mut pacer = FramePacer::default();
        assert_eq!(pacer.remaining(now), Duration::ZERO);
        pacer.last_frame = Some(now);
        assert_eq!(
            pacer.remaining(now + Duration::from_millis(2)),
            Duration::from_millis(3)
        );
        assert_eq!(
            pacer.remaining(now + Duration::from_secs(60)),
            Duration::ZERO
        );
    }

    #[test]
    fn progress_finishes_exactly_and_stays_finished() {
        let transition = ProgressTransition {
            from: 1.0 / 3.0,
            target: 2.0 / 3.0,
            started: 10.0,
        };
        assert!(transition.running(10.125));
        assert!(transition.value(10.125) > transition.from);
        for now in [10.25, 100.0] {
            assert!(!transition.running(now));
            assert_eq!(transition.value(now), transition.target);
        }
    }

    #[test]
    fn initial_progress_has_no_animation() {
        let transition = ProgressTransition {
            from: 0.5,
            target: 0.5,
            started: 0.0,
        };
        assert!(!transition.running(0.0));
        assert_eq!(transition.value(0.0), 0.5);
    }
}
