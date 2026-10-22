//! Platform-independent audio buffering and silent-frame catch-up policy.
use std::time::Duration;

pub const AUDIO_BUFFER_MS: usize = 60;
pub const MAX_AUDIO_CATCHUP_FRAMES: usize = 6;
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

pub fn audio_target_bytes(rate: u32) -> usize {
    rate as usize * 2 * std::mem::size_of::<f32>() * AUDIO_BUFFER_MS / 1000
}

/// Advance frame deadlines due at `now`, limiting catch-up and dropping excess backlog.
pub fn silent_frames_due(deadline: &mut Duration, now: Duration) -> usize {
    let mut frames = 0;
    while *deadline <= now && frames < MAX_AUDIO_CATCHUP_FRAMES {
        *deadline += FRAME_INTERVAL;
        frames += 1;
    }
    if frames == MAX_AUDIO_CATCHUP_FRAMES && *deadline < now {
        *deadline = now;
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_float_buffer_is_rate_based_not_display_frame_based() {
        assert_eq!(audio_target_bytes(48_000), 23_040);
        assert_eq!(audio_target_bytes(44_100), 21_168);
        assert!(MAX_AUDIO_CATCHUP_FRAMES * 798 * 8 > audio_target_bytes(48_000));
    }

    #[test]
    fn missing_audio_device_does_not_double_speed_on_high_refresh_displays() {
        for hz in [50_u32, 60, 120] {
            let mut deadline = Duration::ZERO;
            let count: usize = (0..=hz * 2)
                .map(|tick| {
                    silent_frames_due(
                        &mut deadline,
                        Duration::from_nanos(tick as u64 * 1_000_000_000 / hz as u64),
                    )
                })
                .sum();
            assert_eq!(count, 120, "{hz}Hz");
        }
    }

    #[test]
    fn original_120hz_120_tick_trace_produces_60_frames() {
        let mut deadline = Duration::ZERO;
        let count: usize = (0..120)
            .map(|tick| {
                silent_frames_due(
                    &mut deadline,
                    Duration::from_nanos(tick * 1_000_000_000 / 120),
                )
            })
            .sum();
        assert_eq!(count, 60);
    }

    #[test]
    fn future_and_exact_due_deadlines() {
        let interval = FRAME_INTERVAL;
        let mut deadline = interval;
        assert_eq!(silent_frames_due(&mut deadline, Duration::ZERO), 0);
        assert_eq!(silent_frames_due(&mut deadline, interval), 1);
        assert_eq!(deadline, interval * 2);
    }

    #[test]
    fn severe_stall_is_capped_and_drops_backlog() {
        let mut deadline = Duration::ZERO;
        let now = FRAME_INTERVAL * 100;
        assert_eq!(
            silent_frames_due(&mut deadline, now),
            MAX_AUDIO_CATCHUP_FRAMES
        );
        assert_eq!(deadline, now);
        assert_eq!(silent_frames_due(&mut deadline, now), 1);
    }
}
