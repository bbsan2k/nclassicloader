//! Host-neutral launcher layout, geometry and owned text requests.
//! Font measurement, flags and rasterization resources stay in the native adapter.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 760;
pub const PAGE_SIZE: usize = 6;
pub const BUTTONS: [Rect; 3] = [
    Rect {
        x: 866.0,
        y: 650.0,
        w: 294.0,
        h: 58.0,
    },
    Rect {
        x: 24.0,
        y: 285.0,
        w: 192.0,
        h: 48.0,
    },
    Rect {
        x: 24.0,
        y: 342.0,
        w: 192.0,
        h: 48.0,
    },
];
pub const FILTER_TABS: [Rect; 3] = [
    Rect {
        x: 272.0,
        y: 146.0,
        w: 72.0,
        h: 30.0,
    },
    Rect {
        x: 350.0,
        y: 146.0,
        w: 72.0,
        h: 30.0,
    },
    Rect {
        x: 428.0,
        y: 146.0,
        w: 72.0,
        h: 30.0,
    },
];

pub fn row_rect(row: usize) -> Rect {
    Rect {
        x: 264.0,
        y: 194.0 + row as f32 * 72.0,
        w: 556.0,
        h: 62.0,
    }
}

/// Map host-window coordinates into the letterboxed launcher stage.
/// Points in the bars remain outside the stage; the legacy minimum scale also
/// applies to zero-sized windows. This does not clamp or authorize hit targets.
pub fn coordinates(width: u32, height: u32, x: f32, y: f32) -> (f32, f32) {
    let scale = (width as f32 / WIDTH as f32)
        .min(height as f32 / HEIGHT as f32)
        .max(0.001);
    (
        (x - (width as f32 - WIDTH as f32 * scale) * 0.5) / scale,
        (y - (height as f32 - HEIGHT as f32 * scale) * 0.5) / scale,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeometryCommand {
    FillRect {
        rect: Rect,
        color: Rgba,
    },
    RoundedRect {
        rect: Rect,
        color: Rgba,
        radius: f32,
    },
}

/// Built-in launcher font roles, not per-renderer resource or texture handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontRole {
    Regular,
    Semibold,
}

/// Normalized display region, not a flag texture/resource handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Japan,
    Europe,
    Usa,
    Unknown,
}

impl Region {
    pub fn from_marker(marker: &str) -> Self {
        match marker {
            "JPN" | "Japan" => Self::Japan,
            "EUR" | "Europe" => Self::Europe,
            "USA" => Self::Usa,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Japan => "Japan",
            Self::Europe => "Europe",
            Self::Usa => "United States",
            Self::Unknown => "Unknown",
        }
    }
}

/// Single-line text intent in launcher stage coordinates. `y` is the top of the
/// line, and `max_width` requests ellipsis fitting, not clipping. The host owns
/// font measurement/rasterization; this request contains no borrowed glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCommand {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub size: u16,
    pub font: FontRole,
    pub color: Rgba,
    pub max_width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    Geometry(GeometryCommand),
    Text(TextCommand),
    Flag { region: Region, rect: Rect },
}

/// The flag shown in a library row, positioned relative to its row rectangle.
pub fn row_flag(row: usize, region: Region) -> DrawCommand {
    let row = row_rect(row);
    DrawCommand::Flag {
        region,
        rect: Rect {
            x: row.x + 22.0,
            y: row.y + 20.0,
            w: 32.0,
            h: 24.0,
        },
    }
}

/// The flag shown in the detail metadata panel.
pub fn detail_flag(region: Region) -> DrawCommand {
    DrawCommand::Flag {
        region,
        rect: Rect {
            x: 866.0,
            y: 505.0,
            w: 24.0,
            h: 18.0,
        },
    }
}

/// Ordered detail-panel header presentation; classification and action state
/// remain with the caller.
pub fn detail_header(
    title: &str,
    version: &str,
    controller: Option<ControllerIllustration>,
    entry_exists: bool,
    ready: bool,
) -> Vec<DrawCommand> {
    let mut commands = vec![DrawCommand::Geometry(detail_panel())];
    if let Some(controller) = controller {
        commands.extend(
            controller_illustration(controller, 1010.0, 272.0)
                .into_iter()
                .map(DrawCommand::Geometry),
        );
    } else {
        commands.push(DrawCommand::Geometry(unknown_controller_panel()));
        commands.push(text_command(
            "?",
            993.0,
            242.0,
            44,
            FontRole::Semibold,
            MUTED,
            50.0,
        ));
    }
    commands.push(text_command(
        title,
        866.0,
        353.0,
        25,
        FontRole::Semibold,
        INK,
        290.0,
    ));
    commands.push(text_command(
        version,
        866.0,
        390.0,
        16,
        FontRole::Regular,
        MUTED,
        290.0,
    ));
    commands.push(text_command(
        if ready {
            "VERIFIED CACHE"
        } else if entry_exists {
            "READY TO IMPORT"
        } else {
            "AWAITING GAME DATA"
        },
        866.0,
        433.0,
        12,
        FontRole::Semibold,
        ACCENT,
        290.0,
    ));
    commands.push(DrawCommand::Geometry(detail_separator()));
    commands
}

/// Ordered region and source metadata for the detail panel. `source` is display
/// data supplied by the host; this builder never interprets it as a path.
pub fn detail_metadata(
    region: Region,
    entry_exists: bool,
    ready: bool,
    source: &str,
) -> Vec<DrawCommand> {
    vec![
        text_command("REGION", 866.0, 482.0, 11, FontRole::Semibold, MUTED, 120.0),
        detail_flag(region),
        text_command(
            region.label(),
            902.0,
            503.0,
            16,
            FontRole::Semibold,
            INK,
            102.0,
        ),
        text_command(
            "SOURCE",
            1014.0,
            482.0,
            11,
            FontRole::Semibold,
            MUTED,
            130.0,
        ),
        text_command(
            if ready {
                "Local cache"
            } else if entry_exists {
                "Local image"
            } else {
                "Not selected"
            },
            1014.0,
            503.0,
            18,
            FontRole::Semibold,
            INK,
            140.0,
        ),
        text_command(source, 866.0, 550.0, 13, FontRole::Regular, MUTED, 286.0),
        text_command(
            if ready {
                "No original image needed."
            } else {
                "Imported locally. Never executed."
            },
            866.0,
            576.0,
            14,
            FontRole::Regular,
            MUTED,
            286.0,
        ),
    ]
}

/// Borrowed display data for one library row. Commands built from it own text.
pub struct RowDisplay<'a> {
    pub title: &'a str,
    pub system_label: &'a str,
    pub version: &'a str,
    pub cached: bool,
    pub region: Region,
}

const ACCENT: Rgba = Rgba {
    r: 94,
    g: 234,
    b: 210,
    a: 255,
};
const INK: Rgba = Rgba {
    r: 236,
    g: 242,
    b: 245,
    a: 255,
};
const MUTED: Rgba = Rgba {
    r: 156,
    g: 171,
    b: 186,
    a: 255,
};

fn text_command(
    value: &str,
    x: f32,
    y: f32,
    size: u16,
    font: FontRole,
    color: Rgba,
    max_width: f32,
) -> DrawCommand {
    DrawCommand::Text(TextCommand {
        value: value.to_owned(),
        x,
        y,
        size,
        font,
        color,
        max_width,
    })
}

pub fn brand_accent() -> GeometryCommand {
    GeometryCommand::FillRect {
        rect: Rect {
            x: 28.0,
            y: 122.0,
            w: 36.0,
            h: 3.0,
        },
        color: ACCENT,
    }
}

/// The launcher's ordered branding text and accent commands.
pub fn branding() -> Vec<DrawCommand> {
    vec![
        text_command("CLOVER", 28.0, 35.0, 32, FontRole::Semibold, INK, 180.0),
        text_command(
            "CLASSIC MENU / RUST",
            29.0,
            77.0,
            12,
            FontRole::Semibold,
            ACCENT,
            190.0,
        ),
        DrawCommand::Geometry(brand_accent()),
        text_command(
            "A classic, reimagined.",
            28.0,
            146.0,
            17,
            FontRole::Regular,
            MUTED,
            190.0,
        ),
    ]
}

/// Ordered sidebar geometry and text. Styling inputs carry no action authority.
pub fn sidebar(busy: bool, active_buttons: [bool; 2]) -> Vec<DrawCommand> {
    let mut commands = Vec::with_capacity(11);
    commands.extend(library_heading().into_iter().map(DrawCommand::Geometry));
    commands.push(text_command(
        "Your library",
        38.0,
        226.0,
        19,
        FontRole::Semibold,
        ACCENT,
        164.0,
    ));
    for (offset, label) in [(0, "+  Import image"), (1, "Open data folder")] {
        if let Some(background) =
            sidebar_button_background(offset + 1, active_buttons[offset], busy)
        {
            commands.push(DrawCommand::Geometry(background));
        }
        commands.push(text_command(
            label,
            38.0,
            BUTTONS[offset + 1].y + 12.0,
            18,
            FontRole::Regular,
            if busy { MUTED } else { INK },
            172.0,
        ));
    }
    commands.extend([
        text_command(
            "LOCAL. PRIVATE. YOURS.",
            28.0,
            590.0,
            12,
            FontRole::Semibold,
            ACCENT,
            190.0,
        ),
        text_command(
            "Your images stay read-only.",
            28.0,
            618.0,
            15,
            FontRole::Regular,
            MUTED,
            192.0,
        ),
        text_command(
            "Saves live separately.",
            28.0,
            640.0,
            15,
            FontRole::Regular,
            MUTED,
            192.0,
        ),
        text_command(
            "SDL3 + ares",
            28.0,
            714.0,
            14,
            FontRole::Regular,
            MUTED,
            192.0,
        ),
    ]);
    commands
}

/// Ordered launcher heading, source count and filter-tab commands.
pub fn library_header(
    source_count: usize,
    filter_label: &str,
    hot_tabs: [bool; 3],
    busy: bool,
) -> Vec<DrawCommand> {
    let mut commands = vec![
        text_command(
            "MAKE YOURSELF AT HOME",
            272.0,
            35.0,
            13,
            FontRole::Semibold,
            ACCENT,
            700.0,
        ),
        text_command(
            "Choose your Classic.",
            270.0,
            59.0,
            42,
            FontRole::Semibold,
            INK,
            850.0,
        ),
        text_command(
            "Pick a region and version, or bring your own filesystem image.",
            272.0,
            118.0,
            18,
            FontRole::Regular,
            MUTED,
            870.0,
        ),
        text_command(
            &format!("YOUR LIBRARY  /  {source_count} SOURCES"),
            530.0,
            154.0,
            12,
            FontRole::Semibold,
            MUTED,
            285.0,
        ),
    ];
    for (index, (label, rect)) in ["ALL", "NES", "SNES"]
        .into_iter()
        .zip(FILTER_TABS)
        .enumerate()
    {
        let active = label == filter_label;
        if let Some(background) = filter_tab_background(index, active, hot_tabs[index], busy) {
            commands.push(DrawCommand::Geometry(background));
        }
        commands.push(text_command(
            label,
            rect.x + 14.0,
            rect.y + 6.0,
            13,
            FontRole::Semibold,
            if active {
                Rgba {
                    r: 12,
                    g: 35,
                    b: 38,
                    a: 255,
                }
            } else {
                INK
            },
            52.0,
        ));
    }
    commands
}

pub fn library_heading() -> [GeometryCommand; 2] {
    [
        GeometryCommand::RoundedRect {
            rect: Rect {
                x: 16.0,
                y: 215.0,
                w: 208.0,
                h: 48.0,
            },
            color: Rgba {
                r: 28,
                g: 46,
                b: 55,
                a: 255,
            },
            radius: 8.0,
        },
        GeometryCommand::FillRect {
            rect: Rect {
                x: 16.0,
                y: 230.0,
                w: 3.0,
                h: 18.0,
            },
            color: ACCENT,
        },
    ]
}

pub fn sidebar_button_background(
    index: usize,
    active: bool,
    busy: bool,
) -> Option<GeometryCommand> {
    if !(1..=2).contains(&index) || !active || busy {
        return None;
    }
    Some(GeometryCommand::RoundedRect {
        rect: BUTTONS[index],
        color: Rgba {
            r: 35,
            g: 49,
            b: 64,
            a: 255,
        },
        radius: 8.0,
    })
}

pub fn filter_tab_background(
    index: usize,
    active: bool,
    hot: bool,
    busy: bool,
) -> Option<GeometryCommand> {
    let rect = *FILTER_TABS.get(index)?;
    let color = if active {
        ACCENT
    } else if hot && !busy {
        Rgba {
            r: 45,
            g: 66,
            b: 80,
            a: 255,
        }
    } else {
        Rgba {
            r: 29,
            g: 44,
            b: 58,
            a: 255,
        }
    };
    Some(GeometryCommand::RoundedRect {
        rect,
        color,
        radius: 6.0,
    })
}

pub fn library_row_background(
    row: usize,
    chosen: bool,
    hot: bool,
    busy: bool,
) -> Vec<GeometryCommand> {
    let rect = row_rect(row);
    let mut commands = Vec::with_capacity(if chosen { 2 } else { 1 });
    if chosen {
        commands.push(GeometryCommand::RoundedRect {
            rect,
            color: ACCENT,
            radius: 9.0,
        });
    }
    commands.push(GeometryCommand::RoundedRect {
        rect: if chosen {
            Rect {
                x: rect.x + 1.0,
                y: rect.y + 1.0,
                w: rect.w - 2.0,
                h: rect.h - 2.0,
            }
        } else {
            rect
        },
        color: if chosen {
            Rgba {
                r: 28,
                g: 51,
                b: 60,
                a: 255,
            }
        } else if hot && !busy {
            Rgba {
                r: 36,
                g: 49,
                b: 65,
                a: 255,
            }
        } else {
            Rgba {
                r: 25,
                g: 36,
                b: 49,
                a: 255,
            }
        },
        radius: 8.0,
    });
    commands
}

pub fn library_row_flag_background(row: usize, chosen: bool) -> GeometryCommand {
    let rect = row_rect(row);
    GeometryCommand::RoundedRect {
        rect: Rect {
            x: rect.x + 14.0,
            y: rect.y + 12.0,
            w: 48.0,
            h: 38.0,
        },
        color: if chosen {
            Rgba {
                r: 54,
                g: 83,
                b: 88,
                a: 255,
            }
        } else {
            Rgba {
                r: 41,
                g: 54,
                b: 69,
                a: 255,
            }
        },
        radius: 6.0,
    }
}

/// The complete ordered presentation for a library row.
pub fn library_row(
    row: usize,
    chosen: bool,
    hot: bool,
    busy: bool,
    display: RowDisplay<'_>,
) -> Vec<DrawCommand> {
    let rect = row_rect(row);
    let mut commands: Vec<_> = library_row_background(row, chosen, hot, busy)
        .into_iter()
        .map(DrawCommand::Geometry)
        .collect();
    commands.push(text_command(
        display.system_label,
        rect.x + 438.0,
        rect.y + 14.0,
        12,
        FontRole::Semibold,
        if chosen { ACCENT } else { MUTED },
        70.0,
    ));
    commands.push(DrawCommand::Geometry(library_row_flag_background(
        row, chosen,
    )));
    commands.push(row_flag(row, display.region));
    commands.push(text_command(
        display.title,
        rect.x + 78.0,
        rect.y + 8.0,
        20,
        FontRole::Semibold,
        INK,
        340.0,
    ));
    let kind = if display.cached {
        "Ready to play"
    } else {
        "Filesystem image"
    };
    commands.push(text_command(
        &format!("{}  ·  {kind}", display.version),
        rect.x + 78.0,
        rect.y + 35.0,
        14,
        FontRole::Regular,
        MUTED,
        445.0,
    ));
    if chosen {
        commands.push(text_command(
            "›",
            rect.x + 523.0,
            rect.y + 13.0,
            27,
            FontRole::Semibold,
            ACCENT,
            22.0,
        ));
    }
    commands
}

pub fn empty_library_panel() -> GeometryCommand {
    GeometryCommand::RoundedRect {
        rect: Rect {
            x: 264.0,
            y: 194.0,
            w: 556.0,
            h: 422.0,
        },
        color: Rgba {
            r: 25,
            g: 36,
            b: 49,
            a: 255,
        },
        radius: 10.0,
    }
}

/// Ordered empty-library panel and text requests for the selected filter.
pub fn empty_library(filter_label: &str) -> Vec<DrawCommand> {
    let title = if filter_label == "ALL" {
        "Your library starts here.".to_owned()
    } else {
        format!("No {filter_label} versions found.")
    };
    vec![
        DrawCommand::Geometry(empty_library_panel()),
        text_command(&title, 294.0, 265.0, 28, FontRole::Semibold, INK, 490.0),
        text_command(
            "Import a supplied .hsqs or .tar.gz image to begin.",
            294.0,
            317.0,
            18,
            FontRole::Regular,
            MUTED,
            490.0,
        ),
        text_command(
            "Already have game data? Open its assets folder.",
            294.0,
            347.0,
            18,
            FontRole::Regular,
            MUTED,
            490.0,
        ),
        text_command(
            "No game files are included or downloaded.",
            294.0,
            530.0,
            16,
            FontRole::Regular,
            MUTED,
            490.0,
        ),
    ]
}

pub fn detail_panel() -> GeometryCommand {
    GeometryCommand::RoundedRect {
        rect: Rect {
            x: 844.0,
            y: 194.0,
            w: 332.0,
            h: 422.0,
        },
        color: Rgba {
            r: 23,
            g: 34,
            b: 47,
            a: 255,
        },
        radius: 12.0,
    }
}

pub fn unknown_controller_panel() -> GeometryCommand {
    GeometryCommand::RoundedRect {
        rect: Rect {
            x: 966.0,
            y: 234.0,
            w: 88.0,
            h: 76.0,
        },
        color: Rgba {
            r: 43,
            g: 59,
            b: 77,
            a: 255,
        },
        radius: 12.0,
    }
}

pub fn detail_separator() -> GeometryCommand {
    GeometryCommand::FillRect {
        rect: Rect {
            x: 866.0,
            y: 464.0,
            w: 286.0,
            h: 1.0,
        },
        color: Rgba {
            r: 48,
            g: 62,
            b: 76,
            a: 255,
        },
    }
}

pub fn primary_button_background(enabled: bool, hot: bool) -> GeometryCommand {
    GeometryCommand::RoundedRect {
        rect: BUTTONS[0],
        color: if !enabled {
            Rgba {
                r: 43,
                g: 57,
                b: 68,
                a: 255,
            }
        } else if hot {
            Rgba {
                r: 143,
                g: 249,
                b: 226,
                a: 255,
            }
        } else {
            ACCENT
        },
        radius: 9.0,
    }
}

/// Ordered primary-button geometry and owned text request. Label and eligibility
/// remain caller-supplied presentation data.
pub fn primary_button(label: &str, enabled: bool, hot: bool) -> Vec<DrawCommand> {
    vec![
        DrawCommand::Geometry(primary_button_background(enabled, hot)),
        text_command(
            label,
            890.0,
            666.0,
            21,
            FontRole::Semibold,
            if enabled {
                Rgba {
                    r: 12,
                    g: 35,
                    b: 38,
                    a: 255,
                }
            } else {
                MUTED
            },
            245.0,
        ),
    ]
}

/// Ordered launcher footer presentation; the cancellation snapshot grants no
/// action authority.
pub fn footer(
    busy: bool,
    cancel: crate::frontend::launcher_activity::CancellationView,
    controller_connected: bool,
) -> DrawCommand {
    use crate::frontend::launcher_activity::CancellationView;
    let value = if !busy {
        if controller_connected {
            "D-pad / Stick  Navigate    South / Start  Confirm    East  Back    Shoulders  System    F11  Full screen"
        } else {
            "Up / Down  Select    Enter  Launch    1 / 2 / 3  System    I  Import    D  Folder    F11  Full screen"
        }
    } else {
        match (cancel, controller_connected) {
            (CancellationView::Available, true) => {
                "East  Cancel import    Waiting for a safe stopping point    F11  Full screen"
            }
            (CancellationView::Available, false) => {
                "Escape  Cancel import    Waiting for a safe stopping point    F11  Full screen"
            }
            (CancellationView::Requested, _) => {
                "Cancellation requested    Waiting for a safe stopping point    F11  Full screen"
            }
            (CancellationView::Finishing, _) => {
                "Finishing import    Cancellation is too late    F11  Full screen"
            }
            (CancellationView::Unavailable, true) => {
                "D-pad / Stick  Navigate    South / Start  Confirm    East  Back    Shoulders  System    F11  Full screen"
            }
            (CancellationView::Unavailable, false) => {
                "Up / Down  Select    Enter  Launch    1 / 2 / 3  System    I  Import    D  Folder    F11  Full screen"
            }
        }
    };
    text_command(value, 272.0, 728.0, 13, FontRole::Regular, MUTED, 880.0)
}

/// The legacy page/count indicator. Selection is intentionally not clamped.
pub fn page_indicator(selected: usize, entry_count: usize) -> DrawCommand {
    let page = selected / PAGE_SIZE;
    let displayed_page = if entry_count == 0 { 0 } else { page + 1 };
    text_command(
        &format!("{displayed_page} / {}", entry_count.div_ceil(PAGE_SIZE)),
        272.0,
        626.0,
        13,
        FontRole::Regular,
        MUTED,
        100.0,
    )
}

/// Presentation-only track and thumb, in draw order. The host supplies elapsed
/// seconds; this animation neither advances an import nor acknowledges a result.
pub fn import_progress(busy: bool, elapsed: f32) -> Vec<GeometryCommand> {
    if !busy {
        return Vec::new();
    }
    let offset = (elapsed * 170.0).rem_euclid(420.0);
    vec![
        GeometryCommand::RoundedRect {
            rect: Rect {
                x: 272.0,
                y: 640.0,
                w: 548.0,
                h: 3.0,
            },
            color: Rgba {
                r: 43,
                g: 61,
                b: 74,
                a: 255,
            },
            radius: 1.0,
        },
        GeometryCommand::FillRect {
            rect: Rect {
                x: 272.0 + offset,
                y: 640.0,
                w: 128.0,
                h: 3.0,
            },
            color: ACCENT,
        },
    ]
}

/// Ordered scanline wash, translucent sidebar, then opaque separator.
pub fn background() -> Vec<GeometryCommand> {
    let mut out = Vec::with_capacity(HEIGHT as usize + 2);
    for y in 0..HEIGHT {
        let t = y as f32 / HEIGHT as f32;
        out.push(GeometryCommand::FillRect {
            rect: Rect {
                x: 0.0,
                y: y as f32,
                w: WIDTH as f32,
                h: 1.0,
            },
            color: Rgba {
                r: (20.0 - 7.0 * t) as u8,
                g: (30.0 - 10.0 * t) as u8,
                b: (43.0 - 13.0 * t) as u8,
                a: 255,
            },
        });
    }
    out.push(GeometryCommand::FillRect {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 240.0,
            h: HEIGHT as f32,
        },
        color: Rgba {
            r: 10,
            g: 16,
            b: 24,
            a: 225,
        },
    });
    out.push(GeometryCommand::FillRect {
        rect: Rect {
            x: 239.0,
            y: 0.0,
            w: 1.0,
            h: HEIGHT as f32,
        },
        color: Rgba {
            r: 39,
            g: 52,
            b: 65,
            a: 255,
        },
    });
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControllerIllustration {
    Nes,
    Famicom,
    Snes,
}

fn panel(
    out: &mut Vec<GeometryCommand>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rgb: (u8, u8, u8),
    radius: f32,
) {
    out.push(GeometryCommand::RoundedRect {
        rect: Rect { x, y, w, h },
        color: Rgba {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
            a: 255,
        },
        radius,
    });
}

pub fn controller_illustration(
    kind: ControllerIllustration,
    x: f32,
    y: f32,
) -> Vec<GeometryCommand> {
    let mut out = Vec::new();
    match kind {
        ControllerIllustration::Nes | ControllerIllustration::Famicom => {
            let famicom = kind == ControllerIllustration::Famicom;
            let shell = if famicom {
                (150, 40, 49)
            } else {
                (190, 198, 204)
            };
            let face = if famicom {
                (205, 179, 111)
            } else {
                (31, 39, 46)
            };
            let dark = (17, 25, 32);
            panel(
                &mut out,
                x - 120.0,
                y - 43.0,
                240.0,
                94.0,
                (13, 24, 35),
                8.0,
            );
            panel(&mut out, x - 118.0, y - 49.0, 236.0, 94.0, shell, 6.0);
            panel(&mut out, x - 108.0, y - 39.0, 216.0, 74.0, face, 3.0);
            for dy in [-25.0, -15.0, -5.0, 26.0] {
                panel(
                    &mut out,
                    x - 24.0,
                    y + dy,
                    65.0,
                    5.0,
                    if famicom { shell } else { (85, 97, 107) },
                    2.0,
                );
            }
            panel(&mut out, x - 91.0, y - 6.0, 47.0, 15.0, dark, 2.0);
            panel(&mut out, x - 75.0, y - 22.0, 15.0, 47.0, dark, 2.0);
            for dx in [-18.0, 13.0] {
                panel(&mut out, x + dx, y + 10.0, 22.0, 8.0, (109, 122, 132), 4.0);
            }
            for dx in [60.0, 89.0] {
                panel(
                    &mut out,
                    x + dx - 13.0,
                    y - 1.0,
                    26.0,
                    26.0,
                    if famicom { shell } else { (215, 50, 61) },
                    13.0,
                );
            }
        }
        ControllerIllustration::Snes => {
            panel(
                &mut out,
                x - 118.0,
                y - 42.0,
                236.0,
                94.0,
                (13, 24, 35),
                42.0,
            );
            panel(
                &mut out,
                x - 114.0,
                y - 48.0,
                228.0,
                94.0,
                (59, 76, 94),
                42.0,
            );
            panel(
                &mut out,
                x - 106.0,
                y - 40.0,
                212.0,
                78.0,
                (43, 59, 77),
                34.0,
            );
            panel(&mut out, x - 84.0, y - 8.0, 48.0, 16.0, (18, 29, 41), 3.0);
            panel(&mut out, x - 68.0, y - 24.0, 16.0, 48.0, (18, 29, 41), 3.0);
            for (dx, dy, c) in [
                (60.0, -18.0, (146, 142, 221)),
                (81.0, 1.0, (94, 234, 210)),
                (39.0, 1.0, (91, 170, 194)),
                (60.0, 20.0, (214, 147, 179)),
            ] {
                panel(&mut out, x + dx - 10.0, y + dy - 10.0, 20.0, 20.0, c, 10.0);
            }
            for dx in [-14.0, 9.0] {
                panel(&mut out, x + dx, y + 4.0, 17.0, 6.0, (131, 151, 168), 3.0);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn illustration_sizes_and_offset_geometry_are_stable() {
        let nes = controller_illustration(ControllerIllustration::Nes, 1010.0, 272.0);
        let famicom = controller_illustration(ControllerIllustration::Famicom, 1010.0, 272.0);
        let snes = controller_illustration(ControllerIllustration::Snes, 1010.0, 272.0);
        assert_eq!((nes.len(), famicom.len(), snes.len()), (13, 13, 11));
        assert_eq!(
            nes[0],
            GeometryCommand::RoundedRect {
                rect: Rect {
                    x: 890.0,
                    y: 229.0,
                    w: 240.0,
                    h: 94.0
                },
                color: Rgba {
                    r: 13,
                    g: 24,
                    b: 35,
                    a: 255
                },
                radius: 8.0
            }
        );
        assert_ne!(nes[1], famicom[1]);
        assert_eq!(
            snes[0],
            GeometryCommand::RoundedRect {
                rect: Rect {
                    x: 892.0,
                    y: 230.0,
                    w: 236.0,
                    h: 94.0
                },
                color: Rgba {
                    r: 13,
                    g: 24,
                    b: 35,
                    a: 255
                },
                radius: 42.0
            }
        );
    }
}
