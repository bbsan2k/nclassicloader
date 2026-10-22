//! Host-neutral decoding for packed sprite frames and theme metadata.

use crate::frontend::font::Rect;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub nes: bool,
    pub palette: Value,
    pub region: String,
}

pub fn decode_sprites(packed: &Value) -> Result<HashMap<String, Rect>, String> {
    let sprites = packed
        .get("sprites")
        .and_then(Value::as_object)
        .ok_or("Clover packed atlas has no sprite table")?;
    Ok(sprites
        .iter()
        .filter_map(|(key, item)| {
            let frame = item.get("textureFrame")?.as_array()?;
            if frame.len() != 4 {
                return None;
            }
            Some((
                key.clone(),
                Rect {
                    x: frame[0].as_f64()? as f32,
                    y: frame[1].as_f64()? as f32,
                    w: frame[2].as_f64()? as f32,
                    h: frame[3].as_f64()? as f32,
                },
            ))
        })
        .collect())
}

pub fn decode_theme(theme: &Value) -> Theme {
    Theme {
        nes: theme.get("system").and_then(Value::as_str) == Some("nes"),
        palette: theme
            .get("colors")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        region: theme
            .get("region")
            .and_then(Value::as_str)
            .unwrap_or("Europe")
            .to_owned(),
    }
}

/// Initial title/HUD colors use whole-array validation and wrapping u8 casts.
pub fn initial_color(theme: &Value, key: &str, fallback: [u8; 4]) -> [u8; 4] {
    theme
        .pointer(&format!("/colors/{key}"))
        .and_then(Value::as_array)
        .filter(|values| values.len() == 4)
        .map(|values| {
            [0, 1, 2, 3].map(|index| values[index].as_u64().unwrap_or(fallback[index] as u64) as u8)
        })
        .unwrap_or(fallback)
}

/// Runtime palette colors validate array shape, then fall back per channel.
pub fn palette_color(palette: &Value, key: &str, fallback: [u8; 4]) -> [u8; 4] {
    let Some(channels) = palette
        .get(key)
        .and_then(Value::as_array)
        .filter(|value| value.len() == 4)
    else {
        return fallback;
    };
    [0, 1, 2, 3].map(|index| {
        channels[index]
            .as_u64()
            .filter(|value| *value <= 255)
            .map_or(fallback[index], |value| value as u8)
    })
}

#[cfg(test)]
mod tests {
    use super::{decode_sprites, decode_theme, initial_color, palette_color};
    use crate::frontend::font::Rect;
    use serde_json::json;

    #[test]
    fn sprite_decoder_requires_object_and_skips_bad_frames_without_geometry_rejection() {
        for packed in [json!({}), json!({"sprites": []}), json!({"sprites": null})] {
            assert_eq!(
                decode_sprites(&packed).unwrap_err(),
                "Clover packed atlas has no sprite table"
            );
        }
        let sprites = decode_sprites(&json!({"sprites": {
            "missing": {}, "short": {"textureFrame": [1,2,3]},
            "long": {"textureFrame": [1,2,3,4,5]},
            "text": {"textureFrame": [1,2,"3",4]},
            "packed/with.png": {"textureFrame": [-1.5, 2.25, 9000, -0.75]},
            "ordinary": {"textureFrame": [0, 1, 2, 3]}
        }}))
        .unwrap();
        assert_eq!(sprites.len(), 2);
        assert_eq!(
            sprites["packed/with.png"],
            Rect {
                x: -1.5,
                y: 2.25,
                w: 9000.0,
                h: -0.75
            }
        );
        assert_eq!(
            sprites["ordinary"],
            Rect {
                x: 0.0,
                y: 1.0,
                w: 2.0,
                h: 3.0
            }
        );
    }

    #[test]
    fn initial_and_palette_color_policies_remain_distinct() {
        let fallback = [10, 20, 30, 40];
        let theme = json!({"colors": {
            "wrap": [256, 257, -1, 2.5], "wrong_len": [1, 2]
        }});
        assert_eq!(initial_color(&theme, "wrap", fallback), [0, 1, 30, 40]);
        assert_eq!(initial_color(&theme, "wrong_len", fallback), fallback);
        assert_eq!(initial_color(&json!({}), "missing", fallback), fallback);
        let palette = json!({"channels": [256, 257, -1, 2.5], "wrong_len": [1,2]});
        assert_eq!(
            palette_color(&palette, "channels", fallback),
            [10, 20, 30, 40]
        );
        assert_eq!(palette_color(&palette, "wrong_len", fallback), fallback);
        assert_eq!(palette_color(&json!([]), "channels", fallback), fallback);
    }

    #[test]
    fn theme_metadata_keeps_system_and_region_defaults() {
        let nes = decode_theme(&json!({"system":"nes", "region":"Japan", "colors":[1,2]}));
        assert!(nes.nes);
        assert_eq!(nes.region, "Japan");
        assert_eq!(nes.palette, json!([1, 2]));
        let other = decode_theme(&json!({"system":"NES", "region":false}));
        assert!(!other.nes);
        assert_eq!(other.region, "Europe");
        assert_eq!(other.palette, json!({}));
    }
}
