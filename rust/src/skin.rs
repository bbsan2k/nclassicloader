//! Decode only the static, hash-identified Clover UI data used by the menu.
//! Lua resources are treated as bytes; no console scripts are executed.

use crate::{hash::digest, image_reader::Files};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

type SkinResult<T> = Result<T, String>;

fn require(test: bool, message: &str) -> SkinResult<()> {
    if test {
        Ok(())
    } else {
        Err(format!("Skin importer: {message}"))
    }
}

fn read<'a>(files: &'a Files, root: &str, relative: &str) -> SkinResult<&'a [u8]> {
    files
        .get(&format!("{root}{relative}"))
        .map(Vec::as_slice)
        .ok_or_else(|| format!("Image missing skin member: {relative}"))
}

fn write(path: &Path, bytes: &[u8]) -> SkinResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create skin output directory: {e}"))?;
    }
    fs::write(path, bytes).map_err(|e| format!("Cannot write decoded skin: {e}"))
}

fn write_json(path: &Path, value: &Value) -> SkinResult<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|e| format!("Cannot encode skin JSON: {e}"))?;
    bytes.push(b'\n');
    write(path, &bytes)
}

pub(crate) fn valid_locale(locale: &str) -> bool {
    !locale.is_empty()
        && locale.len() <= 24
        && locale
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

fn import_languages(files: &Files, root: &str, default: &str, output: &Path) -> SkinResult<()> {
    let prefix = format!("{root}strings/");
    let mut languages = Vec::new();
    let mut seen = BTreeSet::new();
    for (path, bytes) in files {
        let Some(locale) = path.strip_prefix(&prefix).and_then(|value| {
            value
                .strip_suffix("/strings.lng")
                .or_else(|| value.strip_suffix("/strings.json"))
        }) else {
            continue;
        };
        require(valid_locale(locale), "Invalid language identifier")?;
        require(seen.insert(locale), "Duplicate language table")?;
        require(
            bytes.len() <= 1024 * 1024,
            "Language table exceeds 1 MiB limit",
        )?;
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|e| format!("Invalid {locale} strings JSON: {e}"))?;
        let strings = value
            .get("strings")
            .and_then(Value::as_object)
            .ok_or("Invalid localized string table")?;
        require(
            strings.len() < 5000
                && strings
                    .values()
                    .all(|value| value.as_str().is_some_and(|text| text.len() <= 65536)),
            "Invalid localized string table",
        )?;
        let table = json!({"locale": locale, "strings": strings, "source": path});
        write_json(
            &output.join("languages").join(format!("{locale}.json")),
            &table,
        )?;
        if locale == default {
            write_json(&output.join("strings.json"), &table)?;
        }
        languages.push(locale.to_owned());
        require(languages.len() <= 64, "Too many language tables")?;
    }
    require(
        languages.iter().any(|locale| locale == default),
        "Missing default language table",
    )?;
    write_json(
        &output.join("languages.json"),
        &json!({"schema": 1, "default": default, "locales": languages}),
    )
}

fn u16le(bytes: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([bytes[p], bytes[p + 1]])
}

fn u32le(bytes: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(bytes[p..p + 4].try_into().expect("validated font u32"))
}

fn i16le(bytes: &[u8], p: usize) -> i16 {
    i16::from_le_bytes([bytes[p], bytes[p + 1]])
}

fn font(bytes: &[u8], output: &Path, name: &str) -> SkinResult<()> {
    require(
        bytes.len() >= 9 && bytes[0] == 1,
        "Unsupported Reed font container",
    )?;
    let metrics = u32le(bytes, 1) as usize;
    let image = u32le(bytes, 5) as usize;
    require(
        9usize
            .checked_add(metrics)
            .and_then(|v| v.checked_add(image))
            == Some(bytes.len())
            && metrics >= 4
            && image >= 8,
        "Invalid Reed font section lengths",
    )?;
    let end = 9 + metrics;
    require(
        bytes.get(9..13) == Some(b"BMF\x03"),
        "Unsupported BMFont version",
    )?;
    require(
        bytes.get(end..end + 8) == Some(&[137, 80, 78, 71, 13, 10, 26, 10]),
        "Invalid font image",
    )?;
    let mut result = json!({"glyphs": {}, "kernings": {}});
    let mut seen = BTreeSet::new();
    let mut p = 13usize;
    while p < end {
        require(end - p >= 5, "Truncated BMFont block")?;
        let kind = bytes[p];
        let size = u32le(bytes, p + 1) as usize;
        p += 5;
        require(
            size <= end - p && seen.insert(kind),
            "Invalid/duplicate BMFont block",
        )?;
        if kind == 1 {
            require(size >= 2, "Invalid font info")?;
            result["fontSize"] = json!(i32::from(i16le(bytes, p)).unsigned_abs());
        } else if kind == 2 {
            require(
                size == 15 && u16le(bytes, p + 8) == 1,
                "Unsupported font page count",
            )?;
            result["lineHeight"] = json!(u16le(bytes, p));
            result["baseline"] = json!(u16le(bytes, p + 2));
            result["width"] = json!(u16le(bytes, p + 4));
            result["height"] = json!(u16le(bytes, p + 6));
        } else if kind == 4 {
            require(size.is_multiple_of(20), "Invalid font glyph block")?;
            let glyphs = result["glyphs"]
                .as_object_mut()
                .expect("initialized glyph map");
            for j in (p..p + size).step_by(20) {
                require(bytes[j + 18] == 0, "Unsupported font glyph page")?;
                let code = u32le(bytes, j).to_string();
                require(!glyphs.contains_key(&code), "Duplicate font glyph")?;
                glyphs.insert(
                    code.clone(),
                    json!({
                        "x": u16le(bytes, j + 4), "y": u16le(bytes, j + 6),
                        "width": u16le(bytes, j + 8), "height": u16le(bytes, j + 10),
                        "xOffset": i16le(bytes, j + 12), "yOffset": i16le(bytes, j + 14),
                        "advance": i16le(bytes, j + 16),
                    }),
                );
                // Older NES Reed fonts encode whitespace without a drawable
                // atlas cell (x=0xffff, with undefined y). Keep its advance,
                // but never pass those sentinel rectangles to the renderer.
                if u16le(bytes, j + 4) == u16::MAX
                    && char::from_u32(u32le(bytes, j)).is_some_and(char::is_whitespace)
                {
                    let glyph = glyphs.get_mut(&code).expect("inserted glyph");
                    for key in ["x", "y", "width", "height"] {
                        glyph[key] = json!(0);
                    }
                }
            }
        } else if kind == 5 {
            require(size.is_multiple_of(10), "Invalid font kerning block")?;
            let kernings = result["kernings"]
                .as_object_mut()
                .expect("initialized kerning map");
            for j in (p..p + size).step_by(10) {
                kernings.insert(
                    format!("{}:{}", u32le(bytes, j), u32le(bytes, j + 4)),
                    json!(i16le(bytes, j + 8)),
                );
            }
        }
        p += size;
    }
    require(
        result["fontSize"].as_u64().unwrap_or(0) > 0
            && result["lineHeight"].as_u64().unwrap_or(0) > 0
            && !result["glyphs"].as_object().expect("glyph map").is_empty(),
        "Missing font metrics",
    )?;
    let width = result["width"].as_u64().unwrap_or(0);
    let height = result["height"].as_u64().unwrap_or(0);
    for glyph in result["glyphs"].as_object().expect("glyph map").values() {
        require(
            glyph["x"].as_u64().unwrap() + glyph["width"].as_u64().unwrap() <= width
                && glyph["y"].as_u64().unwrap() + glyph["height"].as_u64().unwrap() <= height,
            "Glyph outside atlas",
        )?;
    }
    write_json(&output.join(format!("font-{name}.json")), &result)?;
    write(&output.join(format!("font-{name}.png")), &bytes[end..])
}

type Matrix = [f64; 6];

fn array_f64(value: &Value) -> SkinResult<Matrix> {
    let values = value.as_array().ok_or("Invalid Clover scene transform")?;
    require(values.len() == 6, "Invalid Clover scene transform")?;
    let mut result = [0.0; 6];
    for (i, value) in values.iter().enumerate() {
        result[i] = value
            .as_f64()
            .ok_or("Invalid Clover scene transform value")?;
    }
    Ok(result)
}

fn visit_scene(
    node: &Value,
    path: String,
    parent: Matrix,
    depth: usize,
    nodes: &mut BTreeMap<String, Value>,
    world: &mut BTreeMap<String, Matrix>,
) -> SkinResult<()> {
    require(
        depth < 100 && nodes.len() < 30_000,
        "Scene exceeds import limits",
    )?;
    let name = node
        .get("name")
        .and_then(Value::as_str)
        .ok_or("Invalid scene node name")?;
    let path = format!("{path}/{name}");
    nodes.insert(path.clone(), node.clone());
    let t = array_f64(node.get("transform").ok_or("Scene node has no transform")?)?;
    let [a, b, c, d, e, f] = parent;
    let [aa, bb, cc, dd, ee, ff] = t;
    let matrix = [
        a * aa + b * dd,
        a * bb + b * ee,
        a * cc + b * ff + c,
        d * aa + e * dd,
        d * bb + e * ee,
        d * cc + e * ff + f,
    ];
    world.insert(path.clone(), matrix);
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            visit_scene(child, path.clone(), matrix, depth + 1, nodes, world)?;
        }
    }
    Ok(())
}

fn component<'a>(node: &'a Value, path: &str, kind: &str) -> SkinResult<&'a Value> {
    node.get("components")
        .and_then(Value::as_array)
        .and_then(|components| {
            components
                .iter()
                .find(|component| component.get("type").and_then(Value::as_str) == Some(kind))
        })
        .ok_or_else(|| format!("Missing scene component: {path}"))
}

/// Generate the same profile-local assets and static render specifications as
/// the existing importer. All source resources are copied, not modified.
pub fn prepare_skin(files: &Files, ui: &str, output: &Path) -> SkinResult<()> {
    require(
        ["snes-eur", "snes-usa", "shvc", "nes", "hvc"].contains(&ui),
        "Unknown regional UI identity",
    )?;
    let nes = matches!(ui, "nes" | "hvc");
    let japanese = matches!(ui, "shvc" | "hvc");
    let base = if nes {
        "usr/share/clover-ui/".to_owned()
    } else {
        format!("usr/share/ui/{ui}/")
    };
    let root = format!("{base}resources/");
    for (source, target) in [
        (
            if nes {
                if japanese {
                    "sprites/hvc.png"
                } else {
                    "sprites/nes.png"
                }
            } else {
                "sprites/packed.png"
            },
            "packed.png",
        ),
        (
            if nes {
                if japanese {
                    "sprites/hvc.json"
                } else {
                    "sprites/nes.json"
                }
            } else {
                "sprites/packed.json"
            },
            "packed.json",
        ),
        (
            "animation/cursor_thumbnails.spriteanim",
            "cursor-thumbnails.json",
        ),
        ("sounds/hvc/bgm_home.wav", "bgm_home.wav"),
        ("sounds/hvc/se_sys_cursor.wav", "se_sys_cursor.wav"),
        ("sounds/hvc/se_sys_click.wav", "se_sys_click.wav"),
    ] {
        let bytes = read(files, &root, source)?;
        if nes && target == "packed.json" {
            let mut atlas: Value = serde_json::from_slice(bytes)
                .map_err(|e| format!("Invalid NES sprite atlas: {e}"))?;
            let sprites = atlas
                .get_mut("sprites")
                .and_then(Value::as_object_mut)
                .ok_or("Missing NES sprite table")?;
            for (alias, original) in [
                ("screen_analog_preview.png", "capture_analog.png"),
                (
                    "screen_pixel_perfect_preview.png",
                    "capture_pixelPerfect.png",
                ),
                ("switch_disable.png", "switch_off.png"),
            ] {
                if let Some(sprite) = sprites.get(original).cloned() {
                    sprites.insert(alias.into(), sprite);
                }
            }
            write_json(&output.join(target), &atlas)?;
        } else {
            write(&output.join(target), bytes)?;
        }
    }
    if nes {
        write(
            &output.join("wall.png"),
            read(files, &root, "sprites/wallpaper-dots.png")?,
        )?;
    } else {
        write(
            &output.join("wall.png"),
            read(files, &root, "layout/asset/sprite/wallpaper/wall.png")?,
        )?;
    }
    for name in ["title", "m", "s", "l", "copyright"] {
        let font_path = format!("fonts/{name}.font");
        let fallback = format!("fonts/{name}.fnt");
        let nes_path = if matches!(name, "title" | "copyright") {
            format!("usr/share/games/nes/kachikachi/{name}.fnt")
        } else {
            format!("{root}fonts/{ui}/{name}.fnt")
        };
        let bytes = files
            .get(&nes_path)
            .filter(|_| nes)
            .or_else(|| {
                files
                    .get(&format!("{root}{font_path}"))
                    .or_else(|| files.get(&format!("{root}{fallback}")))
            })
            .ok_or_else(|| format!("Image missing skin member: {font_path}"))?;
        font(bytes, output, name)?;
    }

    let locale = if japanese { "jpn_ja" } else { "usa_en" };
    import_languages(files, &root, locale, output)?;

    let palette = if ui == "shvc" {
        "jpn"
    } else if ui == "snes-usa" {
        "usa"
    } else {
        "eur"
    };
    let palette_path = if nes {
        format!("scripts/colors_{ui}.lua")
    } else {
        format!("scripts/env/colors_{palette}.lua")
    };
    let palette_hash = digest(read(files, &root, &palette_path)?);
    let expected_palette = match ui {
        "nes" => "679e8f16ccd77ad74a366da70bfde8ed212303869c9005fc12cd84b9fe185d8b",
        "hvc" => "70d94dd40873be9e92476958080545338e7196ab2710cec082026b62e40eef0c",
        "shvc" => "8a53966a583c45c115fe4d55aff76a56bb63b032512d1570f3879ba09c24a914",
        "snes-usa" => "1181691260ecb050374b10676c4d4c657dafbd143e886385853607949dd4951d",
        _ => "cd85ce84d70c31f83e2a17739e3869eae7790e973851fd4e8c03977ff609073a",
    };
    require(
        palette_hash == expected_palette,
        "Unverified regional palette revision",
    )?;
    let mut colors = json!({
        "title": if matches!(ui, "shvc" | "nes") { json!([255,255,255,255]) } else { json!([0,0,0,255]) },
        "hud": if ui == "shvc" { json!([0,0,0,255]) } else { json!([255,255,255,255]) }
    });
    if ui == "nes" {
        // Statically decoded from the hash-verified LuaJIT palette above.
        // Values are normalized RGBA doubles in the carrier, rounded to 8-bit.
        colors["background"] = json!([0, 21, 61, 255]);
        colors["hud"] = json!([219, 43, 0, 255]);
        colors["caption"] = json!([0, 0, 0, 255]);
        colors["resume_hud"] = json!([219, 43, 0, 255]);
        colors["resume_title"] = json!([219, 43, 0, 255]);
        colors["resume_time"] = json!([0, 0, 0, 255]);
        colors["resume_empty"] = json!([0, 0, 0, 255]);
    } else if ui == "hvc" {
        colors["background"] = json!([235, 226, 213, 255]);
        colors["hud"] = json!([167, 0, 0, 255]);
        colors["caption"] = json!([0, 0, 0, 255]);
        colors["resume_hud"] = json!([0, 0, 0, 255]);
        colors["resume_title"] = json!([255, 255, 255, 255]);
        colors["resume_time"] = json!([255, 255, 255, 255]);
        colors["resume_empty"] = json!([0, 0, 0, 255]);
    }
    write_json(
        &output.join("theme.json"),
        &json!({
            "ui": ui, "locale": locale, "system": if nes { "nes" } else { "snes" }, "region": if japanese { "Japan" } else if matches!(ui, "snes-usa" | "nes") { "USA" } else { "Europe" },
            "colors": colors,
            "paletteSource": format!("{root}{palette_path}"), "paletteSHA256": palette_hash
        }),
    )?;

    let scene: Value = serde_json::from_slice(read(files, &root, "defaultscene.scn")?)
        .map_err(|e| format!("Invalid Clover scene JSON: {e}"))?;
    let mut nodes = BTreeMap::new();
    let mut world = BTreeMap::new();
    visit_scene(
        scene
            .get("rootWorldNode")
            .ok_or("Clover scene has no rootWorldNode")?,
        String::new(),
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        0,
        &mut nodes,
        &mut world,
    )?;
    let position = |path: &str| -> SkinResult<Value> {
        let m = world
            .get(path)
            .ok_or_else(|| format!("Missing scene node: {path}"))?;
        Ok(json!([640.0 + m[2], 360.0 - m[5]]))
    };

    let home = "/Main/homemenu";
    let hud = format!("{home}/hud/hud_gametitle");
    let caption = format!("{home}/elements/menubar_upper/elements/sys_menubar_cm5/caption_down");
    let paths = [
        ("title", format!("{home}/gametitle")),
        ("thumbnails", format!("{home}/thumbnails")),
        ("hud", format!("{home}/hud")),
        ("bottomBar", format!("{home}/menubar")),
        ("topBar", format!("{home}/elements/menubar_upper/menubar")),
        (
            "topButtons",
            format!("{home}/elements/menubar_upper/elements"),
        ),
    ];
    let mut positions = serde_json::Map::new();
    for (key, path) in paths {
        positions.insert(key.to_owned(), position(&path)?);
    }

    let dependencies: Value = serde_json::from_slice(read(files, &base, "dependencies.json")?)
        .map_err(|e| format!("Invalid Clover dependencies JSON: {e}"))?;
    let resources = dependencies
        .get("resources")
        .and_then(Value::as_array)
        .ok_or("Missing Clover font resources")?;
    let mut fonts = BTreeMap::new();
    for resource in resources {
        if resource.get("type").and_then(Value::as_str) == Some("FontResource") {
            let id = resource
                .get("id")
                .and_then(Value::as_str)
                .ok_or("Invalid Clover font resource ID")?;
            let path = resource
                .get("path")
                .and_then(Value::as_str)
                .ok_or("Invalid Clover font resource path")?;
            let name = Path::new(path)
                .file_stem()
                .and_then(|v| v.to_str())
                .ok_or("Invalid Clover font resource name")?;
            fonts.insert(id.to_owned(), name.to_owned());
        }
    }
    let labels_to_read = [
        ("title", format!("{home}/gametitle/gametitle_label")),
        ("hud", format!("{hud}/hud_item1_menuU/hud_2_label")),
        ("tooltip", format!("{caption}/Label")),
    ];
    let mut labels = serde_json::Map::new();
    for (key, path) in labels_to_read {
        let node = nodes
            .get(&path)
            .ok_or_else(|| format!("Missing scene node: {path}"))?;
        let component = component(node, &path, "LabelComponent")?;
        let font_id = component
            .get("fontResource")
            .and_then(Value::as_str)
            .ok_or("Invalid scene font resource")?;
        let mut font_name = fonts
            .get(font_id)
            .ok_or("Missing scene font dependency")?
            .clone();
        if let Some(name) = component
            .pointer("/properties/fontName")
            .and_then(Value::as_str)
        {
            font_name = name.to_owned();
        }
        labels.insert(key.to_owned(), json!({
            "position": position(&path)?, "font": font_name,
            "horizontalAnchor": component.get("horizontalAnchor").ok_or("Missing horizontal text anchor")?,
            "verticalAnchor": component.get("verticalAnchor").ok_or("Missing vertical text anchor")?,
        }));
    }
    let cap_path = format!("{caption}/caption");
    let edge_path = format!("{caption}/caption_r");
    let cap = component(
        nodes
            .get(&cap_path)
            .ok_or("Missing tooltip caption scene node")?,
        &cap_path,
        "SpriteComponent",
    )?;
    let edge = component(
        nodes
            .get(&edge_path)
            .ok_or("Missing tooltip edge scene node")?,
        &edge_path,
        "SpriteComponent",
    )?;
    let list_path = format!("{home}/elements/cardlist");
    let list = nodes
        .get(&list_path)
        .ok_or("Missing card list scene node")?;
    let marker = |key: &str| -> SkinResult<f64> {
        let id = list
            .pointer(&format!("/properties/{key}/linkId"))
            .ok_or("Missing card scene marker link")?;
        for node in nodes.values() {
            if node.get("id") == Some(id) {
                return node
                    .pointer("/transform/2")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| "Invalid card marker transform".to_owned());
            }
        }
        Err("Missing card scene marker".to_owned())
    };
    let card: Value = serde_json::from_slice(read(files, &root, "prefab/sys_game_card.scn")?)
        .map_err(|e| format!("Invalid game card scene JSON: {e}"))?;
    let card_root = card
        .get("rootWorldNode")
        .and_then(Value::as_object)
        .ok_or("Missing game card root")?;
    let mut card_size = None;
    if let Some(children) = card_root.get("children").and_then(Value::as_array) {
        for inactive in children
            .iter()
            .filter(|v| v.get("name").and_then(Value::as_str) == Some("non_active"))
        {
            if let Some(cards) = inactive.get("children").and_then(Value::as_array) {
                for child in cards
                    .iter()
                    .filter(|v| v.get("name").and_then(Value::as_str) == Some("card"))
                {
                    card_size = child.pointer("/components/0/size").cloned();
                }
            }
        }
    }
    let card_size = card_size.ok_or("Missing card size")?;
    let layout = json!({
        "width": 1280, "height": 720, "positions": positions, "labels": labels,
        "hudItemGap": nodes.get(&format!("{hud}/hud_item1_menuU")).and_then(|v| v.pointer("/properties/VGap")).ok_or("Missing HUD item gap")?,
        "hudGroupGap": nodes.get(&hud).and_then(|v| v.pointer("/properties/VGap")).ok_or("Missing HUD group gap")?,
        "tooltip": {
            "bodyY": position(&cap_path)?[1],
            "minimumWidth": cap.pointer("/size/0").ok_or("Missing tooltip width")?,
            "height": cap.pointer("/size/1").ok_or("Missing tooltip height")?,
            "edgeWidth": edge.pointer("/size/0").ok_or("Missing tooltip edge width")?
        },
        "cardSize": card_size,
        "cardSpacing": marker("narabe2")? - marker("narabe1")?,
        "wallpaperDuration": if nes { json!(20.0) } else { serde_json::from_slice::<Value>(read(files, &root, "layout/lyt_wallpaper_default.sceneanimation")?)
            .map_err(|e| format!("Invalid wallpaper animation JSON: {e}"))?.get("duration").ok_or("Missing wallpaper animation duration")?.clone() }
    });
    require(
        layout["cardSpacing"].as_f64().unwrap_or(0.0) > 0.0
            && layout["wallpaperDuration"].as_f64().unwrap_or(0.0) > 0.0,
        "Invalid scene timing/spacing",
    )?;
    write_json(&output.join("layout.json"), &layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_whitespace_has_advance_but_no_out_of_atlas_rectangle() {
        let mut bm = b"BMF\x03".to_vec();
        fn block(bm: &mut Vec<u8>, kind: u8, data: &[u8]) {
            bm.push(kind);
            bm.extend_from_slice(&(data.len() as u32).to_le_bytes());
            bm.extend_from_slice(data);
        }
        block(&mut bm, 1, &[12, 0]);
        let mut common = vec![0; 15];
        for at in [0, 2, 4, 6, 8] {
            common[at] = 1;
        }
        block(&mut bm, 2, &common);
        let mut glyph = vec![0; 20];
        glyph[..4].copy_from_slice(&32_u32.to_le_bytes());
        glyph[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        glyph[6..8].copy_from_slice(&62839_u16.to_le_bytes());
        glyph[8] = 2;
        glyph[10] = 2;
        glyph[16] = 6;
        block(&mut bm, 4, &glyph);
        let mut image = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut image, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255; 4])
                .unwrap();
        }
        let mut bytes = vec![1];
        bytes.extend_from_slice(&(bm.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(image.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&bm);
        bytes.extend_from_slice(&image);
        let output = std::env::temp_dir().join(format!("clover-whitespace-{}", std::process::id()));
        font(&bytes, &output, "s").unwrap();
        let result: Value =
            serde_json::from_slice(&fs::read(output.join("font-s.json")).unwrap()).unwrap();
        assert_eq!(result["glyphs"]["32"]["width"], 0);
        assert_eq!(result["glyphs"]["32"]["advance"], 6);
        // A drawable letter may not hide invalid atlas coordinates as whitespace.
        let glyph_at = 9 + 4 + 7 + 20 + 5;
        bytes[glyph_at..glyph_at + 4].copy_from_slice(&65_u32.to_le_bytes());
        assert!(font(&bytes, &output, "s").is_err());
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn imports_every_language_and_retains_default_table() {
        let output = std::env::temp_dir().join(format!("clover-languages-{}", std::process::id()));
        let mut files = Files::new();
        for (locale, text) in [("usa_en", "Menu"), ("eur_de", "Menü"), ("eur_ru", "Меню")] {
            files.insert(
                format!("root/strings/{locale}/strings.lng"),
                serde_json::to_vec(&json!({"strings": {"menu": text}})).unwrap(),
            );
        }
        import_languages(&files, "root/", "usa_en", &output).unwrap();
        let index: Value =
            serde_json::from_slice(&fs::read(output.join("languages.json")).unwrap()).unwrap();
        assert_eq!(index["locales"].as_array().unwrap().len(), 3);
        let german: Value =
            serde_json::from_slice(&fs::read(output.join("languages/eur_de.json")).unwrap())
                .unwrap();
        assert_eq!(german["strings"]["menu"], "Menü");
        let default: Value =
            serde_json::from_slice(&fs::read(output.join("strings.json")).unwrap()).unwrap();
        assert_eq!(default["locale"], "usa_en");
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn rejects_unsafe_language_ids_and_non_string_values() {
        assert!(!valid_locale("../../escape"));
        assert!(!valid_locale("eur_de/extra"));
        let mut files = Files::new();
        files.insert(
            "root/strings/usa_en/strings.lng".into(),
            br#"{"strings":{"bad":42}}"#.to_vec(),
        );
        assert!(
            import_languages(
                &files,
                "root/",
                "usa_en",
                Path::new("unused-language-output")
            )
            .is_err()
        );
    }
}
