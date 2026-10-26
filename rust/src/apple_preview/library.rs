//! Real owned-cartridge library presentation. No filesystem access in drawing or
//! hit testing; opening a captured digest still requires fresh worker validation.
use super::*;
use crate::frontend::menu_frame::{Hit, HitAction};
use std::{ffi::CStr, path::Path};

const GLYPHS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const MASKS: [[u8; 7]; 36] = [
    [14, 17, 17, 31, 17, 17, 17],
    [30, 17, 17, 30, 17, 17, 30],
    [14, 17, 16, 16, 16, 17, 14],
    [30, 17, 17, 17, 17, 17, 30],
    [31, 16, 16, 30, 16, 16, 31],
    [31, 16, 16, 30, 16, 16, 16],
    [14, 17, 16, 23, 17, 17, 15],
    [17, 17, 17, 31, 17, 17, 17],
    [31, 4, 4, 4, 4, 4, 31],
    [7, 2, 2, 2, 18, 18, 12],
    [17, 18, 20, 24, 20, 18, 17],
    [16, 16, 16, 16, 16, 16, 31],
    [17, 27, 21, 21, 17, 17, 17],
    [17, 25, 25, 21, 19, 19, 17],
    [14, 17, 17, 17, 17, 17, 14],
    [30, 17, 17, 30, 16, 16, 16],
    [14, 17, 17, 17, 21, 18, 13],
    [30, 17, 17, 30, 20, 18, 17],
    [15, 16, 16, 14, 1, 1, 30],
    [31, 4, 4, 4, 4, 4, 4],
    [17, 17, 17, 17, 17, 17, 14],
    [17, 17, 17, 17, 17, 10, 4],
    [17, 17, 17, 21, 21, 27, 17],
    [17, 17, 10, 4, 10, 17, 17],
    [17, 17, 10, 4, 4, 4, 4],
    [31, 1, 2, 4, 8, 16, 31],
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
];

#[derive(Debug)]
pub(super) struct Catalog {
    revision: u64,
    entries: Vec<String>,
    selected: Option<String>,
    can_import: bool,
    loading: bool,
    limited: bool,
}
impl Catalog {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    fn parse(json: &str) -> Result<Self, String> {
        if json.len() > 32768 {
            return Err("library scene is too large".into());
        }
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let revision = value["revision"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("invalid library revision")?;
        let rows = value["entries"]
            .as_array()
            .filter(|a| a.len() <= 64)
            .ok_or("invalid library entries")?;
        let mut entries = Vec::new();
        for row in rows {
            let digest = row.as_str().ok_or("invalid library digest")?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || entries.iter().any(|id| id == digest)
            {
                return Err("invalid or duplicate library identity".into());
            }
            entries.push(digest.to_owned());
        }
        let selected = value["selected"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if selected.as_ref().is_some_and(|id| !entries.contains(id)) {
            return Err("selection is not in the current library".into());
        }
        Ok(Self {
            revision,
            entries,
            selected,
            can_import: value["can_import"]
                .as_bool()
                .ok_or("missing library capability")?,
            loading: value["loading"]
                .as_bool()
                .ok_or("missing library scan state")?,
            limited: value["limited"]
                .as_bool()
                .ok_or("missing library limit state")?,
        })
    }
    fn scene(&self, generation: usize) -> BaseMenuScene {
        let id = TextureId {
            generation,
            slot: 1,
        };
        let fonts = BTreeMap::from([("library".into(), id)]);
        let mut steps = vec![Step::Draw(Command::Fill {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 1280.0,
                h: 720.0,
            },
            color: Rgba {
                r: 16,
                g: 23,
                b: 38,
                a: 255,
            },
        })];
        let text = |label: &str, x: f32, y: f32| {
            let mut glyphs = Vec::new();
            let mut advance = 0.0;
            for ch in label.chars() {
                if let Some(index) = GLYPHS.chars().position(|candidate| candidate == ch) {
                    glyphs.push(PlacedGlyph {
                        font: "library".into(),
                        source: Rect {
                            x: (index * 6) as f32,
                            y: 0.0,
                            w: 5.0,
                            h: 7.0,
                        },
                        x: advance,
                        y: 0.0,
                        width: 20.0,
                        height: 28.0,
                    });
                }
                advance += 24.0;
            }
            let layout = TextLayout {
                glyphs,
                advance,
                line: 36.0,
                left: 0.0,
                top: 0.0,
                right: advance,
                bottom: 28.0,
            };
            menu_scene::text(
                &layout,
                &fonts,
                menu_scene::TextPosition {
                    x,
                    y,
                    max_width: 1050.0,
                    line_anchor: false,
                    tint: WHITE,
                },
            )
        };
        steps.extend(
            text("CLOVER NES LIBRARY", 180.0, 110.0)
                .into_iter()
                .map(Step::Draw),
        );
        if self.entries.is_empty() {
            steps.extend(
                text(
                    if self.loading {
                        "LOADING LIBRARY"
                    } else {
                        "NO NES CARTRIDGES"
                    },
                    180.0,
                    300.0,
                )
                .into_iter()
                .map(Step::Draw),
            );
            steps.extend(
                text(
                    if self.can_import {
                        "USE IMPORT NES TO ADD A GAME"
                    } else {
                        "TRANSFER NOT AVAILABLE"
                    },
                    180.0,
                    370.0,
                )
                .into_iter()
                .map(Step::Draw),
            );
        } else {
            let selected = self
                .selected
                .as_ref()
                .and_then(|id| self.entries.iter().position(|row| row == id))
                .unwrap_or(0);
            let start = selected
                .saturating_sub(2)
                .min(self.entries.len().saturating_sub(5));
            steps.extend(
                text(&format!("{} GAMES", self.entries.len()), 180.0, 170.0)
                    .into_iter()
                    .map(Step::Draw),
            );
            for (index, digest) in self.entries.iter().enumerate().skip(start).take(5) {
                let rect = Rect {
                    x: 160.0,
                    y: 235.0 + (index - start) as f32 * 74.0,
                    w: 960.0,
                    h: 64.0,
                };
                steps.push(Step::Draw(Command::Fill {
                    rect,
                    color: if index == selected {
                        Rgba {
                            r: 32,
                            g: 91,
                            b: 153,
                            a: 255,
                        }
                    } else {
                        Rgba {
                            r: 26,
                            g: 37,
                            b: 56,
                            a: 255,
                        }
                    },
                }));
                steps.extend(
                    text(
                        &format!("NES {}", digest[..8].to_ascii_uppercase()),
                        190.0,
                        rect.y + 17.0,
                    )
                    .into_iter()
                    .map(Step::Draw),
                );
                steps.push(Step::Hit(Hit {
                    action: HitAction::Game(GameKey(digest.clone())),
                    rect,
                }));
            }
            steps.extend(
                text(
                    if self.limited {
                        "LIBRARY LIMIT REACHED"
                    } else {
                        "SELECT A GAME THEN OPEN"
                    },
                    180.0,
                    650.0,
                )
                .into_iter()
                .map(Step::Draw),
            );
        }
        BaseMenuScene { generation, steps }
    }
}

fn atlas(host: &Host) -> Result<OwnedTexture, String> {
    let mut bytes = vec![0; 256 * 8 * 4];
    for (index, rows) in MASKS.iter().enumerate() {
        for (y, mask) in rows.iter().enumerate() {
            for x in 0..5 {
                if mask & (1 << (4 - x)) != 0 {
                    let at = (y * 256 + index * 6 + x) * 4;
                    bytes[at..at + 4].fill(255);
                }
            }
        }
    }
    let mut texture = OwnedTexture(Some(
        host.canvas
            .texture_creator()
            .create_texture_static(PixelFormat::RGBA32, 256, 8)
            .map_err(|e| e.to_string())?,
    ));
    texture
        .update(None, &bytes, 256 * 4)
        .map_err(|e| e.to_string())?;
    texture.set_blend_mode(BlendMode::Blend);
    Ok(texture)
}

/// # Safety
/// The host is live/exclusive on its owner thread; JSON is a live UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_library_scene(
    state: *mut c_void,
    json: *const c_char,
    error: *mut c_char,
    size: usize,
) -> i32 {
    guard(error, size, || {
        if json.is_null() {
            return Err("null library scene".into());
        }
        let host = unsafe { host(state)? };
        if host.game.is_some() {
            return Err("cannot replace the active game with a library scene".into());
        }
        let catalog = Catalog::parse(
            unsafe { CStr::from_ptr(json) }
                .to_str()
                .map_err(|e| e.to_string())?,
        )?;
        if host
            .library
            .as_ref()
            .is_some_and(|old| catalog.revision <= old.revision)
        {
            return Err("stale library revision".into());
        }
        let scene = catalog.scene(host.generation);
        if host.library_atlas.is_none() {
            host.library_atlas = Some(atlas(host)?);
        }
        host.scene = scene;
        host.library = Some(catalog);
        host.library_presented_revision = 0;
        host.paused = false;
        Ok(())
    })
}

/// # Safety
/// The host is live/exclusive on its owner thread. Output has at least 65 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_library_pick(
    state: *mut c_void,
    revision: u64,
    window: u32,
    x: f32,
    y: f32,
    digest: *mut c_char,
    size: usize,
    error: *mut c_char,
    error_size: usize,
) -> i32 {
    guard(error, error_size, || {
        if digest.is_null() || size < 65 {
            return Err("short library pick output".into());
        }
        super::error_out(digest, size, "");
        let host = unsafe { host(state)? };
        let catalog = host.library.as_ref().ok_or("no library scene")?;
        if host.game.is_some()
            || catalog.revision != revision
            || host.library_presented_revision != revision
            || host.scene.generation != host.generation
            || window != host.canvas.window().id()
        {
            return Ok(());
        }
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Ok(());
        }
        let (w, h) = host.canvas.output_size().map_err(|e| e.to_string())?;
        let scale = (w as f32 / 1280.0).min(h as f32 / 720.0);
        if scale <= 0.0 {
            return Ok(());
        }
        let x = (x * w as f32 - (w as f32 - 1280.0 * scale) * 0.5) / scale;
        let y = (y * h as f32 - (h as f32 - 720.0 * scale) * 0.5) / scale;
        let hits: Vec<_> = host
            .scene
            .steps
            .iter()
            .filter_map(|step| {
                if let Step::Hit(hit) = step {
                    Some(hit)
                } else {
                    None
                }
            })
            .collect();
        if let Some(hit) = menu_frame::last_hit(&hits, x, y, |hit| hit.rect)
            && let HitAction::Game(key) = &hit.action
            && catalog.entries.contains(&key.0)
        {
            super::error_out(digest, size, &key.0);
        }
        Ok(())
    })
}

fn path(pointer: *const c_char) -> Result<std::path::PathBuf, String> {
    if pointer.is_null() {
        return Err("null library path".into());
    }
    // SAFETY: the private ABI caller supplies a live, NUL-terminated path.
    let path = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map_err(|e| e.to_string())?;
    if path.len() > 1024 {
        return Err("library path too long".into());
    }
    Ok(Path::new(path).to_owned())
}

/// # Safety
/// Host is live/exclusive on its owner thread; output/error buffers are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_library_report(
    state: *mut c_void,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    error_size: usize,
) -> i32 {
    guard(error, error_size, || {
        if json.is_null() || size < 16384 {
            return Err("short library report buffer".into());
        }
        let host = unsafe { host(state)? };
        let (revision, entries, selected) = host
            .library
            .as_ref()
            .map(|catalog| {
                (
                    catalog.revision,
                    catalog.entries.clone(),
                    catalog.selected.clone(),
                )
            })
            .unwrap_or_default();
        let visible = revision != 0
            && revision == host.library_presented_revision
            && host.game.is_none()
            && !host.paused;
        let checksum = if visible { super::checksum(host)? } else { 0 };
        let report=serde_json::json!({"revision":revision,"presented_revision":host.library_presented_revision,
            "entries":entries,"selected":selected.unwrap_or_default(),"visible":visible,"checksum":checksum.to_string()}).to_string();
        if report.len() >= size {
            return Err("library report too large".into());
        }
        super::error_out(json, size, &report);
        Ok(())
    })
}

/// # Safety
/// Path is a live UTF-8 C string; buffers are writable and nonoverlapping. No host access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_library_scan(
    root: *const c_char,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    error_size: usize,
) -> i32 {
    guard(error, error_size, || {
        if json.is_null() || size < 262144 {
            return Err("library scan needs a 262144-byte output buffer".into());
        }
        let snapshot = crate::apple_library::scan_library(&path(root)?)?;
        let entries: Vec<_> = snapshot
            .entries
            .iter()
            .map(|entry| serde_json::json!({"digest":entry.digest,"mapper":entry.mapper}))
            .collect();
        let report=serde_json::json!({"entries":entries,"skipped":snapshot.skipped,"truncated":snapshot.truncated}).to_string();
        if report.len() >= size {
            return Err("library scan output too large".into());
        }
        super::error_out(json, size, &report);
        Ok(())
    })
}

/// # Safety
/// Paths/identity are live C strings; output/error buffers are writable and nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clover_apple_library_open(
    root: *const c_char,
    digest: *const c_char,
    json: *mut c_char,
    size: usize,
    error: *mut c_char,
    error_size: usize,
) -> i32 {
    guard(error, error_size, || {
        if digest.is_null() || json.is_null() || size < 4096 {
            return Err("invalid library open buffers".into());
        }
        let digest = unsafe { CStr::from_ptr(digest) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let entry = crate::apple_library::inspect_cartridge(&path(root)?, digest)?;
        let report=serde_json::json!({"rom":entry.rom,"saves":entry.saves,"digest":entry.digest,"mapper":entry.mapper}).to_string();
        if report.len() >= size {
            return Err("library open output too large".into());
        }
        super::error_out(json, size, &report);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_rejects_ambiguous_or_stale_identity_data() {
        let id = "a".repeat(64);
        let good = serde_json::json!({"revision":1,"entries":[id],"selected":id,"can_import":true,"loading":false,"limited":false});
        assert!(Catalog::parse(&good.to_string()).is_ok());
        let mut bad = good.clone();
        bad["entries"] = serde_json::json!([id, id]);
        assert!(Catalog::parse(&bad.to_string()).is_err());
        bad = good.clone();
        bad["selected"] = serde_json::json!("../escape");
        assert!(Catalog::parse(&bad.to_string()).is_err());
        bad = good;
        bad["revision"] = serde_json::json!(0);
        assert!(Catalog::parse(&bad.to_string()).is_err());
    }

    #[test]
    #[ignore = "requires caller SDL dummy/software drivers"]
    fn library_scene_is_real_and_picks_require_current_owner_revision_and_window() {
        assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
        let mut host = super::super::build().unwrap();
        host.canvas.window_mut().set_size(1600, 1000).unwrap();
        let id = "b".repeat(64);
        let value = serde_json::json!({"revision":1,"entries":[id],"selected":id,"can_import":true,"loading":false,"limited":false});
        let json = std::ffi::CString::new(value.to_string()).unwrap();
        let ptr = (&mut host as *mut Host).cast();
        let mut error = [0; 512];
        unsafe {
            assert_eq!(
                clover_apple_library_scene(ptr, json.as_ptr(), error.as_mut_ptr(), error.len()),
                0
            );
            for _ in 0..3 {
                assert_eq!(
                    super::super::clover_apple_preview_iterate(
                        ptr,
                        error.as_mut_ptr(),
                        error.len()
                    ),
                    0
                );
            }
            let mut report = vec![0; 16384];
            assert_eq!(
                clover_apple_library_report(
                    ptr,
                    report.as_mut_ptr(),
                    report.len(),
                    error.as_mut_ptr(),
                    error.len()
                ),
                0
            );
            let report: serde_json::Value =
                serde_json::from_str(CStr::from_ptr(report.as_ptr()).to_str().unwrap()).unwrap();
            assert_eq!(report["visible"], true);
            assert_eq!(report["entries"], serde_json::json!([id]));
            assert_ne!(report["checksum"], "0");
            let (w, h) = host.canvas.output_size().unwrap();
            let scale = (w as f32 / 1280.0).min(h as f32 / 720.0);
            let x = (200.0 * scale + (w as f32 - 1280.0 * scale) * 0.5) / w as f32;
            let y = (260.0 * scale + (h as f32 - 720.0 * scale) * 0.5) / h as f32;
            let window = host.canvas.window().id();
            let mut digest = [0; 65];
            for (revision, window, px, py, expected) in [
                (1, window, x, y, id.as_str()),
                (0, window, x, y, ""),
                (1, window + 1, x, y, ""),
                (1, window, f32::NAN, y, ""),
                (1, window, x, 0.0, ""),
            ] {
                assert_eq!(
                    clover_apple_library_pick(
                        ptr,
                        revision,
                        window,
                        px,
                        py,
                        digest.as_mut_ptr(),
                        digest.len(),
                        error.as_mut_ptr(),
                        error.len()
                    ),
                    0
                );
                assert_eq!(CStr::from_ptr(digest.as_ptr()).to_str().unwrap(), expected);
            }
            assert_eq!(
                clover_apple_library_scene(ptr, json.as_ptr(), error.as_mut_ptr(), error.len()),
                1
            );
            host.scene.generation += 1;
            assert_eq!(
                clover_apple_library_pick(
                    ptr,
                    1,
                    window,
                    x,
                    y,
                    digest.as_mut_ptr(),
                    digest.len(),
                    error.as_mut_ptr(),
                    error.len()
                ),
                0
            );
            assert_eq!(digest[0], 0);
        }
    }
}
