//! Stick hysteresis and bounded directional menu repeat. Gameplay buttons stay
//! in the existing SNES-layout mask, shared by both emulation cores.
use super::*;
use std::time::Duration;

#[derive(Default)]
pub(super) struct Stick {
    axes: [i8; 2],
}

impl Stick {
    pub(super) fn update(&mut self, axis: usize, value: i16) {
        if let Some(current) = self.axes.get_mut(axis) {
            *current = direction(*current, value);
        }
    }
    pub(super) fn mask(&self) -> u16 {
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
    pub(super) fn menu_key(&self) -> Option<Scancode> {
        match self.axes {
            [_, -1] => Some(Scancode::Up),
            [_, 1] => Some(Scancode::Down),
            [-1, _] => Some(Scancode::Left),
            [1, _] => Some(Scancode::Right),
            _ => None,
        }
    }
    pub(super) fn clear(&mut self) {
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

#[derive(Default)]
pub(super) struct Repeat {
    key: Option<Scancode>,
    due: Option<Instant>,
}

impl Repeat {
    pub(super) fn pressed(&mut self, key: Scancode, now: Instant) {
        self.key = Some(key);
        self.due = Some(now + Duration::from_millis(350));
    }
    pub(super) fn poll(&mut self, key: Option<Scancode>, now: Instant) -> Option<(Scancode, bool)> {
        if key != self.key {
            self.key = key;
            self.due = key.map(|_| now + Duration::from_millis(350));
            return key.map(|key| (key, false));
        }
        if let (Some(key), Some(due)) = (key, self.due)
            && now >= due
        {
            self.due = Some(now + Duration::from_millis(110));
            return Some((key, true));
        }
        None
    }
    pub(super) fn clear(&mut self) {
        self.key = None;
        self.due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn repeat_is_delayed_bounded_and_released() {
        let mut repeat = Repeat::default();
        let now = Instant::now();
        let key = Some(Scancode::Right);
        assert_eq!(repeat.poll(key, now), Some((Scancode::Right, false)));
        assert_eq!(repeat.poll(key, now + Duration::from_millis(100)), None);
        assert_eq!(
            repeat.poll(key, now + Duration::from_secs(3)),
            Some((Scancode::Right, true))
        );
        assert_eq!(repeat.poll(key, now + Duration::from_secs(3)), None);
        repeat.clear();
        assert_eq!(repeat.poll(None, now), None);
    }
}
