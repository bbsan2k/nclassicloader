use clover_portable::frontend::launcher_activity::CancellationView;
use clover_portable::frontend::launcher_presentation::{
    Presentation, PresentedDetail, PresentedPrimary, PresentedRow, Scene, compose,
};
use clover_portable::frontend::launcher_scene::{
    self, ControllerIllustration, DrawCommand, FontRole, Region, Rgba, RowDisplay, TextCommand,
};
use clover_portable::frontend::launcher_status::StatusText;

fn presentation() -> Presentation {
    Presentation {
        entry_count: 1,
        selected: 0,
        busy: true,
        cancel: CancellationView::Available,
        status: "Working status".into(),
        error: true,
        filter_label: "ALL".into(),
        controller_connected: true,
        elapsed: 1.5,
        sidebar_active: [true, false],
        hot_tabs: [false, true, false],
        rows: vec![PresentedRow {
            row: 0,
            chosen: true,
            hot: false,
            title: "Visible row title".into(),
            system_label: "SNES".into(),
            version: "Version 1".into(),
            cached: true,
            region: Region::Europe,
        }],
        detail: PresentedDetail {
            title: "Detail title".into(),
            version: "Detail version".into(),
            region: Region::Europe,
            controller: Some(ControllerIllustration::Snes),
            entry_exists: true,
            ready: true,
            source: "display-only source".into(),
        },
        primary: PresentedPrimary {
            label: "Primary action".into(),
            enabled: true,
            hot: true,
        },
    }
}

fn expected_groups(input: &Presentation) -> (Vec<DrawCommand>, Vec<DrawCommand>) {
    let mut before: Vec<_> = launcher_scene::background()
        .into_iter()
        .map(DrawCommand::Geometry)
        .collect();
    before.extend(launcher_scene::branding());
    before.extend(launcher_scene::sidebar(input.busy, input.sidebar_active));
    before.extend(launcher_scene::library_header(
        input.entry_count,
        &input.filter_label,
        input.hot_tabs,
        input.busy,
    ));
    for row in &input.rows {
        before.extend(launcher_scene::library_row(
            row.row,
            row.chosen,
            row.hot,
            input.busy,
            RowDisplay {
                title: &row.title,
                system_label: &row.system_label,
                version: &row.version,
                cached: row.cached,
                region: row.region,
            },
        ));
    }
    if input.entry_count == 0 {
        before.extend(launcher_scene::empty_library(&input.filter_label));
    }
    before.extend(launcher_scene::detail_header(
        &input.detail.title,
        &input.detail.version,
        input.detail.controller,
        input.detail.entry_exists,
        input.detail.ready,
    ));
    before.extend(launcher_scene::detail_metadata(
        input.detail.region,
        input.detail.entry_exists,
        input.detail.ready,
        &input.detail.source,
    ));
    before.extend(launcher_scene::primary_button(
        &input.primary.label,
        input.primary.enabled,
        input.primary.hot,
    ));
    before.push(launcher_scene::page_indicator(
        input.selected,
        input.entry_count,
    ));

    let mut after: Vec<_> = launcher_scene::import_progress(input.busy, input.elapsed)
        .into_iter()
        .map(DrawCommand::Geometry)
        .collect();
    after.push(launcher_scene::footer(
        input.busy,
        input.cancel,
        input.controller_connected,
    ));
    (before, after)
}

fn text_value(command: &DrawCommand) -> Option<&str> {
    match command {
        DrawCommand::Text(TextCommand { value, .. }) => Some(value),
        DrawCommand::Geometry(_) | DrawCommand::Flag { .. } => None,
    }
}

#[test]
fn composition_matches_complete_ordered_groups_and_status_split() {
    let input = presentation();
    let scene = compose(&input);
    let (expected_before, expected_after) = expected_groups(&input);
    assert_eq!(scene.before_status, expected_before);
    assert_eq!(scene.after_status, expected_after);
    assert_eq!(scene.status, input.status);
    assert_eq!(scene.error, input.error);

    let before = &scene.before_status;
    let row_title = before
        .iter()
        .position(|command| text_value(command) == Some("Visible row title"))
        .unwrap();
    let detail_title = before
        .iter()
        .position(|command| text_value(command) == Some("Detail title"))
        .unwrap();
    let detail_flag = before
        .iter()
        .position(|command| {
            matches!(command, DrawCommand::Flag { region: Region::Europe, rect } if rect.x == 866.0)
        })
        .unwrap();
    let primary = before
        .iter()
        .position(|command| text_value(command) == Some("Primary action"))
        .unwrap();
    let page = before
        .iter()
        .position(|command| text_value(command) == Some("1 / 1"))
        .unwrap();
    assert!(row_title < detail_title && detail_title < detail_flag);
    assert!(detail_flag < primary && primary < page);
    assert!(matches!(before.last(), Some(DrawCommand::Text(_))));
    assert!(matches!(scene.after_status[0], DrawCommand::Geometry(_)));
    assert!(matches!(scene.after_status[1], DrawCommand::Geometry(_)));
    assert!(matches!(
        scene.after_status.last(),
        Some(DrawCommand::Text(_))
    ));
}

#[test]
fn empty_library_and_malformed_positive_selection_keep_legacy_composition_policy() {
    let mut empty = presentation();
    empty.entry_count = 0;
    empty.selected = 41;
    empty.rows.clear();
    let empty_scene = compose(&empty);
    let expected_empty = launcher_scene::empty_library("ALL");
    assert!(
        empty_scene
            .before_status
            .windows(expected_empty.len())
            .any(|part| part == expected_empty)
    );
    assert!(
        empty_scene
            .before_status
            .iter()
            .any(|command| text_value(command) == Some("0 / 0"))
    );

    let mut malformed = presentation();
    malformed.entry_count = 8;
    malformed.selected = 99;
    malformed.rows.clear();
    let malformed_scene = compose(&malformed);
    assert!(
        !malformed_scene
            .before_status
            .iter()
            .any(|command| text_value(command) == Some("Your library starts here."))
    );
    assert!(
        malformed_scene
            .before_status
            .iter()
            .any(|command| text_value(command) == Some("17 / 2"))
    );
    assert!(
        malformed_scene
            .before_status
            .iter()
            .any(|command| text_value(command) == Some("Detail title"))
    );
}

#[test]
fn rows_are_composed_in_supplied_order_without_reclassification_or_authority() {
    let mut input = presentation();
    input.entry_count = 50;
    input.selected = 23;
    input.rows = vec![
        PresentedRow {
            row: 4,
            chosen: false,
            hot: true,
            title: "First supplied".into(),
            system_label: "caller system".into(),
            version: "caller version".into(),
            cached: false,
            region: Region::Unknown,
        },
        PresentedRow {
            row: 1,
            chosen: true,
            hot: false,
            title: "Second supplied".into(),
            system_label: "also caller".into(),
            version: "also supplied".into(),
            cached: true,
            region: Region::Japan,
        },
    ];
    input.primary.enabled = false;
    input.primary.hot = true;

    let scene = compose(&input);
    let first = scene
        .before_status
        .iter()
        .position(|command| text_value(command) == Some("First supplied"))
        .unwrap();
    let second = scene
        .before_status
        .iter()
        .position(|command| text_value(command) == Some("Second supplied"))
        .unwrap();
    assert!(first < second);
    assert!(scene.before_status.iter().any(|command| {
        matches!(
            command,
            DrawCommand::Flag {
                region: Region::Unknown,
                ..
            }
        )
    }));
    assert!(scene.before_status.iter().any(|command| {
        matches!(
            command,
            DrawCommand::Flag {
                region: Region::Japan,
                ..
            }
        )
    }));
    assert!(scene.before_status.iter().any(|command| {
        matches!(command, DrawCommand::Text(TextCommand { value, .. }) if value == "caller system")
    }));
    assert!(scene.before_status.iter().any(|command| {
        matches!(command, DrawCommand::Text(TextCommand { value, .. }) if value == "Primary action")
    }));
    assert!(
        scene
            .before_status
            .iter()
            .any(|command| text_value(command) == Some("4 / 9"))
    );
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Draw(DrawCommand),
    Measure(String),
}

#[derive(Clone, Copy)]
enum Measurement {
    Natural,
    ForceCutoff,
}

/// Test-only fake executor. Error injection characterizes the pure protocol and
/// does not claim native SDL/font failure injection.
struct FakeHost {
    owner_generation: u64,
    events: Vec<Event>,
    resolved_fonts: Vec<(u64, FontRole)>,
    resolved_regions: Vec<(u64, Region)>,
    draw_ordinal: usize,
    fail_draw_at: Option<usize>,
    measurement: Measurement,
}

impl FakeHost {
    fn new(owner_generation: u64, fail_draw_at: Option<usize>, measurement: Measurement) -> Self {
        Self {
            owner_generation,
            events: Vec::new(),
            resolved_fonts: Vec::new(),
            resolved_regions: Vec::new(),
            draw_ordinal: 0,
            fail_draw_at,
            measurement,
        }
    }

    fn draw(&mut self, command: DrawCommand) -> Result<(), usize> {
        let ordinal = self.draw_ordinal;
        self.draw_ordinal += 1;
        match &command {
            DrawCommand::Text(text) => self.resolved_fonts.push((self.owner_generation, text.font)),
            DrawCommand::Flag { region, .. } => {
                self.resolved_regions.push((self.owner_generation, *region))
            }
            DrawCommand::Geometry(_) => {}
        }
        self.events.push(Event::Draw(command));
        if self.fail_draw_at == Some(ordinal) {
            Err(ordinal)
        } else {
            Ok(())
        }
    }

    fn measure(&mut self, candidate: &str) -> f32 {
        self.events.push(Event::Measure(candidate.to_owned()));
        match self.measurement {
            Measurement::Natural => candidate.chars().count() as f32,
            Measurement::ForceCutoff if candidate.trim().contains(' ') => 551.0,
            Measurement::ForceCutoff => 1.0,
        }
    }

    fn replay(&mut self, scene: &Scene) -> Result<(), usize> {
        for command in &scene.before_status {
            self.draw(command.clone())?;
        }
        let mut status = StatusText::new(&scene.status, scene.error);
        while let Some(candidate) = status.candidate() {
            let width = self.measure(candidate);
            if let Some(command) = status.advance(width) {
                self.draw(command)?;
            }
        }
        self.draw(status.finish())?;
        for command in &scene.after_status {
            self.draw(command.clone())?;
        }
        Ok(())
    }
}

fn status_draws(events: &[Event]) -> Vec<&TextCommand> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Draw(DrawCommand::Text(text))
                if text.x == 272.0 && text.size == 15 && (657.0..=720.0).contains(&text.y) =>
            {
                Some(text)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn fake_host_interleaves_status_measurement_and_preserves_terminal_empty_line() {
    let mut input = presentation();
    input.status = "a b c d".into();
    input.error = false;
    let scene = compose(&input);
    let mut host = FakeHost::new(1, None, Measurement::ForceCutoff);
    assert_eq!(host.replay(&scene), Ok(()));

    let first_status_draw = host
        .events
        .iter()
        .position(|event| matches!(event, Event::Draw(DrawCommand::Text(text)) if text.y == 657.0))
        .unwrap();
    let measure_before = host.events[..first_status_draw]
        .iter()
        .filter(|event| matches!(event, Event::Measure(_)))
        .count();
    assert_eq!(measure_before, 2);
    assert!(matches!(
        host.events[first_status_draw - 1],
        Event::Measure(_)
    ));
    let lines = status_draws(&host.events);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.value.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", ""]
    );
    assert_eq!(
        lines.iter().map(|line| line.y).collect::<Vec<_>>(),
        [657.0, 678.0, 699.0, 720.0]
    );
    assert!(host.events.iter().any(|event| matches!(event, Event::Draw(command) if command == scene.after_status.first().unwrap())));
}

#[test]
fn fake_host_stops_at_before_status_status_and_after_status_draw_errors() {
    let scene = compose(&presentation());

    let mut before_error = FakeHost::new(1, Some(0), Measurement::Natural);
    assert_eq!(before_error.replay(&scene), Err(0));
    assert!(
        !before_error
            .events
            .iter()
            .any(|event| matches!(event, Event::Measure(_)))
    );

    let mut status_error = FakeHost::new(2, Some(scene.before_status.len()), Measurement::Natural);
    assert_eq!(status_error.replay(&scene), Err(scene.before_status.len()));
    assert_eq!(
        status_error
            .events
            .iter()
            .filter(|event| matches!(event, Event::Measure(_)))
            .count(),
        2
    );
    assert!(!status_error.events.iter().any(|event| {
        matches!(event, Event::Draw(command) if scene.after_status.contains(command))
    }));
    assert!(matches!(
        status_error.events.last(),
        Some(Event::Draw(DrawCommand::Text(_)))
    ));

    let after_start = scene.before_status.len() + 1;
    let mut after_error = FakeHost::new(3, Some(after_start), Measurement::Natural);
    assert_eq!(after_error.replay(&scene), Err(after_start));
    assert_eq!(
        after_error
            .events
            .iter()
            .filter(|event| matches!(event, Event::Measure(_)))
            .count(),
        2
    );
    assert_eq!(
        after_error
            .events
            .iter()
            .filter(|event| matches!(event, Event::Draw(DrawCommand::Geometry(_))))
            .count(),
        scene
            .before_status
            .iter()
            .filter(|command| matches!(command, DrawCommand::Geometry(_)))
            .count()
            + 1
    );
    assert_eq!(
        after_error.events.last(),
        Some(&Event::Draw(scene.after_status[0].clone()))
    );
}

#[test]
fn bounded_busy_error_and_cancellation_fixtures_preserve_host_supplied_state() {
    let cases = [
        (
            false,
            CancellationView::Unavailable,
            false,
            false,
            "Up / Down  Select    Enter  Launch    1 / 2 / 3  System    I  Import    D  Folder    F11  Full screen",
            0,
        ),
        (
            true,
            CancellationView::Available,
            false,
            false,
            "Escape  Cancel import    Waiting for a safe stopping point    F11  Full screen",
            2,
        ),
        (
            true,
            CancellationView::Requested,
            true,
            true,
            "Cancellation requested    Waiting for a safe stopping point    F11  Full screen",
            2,
        ),
        (
            true,
            CancellationView::Finishing,
            false,
            false,
            "Finishing import    Cancellation is too late    F11  Full screen",
            2,
        ),
    ];
    for (busy, cancel, controller_connected, error, footer, progress_geometry_count) in cases {
        let mut input = presentation();
        input.busy = busy;
        input.cancel = cancel;
        input.controller_connected = controller_connected;
        input.error = error;
        let scene = compose(&input);
        assert_eq!(scene.error, error);
        assert_eq!(scene.after_status.len(), progress_geometry_count + 1);
        assert_eq!(text_value(scene.after_status.last().unwrap()), Some(footer));

        let mut host = FakeHost::new(7, None, Measurement::Natural);
        assert_eq!(host.replay(&scene), Ok(()));
        let status_lines = status_draws(&host.events);
        assert!(status_lines.iter().all(|line| {
            line.color
                == if error {
                    Rgba {
                        r: 255,
                        g: 163,
                        b: 156,
                        a: 255,
                    }
                } else {
                    Rgba {
                        r: 156,
                        g: 171,
                        b: 186,
                        a: 255,
                    }
                }
        }));
    }
}

#[test]
fn retained_scene_replays_with_fresh_fakehost_resource_generations() {
    let scene = {
        let mut input = presentation();
        let retained = compose(&input);
        input.status.clear();
        input.rows[0].title.clear();
        input.detail.source.clear();
        input.primary.label.clear();
        drop(input);
        retained
    };
    let retained_copy = scene.clone();
    let mut first_host = FakeHost::new(101, None, Measurement::Natural);
    let mut second_host = FakeHost::new(202, None, Measurement::Natural);
    assert_eq!(first_host.replay(&scene), Ok(()));
    assert_eq!(second_host.replay(&scene), Ok(()));
    assert_eq!(scene, retained_copy);
    assert_eq!(first_host.events, second_host.events);
    for expected in [
        "Working status",
        "Visible row title",
        "display-only source",
        "Primary action",
    ] {
        assert!(first_host.events.iter().any(|event| {
            matches!(event, Event::Draw(DrawCommand::Text(text)) if text.value == expected)
                || matches!(event, Event::Measure(value) if value == expected)
        }));
    }
    assert!(!first_host.resolved_fonts.is_empty());
    assert!(!first_host.resolved_regions.is_empty());
    assert!(
        first_host
            .resolved_fonts
            .iter()
            .all(|(generation, _)| *generation == 101)
    );
    assert!(
        second_host
            .resolved_fonts
            .iter()
            .all(|(generation, _)| *generation == 202)
    );
    assert!(
        first_host
            .resolved_regions
            .iter()
            .all(|(generation, _)| *generation == 101)
    );
    assert!(
        second_host
            .resolved_regions
            .iter()
            .all(|(generation, _)| *generation == 202)
    );
}
