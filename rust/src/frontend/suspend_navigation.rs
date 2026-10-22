//! Pure keyboard policy for the suspend panel.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub slot: usize,
    pub focus: usize,
    pub controls: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Left,
    Right,
    Up,
    Down,
    Tab,
    Confirm {
        repeat: bool,
        pending: bool,
        nes: bool,
    },
    Escape,
    Save,
    Resume,
    Continue,
    Lock,
    Delete,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    SaveSettings,
    Dismiss,
    ActivateSave,
    ActivateResume,
    ActivateLock,
    ActivateDelete,
    ActivateClose,
    ActivateChooseSuspend,
    RequestSave,
    ResumeSlot,
    ResumePending,
    ToggleLock,
    RequestDelete,
}

pub fn transition(mut state: State, input: Input) -> (State, Vec<Effect>) {
    let effects = match input {
        Input::Escape => vec![Effect::Dismiss],
        Input::Up if !state.controls => vec![Effect::Dismiss],
        Input::Up => {
            state.controls = false;
            vec![]
        }
        Input::Down | Input::Tab => {
            state.controls = true;
            state.focus = 0;
            vec![]
        }
        Input::Left | Input::Right => {
            if state.controls {
                state.focus = if input == Input::Left {
                    (state.focus % 5 + 4) % 5
                } else {
                    (state.focus % 5 + 1) % 5
                };
                vec![]
            } else {
                state.slot = if input == Input::Left {
                    (state.slot % 4 + 3) % 4
                } else {
                    (state.slot % 4 + 1) % 4
                };
                vec![Effect::SaveSettings]
            }
        }
        Input::Confirm { repeat: true, .. } => vec![],
        Input::Confirm { nes, pending, .. } => {
            if state.controls {
                vec![if nes && state.focus == 0 {
                    Effect::ActivateChooseSuspend
                } else {
                    [
                        Effect::ActivateSave,
                        Effect::ActivateResume,
                        Effect::ActivateLock,
                        Effect::ActivateDelete,
                        Effect::ActivateClose,
                    ][state.focus.min(4)]
                }]
            } else if pending {
                vec![Effect::RequestSave]
            } else {
                vec![Effect::ResumeSlot]
            }
        }
        Input::Save => vec![Effect::RequestSave],
        Input::Resume => vec![Effect::ResumeSlot],
        Input::Continue => vec![Effect::ResumePending],
        Input::Lock => vec![Effect::ToggleLock],
        Input::Delete => vec![Effect::RequestDelete],
        Input::Other => vec![],
    };
    (state, effects)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(slot: usize, focus: usize, controls: bool) -> State {
        State {
            slot,
            focus,
            controls,
        }
    }
    #[test]
    fn wraps_and_transitions() {
        assert_eq!(transition(s(0, 0, false), Input::Left).0.slot, 3);
        assert_eq!(transition(s(3, 0, false), Input::Right).0.slot, 0);
        assert_eq!(transition(s(1, 0, false), Input::Left).0.slot, 0);
        assert_eq!(transition(s(2, 0, false), Input::Right).0.slot, 3);
        assert_eq!(transition(s(0, 0, true), Input::Left).0.focus, 4);
        assert_eq!(transition(s(0, 4, true), Input::Right).0.focus, 0);
        assert_eq!(transition(s(0, 1, true), Input::Left).0.focus, 0);
        assert_eq!(transition(s(0, 3, true), Input::Right).0.focus, 4);
        assert_eq!(
            transition(s(0, 2, false), Input::Up).1,
            vec![Effect::Dismiss]
        );
        assert_eq!(transition(s(0, 2, true), Input::Up).0, s(0, 2, false));
        assert!(transition(s(0, 4, false), Input::Down).0.controls);
        assert_eq!(transition(s(0, 4, false), Input::Tab).0.focus, 0);
        assert_eq!(transition(s(0, 4, false), Input::Down).0.focus, 0);
    }
    #[test]
    fn actions_and_bounds() {
        assert_eq!(
            transition(
                s(0, 0, true),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: true
                }
            )
            .1,
            vec![Effect::ActivateChooseSuspend]
        );
        assert_eq!(
            transition(
                s(0, 0, false),
                Input::Confirm {
                    repeat: false,
                    pending: true,
                    nes: false
                }
            )
            .1,
            vec![Effect::RequestSave]
        );
        assert_eq!(
            transition(
                s(0, 0, false),
                Input::Confirm {
                    repeat: true,
                    pending: false,
                    nes: false
                }
            )
            .1,
            Vec::<Effect>::new()
        );
        assert_eq!(transition(s(8, 8, false), Input::Other).0, s(8, 8, false));
        assert_eq!(
            transition(
                s(0, 8, true),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: false
                }
            )
            .1,
            vec![Effect::ActivateClose]
        );
        assert_eq!(
            transition(
                s(0, 0, true),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: false
                }
            )
            .1,
            vec![Effect::ActivateSave]
        );
        for (focus, expected) in [
            (1, Effect::ActivateResume),
            (2, Effect::ActivateLock),
            (3, Effect::ActivateDelete),
            (4, Effect::ActivateClose),
        ] {
            assert_eq!(
                transition(
                    s(0, focus, true),
                    Input::Confirm {
                        repeat: false,
                        pending: false,
                        nes: true
                    }
                )
                .1,
                vec![expected]
            );
        }
        assert_eq!(
            transition(
                s(0, 0, true),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: true
                }
            )
            .1,
            vec![Effect::ActivateChooseSuspend]
        );
        assert_eq!(
            transition(
                s(0, 0, false),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: true
                }
            )
            .1,
            vec![Effect::ResumeSlot]
        );
        assert_eq!(
            transition(
                s(0, 0, false),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: false
                }
            )
            .1,
            vec![Effect::ResumeSlot]
        );
        assert_eq!(
            transition(
                s(0, 0, false),
                Input::Confirm {
                    repeat: false,
                    pending: true,
                    nes: false
                }
            )
            .1,
            vec![Effect::RequestSave]
        );
        let state = s(2, 3, true);
        assert_eq!(
            transition(
                state,
                Input::Confirm {
                    repeat: true,
                    pending: true,
                    nes: true
                }
            ),
            (state, vec![])
        );
        assert_eq!(transition(state, Input::Escape).1, vec![Effect::Dismiss]);
        assert_eq!(
            transition(
                s(0, 4, true),
                Input::Confirm {
                    repeat: false,
                    pending: false,
                    nes: false
                }
            )
            .1,
            vec![Effect::ActivateClose]
        );
        for (input, expected) in [
            (Input::Save, Effect::RequestSave),
            (Input::Resume, Effect::ResumeSlot),
            (Input::Continue, Effect::ResumePending),
            (Input::Lock, Effect::ToggleLock),
            (Input::Delete, Effect::RequestDelete),
        ] {
            assert_eq!(transition(state, input).1, vec![expected]);
        }
        // Directional repeats are not filtered by the policy; the adapter maps them identically.
        assert_eq!(transition(s(0, 0, false), Input::Right).0.slot, 1);
    }
}
