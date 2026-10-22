//! Independent characterization of the pre-extraction resume-panel key branch.
use clover_portable::frontend::suspend_navigation::{Effect, Input, State, transition};

fn legacy(mut state: State, input: Input) -> (State, Vec<&'static str>) {
    let action = match input {
        Input::Escape => Some("dismiss"),
        Input::Up if !state.controls => Some("dismiss"),
        Input::Up => {
            state.controls = false;
            None
        }
        Input::Down | Input::Tab => {
            state.controls = true;
            state.focus = 0;
            None
        }
        Input::Left | Input::Right => {
            let direction = if input == Input::Left { -1 } else { 1 };
            if state.controls {
                state.focus = (state.focus as i32 + direction).rem_euclid(5) as usize;
                None
            } else {
                state.slot = (state.slot as i32 + direction).rem_euclid(4) as usize;
                Some("save-settings")
            }
        }
        Input::Confirm { repeat: true, .. } => None,
        Input::Confirm { nes, pending, .. } => {
            if state.controls {
                Some(if nes && state.focus == 0 {
                    "activate-choose-suspend"
                } else {
                    [
                        "activate-save",
                        "activate-resume",
                        "activate-lock",
                        "activate-delete",
                        "activate-close",
                    ][state.focus.min(4)]
                })
            } else if pending {
                Some("request-save")
            } else {
                Some("resume-slot")
            }
        }
        Input::Save => Some("request-save"),
        Input::Resume => Some("resume-slot"),
        Input::Continue => Some("resume-pending"),
        Input::Lock => Some("toggle-lock"),
        Input::Delete => Some("request-delete"),
        Input::Other => None,
    };
    (state, action.into_iter().collect())
}

fn effect_name(effect: Effect) -> &'static str {
    match effect {
        Effect::SaveSettings => "save-settings",
        Effect::Dismiss => "dismiss",
        Effect::ActivateSave => "activate-save",
        Effect::ActivateResume => "activate-resume",
        Effect::ActivateLock => "activate-lock",
        Effect::ActivateDelete => "activate-delete",
        Effect::ActivateClose => "activate-close",
        Effect::ActivateChooseSuspend => "activate-choose-suspend",
        Effect::RequestSave => "request-save",
        Effect::ResumeSlot => "resume-slot",
        Effect::ResumePending => "resume-pending",
        Effect::ToggleLock => "toggle-lock",
        Effect::RequestDelete => "request-delete",
    }
}

#[test]
fn all_valid_picker_states_match_legacy_navigation_and_effect_routes() {
    let mut inputs = vec![
        Input::Left,
        Input::Right,
        Input::Up,
        Input::Down,
        Input::Tab,
        Input::Escape,
        Input::Save,
        Input::Resume,
        Input::Continue,
        Input::Lock,
        Input::Delete,
        Input::Other,
    ];
    for repeat in [false, true] {
        for pending in [false, true] {
            for nes in [false, true] {
                inputs.push(Input::Confirm {
                    repeat,
                    pending,
                    nes,
                });
            }
        }
    }
    for slot in 0..4 {
        for focus in 0..5 {
            for controls in [false, true] {
                let state = State {
                    slot,
                    focus,
                    controls,
                };
                for &input in &inputs {
                    let (next, effects) = transition(state, input);
                    let effects: Vec<_> = effects.into_iter().map(effect_name).collect();
                    assert_eq!(
                        (next, effects),
                        legacy(state, input),
                        "{state:?}, {input:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_focus_does_not_turn_noop_or_close_into_a_destructive_action() {
    let state = State {
        slot: 0,
        focus: usize::MAX,
        controls: true,
    };
    assert_eq!(transition(state, Input::Other), (state, vec![]));
    for nes in [false, true] {
        let input = Input::Confirm {
            repeat: false,
            pending: true,
            nes,
        };
        let (next, effects) = transition(state, input);
        assert_eq!(next, state);
        assert_eq!(effects, vec![Effect::ActivateClose]);
    }
}
