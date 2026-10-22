//! Stick hysteresis and bounded directional menu repeat. Gameplay buttons stay
//! in the existing SNES-layout mask, shared by both emulation cores.
use super::*;
use crate::frontend::menu_input::Key;
use std::time::Duration;

#[derive(Default)]
pub(super) struct Stick(crate::frontend::gamepad::Stick);

impl Stick {
    pub(super) fn update(&mut self, axis: usize, value: i16) {
        self.0.update(axis, value);
    }
    pub(super) fn mask(&self) -> u16 {
        self.0.mask()
    }
    pub(super) fn menu_key(&self) -> Option<Key> {
        match self.0.menu_direction() {
            Some(crate::frontend::gamepad::Direction::Up) => Some(Key::Up),
            Some(crate::frontend::gamepad::Direction::Down) => Some(Key::Down),
            Some(crate::frontend::gamepad::Direction::Left) => Some(Key::Left),
            Some(crate::frontend::gamepad::Direction::Right) => Some(Key::Right),
            None => None,
        }
    }
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Default)]
pub(super) struct Repeat {
    policy: crate::frontend::gamepad::Repeat<Key>,
    origin: Option<Instant>,
}

impl Repeat {
    pub(super) fn pressed(&mut self, key: Key, now: Instant) {
        let elapsed = self.elapsed(now);
        self.policy.pressed(key, elapsed);
    }
    pub(super) fn poll(&mut self, key: Option<Key>, now: Instant) -> Option<(Key, bool)> {
        let elapsed = self.elapsed(now);
        self.policy.poll(key, elapsed)
    }
    pub(super) fn clear(&mut self) {
        self.policy.clear();
    }
    fn elapsed(&mut self, now: Instant) -> Duration {
        let origin = *self.origin.get_or_insert(now);
        now.saturating_duration_since(origin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_is_delayed_bounded_and_released() {
        let mut repeat = Repeat::default();
        let now = Instant::now();
        let key = Some(Key::Right);
        assert_eq!(repeat.poll(key, now), Some((Key::Right, false)));
        assert_eq!(repeat.poll(key, now + Duration::from_millis(100)), None);
        assert_eq!(
            repeat.poll(key, now + Duration::from_secs(3)),
            Some((Key::Right, true))
        );
        assert_eq!(repeat.poll(key, now + Duration::from_secs(3)), None);
        repeat.clear();
        assert_eq!(repeat.poll(None, now), None);
    }

    #[test]
    fn repeat_adapter_uses_a_stable_elapsed_origin() {
        let mut repeat = Repeat::default();
        let origin = Instant::now();
        assert_eq!(
            repeat.poll(Some(Key::Right), origin),
            Some((Key::Right, false))
        );
        assert_eq!(
            repeat.poll(Some(Key::Right), origin + Duration::from_millis(349)),
            None
        );
        assert_eq!(
            repeat.poll(Some(Key::Right), origin + Duration::from_millis(350)),
            Some((Key::Right, true))
        );
        repeat.clear();
        assert_eq!(
            repeat.poll(Some(Key::Right), origin + Duration::from_millis(351)),
            Some((Key::Right, false))
        );
    }

    #[test]
    fn repeat_adapter_matches_legacy_instant_policy_on_monotonic_trace() {
        let origin = Instant::now();
        let mut adapter = Repeat::default();
        let mut legacy_key = None;
        let mut legacy_due = None;
        for (key, millis) in [
            (Some(Key::Right), 0),
            (Some(Key::Right), 349),
            (Some(Key::Right), 350),
            (Some(Key::Right), 500),
            (Some(Key::Left), 501),
            (Some(Key::Left), 900),
            (None, 901),
            (Some(Key::Left), 902),
            (Some(Key::Left), 5_000),
        ] {
            let now = origin + Duration::from_millis(millis);
            let actual = adapter.poll(key, now);
            let expected = if key != legacy_key {
                legacy_key = key;
                legacy_due = key.map(|_| now + Duration::from_millis(350));
                key.map(|key| (key, false))
            } else if let (Some(key), Some(due)) = (key, legacy_due)
                && now >= due
            {
                legacy_due = Some(now + Duration::from_millis(110));
                Some((key, true))
            } else {
                None
            };
            assert_eq!(actual, expected);
        }

        let pressed_at = origin + Duration::from_millis(5_001);
        adapter.pressed(Key::Down, pressed_at);
        assert_eq!(adapter.poll(Some(Key::Down), pressed_at), None);
        adapter.clear();
        let after_clear = origin + Duration::from_millis(5_002);
        assert_eq!(
            adapter.poll(Some(Key::Up), after_clear),
            Some((Key::Up, false))
        );
    }
}
