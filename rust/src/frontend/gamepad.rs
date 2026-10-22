//! Backend-neutral main-menu stick hysteresis and SNES-layout button mask.

/// A directional input resolved from the stick for menu navigation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Stick state using the main-menu's engage/release hysteresis policy.
#[derive(Default)]
pub struct Stick {
    axes: [i8; 2],
}

impl Stick {
    /// Update one axis. Axis indices other than 0 (horizontal) and 1 (vertical) are ignored.
    pub fn update(&mut self, axis: usize, value: i16) {
        if let Some(current) = self.axes.get_mut(axis) {
            *current = direction(*current, value);
        }
    }

    /// Return the held directions in the existing SNES button-mask layout.
    pub fn mask(&self) -> u16 {
        (match self.axes[0] {
            -1 => 1 << 6,
            1 => 1 << 7,
            _ => 0,
        }) | (match self.axes[1] {
            -1 => 1 << 4,
            1 => 1 << 5,
            _ => 0,
        })
    }

    /// Resolve menu navigation, prioritizing vertical input over horizontal input.
    pub fn menu_direction(&self) -> Option<Direction> {
        match self.axes {
            [_, -1] => Some(Direction::Up),
            [_, 1] => Some(Direction::Down),
            [-1, _] => Some(Direction::Left),
            [1, _] => Some(Direction::Right),
            _ => None,
        }
    }

    /// Clear all held directions.
    pub fn clear(&mut self) {
        self.axes = [0; 2];
    }
}

fn direction(current: i8, value: i16) -> i8 {
    if value >= 16000 {
        1
    } else if value <= -16000 {
        -1
    } else if current == 1 && value > 10000 {
        1
    } else if current == -1 && value < -10000 {
        -1
    } else {
        0
    }
}

/// Main-menu key repeat policy driven entirely by caller-provided elapsed time.
/// If a future deadline cannot be represented, repetition stops until a new key edge.
pub struct Repeat<K: Copy + Eq> {
    key: Option<K>,
    due: Option<std::time::Duration>,
}

impl<K: Copy + Eq> Default for Repeat<K> {
    fn default() -> Self {
        Self {
            key: None,
            due: None,
        }
    }
}

impl<K: Copy + Eq> Repeat<K> {
    /// Record an already-emitted press and schedule its first repeat.
    pub fn pressed(&mut self, key: K, now: std::time::Duration) {
        self.key = Some(key);
        self.due = now.checked_add(std::time::Duration::from_millis(350));
    }

    /// Emit a new key edge immediately, or at most one repeat when its deadline is due.
    pub fn poll(&mut self, key: Option<K>, now: std::time::Duration) -> Option<(K, bool)> {
        if key != self.key {
            self.key = key;
            self.due = key.and_then(|_| now.checked_add(std::time::Duration::from_millis(350)));
            return key.map(|key| (key, false));
        }
        if let (Some(key), Some(due)) = (key, self.due)
            && now >= due
        {
            self.due = now.checked_add(std::time::Duration::from_millis(110));
            return Some((key, true));
        }
        None
    }

    /// Forget the held key and its repeat deadline.
    pub fn clear(&mut self) {
        self.key = None;
        self.due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestKey {
        A,
        B,
    }

    #[test]
    fn stick_has_deadzone_hysteresis_and_diagonal_gameplay() {
        let mut stick = Stick::default();
        stick.update(0, 15000);
        assert_eq!(stick.mask(), 0);
        stick.update(0, 20000);
        stick.update(0, 12000);
        assert_eq!(stick.mask(), 1 << 7);
        stick.update(1, -22000);
        assert_eq!(stick.mask(), (1 << 7) | (1 << 4));
        stick.update(0, 9000);
        assert_eq!(stick.mask(), 1 << 4);
        stick.clear();
        assert_eq!(stick.mask(), 0);
    }

    #[test]
    fn engage_and_release_boundaries_are_symmetric() {
        for (sign, bit) in [(1, 1 << 7), (-1, 1 << 6)] {
            let mut stick = Stick::default();
            stick.update(0, sign * 15999);
            assert_eq!(stick.mask(), 0);
            stick.update(0, sign * 16000);
            assert_eq!(stick.mask(), bit);
            stick.update(0, sign * 10001);
            assert_eq!(stick.mask(), bit);
            stick.update(0, sign * 10000);
            assert_eq!(stick.mask(), 0);
        }
    }

    #[test]
    fn extreme_axes_and_invalid_axis_are_handled() {
        let mut stick = Stick::default();
        stick.update(0, i16::MAX);
        stick.update(1, i16::MIN);
        assert_eq!(stick.mask(), (1 << 7) | (1 << 4));
        let before = stick.mask();
        stick.update(2, i16::MIN);
        assert_eq!(stick.mask(), before);
    }

    #[test]
    fn menu_directions_and_vertical_diagonal_priority() {
        let cases = [
            (0, -20000, Some(Direction::Up)),
            (0, 20000, Some(Direction::Down)),
            (-20000, 0, Some(Direction::Left)),
            (20000, 0, Some(Direction::Right)),
            (20000, -20000, Some(Direction::Up)),
            (-20000, 20000, Some(Direction::Down)),
            (0, 0, None),
        ];
        for (x, y, expected) in cases {
            let mut stick = Stick::default();
            stick.update(0, x);
            stick.update(1, y);
            assert_eq!(stick.menu_direction(), expected);
        }
    }

    #[test]
    fn repeat_edges_and_exact_deadlines() {
        let mut repeat = Repeat::default();
        let key = Some(TestKey::A);
        assert_eq!(repeat.poll(key, Duration::ZERO), Some((TestKey::A, false)));
        assert_eq!(repeat.poll(key, Duration::from_millis(349)), None);
        assert_eq!(
            repeat.poll(key, Duration::from_millis(350)),
            Some((TestKey::A, true))
        );
        assert_eq!(repeat.poll(key, Duration::from_millis(459)), None);
        assert_eq!(
            repeat.poll(key, Duration::from_millis(460)),
            Some((TestKey::A, true))
        );
    }

    #[test]
    fn repeat_pressed_release_reversal_and_clear() {
        let mut repeat = Repeat::default();
        repeat.pressed(TestKey::A, Duration::ZERO);
        assert_eq!(repeat.poll(Some(TestKey::A), Duration::ZERO), None);
        assert_eq!(
            repeat.poll(Some(TestKey::B), Duration::from_millis(100)),
            Some((TestKey::B, false))
        );
        assert_eq!(repeat.poll(None, Duration::from_millis(101)), None);
        assert_eq!(
            repeat.poll(Some(TestKey::B), Duration::from_millis(102)),
            Some((TestKey::B, false))
        );
        repeat.clear();
        assert_eq!(
            repeat.poll(Some(TestKey::B), Duration::from_millis(103)),
            Some((TestKey::B, false))
        );
    }

    #[test]
    fn stalled_repeat_emits_once_and_clock_exhaustion_stops_repeats() {
        let mut repeat = Repeat::default();
        let key = Some(TestKey::A);
        assert_eq!(repeat.poll(key, Duration::ZERO), Some((TestKey::A, false)));
        assert_eq!(
            repeat.poll(key, Duration::from_secs(10)),
            Some((TestKey::A, true))
        );
        assert_eq!(repeat.poll(key, Duration::from_secs(10)), None);

        repeat.clear();
        repeat.pressed(TestKey::A, Duration::MAX);
        assert_eq!(repeat.poll(key, Duration::MAX), None);

        repeat.clear();
        let near_max = Duration::MAX - Duration::from_millis(350);
        assert_eq!(repeat.poll(key, near_max), Some((TestKey::A, false)));
        assert_eq!(repeat.poll(key, Duration::MAX), Some((TestKey::A, true)));
        assert_eq!(repeat.poll(key, Duration::MAX), None);
    }
}
