//! Host-neutral, single-flight request tracking for launcher image selection.

/// Opaque identity local to one tracker and its private completion channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capability {
    Available,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BeginError {
    AlreadyPending,
    Closed,
    Exhausted,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome<T, E> {
    Selected(T),
    Cancelled,
    Denied(E),
    Failed(E),
}

/// A one-request tracker. IDs are not global and must not be shared across channels.
pub struct RequestTracker {
    next: Option<u64>,
    pending: Option<RequestId>,
    closed: bool,
}

impl Default for RequestTracker {
    fn default() -> Self {
        Self {
            next: Some(0),
            pending: None,
            closed: false,
        }
    }
}

impl RequestTracker {
    pub fn begin(&mut self, capability: Capability) -> Result<RequestId, BeginError> {
        if self.closed {
            return Err(BeginError::Closed);
        }
        if self.pending.is_some() {
            return Err(BeginError::AlreadyPending);
        }
        if capability == Capability::Unsupported {
            return Err(BeginError::Unsupported);
        }
        let id = RequestId(self.next.ok_or(BeginError::Exhausted)?);
        self.next = id.0.checked_add(1);
        self.pending = Some(id);
        Ok(id)
    }

    pub fn complete<T, E>(
        &mut self,
        id: RequestId,
        outcome: Outcome<T, E>,
    ) -> Option<Outcome<T, E>> {
        if self.pending == Some(id) {
            self.pending = None;
            Some(outcome)
        } else {
            None
        }
    }

    pub fn cancel(&mut self, id: RequestId) -> bool {
        if self.pending == Some(id) {
            self.pending = None;
            true
        } else {
            false
        }
    }

    pub fn invalidate(&mut self) {
        self.pending = None;
    }
    pub fn close(&mut self) {
        self.pending = None;
        self.closed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_capabilities_and_duplicate_begin() {
        let mut t = RequestTracker::default();
        assert_eq!(
            t.begin(Capability::Unsupported),
            Err(BeginError::Unsupported)
        );
        let id = t.begin(Capability::Available).unwrap();
        assert_eq!(
            t.begin(Capability::Available),
            Err(BeginError::AlreadyPending)
        );
        assert_eq!(
            t.complete(id, Outcome::<(), ()>::Cancelled),
            Some(Outcome::Cancelled)
        );
    }

    #[test]
    fn stale_duplicate_and_cancelled_old_completion_preserve_new_request() {
        let mut t = RequestTracker::default();
        let old = t.begin(Capability::Available).unwrap();
        assert!(t.cancel(old));
        let new = t.begin(Capability::Available).unwrap();
        assert_eq!(t.complete(old, Outcome::<(), ()>::Cancelled), None);
        assert_eq!(
            t.complete(new, Outcome::<(), ()>::Cancelled),
            Some(Outcome::Cancelled)
        );
        assert_eq!(t.complete(new, Outcome::<(), ()>::Cancelled), None);
    }

    #[test]
    fn invalidate_close_outcomes_and_exhaustion() {
        let mut t = RequestTracker::default();
        let id = t.begin(Capability::Available).unwrap();
        t.invalidate();
        assert_eq!(t.complete(id, Outcome::<(), ()>::Cancelled), None);
        for outcome in [
            Outcome::Selected(1),
            Outcome::Cancelled,
            Outcome::Denied("d"),
            Outcome::Failed("f"),
        ] {
            let id = t.begin(Capability::Available).unwrap();
            assert_eq!(t.complete(id, outcome.clone()), Some(outcome));
        }
        t.next = Some(u64::MAX);
        let max = t.begin(Capability::Available).unwrap();
        assert_eq!(
            t.complete(max, Outcome::<(), ()>::Cancelled),
            Some(Outcome::Cancelled)
        );
        assert_eq!(t.begin(Capability::Available), Err(BeginError::Exhausted));
        t.close();
        assert_eq!(t.begin(Capability::Available), Err(BeginError::Closed));
    }

    #[test]
    fn stale_cancel_and_close_with_outstanding_result_are_rejected() {
        let mut tracker = RequestTracker::default();
        let old = tracker.begin(Capability::Available).unwrap();
        tracker.cancel(old);
        let current = tracker.begin(Capability::Available).unwrap();
        assert!(!tracker.cancel(old));
        tracker.close();
        assert_eq!(
            tracker.complete(current, Outcome::<(), ()>::Cancelled),
            None
        );
        assert_eq!(
            tracker.begin(Capability::Available),
            Err(BeginError::Closed)
        );
    }
}
