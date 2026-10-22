//! A non-SDL host can drive the public controller policy using only snapshots
//! and an injected monotonic elapsed time.
use clover_portable::frontend::launcher_controller::{Action, Button, Input, Snapshot};
use clover_portable::frontend::{
    gamepad::{Direction, Repeat, Stick},
    language_navigation::{LanguageInput, next_index},
    library::{LibraryFilter, SystemFamily, change_filter, visible_entries},
    timing,
};
use std::time::Duration;

#[test]
fn alternate_host_requires_release_after_connection_then_emits_semantic_action() {
    let mut held = Snapshot::default();
    held.buttons[Button::South as usize] = true;
    let mut input = Input::default();

    input.seed(held);
    assert!(
        input
            .advance(held, Duration::from_millis(1), true)
            .is_empty()
    );
    assert!(
        input
            .advance(Snapshot::default(), Duration::from_millis(2), true)
            .is_empty()
    );
    assert_eq!(
        input.advance(held, Duration::from_millis(3), true),
        vec![Action::Confirm]
    );
}

#[test]
fn alternate_host_drives_stick_and_repeat_without_native_events() {
    let mut stick = Stick::default();
    let mut repeat = Repeat::default();
    stick.update(0, i16::MAX);
    stick.update(1, i16::MIN);
    assert_eq!(stick.mask(), (1 << 7) | (1 << 4));
    assert_eq!(stick.menu_direction(), Some(Direction::Up));
    assert_eq!(
        repeat.poll(stick.menu_direction(), Duration::ZERO),
        Some((Direction::Up, false))
    );
    assert_eq!(
        repeat.poll(stick.menu_direction(), Duration::from_millis(349)),
        None
    );
    assert_eq!(
        repeat.poll(stick.menu_direction(), Duration::from_millis(350)),
        Some((Direction::Up, true))
    );
    stick.clear();
    assert_eq!(
        repeat.poll(stick.menu_direction(), Duration::from_millis(351)),
        None
    );
}

#[test]
fn alternate_host_injects_time_and_language_navigation() {
    let mut deadline = Duration::ZERO;
    let frames: usize = (0..120)
        .map(|tick| {
            timing::silent_frames_due(
                &mut deadline,
                Duration::from_nanos(tick * 1_000_000_000 / 120),
            )
        })
        .sum();
    assert_eq!(frames, 60);
    assert_eq!(timing::audio_target_bytes(48_000), 23_040);
    assert_eq!(next_index(3, 9, LanguageInput::Down), 8);
    assert_eq!(next_index(8, 9, LanguageInput::Next), 0);
    assert_eq!(next_index(0, 0, LanguageInput::Down), 0);
}

#[test]
fn alternate_host_filters_opaque_items_without_profiles_or_paths() {
    let entries = [
        ("nes-a", SystemFamily::Nes),
        ("snes-a", SystemFamily::Snes),
        ("nes-b", SystemFamily::Nes),
    ];
    let mut filter = LibraryFilter::All;
    let mut selected = 2;
    let mut focus = Some(1);
    change_filter(
        &entries,
        |entry| entry.1,
        |entry| entry.0,
        &mut filter,
        LibraryFilter::Nes,
        &mut selected,
        &mut focus,
    );
    let visible = visible_entries(&entries, |entry| entry.1, filter);
    assert_eq!(selected, 1);
    assert_eq!(visible[selected].0, "nes-b");
    assert_eq!(focus, None);
}
