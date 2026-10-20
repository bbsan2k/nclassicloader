//! Presentation follows the display when SDL supports VSync; fallback pacing
//! uses deadlines, never an extra fixed sleep after a blocking presentation.
use super::*;
use std::time::Duration;

pub(super) const AUDIO_BUFFER_MS: usize = 60;
pub(super) const MAX_AUDIO_CATCHUP_FRAMES: usize = 6;

pub(super) fn audio_target_bytes(rate: u32) -> usize {
    rate as usize * 2 * std::mem::size_of::<f32>() * AUDIO_BUFFER_MS / 1000
}

pub(super) fn silent_frames_due(deadline: &mut Instant, now: Instant) -> usize {
    let mut frames = 0;
    while *deadline <= now && frames < MAX_AUDIO_CATCHUP_FRAMES {
        *deadline += Duration::from_nanos(16_666_667);
        frames += 1;
    }
    if frames == MAX_AUDIO_CATCHUP_FRAMES && *deadline < now {
        *deadline = now;
    }
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
    fn stereo_float_buffer_is_rate_based_not_display_frame_based() {
        assert_eq!(audio_target_bytes(48_000), 23_040);
        assert_eq!(audio_target_bytes(44_100), 21_168);
        assert!(MAX_AUDIO_CATCHUP_FRAMES * 798 * 8 > audio_target_bytes(48_000));
    }
    #[test]
    fn missing_audio_device_does_not_double_speed_on_high_refresh_displays() {
        let start = Instant::now();
        let mut deadline = start;
        let count: usize = (0..120)
            .map(|tick| {
                silent_frames_due(
                    &mut deadline,
                    start + Duration::from_nanos(tick * 1_000_000_000 / 120),
                )
            })
            .sum();
        assert_eq!(count, 60);
    }
}
