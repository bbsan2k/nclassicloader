//! Small host-independent coordination for one launcher input frame.
use crate::frontend::{
    launcher_activity::{self, Activity, ImportOutcome},
    launcher_navigation::{self, Event as NavigationEvent, Item, Navigation},
    library::LibraryFilter,
};

/// Maximum raw host inputs reduced in one launcher input frame.
pub const INPUT_BUDGET: usize = 128;
/// Maximum owner-validated host notifications reduced in one launcher tick.
pub const COMPLETION_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputFrame {
    pending_activation: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    First,
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostEffect {
    CancelImport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    Selected,
    ImagePicker,
    FolderPicker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramePlan {
    pub activation: Option<Activation>,
    pub shutdown_services: bool,
    pub exit: bool,
    pub persist_filter: Option<LibraryFilter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerOutcome {
    SelectedImage,
    SelectedFolder,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerDecision {
    Rejected,
    Done,
    ImportImage,
    ResolveFolder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportDecision {
    OpenImported,
    Stay,
    Exit,
}

pub fn source_allowed(busy: bool, closing: bool, picker_pending: bool) -> bool {
    !busy && !closing && !picker_pending
}

/// Apply one owner-validated picker completion and decide whether its native
/// selected path may be handed to the corresponding host operation.
pub fn complete_picker(activity: &mut Activity, outcome: &PickerOutcome) -> PickerDecision {
    let error = match outcome {
        PickerOutcome::Failed(message) => Some(message.clone()),
        _ => None,
    };
    if !activity.reduce(launcher_activity::Event::PickerFinished { error }) {
        return PickerDecision::Rejected;
    }
    if activity.busy || activity.closing {
        return PickerDecision::Done;
    }
    match outcome {
        PickerOutcome::SelectedImage => PickerDecision::ImportImage,
        PickerOutcome::SelectedFolder => PickerDecision::ResolveFolder,
        PickerOutcome::Cancelled | PickerOutcome::Failed(_) => PickerDecision::Done,
    }
}

/// Apply one owner-validated import completion. Activity remains the authority
/// for rejecting duplicate or out-of-phase results.
pub fn complete_import(activity: &mut Activity, outcome: ImportOutcome) -> Option<ImportDecision> {
    if !activity.reduce(launcher_activity::Event::ImportFinished(outcome.clone())) {
        return None;
    }
    if activity.closing {
        Some(ImportDecision::Exit)
    } else if outcome == ImportOutcome::Succeeded {
        Some(ImportDecision::OpenImported)
    } else {
        Some(ImportDecision::Stay)
    }
}

/// Set an error for a host operation without changing activity phase or
/// replacing the closing screen's existing notice.
pub fn host_error(activity: &mut Activity, message: String) -> bool {
    if activity.closing {
        return false;
    }
    activity.status = message;
    activity.error = true;
    true
}

impl InputFrame {
    pub fn capture_activation(&mut self, index: i32, capture: Capture) {
        if !(0..=2).contains(&index) {
            return;
        }
        match capture {
            Capture::First => {
                self.pending_activation.get_or_insert(index);
            }
            Capture::Replace => self.pending_activation = Some(index),
        }
    }

    pub fn request_close(&mut self, activity: &mut Activity) {
        self.pending_activation = None;
        activity.reduce(launcher_activity::Event::CloseRequested);
    }

    pub fn reduce_navigation(
        &mut self,
        navigation: &mut Navigation,
        activity: &mut Activity,
        items: &[Item],
        focused: bool,
        event: NavigationEvent,
        capture: Capture,
    ) -> Option<HostEffect> {
        let effect = navigation.reduce(
            items,
            launcher_navigation::Gate {
                busy: activity.busy,
                picker_pending: activity.picker_pending,
                closing: activity.closing,
                focused,
            },
            event,
        );
        match effect {
            Some(launcher_navigation::Effect::Activate(index)) => {
                self.capture_activation(index, capture);
                None
            }
            Some(launcher_navigation::Effect::Close) => {
                self.request_close(activity);
                None
            }
            Some(launcher_navigation::Effect::CancelImport) => Some(HostEffect::CancelImport),
            None => None,
        }
    }

    pub fn finish_input(
        &mut self,
        navigation: &Navigation,
        activity: &Activity,
        items: &[Item],
        saved_filter: Option<LibraryFilter>,
    ) -> FramePlan {
        let activation = if activity.busy || activity.picker_pending || activity.closing {
            None
        } else {
            self.pending_activation.and_then(|index| match index {
                0 if !items
                    .iter()
                    .any(|item| navigation.filter.matches(item.family)) =>
                {
                    Some(Activation::ImagePicker)
                }
                0 => Some(Activation::Selected),
                1 => Some(Activation::ImagePicker),
                2 => Some(Activation::FolderPicker),
                _ => None,
            })
        };
        self.pending_activation = None;
        FramePlan {
            activation,
            shutdown_services: activity.closing,
            exit: activity.closing && !activity.busy,
            persist_filter: saved_filter
                .filter(|filter| *filter != navigation.filter)
                .map(|_| navigation.filter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::library::SystemFamily;

    fn items() -> Vec<Item> {
        vec![Item {
            key: "item".into(),
            family: SystemFamily::Nes,
        }]
    }

    fn focused() -> bool {
        true
    }

    fn reduce(
        frame: &mut InputFrame,
        nav: &mut Navigation,
        activity: &mut Activity,
        event: NavigationEvent,
        capture: Capture,
    ) -> Option<HostEffect> {
        frame.reduce_navigation(nav, activity, &items(), focused(), event, capture)
    }

    #[test]
    fn first_capture_wins_and_replace_capture_replaces_it() {
        let mut frame = InputFrame::default();
        frame.capture_activation(1, Capture::First);
        frame.capture_activation(2, Capture::First);
        let plan = frame.finish_input(&Navigation::default(), &Activity::new(""), &items(), None);
        assert_eq!(plan.activation, Some(Activation::ImagePicker));
        assert_eq!(
            frame
                .finish_input(&Navigation::default(), &Activity::new(""), &items(), None)
                .activation,
            None
        );

        frame.capture_activation(1, Capture::First);
        frame.capture_activation(2, Capture::Replace);
        assert_eq!(
            frame
                .finish_input(&Navigation::default(), &Activity::new(""), &items(), None)
                .activation,
            Some(Activation::FolderPicker)
        );
    }

    #[test]
    fn close_before_or_after_capture_suppresses_activation() {
        let mut nav = Navigation::default();
        let mut activity = Activity::new("");
        let mut frame = InputFrame::default();
        frame.capture_activation(1, Capture::First);
        frame.request_close(&mut activity);
        frame.capture_activation(2, Capture::Replace);
        assert_eq!(
            frame
                .finish_input(&nav, &activity, &items(), None)
                .activation,
            None
        );

        activity = Activity::new("");
        assert_eq!(
            reduce(
                &mut frame,
                &mut nav,
                &mut activity,
                NavigationEvent::BrowseImage { repeat: false },
                Capture::First
            ),
            None
        );
        assert_eq!(
            reduce(
                &mut frame,
                &mut nav,
                &mut activity,
                NavigationEvent::Escape { repeat: false },
                Capture::First
            ),
            None
        );
        assert_eq!(
            frame
                .finish_input(&nav, &activity, &items(), None)
                .activation,
            None
        );
    }

    #[test]
    fn busy_and_picker_gates_hold_captured_activation() {
        for (busy, picker_pending) in [(true, false), (false, true)] {
            let mut activity = Activity::new("");
            activity.busy = busy;
            activity.picker_pending = picker_pending;
            let mut frame = InputFrame::default();
            frame.capture_activation(1, Capture::First);
            let plan = frame.finish_input(&Navigation::default(), &activity, &items(), None);
            assert_eq!(plan.activation, None);
            assert_eq!(
                frame
                    .finish_input(&Navigation::default(), &Activity::new(""), &items(), None)
                    .activation,
                None
            );
        }
    }

    #[test]
    fn empty_selected_activation_falls_back_to_image_picker_and_invalid_codes_are_ignored() {
        let mut frame = InputFrame::default();
        for invalid in [-1, 3, i32::MAX] {
            frame.capture_activation(invalid, Capture::Replace);
        }
        assert_eq!(
            frame
                .finish_input(&Navigation::default(), &Activity::new(""), &[], None)
                .activation,
            None
        );
        frame.capture_activation(0, Capture::First);
        assert_eq!(
            frame
                .finish_input(&Navigation::default(), &Activity::new(""), &[], None)
                .activation,
            Some(Activation::ImagePicker)
        );
        frame.capture_activation(0, Capture::First);
        assert_eq!(
            frame
                .finish_input(&Navigation::default(), &Activity::new(""), &items(), None)
                .activation,
            Some(Activation::Selected)
        );
    }

    #[test]
    fn filter_persistence_is_changed_only_and_preview_can_disable_it() {
        let mut nav = Navigation {
            filter: LibraryFilter::Nes,
            ..Navigation::default()
        };
        let activity = Activity::new("");
        let mut frame = InputFrame::default();
        assert_eq!(
            frame
                .finish_input(&nav, &activity, &items(), Some(LibraryFilter::All))
                .persist_filter,
            Some(LibraryFilter::Nes)
        );
        assert_eq!(
            frame
                .finish_input(&nav, &activity, &items(), Some(LibraryFilter::Nes))
                .persist_filter,
            None
        );
        assert_eq!(
            frame
                .finish_input(&nav, &activity, &items(), None)
                .persist_filter,
            None
        );
        nav.filter = LibraryFilter::All;
        assert_eq!(
            frame
                .finish_input(&nav, &activity, &items(), Some(LibraryFilter::All))
                .persist_filter,
            None
        );
    }

    #[test]
    fn cancel_is_a_host_request_and_does_not_acknowledge_or_clear_busy() {
        let mut activity = Activity::new("");
        activity.busy = true;
        let mut frame = InputFrame::default();
        assert_eq!(
            reduce(
                &mut frame,
                &mut Navigation::default(),
                &mut activity,
                NavigationEvent::Escape { repeat: false },
                Capture::First
            ),
            Some(HostEffect::CancelImport)
        );
        assert!(activity.busy);
    }

    #[test]
    fn quit_plan_shuts_down_and_busy_controls_exit_not_picker_state() {
        let mut activity = Activity::new("");
        activity.picker_pending = true;
        let mut frame = InputFrame::default();
        frame.request_close(&mut activity);
        let plan = frame.finish_input(&Navigation::default(), &activity, &items(), None);
        assert!(plan.shutdown_services && plan.exit);

        activity.busy = true;
        let plan = frame.finish_input(&Navigation::default(), &activity, &items(), None);
        assert!(plan.shutdown_services && !plan.exit);
    }

    #[test]
    fn picker_completions_clear_pending_once_and_dispatch_only_eligible_sources() {
        for (outcome, decision) in [
            (PickerOutcome::SelectedImage, PickerDecision::ImportImage),
            (PickerOutcome::SelectedFolder, PickerDecision::ResolveFolder),
            (PickerOutcome::Cancelled, PickerDecision::Done),
            (
                PickerOutcome::Failed("picker failed".into()),
                PickerDecision::Done,
            ),
        ] {
            let mut activity = Activity::new("before");
            activity.reduce(launcher_activity::Event::PickerOpened);
            assert_eq!(complete_picker(&mut activity, &outcome), decision);
            assert!(!activity.picker_pending);
            assert_eq!(
                complete_picker(&mut activity, &outcome),
                PickerDecision::Rejected
            );
            assert!(!activity.busy);
            if let PickerOutcome::Failed(message) = outcome {
                assert_eq!(activity.status, message);
                assert!(activity.error);
            }
        }
    }

    #[test]
    fn picker_phase_rejection_and_busy_or_closing_selection_never_dispatch_sources() {
        let mut idle = Activity::new("idle");
        assert_eq!(
            complete_picker(&mut idle, &PickerOutcome::SelectedImage),
            PickerDecision::Rejected
        );
        assert_eq!(idle.status, "idle");

        for (busy, closing) in [(true, false), (false, true)] {
            for outcome in [PickerOutcome::SelectedImage, PickerOutcome::SelectedFolder] {
                let mut activity = Activity::new("before");
                activity.busy = busy;
                activity.closing = closing;
                activity.picker_pending = true;
                assert_eq!(
                    complete_picker(&mut activity, &outcome),
                    PickerDecision::Done
                );
                assert!(!activity.picker_pending);
                assert_eq!(activity.busy, busy);
            }
        }
    }

    #[test]
    fn import_completions_distinguish_success_failure_close_and_duplicates() {
        for (outcome, decision) in [
            (ImportOutcome::Succeeded, ImportDecision::OpenImported),
            (ImportOutcome::Cancelled, ImportDecision::Stay),
            (
                ImportOutcome::Failed("import failed".into()),
                ImportDecision::Stay,
            ),
            (ImportOutcome::Retired, ImportDecision::Stay),
        ] {
            let mut activity = Activity::new("before");
            activity.busy = true;
            assert_eq!(
                complete_import(&mut activity, outcome.clone()),
                Some(decision)
            );
            assert!(!activity.busy);
            assert_eq!(complete_import(&mut activity, outcome), None);
        }

        let mut closing = Activity::new("existing");
        closing.busy = true;
        closing.closing = true;
        assert_eq!(
            complete_import(&mut closing, ImportOutcome::Succeeded),
            Some(ImportDecision::Exit)
        );
        assert_eq!(closing.status, "existing");
        assert!(!closing.busy);
    }

    #[test]
    fn source_guard_covers_every_busy_closing_picker_combination() {
        for busy in [false, true] {
            for closing in [false, true] {
                for picker_pending in [false, true] {
                    assert_eq!(
                        source_allowed(busy, closing, picker_pending),
                        !busy && !closing && !picker_pending
                    );
                }
            }
        }
    }

    #[test]
    fn host_error_preserves_busy_and_closing_notice() {
        let mut activity = Activity::new("before");
        activity.busy = true;
        assert!(host_error(&mut activity, "spawn failed".into()));
        assert!(activity.busy && activity.error);
        assert_eq!(activity.status, "spawn failed");

        activity.closing = true;
        activity.error = false;
        assert!(!host_error(&mut activity, "late error".into()));
        assert!(activity.busy && !activity.error);
        assert_eq!(activity.status, "spawn failed");
    }
}
