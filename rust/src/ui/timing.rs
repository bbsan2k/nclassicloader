//! Presentation follows the display when SDL supports VSync; fallback pacing
//! uses deadlines, never an extra fixed sleep after a blocking presentation.
use super::*;
use std::time::Duration;

pub(super) use crate::frontend::timing::{MAX_AUDIO_CATCHUP_FRAMES, audio_target_bytes};

pub(super) fn silent_frames_due(deadline: &mut Instant, now: Instant) -> usize {
    if now < *deadline {
        return 0;
    }
    let origin = *deadline;
    let elapsed = now.duration_since(origin);
    let mut relative_deadline = Duration::ZERO;
    let frames = crate::frontend::timing::silent_frames_due(&mut relative_deadline, elapsed);
    *deadline = origin + relative_deadline;
    frames
}

pub(super) struct FramePacer {
    vsync: bool,
    deadline: Instant,
}

impl FramePacer {
    pub(super) fn new(canvas: &Canvas<Window>, interactive: bool) -> Self {
        // SAFETY: the canvas owns this live renderer. SDL reports unsupported
        // renderers explicitly; headless snapshots do not request VSync.
        let vsync =
            interactive && unsafe { sdl3::sys::render::SDL_SetRenderVSync(canvas.raw(), 1) };
        if interactive && !vsync {
            eprintln!("VSync unavailable; using deadline-based presentation pacing");
        }
        Self {
            vsync,
            deadline: Instant::now(),
        }
    }

    pub(super) fn finish_frame(&mut self) {
        if self.vsync {
            return;
        }
        self.deadline = fallback_deadline(self.deadline, Instant::now());
        let wait = self.deadline.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
}

fn fallback_deadline(previous: Instant, now: Instant) -> Instant {
    (previous + Duration::from_nanos(16_666_667)).max(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_accounts_for_render_time_without_double_sleeping() {
        let start = Instant::now();
        assert_eq!(
            fallback_deadline(start, start + Duration::from_millis(4)),
            start + Duration::from_nanos(16_666_667)
        );
        assert_eq!(
            fallback_deadline(start, start + Duration::from_millis(40)),
            start + Duration::from_millis(40)
        );
    }
    #[test]
    fn adapter_matches_original_instant_algorithm_trace() {
        let start = Instant::now();
        let samples = [0, 16_666_667, 16_666_668, 50_000_001, 2_000_000_000];
        let mut adapted = start;
        let mut original = start;
        for nanos in samples {
            let now = start + Duration::from_nanos(nanos);
            let a = silent_frames_due(&mut adapted, now);
            let mut b = 0;
            while original <= now && b < MAX_AUDIO_CATCHUP_FRAMES {
                original += Duration::from_nanos(16_666_667);
                b += 1;
            }
            if b == MAX_AUDIO_CATCHUP_FRAMES && original < now {
                original = now;
            }
            assert_eq!((a, adapted), (b, original));
        }
    }
}
