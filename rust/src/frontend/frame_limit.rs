/// Rendering count and screenshot/exit boundaries for a native frame driver.
/// A zero limit means unbounded execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLimit {
    limit: usize,
    rendered: usize,
}

impl FrameLimit {
    pub const fn new(limit: usize) -> Self {
        Self { limit, rendered: 0 }
    }

    /// Whether the frame about to be presented is the finite run's last frame.
    pub const fn capture_due(self) -> bool {
        self.limit != 0 && self.rendered.saturating_add(1) >= self.limit
    }

    /// Record a presented frame and report whether the finite run has ended.
    pub fn record_presented(&mut self) -> bool {
        self.rendered = self.rendered.saturating_add(1);
        self.limit != 0 && self.rendered >= self.limit
    }

    #[cfg(test)]
    fn with_rendered(limit: usize, rendered: usize) -> Self {
        Self { limit, rendered }
    }
}

#[cfg(test)]
mod tests {
    use super::FrameLimit;

    #[test]
    fn unlimited_and_finite_capture_and_exit_boundaries() {
        let mut unlimited = FrameLimit::new(0);
        assert!(!unlimited.capture_due());
        assert!(!unlimited.record_presented());
        assert!(!unlimited.capture_due());
        assert!(!unlimited.record_presented());

        let mut one = FrameLimit::new(1);
        assert!(one.capture_due());
        assert!(one.record_presented());

        let mut two = FrameLimit::new(2);
        assert!(!two.capture_due());
        assert!(!two.record_presented());
        assert!(two.capture_due());
        assert!(two.record_presented());
    }

    #[test]
    fn last_frame_capture_and_counter_saturation_are_overflow_safe() {
        let mut last = FrameLimit::with_rendered(3, 2);
        assert!(last.capture_due());
        assert!(last.record_presented());

        let mut exhausted = FrameLimit::with_rendered(usize::MAX, usize::MAX);
        assert!(exhausted.capture_due());
        assert!(exhausted.record_presented());
        assert_eq!(exhausted.rendered, usize::MAX);
    }
}
