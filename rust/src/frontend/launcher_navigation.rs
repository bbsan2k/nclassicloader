//! Host-independent launcher keyboard and controller navigation policy.

use crate::frontend::{
    launcher_controller::Action as ControllerAction,
    library::{self, LibraryFilter, SystemFamily},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Opaque host identity. The reducer compares it only for equality.
    pub key: String,
    pub family: SystemFamily,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Navigation {
    pub selected: usize,
    pub filter: LibraryFilter,
    pub focus: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gate {
    pub busy: bool,
    pub picker_pending: bool,
    pub closing: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Up,
    Down,
    NextFocus,
    Filter { filter: LibraryFilter, repeat: bool },
    Confirm { repeat: bool },
    BrowseImage { repeat: bool },
    BrowseFolder { repeat: bool },
    Escape { repeat: bool },
    SelectVisible { index: usize, double_click: bool },
    ActivateButton(u8),
    Wheel { up: bool },
    Controller(ControllerAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Activate(i32),
    Close,
    CancelImport,
}

fn visible_count(items: &[Item], filter: LibraryFilter) -> usize {
    items
        .iter()
        .filter(|item| filter.matches(item.family))
        .count()
}

impl Navigation {
    /// Reduce one logical event. No host services or native resources are accessed.
    pub fn reduce(&mut self, items: &[Item], gate: Gate, event: Event) -> Option<Effect> {
        match event {
            Event::Escape { repeat } if gate.busy => {
                return (gate.focused && !gate.picker_pending && !gate.closing && !repeat)
                    .then_some(Effect::CancelImport);
            }
            Event::Escape { .. } if !gate.busy && !gate.picker_pending && !gate.closing => {
                return Some(Effect::Close);
            }
            Event::Controller(action) if gate.focused && !gate.picker_pending && !gate.closing => {
                if gate.busy {
                    return (action == ControllerAction::Cancel).then_some(Effect::CancelImport);
                }
                if action == ControllerAction::Confirm && self.focus.is_some_and(|index| index >= 3)
                {
                    return None;
                }
                return match library::pad_action(
                    action,
                    items,
                    |item| item.family,
                    |item| item.key.clone(),
                    &mut self.filter,
                    &mut self.selected,
                    &mut self.focus,
                ) {
                    Some(-1) => Some(Effect::Close),
                    Some(index) if (0..=2).contains(&index) => Some(Effect::Activate(index)),
                    None => None,
                    Some(_) => None,
                };
            }
            _ => {}
        }

        if gate.busy || gate.picker_pending || gate.closing {
            return None;
        }
        match event {
            Event::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.focus = None;
            }
            Event::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(visible_count(items, self.filter).saturating_sub(1));
                self.focus = None;
            }
            Event::NextFocus => {
                let focus = self.focus.filter(|index| *index < 3).unwrap_or(2);
                self.focus = Some((focus + 1) % 3);
            }
            Event::Filter {
                filter,
                repeat: false,
            } => self.change_filter(items, filter),
            Event::Confirm { repeat: false } => {
                if self.focus.is_some_and(|index| index >= 3) {
                    return None;
                }
                return Some(Effect::Activate(self.focus.unwrap_or(0) as i32));
            }
            Event::BrowseImage { repeat: false } => return Some(Effect::Activate(1)),
            Event::BrowseFolder { repeat: false } => return Some(Effect::Activate(2)),
            Event::SelectVisible {
                index,
                double_click,
            } if index < visible_count(items, self.filter) => {
                self.selected = index;
                self.focus = None;
                if double_click {
                    return Some(Effect::Activate(0));
                }
            }
            Event::ActivateButton(index) if index < 3 => {
                return Some(Effect::Activate(i32::from(index)));
            }
            Event::Wheel { up: true } => {
                self.selected = self.selected.saturating_sub(1);
            }
            Event::Wheel { up: false } => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(visible_count(items, self.filter).saturating_sub(1));
            }
            _ => {}
        }
        None
    }

    fn change_filter(&mut self, items: &[Item], next: LibraryFilter) {
        library::change_filter(
            items,
            |item| item.family,
            |item| item.key.clone(),
            &mut self.filter,
            next,
            &mut self.selected,
            &mut self.focus,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Item> {
        [
            ("n1", SystemFamily::Nes),
            ("s1", SystemFamily::Snes),
            ("u1", SystemFamily::Unknown),
            ("n2", SystemFamily::Nes),
        ]
        .into_iter()
        .map(|(key, family)| Item {
            key: key.into(),
            family,
        })
        .collect()
    }

    fn gate() -> Gate {
        Gate {
            focused: true,
            ..Gate::default()
        }
    }

    #[test]
    fn keyboard_navigation_clamps_empty_and_edges_and_cycles_focus() {
        let items = items();
        let mut nav = Navigation {
            selected: 0,
            ..Navigation::default()
        };
        assert_eq!(nav.reduce(&items, Gate::default(), Event::Up), None);
        assert_eq!(nav.selected, 0);
        nav.reduce(&items, Gate::default(), Event::Down);
        assert_eq!(nav.selected, 1);
        nav.selected = usize::MAX;
        nav.reduce(&items, Gate::default(), Event::Down);
        assert_eq!(nav.selected, 3);
        nav.focus = Some(1);
        nav.reduce(&items, Gate::default(), Event::Down);
        assert_eq!(nav.focus, None);
        for expected in [Some(0), Some(1), Some(2), Some(0)] {
            nav.reduce(&items, Gate::default(), Event::NextFocus);
            assert_eq!(nav.focus, expected);
        }
        nav.selected = 9;
        nav.filter = LibraryFilter::Snes;
        nav.reduce(&[], Gate::default(), Event::Down);
        assert_eq!(nav.selected, 0);
    }

    #[test]
    fn filters_keep_opaque_identity_or_fallback_and_clear_focus() {
        let items = items();
        let mut nav = Navigation {
            selected: 3,
            focus: Some(2),
            ..Navigation::default()
        };
        nav.reduce(
            &items,
            Gate::default(),
            Event::Filter {
                filter: LibraryFilter::Nes,
                repeat: false,
            },
        );
        assert_eq!(
            (nav.filter, nav.selected, nav.focus),
            (LibraryFilter::Nes, 1, None)
        );
        nav.reduce(
            &items,
            Gate::default(),
            Event::Filter {
                filter: LibraryFilter::Snes,
                repeat: false,
            },
        );
        assert_eq!((nav.filter, nav.selected), (LibraryFilter::Snes, 0));
        let before_repeat = nav;
        nav.reduce(
            &items,
            Gate::default(),
            Event::Filter {
                filter: LibraryFilter::Nes,
                repeat: true,
            },
        );
        assert_eq!(nav, before_repeat);
        for filter in [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes] {
            let mut nav = Navigation::default();
            nav.reduce(
                &items,
                Gate::default(),
                Event::Filter {
                    filter,
                    repeat: false,
                },
            );
            assert_eq!(nav.filter, filter);
        }
    }

    #[test]
    fn keyboard_activation_repeat_and_gate_characterization() {
        let mut nav = Navigation::default();
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::Confirm { repeat: false }),
            Some(Effect::Activate(0))
        );
        nav.focus = Some(2);
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::Confirm { repeat: true }),
            None
        );
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::BrowseImage { repeat: true }),
            None
        );
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::BrowseImage { repeat: false }),
            Some(Effect::Activate(1))
        );
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::BrowseFolder { repeat: true }),
            None
        );
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::BrowseFolder { repeat: false }),
            Some(Effect::Activate(2))
        );
        assert_eq!(
            nav.reduce(
                &[],
                Gate {
                    busy: true,
                    focused: true,
                    ..Gate::default()
                },
                Event::Escape { repeat: false }
            ),
            Some(Effect::CancelImport)
        );
        assert_eq!(
            nav.reduce(
                &[],
                Gate {
                    busy: true,
                    focused: true,
                    ..Gate::default()
                },
                Event::Escape { repeat: true }
            ),
            None
        );
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::Escape { repeat: true }),
            Some(Effect::Close)
        );
        assert_eq!(
            nav.reduce(
                &[],
                Gate {
                    picker_pending: true,
                    ..Gate::default()
                },
                Event::Escape { repeat: false }
            ),
            None
        );
    }

    #[test]
    fn controller_requires_focus_and_preserves_wrapping_policy() {
        let items = items();
        let mut nav = Navigation::default();
        assert_eq!(
            nav.reduce(
                &items,
                Gate::default(),
                Event::Controller(ControllerAction::Cancel)
            ),
            None
        );
        assert_eq!(
            nav.reduce(&items, gate(), Event::Controller(ControllerAction::Cancel)),
            Some(Effect::Close)
        );
        for expected in [LibraryFilter::Snes, LibraryFilter::Nes, LibraryFilter::All] {
            nav.reduce(
                &items,
                gate(),
                Event::Controller(ControllerAction::FilterPrevious),
            );
            assert_eq!(nav.filter, expected);
        }
        nav.reduce(
            &items,
            gate(),
            Event::Controller(ControllerAction::LibraryNext),
        );
        assert_eq!(nav.selected, 1);
        nav.reduce(
            &items,
            gate(),
            Event::Controller(ControllerAction::FocusNext),
        );
        assert_eq!(nav.focus, Some(0));
        assert_eq!(
            nav.reduce(
                &items,
                Gate {
                    focused: true,
                    busy: true,
                    ..Gate::default()
                },
                Event::Controller(ControllerAction::Confirm)
            ),
            None
        );
        assert_eq!(
            nav.reduce(
                &items,
                Gate {
                    focused: true,
                    busy: true,
                    ..Gate::default()
                },
                Event::Controller(ControllerAction::Cancel)
            ),
            Some(Effect::CancelImport)
        );
        assert_eq!(
            nav.reduce(&items, gate(), Event::Controller(ControllerAction::Import)),
            Some(Effect::Activate(1))
        );
        assert_eq!(
            nav.reduce(
                &items,
                gate(),
                Event::Controller(ControllerAction::OpenFolder)
            ),
            Some(Effect::Activate(2))
        );
    }

    #[test]
    fn idle_focused_controller_matches_legacy_library_policy_for_counts_and_actions() {
        let all_items = items();
        let actions = [
            ControllerAction::LibraryPrevious,
            ControllerAction::LibraryNext,
            ControllerAction::FocusPrevious,
            ControllerAction::FocusNext,
            ControllerAction::FilterPrevious,
            ControllerAction::FilterNext,
            ControllerAction::Confirm,
            ControllerAction::Cancel,
            ControllerAction::Import,
            ControllerAction::OpenFolder,
        ];
        for count in 0..=all_items.len() {
            let visible_subset = &all_items[..count];
            for action in actions {
                for filter in [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes] {
                    for selected in [0, usize::MAX] {
                        for focus in [None, Some(0), Some(1), Some(2)] {
                            let initial = Navigation {
                                selected,
                                filter,
                                focus,
                            };
                            let mut shared = initial;
                            let effect =
                                shared.reduce(visible_subset, gate(), Event::Controller(action));
                            let (mut filter, mut selected, mut focus) =
                                (initial.filter, initial.selected, initial.focus);
                            let legacy_effect = library::pad_action(
                                action,
                                visible_subset,
                                |item| item.family,
                                |item| item.key.clone(),
                                &mut filter,
                                &mut selected,
                                &mut focus,
                            );
                            let expected_effect = match legacy_effect {
                                Some(-1) => Some(Effect::Close),
                                Some(index) if (0..=2).contains(&index) => {
                                    Some(Effect::Activate(index))
                                }
                                Some(_) | None => None,
                            };
                            assert_eq!(
                                effect, expected_effect,
                                "count={count}, filter={filter:?}, selected={selected}, focus={focus:?}, action={action:?}"
                            );
                            assert_eq!(
                                shared,
                                Navigation {
                                    selected,
                                    filter,
                                    focus,
                                },
                                "count={count}, action={action:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_keyboard_focus_is_treated_as_absent_for_cycle_but_not_confirm() {
        for invalid in [Some(3), Some(usize::MAX)] {
            let mut nav = Navigation {
                focus: invalid,
                ..Navigation::default()
            };
            assert_eq!(nav.reduce(&[], Gate::default(), Event::NextFocus), None);
            assert_eq!(nav.focus, Some(0));

            nav.focus = invalid;
            assert_eq!(
                nav.reduce(&[], Gate::default(), Event::Confirm { repeat: false }),
                None
            );
            assert_eq!(nav.focus, invalid);
        }
        let mut nav = Navigation::default();
        assert_eq!(
            nav.reduce(&[], Gate::default(), Event::Confirm { repeat: false }),
            Some(Effect::Activate(0))
        );
    }

    #[test]
    fn invalid_controller_confirm_focus_is_ignored_without_blocking_shortcuts() {
        let items = items();
        for invalid in [Some(3), Some(usize::MAX)] {
            let mut nav = Navigation {
                focus: invalid,
                ..Navigation::default()
            };
            assert_eq!(
                nav.reduce(&items, gate(), Event::Controller(ControllerAction::Confirm)),
                None
            );
            assert_eq!(nav.focus, invalid);
            assert_eq!(
                nav.reduce(&items, gate(), Event::Controller(ControllerAction::Import)),
                Some(Effect::Activate(1))
            );
            assert_eq!(
                nav.reduce(
                    &items,
                    gate(),
                    Event::Controller(ControllerAction::OpenFolder)
                ),
                Some(Effect::Activate(2))
            );
            assert_eq!(
                nav.reduce(&items, gate(), Event::Controller(ControllerAction::Cancel)),
                Some(Effect::Close)
            );
        }
    }

    #[test]
    fn pointer_events_match_row_button_filter_and_wheel_policy() {
        let items = items();
        let mut nav = Navigation {
            selected: 1,
            focus: Some(2),
            ..Navigation::default()
        };
        assert_eq!(
            nav.reduce(
                &items,
                Gate::default(),
                Event::SelectVisible {
                    index: 3,
                    double_click: false,
                }
            ),
            None
        );
        assert_eq!((nav.selected, nav.focus), (3, None));
        nav.focus = Some(1);
        assert_eq!(
            nav.reduce(
                &items,
                Gate::default(),
                Event::SelectVisible {
                    index: 2,
                    double_click: true,
                }
            ),
            Some(Effect::Activate(0))
        );
        assert_eq!((nav.selected, nav.focus), (2, None));
        assert_eq!(
            nav.reduce(
                &items,
                Gate::default(),
                Event::SelectVisible {
                    index: 4,
                    double_click: true,
                }
            ),
            None
        );
        for index in 0..=3 {
            assert_eq!(
                nav.reduce(&items, Gate::default(), Event::ActivateButton(index)),
                (index < 3).then_some(Effect::Activate(i32::from(index)))
            );
        }

        nav.selected = 0;
        nav.focus = Some(2);
        nav.reduce(&items, Gate::default(), Event::Wheel { up: true });
        assert_eq!((nav.selected, nav.focus), (0, Some(2)));
        nav.selected = usize::MAX;
        nav.reduce(&items, Gate::default(), Event::Wheel { up: false });
        assert_eq!((nav.selected, nav.focus), (3, Some(2)));

        nav.selected = 3;
        nav.focus = Some(1);
        nav.reduce(
            &items,
            Gate::default(),
            Event::Filter {
                filter: LibraryFilter::Nes,
                repeat: false,
            },
        );
        assert_eq!(
            (nav.filter, nav.selected, nav.focus),
            (LibraryFilter::Nes, 1, None)
        );

        for gate in [
            Gate {
                busy: true,
                ..Gate::default()
            },
            Gate {
                picker_pending: true,
                ..Gate::default()
            },
            Gate {
                closing: true,
                ..Gate::default()
            },
        ] {
            let before = nav;
            assert_eq!(
                nav.reduce(
                    &items,
                    gate,
                    Event::SelectVisible {
                        index: 0,
                        double_click: true
                    }
                ),
                None
            );
            assert_eq!(nav.reduce(&items, gate, Event::ActivateButton(1)), None);
            assert_eq!(nav.reduce(&items, gate, Event::Wheel { up: false }), None);
            assert_eq!(nav, before);
        }
    }
}
