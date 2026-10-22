//! Read-only native launcher presentation snapshot and SDL draw adapter.
use super::*;
use sdl3::{render::Canvas, video::Window};

pub(super) struct PresentationSnapshot<'a> {
    pub entries: Vec<&'a Entry>,
    pub selected: usize,
    pub focus: Option<usize>,
    pub filter: LibraryFilter,
    pub busy: bool,
    pub cancel: import_worker::CancellationView,
    pub status: &'a str,
    pub error: bool,
    pub hover: Option<(f32, f32)>,
    pub controller_connected: bool,
    pub elapsed: f32,
}

impl<'a> PresentationSnapshot<'a> {
    pub fn from_native(
        state: &'a session::SessionState,
        worker: &import_worker::Worker,
        preview: bool,
        controller_connected: bool,
        elapsed: f32,
    ) -> Self {
        let cancel = if preview {
            state
                .preview_cancel
                .unwrap_or_else(|| worker.cancellation_view())
        } else {
            worker.cancellation_view()
        };
        Self {
            entries: visible_entries(&state.entries, state.navigation.filter),
            selected: state.navigation.selected,
            focus: state.navigation.focus,
            filter: state.navigation.filter,
            busy: state.activity.busy,
            cancel,
            status: &state.activity.status,
            error: state.activity.error,
            hover: state.hover,
            controller_connected: controller_connected
                || (preview
                    && std::env::var("CLOVER_LAUNCHER_PREVIEW_CONTROLLER").as_deref() == Ok("1")),
            elapsed,
        }
    }
}

pub(super) enum DrawResult {
    Drawn,
    PreviewSaved,
}

pub(super) fn draw(
    view: &mut view::View,
    canvas: &mut Canvas<Window>,
    snapshot: &PresentationSnapshot<'_>,
    preview: Option<&Path>,
) -> Result<DrawResult, String> {
    view.draw(
        canvas,
        &snapshot.entries,
        snapshot.selected,
        snapshot.busy,
        snapshot.cancel,
        snapshot.status,
        snapshot.error,
        snapshot.hover,
        snapshot.focus,
        snapshot.filter.label(),
        snapshot.controller_connected,
        snapshot.elapsed,
    )?;
    if let Some(path) = preview {
        super::super::save_screenshot(canvas, path)?;
        Ok(DrawResult::PreviewSaved)
    } else {
        Ok(DrawResult::Drawn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> session::SessionState {
        session::SessionState::new(
            startup::Startup {
                entries: vec![Entry::Image(PathBuf::from("fixture.hsqs"))],
                selected: 0,
                filter: LibraryFilter::All,
            },
            true,
        )
    }

    #[test]
    fn snapshot_preserves_visible_navigation_activity_and_host_values() {
        let mut state = state();
        state.navigation.focus = Some(2);
        state.activity.busy = true;
        state.activity.status = "current status".into();
        state.activity.error = true;
        state.hover = Some((34.5, 67.25));
        state.preview_cancel = Some(import_worker::CancellationView::Finishing);
        let services = session::NativeServices::default();
        let before = (
            state.navigation.selected,
            state.navigation.focus,
            state.navigation.filter,
            state.activity.clone(),
            state.hover,
            services.worker.cancellation_view(),
        );

        let normal = PresentationSnapshot::from_native(&state, &services.worker, false, true, 1.25);
        assert_eq!(normal.entries.len(), 1);
        assert_eq!(entry_key(normal.entries[0]), "fixture.hsqs");
        assert_eq!(normal.selected, 0);
        assert_eq!(normal.focus, Some(2));
        assert_eq!(normal.filter, LibraryFilter::All);
        assert!(normal.busy);
        assert_eq!(normal.status, "current status");
        assert!(normal.error);
        assert_eq!(normal.hover, Some((34.5, 67.25)));
        assert_eq!(normal.cancel, import_worker::CancellationView::Unavailable);
        assert!(normal.controller_connected);
        assert_eq!(normal.elapsed, 1.25);

        let preview = PresentationSnapshot::from_native(&state, &services.worker, true, false, 2.5);
        assert_eq!(preview.cancel, import_worker::CancellationView::Finishing);
        assert_eq!(preview.elapsed, 2.5);
        assert_eq!(
            before,
            (
                state.navigation.selected,
                state.navigation.focus,
                state.navigation.filter,
                state.activity.clone(),
                state.hover,
                services.worker.cancellation_view(),
            )
        );
    }
}
