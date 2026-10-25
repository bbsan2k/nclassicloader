//! Owned, native-resource-free display data for the launcher.

use crate::frontend::launcher_activity::CancellationView;
use crate::frontend::launcher_scene::{
    self, ControllerIllustration, DrawCommand, Region, RowDisplay,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub entry_count: usize,
    pub selected: usize,
    pub busy: bool,
    pub cancel: CancellationView,
    pub status: String,
    pub error: bool,
    pub filter_label: String,
    pub controller_connected: bool,
    pub elapsed: f32,
    pub sidebar_active: [bool; 2],
    pub hot_tabs: [bool; 3],
    pub rows: Vec<PresentedRow>,
    pub detail: PresentedDetail,
    pub primary: PresentedPrimary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentedRow {
    pub row: usize,
    pub chosen: bool,
    pub hot: bool,
    pub title: String,
    pub system_label: String,
    pub version: String,
    pub cached: bool,
    pub region: Region,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentedDetail {
    pub title: String,
    pub version: String,
    pub region: Region,
    pub controller: Option<ControllerIllustration>,
    pub entry_exists: bool,
    pub ready: bool,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentedPrimary {
    pub label: String,
    pub enabled: bool,
    pub hot: bool,
}

/// Fully owned launcher draw groups separated by the native status-measurement barrier.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub before_status: Vec<DrawCommand>,
    pub status: String,
    pub error: bool,
    pub after_status: Vec<DrawCommand>,
}

/// Compose ordered launcher presentation requests without measuring or drawing them.
pub fn compose(presentation: &Presentation) -> Scene {
    let mut before_status: Vec<_> = launcher_scene::background()
        .into_iter()
        .map(DrawCommand::Geometry)
        .collect();
    before_status.extend(launcher_scene::branding());
    before_status.extend(launcher_scene::sidebar(
        presentation.busy,
        presentation.sidebar_active,
    ));
    before_status.extend(launcher_scene::library_header(
        presentation.entry_count,
        &presentation.filter_label,
        presentation.hot_tabs,
        presentation.busy,
    ));
    for item in &presentation.rows {
        before_status.extend(launcher_scene::library_row(
            item.row,
            item.chosen,
            item.hot,
            presentation.busy,
            RowDisplay {
                title: &item.title,
                system_label: &item.system_label,
                version: &item.version,
                cached: item.cached,
                region: item.region,
            },
        ));
    }
    if presentation.entry_count == 0 {
        before_status.extend(launcher_scene::empty_library(&presentation.filter_label));
    }
    before_status.extend(launcher_scene::detail_header(
        &presentation.detail.title,
        &presentation.detail.version,
        presentation.detail.controller,
        presentation.detail.entry_exists,
        presentation.detail.ready,
    ));
    before_status.extend(launcher_scene::detail_metadata(
        presentation.detail.region,
        presentation.detail.entry_exists,
        presentation.detail.ready,
        &presentation.detail.source,
    ));
    before_status.extend(launcher_scene::primary_button(
        &presentation.primary.label,
        presentation.primary.enabled,
        presentation.primary.hot,
    ));
    before_status.push(launcher_scene::page_indicator(
        presentation.selected,
        presentation.entry_count,
    ));

    let mut after_status: Vec<_> =
        launcher_scene::import_progress(presentation.busy, presentation.elapsed)
            .into_iter()
            .map(DrawCommand::Geometry)
            .collect();
    after_status.push(launcher_scene::footer(
        presentation.busy,
        presentation.cancel,
        presentation.controller_connected,
    ));

    Scene {
        before_status,
        status: presentation.status.clone(),
        error: presentation.error,
        after_status,
    }
}
