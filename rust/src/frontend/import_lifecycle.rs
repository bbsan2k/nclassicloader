//! Platform-neutral lifecycle policy for one import operation.
//!
//! `Committing` marks the point after which cancellation is too late; it does
//! not imply that publication or durable storage has succeeded.

/// The lifecycle state of one import operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum State {
    /// The operation has not been started.
    #[default]
    Created,
    /// The operation is running and may still be cancelled.
    Running,
    /// Cancellation has been requested and must be acknowledged at a
    /// checkpoint or commit boundary.
    CancelRequested,
    /// The operation crossed the cancellation boundary and is attempting to
    /// publish its result.
    Committing,
    /// The operation successfully completed.
    Completed,
    /// Cancellation was acknowledged.
    Cancelled,
    /// The operation failed.
    Failed,
}

/// Errors returned when starting an import lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    /// Cancellation was requested before the operation started.
    Cancelled,
    /// The lifecycle has already started or is otherwise no longer startable.
    AlreadyStarted,
}

/// Outcome of requesting cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelResult {
    /// This request changed a cancellable state to `CancelRequested`.
    Requested,
    /// Cancellation had already been requested.
    AlreadyRequested,
    /// The operation crossed the commit boundary or completed successfully.
    TooLate,
    /// The operation is terminal and cannot accept cancellation.
    Finished,
}

/// Error returned by a checkpoint or a commit-boundary attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckError {
    /// Cancellation has been acknowledged (or was acknowledged by this call).
    Cancelled,
    /// The operation is not in a state where this transition is valid.
    InvalidState,
}

/// State machine for one import operation. A lifecycle is never reusable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lifecycle {
    state: State,
    started: bool,
}

impl Lifecycle {
    /// Starts a newly-created import operation.
    pub fn start(&mut self) -> Result<(), StartError> {
        match self.state {
            State::Created => {
                self.state = State::Running;
                self.started = true;
                Ok(())
            }
            State::CancelRequested => {
                if self.started {
                    Err(StartError::AlreadyStarted)
                } else {
                    self.state = State::Cancelled;
                    Err(StartError::Cancelled)
                }
            }
            _ => Err(StartError::AlreadyStarted),
        }
    }

    /// Requests cancellation if the operation has not crossed its commit
    /// boundary.
    pub fn request_cancel(&mut self) -> CancelResult {
        match self.state {
            State::Created | State::Running => {
                self.state = State::CancelRequested;
                CancelResult::Requested
            }
            State::CancelRequested => CancelResult::AlreadyRequested,
            State::Committing | State::Completed => CancelResult::TooLate,
            State::Cancelled | State::Failed => CancelResult::Finished,
        }
    }

    /// Checks whether work may continue. A pending cancellation is
    /// acknowledged by transitioning to `Cancelled`.
    pub fn checkpoint(&mut self) -> Result<(), CheckError> {
        match self.state {
            State::Running => Ok(()),
            State::CancelRequested => {
                self.state = State::Cancelled;
                Err(CheckError::Cancelled)
            }
            State::Cancelled => Err(CheckError::Cancelled),
            _ => Err(CheckError::InvalidState),
        }
    }

    /// Crosses the boundary after which cancellation is too late.
    pub fn begin_commit(&mut self) -> Result<(), CheckError> {
        match self.state {
            State::Running => {
                self.state = State::Committing;
                Ok(())
            }
            State::CancelRequested => {
                self.state = State::Cancelled;
                Err(CheckError::Cancelled)
            }
            State::Cancelled => Err(CheckError::Cancelled),
            _ => Err(CheckError::InvalidState),
        }
    }

    /// Marks successful completion, only from `Committing`.
    pub fn finish_success(&mut self) -> bool {
        if self.state == State::Committing {
            self.state = State::Completed;
            true
        } else {
            false
        }
    }

    /// Marks failure without overwriting an acknowledged cancellation or a
    /// completed lifecycle.
    pub fn finish_failure(&mut self) -> bool {
        match self.state {
            State::Running | State::CancelRequested | State::Committing => {
                self.state = State::Failed;
                true
            }
            State::Created | State::Completed | State::Cancelled | State::Failed => false,
        }
    }

    /// Returns the current lifecycle state without allowing mutation.
    pub fn state(&self) -> State {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::{CancelResult, CheckError, Lifecycle, StartError, State};

    #[derive(Clone, Copy)]
    enum Operation {
        Start,
        RequestCancel,
        Checkpoint,
        BeginCommit,
        FinishSuccess,
        FinishFailure,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Outcome {
        Started(Result<(), StartError>),
        CancelRequested(CancelResult),
        Checked(Result<(), CheckError>),
        CommitStarted(Result<(), CheckError>),
        Succeeded(bool),
        Failed(bool),
    }

    fn apply(lifecycle: &mut Lifecycle, operation: Operation) -> Outcome {
        match operation {
            Operation::Start => Outcome::Started(lifecycle.start()),
            Operation::RequestCancel => Outcome::CancelRequested(lifecycle.request_cancel()),
            Operation::Checkpoint => Outcome::Checked(lifecycle.checkpoint()),
            Operation::BeginCommit => Outcome::CommitStarted(lifecycle.begin_commit()),
            Operation::FinishSuccess => Outcome::Succeeded(lifecycle.finish_success()),
            Operation::FinishFailure => Outcome::Failed(lifecycle.finish_failure()),
        }
    }

    fn assert_transition(
        initial: State,
        operation: Operation,
        expected_outcome: Outcome,
        expected_state: State,
    ) {
        let mut lifecycle = Lifecycle {
            state: initial,
            started: initial != State::Created && initial != State::CancelRequested,
        };
        assert_eq!(apply(&mut lifecycle, operation), expected_outcome);
        assert_eq!(lifecycle.state(), expected_state);
    }

    #[test]
    fn start_transition_table() {
        use Outcome::Started;
        for (initial, outcome, expected) in [
            (State::Created, Started(Ok(())), State::Running),
            (
                State::Running,
                Started(Err(StartError::AlreadyStarted)),
                State::Running,
            ),
            (
                State::CancelRequested,
                Started(Err(StartError::Cancelled)),
                State::Cancelled,
            ),
            (
                State::Committing,
                Started(Err(StartError::AlreadyStarted)),
                State::Committing,
            ),
            (
                State::Completed,
                Started(Err(StartError::AlreadyStarted)),
                State::Completed,
            ),
            (
                State::Cancelled,
                Started(Err(StartError::AlreadyStarted)),
                State::Cancelled,
            ),
            (
                State::Failed,
                Started(Err(StartError::AlreadyStarted)),
                State::Failed,
            ),
        ] {
            assert_transition(initial, Operation::Start, outcome, expected);
        }

        let mut running_then_cancelled = Lifecycle::default();
        assert_eq!(running_then_cancelled.start(), Ok(()));
        assert_eq!(
            running_then_cancelled.request_cancel(),
            CancelResult::Requested
        );
        assert_eq!(
            running_then_cancelled.start(),
            Err(StartError::AlreadyStarted)
        );
        assert_eq!(running_then_cancelled.state(), State::CancelRequested);
        assert_eq!(
            running_then_cancelled.checkpoint(),
            Err(CheckError::Cancelled)
        );
        assert_eq!(running_then_cancelled.state(), State::Cancelled);
    }

    #[test]
    fn cancellation_transition_table() {
        use Outcome::CancelRequested;
        for (initial, outcome, expected) in [
            (
                State::Created,
                CancelRequested(CancelResult::Requested),
                State::CancelRequested,
            ),
            (
                State::Running,
                CancelRequested(CancelResult::Requested),
                State::CancelRequested,
            ),
            (
                State::CancelRequested,
                CancelRequested(CancelResult::AlreadyRequested),
                State::CancelRequested,
            ),
            (
                State::Committing,
                CancelRequested(CancelResult::TooLate),
                State::Committing,
            ),
            (
                State::Completed,
                CancelRequested(CancelResult::TooLate),
                State::Completed,
            ),
            (
                State::Cancelled,
                CancelRequested(CancelResult::Finished),
                State::Cancelled,
            ),
            (
                State::Failed,
                CancelRequested(CancelResult::Finished),
                State::Failed,
            ),
        ] {
            assert_transition(initial, Operation::RequestCancel, outcome, expected);
        }
    }

    #[test]
    fn checkpoint_transition_table() {
        use Outcome::Checked;
        for (initial, outcome, expected) in [
            (
                State::Created,
                Checked(Err(CheckError::InvalidState)),
                State::Created,
            ),
            (State::Running, Checked(Ok(())), State::Running),
            (
                State::CancelRequested,
                Checked(Err(CheckError::Cancelled)),
                State::Cancelled,
            ),
            (
                State::Committing,
                Checked(Err(CheckError::InvalidState)),
                State::Committing,
            ),
            (
                State::Completed,
                Checked(Err(CheckError::InvalidState)),
                State::Completed,
            ),
            (
                State::Cancelled,
                Checked(Err(CheckError::Cancelled)),
                State::Cancelled,
            ),
            (
                State::Failed,
                Checked(Err(CheckError::InvalidState)),
                State::Failed,
            ),
        ] {
            assert_transition(initial, Operation::Checkpoint, outcome, expected);
        }
    }

    #[test]
    fn begin_commit_transition_table() {
        use Outcome::CommitStarted;
        for (initial, outcome, expected) in [
            (
                State::Created,
                CommitStarted(Err(CheckError::InvalidState)),
                State::Created,
            ),
            (State::Running, CommitStarted(Ok(())), State::Committing),
            (
                State::CancelRequested,
                CommitStarted(Err(CheckError::Cancelled)),
                State::Cancelled,
            ),
            (
                State::Committing,
                CommitStarted(Err(CheckError::InvalidState)),
                State::Committing,
            ),
            (
                State::Completed,
                CommitStarted(Err(CheckError::InvalidState)),
                State::Completed,
            ),
            (
                State::Cancelled,
                CommitStarted(Err(CheckError::Cancelled)),
                State::Cancelled,
            ),
            (
                State::Failed,
                CommitStarted(Err(CheckError::InvalidState)),
                State::Failed,
            ),
        ] {
            assert_transition(initial, Operation::BeginCommit, outcome, expected);
        }
    }

    #[test]
    fn finish_success_transition_table() {
        use Outcome::Succeeded;
        for (initial, outcome, expected) in [
            (State::Created, Succeeded(false), State::Created),
            (State::Running, Succeeded(false), State::Running),
            (
                State::CancelRequested,
                Succeeded(false),
                State::CancelRequested,
            ),
            (State::Committing, Succeeded(true), State::Completed),
            (State::Completed, Succeeded(false), State::Completed),
            (State::Cancelled, Succeeded(false), State::Cancelled),
            (State::Failed, Succeeded(false), State::Failed),
        ] {
            assert_transition(initial, Operation::FinishSuccess, outcome, expected);
        }
    }

    #[test]
    fn finish_failure_transition_table() {
        use Outcome::Failed;
        for (initial, outcome, expected) in [
            (State::Created, Failed(false), State::Created),
            (State::Running, Failed(true), State::Failed),
            (State::CancelRequested, Failed(true), State::Failed),
            (State::Committing, Failed(true), State::Failed),
            (State::Completed, Failed(false), State::Completed),
            (State::Cancelled, Failed(false), State::Cancelled),
            (State::Failed, Failed(false), State::Failed),
        ] {
            assert_transition(initial, Operation::FinishFailure, outcome, expected);
        }
    }

    #[test]
    fn cancellation_ordering_and_terminal_reuse() {
        let mut before_start = Lifecycle::default();
        assert_eq!(before_start.request_cancel(), CancelResult::Requested);
        assert_eq!(before_start.start(), Err(StartError::Cancelled));
        assert_eq!(before_start.state(), State::Cancelled);

        let mut before_commit = Lifecycle::default();
        assert_eq!(before_commit.start(), Ok(()));
        assert_eq!(before_commit.request_cancel(), CancelResult::Requested);
        assert_eq!(before_commit.begin_commit(), Err(CheckError::Cancelled));
        assert_eq!(before_commit.state(), State::Cancelled);
        assert!(!before_commit.finish_failure());
        assert_eq!(before_commit.start(), Err(StartError::AlreadyStarted));

        let mut after_commit = Lifecycle::default();
        assert_eq!(after_commit.start(), Ok(()));
        assert_eq!(after_commit.begin_commit(), Ok(()));
        assert_eq!(after_commit.request_cancel(), CancelResult::TooLate);
        assert_eq!(after_commit.state(), State::Committing);
        assert!(after_commit.finish_success());
        assert_eq!(after_commit.state(), State::Completed);
        assert!(!after_commit.finish_failure());
        assert_eq!(after_commit.start(), Err(StartError::AlreadyStarted));
    }

    #[test]
    fn failure_after_request_is_failure_not_cancel_acknowledgement() {
        let mut lifecycle = Lifecycle::default();
        assert_eq!(lifecycle.start(), Ok(()));
        assert_eq!(lifecycle.request_cancel(), CancelResult::Requested);
        assert!(lifecycle.finish_failure());
        assert_eq!(lifecycle.state(), State::Failed);
        assert_eq!(lifecycle.request_cancel(), CancelResult::Finished);
    }
}
