use super::*;
use crate::frontend::launcher_presentation::Presentation;

#[allow(clippy::too_many_arguments)]
pub(super) fn project(
    entries: &[&Entry],
    selected: usize,
    busy: bool,
    cancel: CancellationView,
    status: &str,
    error: bool,
    hover: Option<(f32, f32)>,
    focus: Option<usize>,
    filter_label: &str,
    controller_connected: bool,
    elapsed: f32,
) -> Presentation {
    use crate::frontend::launcher_presentation::*;
    use crate::frontend::launcher_scene::{ControllerIllustration, PAGE_SIZE};

    let sidebar_active = [1, 2].map(|index| {
        hover.is_some_and(|(x, y)| point_inside(x, y, BUTTONS[index])) || focus == Some(index)
    });
    let hot_tabs =
        [0, 1, 2].map(|index| hover.is_some_and(|(x, y)| point_inside(x, y, FILTER_TABS[index])));
    let page = selected / PAGE_SIZE;
    let rows = entries
        .iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .enumerate()
        .map(|(row, entry)| {
            let (title, region, version) = identity(entry);
            PresentedRow {
                row,
                chosen: page * PAGE_SIZE + row == selected,
                hot: hover.is_some_and(|(x, y)| point_inside(x, y, row_rect(row))),
                title,
                system_label: system_label(entry).to_owned(),
                version,
                cached: matches!(entry, Entry::Cached(_)),
                region: flags::Region::from_marker(&region),
            }
        })
        .collect();

    let entry = entries.get(selected).copied();
    let controller = match entry.map(console) {
        Some(Console::Nes) => Some(ControllerIllustration::Nes),
        Some(Console::Famicom) => Some(ControllerIllustration::Famicom),
        Some(Console::Snes) => Some(ControllerIllustration::Snes),
        Some(Console::Unknown) | None => None,
    };
    let (title, region, version) = entry.map(identity).unwrap_or_else(|| {
        (
            "Classic Menu".into(),
            "ALL".into(),
            "Your region. Your version.".into(),
        )
    });
    let ready = matches!(entry, Some(Entry::Cached(_)));
    let source = entry
        .map(|entry| match entry {
            Entry::Cached(profile) => profile.directory.display().to_string(),
            Entry::Image(path) => path.display().to_string(),
        })
        .unwrap_or_else(|| "Use Import image or Open data folder.".into());
    let button = primary_button(busy, cancel, entry.is_some(), ready);
    let primary = PresentedPrimary {
        label: button.label.to_owned(),
        enabled: button.enabled,
        hot: button.enabled
            && (hover.is_some_and(|(x, y)| point_inside(x, y, BUTTONS[0])) || focus == Some(0)),
    };

    Presentation {
        entry_count: entries.len(),
        selected,
        busy,
        cancel,
        status: status.to_owned(),
        error,
        filter_label: filter_label.to_owned(),
        controller_connected,
        elapsed,
        sidebar_active,
        hot_tabs,
        rows,
        detail: PresentedDetail {
            title,
            version,
            region: flags::Region::from_marker(&region),
            controller,
            entry_exists: entry.is_some(),
            ready,
            source,
        },
        primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::launcher_activity::CancellationView;
    use std::path::PathBuf;

    fn project_entries(
        entries: &[&Entry],
        selected: usize,
        busy: bool,
        cancel: CancellationView,
    ) -> Presentation {
        project(
            entries, selected, busy, cancel, "status", true, None, None, "ALL", false, 1.25,
        )
    }

    #[test]
    fn projection_owns_visible_rows_and_only_projects_the_selected_page() {
        let paths: Vec<_> = (0..8)
            .map(|index| PathBuf::from(format!("dp-snesusa-release-v{index}.hsqs")))
            .collect();
        let entries: Vec<_> = paths.iter().cloned().map(Entry::Image).collect();
        let entry_refs: Vec<_> = entries.iter().collect();
        let view = project_entries(&entry_refs, 6, false, CancellationView::Unavailable);
        assert_eq!(view.entry_count, 8);
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[0].row, 0);
        assert!(view.rows[0].chosen);
        assert_eq!(view.rows[1].row, 1);
        assert!(!view.rows[1].chosen);
        assert_eq!(view.rows[0].title, "SNES Classic · United States");
        assert_eq!(view.detail.source, "dp-snesusa-release-v6.hsqs");
        drop(paths);
        assert_eq!(view.rows[0].version, "Version 6.hsqs");
    }

    #[test]
    fn empty_and_busy_projection_preserves_default_detail_and_primary_eligibility() {
        let empty = project_entries(&[], 17, true, CancellationView::Requested);
        assert_eq!(empty.selected, 17);
        assert!(empty.rows.is_empty());
        assert_eq!(empty.detail.title, "Classic Menu");
        assert!(!empty.detail.entry_exists);
        assert_eq!(empty.primary.label, "Cancelling…");
        assert!(!empty.primary.enabled);
        assert_eq!(empty.cancel, CancellationView::Requested);

        let entries = [Entry::Image(PathBuf::from("custom.hsqs"))];
        let busy = project_entries(&[&entries[0]], 0, true, CancellationView::Available);
        assert_eq!(busy.primary.label, "Cancel import");
        assert!(busy.primary.enabled);
        assert!(!busy.primary.hot);
    }

    fn project_hover(
        entries: &[&Entry],
        point: (f32, f32),
        focus: Option<usize>,
        busy: bool,
        cancel: CancellationView,
    ) -> Presentation {
        project(
            entries,
            0,
            busy,
            cancel,
            "status",
            false,
            Some(point),
            focus,
            "ALL",
            false,
            0.0,
        )
    }

    fn window_point(width: u32, height: u32, x: f32, y: f32) -> (f32, f32) {
        let scale = (width as f32 / WIDTH as f32)
            .min(height as f32 / HEIGHT as f32)
            .max(0.001);
        (
            (x - WIDTH as f32 / 2.0) * scale + width as f32 / 2.0,
            (y - HEIGHT as f32 / 2.0) * scale + height as f32 / 2.0,
        )
    }

    #[test]
    fn projection_hits_match_native_targets_across_letterboxed_window_shapes() {
        let entries = [Entry::Image(PathBuf::from("dp-snesusa-release-v1.hsqs"))];
        let refs = [&entries[0]];
        for (width, height) in [(1200, 760), (2400, 1520), (1600, 760), (1200, 1200)] {
            for rect in BUTTONS
                .into_iter()
                .chain(FILTER_TABS)
                .chain((0..PAGE_SIZE).map(row_rect))
            {
                for (x, y, inside) in [
                    (rect.x, rect.y, true),
                    (rect.x + rect.w, rect.y + 1.0, false),
                    (rect.x + 1.0, rect.y + rect.h, false),
                    (rect.x - 1.0, rect.y + 1.0, false),
                ] {
                    let screen = window_point(width, height, x, y);
                    let mapped =
                        crate::ui::launcher::coordinates(width, height, screen.0, screen.1);
                    assert_eq!(
                        point_inside(mapped.0, mapped.1, rect),
                        inside,
                        "window={width}x{height}, stage=({x},{y}), rect={rect:?}"
                    );
                }
            }
            let center = |rect: FRect| (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
            let screen = window_point(width, height, center(BUTTONS[0]).0, center(BUTTONS[0]).1);
            let mapped =
                crate::frontend::launcher_scene::coordinates(width, height, screen.0, screen.1);
            assert!(point_inside(mapped.0, mapped.1, BUTTONS[0]));
            assert!(
                project_hover(&refs, mapped, None, false, CancellationView::Unavailable)
                    .primary
                    .hot
            );

            let screen = window_point(width, height, center(BUTTONS[1]).0, center(BUTTONS[1]).1);
            let mapped =
                crate::frontend::launcher_scene::coordinates(width, height, screen.0, screen.1);
            assert!(
                project_hover(&refs, mapped, None, false, CancellationView::Unavailable)
                    .sidebar_active[0]
            );

            let screen = window_point(
                width,
                height,
                center(FILTER_TABS[2]).0,
                center(FILTER_TABS[2]).1,
            );
            let mapped =
                crate::frontend::launcher_scene::coordinates(width, height, screen.0, screen.1);
            assert!(
                project_hover(&refs, mapped, None, false, CancellationView::Unavailable).hot_tabs
                    [2]
            );

            let screen = window_point(width, height, center(row_rect(0)).0, center(row_rect(0)).1);
            let mapped =
                crate::frontend::launcher_scene::coordinates(width, height, screen.0, screen.1);
            assert!(
                project_hover(&refs, mapped, None, false, CancellationView::Unavailable).rows[0]
                    .hot
            );

            if width == 1600 {
                let mapped =
                    crate::frontend::launcher_scene::coordinates(width, height, 100.0, 380.0);
                assert!(!point_inside(mapped.0, mapped.1, BUTTONS[0]));
                assert!(
                    !project_hover(&refs, mapped, None, false, CancellationView::Unavailable)
                        .primary
                        .hot
                );
            }
            if height == 1200 {
                let mapped =
                    crate::frontend::launcher_scene::coordinates(width, height, 600.0, 100.0);
                assert!(!point_inside(mapped.0, mapped.1, BUTTONS[0]));
                assert!(
                    !project_hover(&refs, mapped, None, false, CancellationView::Unavailable)
                        .primary
                        .hot
                );
            }
        }

        let button = BUTTONS[0];
        for (x, expected) in [(button.x, true), (button.x + button.w, false)] {
            let view = project_hover(
                &refs,
                (x, button.y + button.h / 2.0),
                None,
                false,
                CancellationView::Unavailable,
            );
            assert_eq!(view.primary.hot, expected);
        }
        let row = row_rect(0);
        assert!(
            !project_hover(
                &refs,
                (row.x + row.w, row.y + row.h / 2.0),
                None,
                false,
                CancellationView::Unavailable
            )
            .rows[0]
                .hot
        );
    }

    #[test]
    fn focus_hover_and_busy_cancellation_keep_input_state_separate_from_styling() {
        let entries = [Entry::Image(PathBuf::from("dp-snesusa-release-v1.hsqs"))];
        let refs = [&entries[0]];
        for (focus, primary, sidebar) in [
            (Some(0), true, [false, false]),
            (Some(1), false, [true, false]),
            (Some(2), false, [false, true]),
            (Some(99), false, [false, false]),
            (None, false, [false, false]),
        ] {
            let view = project(
                &refs,
                0,
                false,
                CancellationView::Unavailable,
                "",
                false,
                None,
                focus,
                "ALL",
                false,
                0.0,
            );
            assert_eq!(view.primary.hot, primary);
            assert_eq!(view.sidebar_active, sidebar);
            assert!(view.rows[0].chosen);
        }

        for cancel in [
            CancellationView::Available,
            CancellationView::Requested,
            CancellationView::Finishing,
            CancellationView::Unavailable,
        ] {
            let view = project_hover(
                &refs,
                (BUTTONS[0].x + 1.0, BUTTONS[0].y + 1.0),
                Some(1),
                true,
                cancel,
            );
            assert_eq!(view.primary.hot, cancel == CancellationView::Available);
            assert!(view.sidebar_active[0]);
            assert!(view.rows[0].chosen);
            let scene = crate::frontend::launcher_presentation::compose(&view);
            assert!(!scene.before_status.iter().any(|command| {
                matches!(command, crate::frontend::launcher_scene::DrawCommand::Geometry(geometry)
                    if geometry == &crate::frontend::launcher_scene::sidebar_button_background(1, true, false).unwrap())
            }));
            let selected_row = crate::frontend::launcher_scene::library_row(
                0,
                true,
                true,
                true,
                crate::frontend::launcher_scene::RowDisplay {
                    title: &view.rows[0].title,
                    system_label: &view.rows[0].system_label,
                    version: &view.rows[0].version,
                    cached: view.rows[0].cached,
                    region: view.rows[0].region,
                },
            );
            assert!(
                scene
                    .before_status
                    .windows(selected_row.len())
                    .any(|group| group == selected_row)
            );
        }
    }

    #[test]
    fn invalid_nonempty_selections_and_native_classification_remain_unclamped() {
        let entries = [
            Entry::Image(PathBuf::from("dp-nes-release-v1.hsqs")),
            Entry::Image(PathBuf::from("dp-hvcj-release-v1.hsqs")),
            Entry::Image(PathBuf::from("dp-sneseur-release-v1.hsqs")),
            Entry::Image(PathBuf::from("custom.hsqs")),
            Entry::Cached(crate::profile::Profile {
                directory: PathBuf::from("display-cache-label"),
                manifest: serde_json::json!({"system":"nes", "ui":"hvc", "region":"Japan", "version":"1"}),
            }),
        ];
        let refs: Vec<_> = entries.iter().collect();
        let short_refs = &refs[..3];
        for (selected, selection_refs) in
            [(4, short_refs), (12, &refs[..]), (usize::MAX, &refs[..])]
        {
            let view = project_entries(
                selection_refs,
                selected,
                false,
                CancellationView::Unavailable,
            );
            assert_eq!(view.entry_count, selection_refs.len());
            assert_eq!(view.selected, selected);
            assert!(view.rows.iter().all(|row| !row.chosen));
            assert_eq!(view.detail.title, "Classic Menu");
            assert!(!view.detail.entry_exists);
            let scene = crate::frontend::launcher_presentation::compose(&view);
            assert!(!scene.before_status.iter().any(|command| {
                matches!(command, crate::frontend::launcher_scene::DrawCommand::Geometry(geometry)
                    if *geometry == crate::frontend::launcher_scene::empty_library_panel())
            }));
        }
        for (index, controller, title, label) in [
            (
                0,
                Some(crate::frontend::launcher_scene::ControllerIllustration::Nes),
                "NES Classic",
                "NES",
            ),
            (
                1,
                Some(crate::frontend::launcher_scene::ControllerIllustration::Famicom),
                "Famicom Classic",
                "NES",
            ),
            (
                2,
                Some(crate::frontend::launcher_scene::ControllerIllustration::Snes),
                "SNES Classic · Europe",
                "SNES",
            ),
            (3, None, "Classic Menu image", "IMAGE"),
            (
                4,
                Some(crate::frontend::launcher_scene::ControllerIllustration::Famicom),
                "Famicom Classic",
                "NES",
            ),
        ] {
            let view = project_entries(&refs, index, false, CancellationView::Unavailable);
            assert_eq!(view.detail.controller, controller);
            assert_eq!(view.detail.title, title);
            assert_eq!(view.rows[index].system_label, label);
        }
        assert_eq!(
            project_entries(&refs, 4, false, CancellationView::Unavailable)
                .detail
                .source,
            "display-cache-label"
        );
    }

    #[test]
    fn presentation_owns_strings_after_all_native_entries_and_inputs_are_dropped() {
        let view = {
            let entries = [Entry::Image(PathBuf::from("dp-snesusa-release-v7.hsqs"))];
            let status = String::from("native status text");
            let filter = String::from("SNES");
            let view = project(
                &[&entries[0]],
                0,
                false,
                CancellationView::Unavailable,
                &status,
                false,
                None,
                None,
                &filter,
                false,
                0.0,
            );
            drop(entries);
            drop(status);
            drop(filter);
            view
        };
        assert_eq!(view.status, "native status text");
        assert_eq!(view.filter_label, "SNES");
        assert_eq!(view.rows[0].title, "SNES Classic · United States");
        assert_eq!(view.detail.source, "dp-snesusa-release-v7.hsqs");
        let scene = crate::frontend::launcher_presentation::compose(&view);
        assert!(scene.before_status.iter().any(|command| {
            matches!(command, crate::frontend::launcher_scene::DrawCommand::Text(text) if text.value == "SNES Classic · United States")
        }));
        assert_eq!(scene.status, "native status text");
    }
}
