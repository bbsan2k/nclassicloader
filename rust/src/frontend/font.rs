//! Host-neutral bitmap-font metadata and permissive skin JSON decoding.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub source: Rect,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub size: f32,
    pub line: f32,
    pub baseline: f32,
    pub glyphs: BTreeMap<char, Glyph>,
    pub kernings: BTreeMap<(char, char), f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedGlyph {
    pub font: String,
    pub source: Rect,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub advance: f32,
    pub line: f32,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextPlacement {
    pub left: f32,
    pub top: f32,
    pub scale: f32,
}

impl TextPlacement {
    /// Returns this glyph's destination rectangle without changing its atlas source.
    pub fn glyph_rect(self, glyph: &PlacedGlyph) -> Rect {
        Rect {
            x: self.left + glyph.x * self.scale,
            y: self.top + glyph.y * self.scale,
            w: glyph.width * self.scale,
            h: glyph.height * self.scale,
        }
    }
}

/// Apply the App's existing bitmap-text centering and maximum-width scaling.
pub fn placement(
    layout: &TextLayout,
    x: f32,
    y: f32,
    max_width: f32,
    line_anchor: bool,
) -> TextPlacement {
    let extent = if line_anchor {
        layout.advance.max(layout.right)
    } else {
        layout.right - layout.left
    };
    let scale = (max_width / extent.max(1.0)).min(1.0);
    let left = x
        - (if line_anchor {
            layout.advance * 0.5
        } else {
            (layout.left + layout.right) * 0.5
        }) * scale;
    let top = y
        - (if line_anchor {
            layout.line * 0.5
        } else {
            (layout.top + layout.bottom) * 0.5
        }) * scale;
    TextPlacement { left, top, scale }
}

/// Measure text with the skin font fallback and bitmap metric policy.
pub fn measure(fonts: &BTreeMap<String, Font>, value: &str, primary_name: &str) -> TextLayout {
    let Some(primary) = fonts.get(primary_name) else {
        return TextLayout {
            glyphs: Vec::new(),
            advance: 0.0,
            line: 0.0,
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
    };
    let mut layout = TextLayout {
        glyphs: Vec::new(),
        advance: 0.0,
        line: primary.line,
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    let mut first = true;
    let mut previous: Option<(String, char)> = None;
    for character in value.chars().take(4096) {
        let chosen = if primary.glyphs.contains_key(&character) {
            Some(primary_name.to_owned())
        } else {
            fonts
                .iter()
                .find_map(|(name, font)| font.glyphs.contains_key(&character).then(|| name.clone()))
        };
        let mut character = character;
        let name = chosen.or_else(|| {
            character = '?';
            if primary.glyphs.contains_key(&character) {
                Some(primary_name.to_owned())
            } else {
                fonts.iter().find_map(|(name, font)| {
                    font.glyphs.contains_key(&character).then(|| name.clone())
                })
            }
        });
        let Some(name) = name else { continue };
        let Some(font) = fonts.get(&name) else {
            continue;
        };
        let Some(glyph) = font.glyphs.get(&character).copied() else {
            continue;
        };
        let scale = primary.size / font.size.max(1.0);
        if let Some((_, previous_char)) = previous
            .as_ref()
            .filter(|(previous_name, _)| previous_name == &name)
        {
            layout.advance += font
                .kernings
                .get(&(*previous_char, character))
                .copied()
                .unwrap_or(0.0)
                * scale;
        }
        let x = layout.advance + glyph.x * scale;
        let y = primary.baseline - font.baseline * scale + glyph.y * scale;
        let width = glyph.source.w * scale;
        let height = glyph.source.h * scale;
        layout.glyphs.push(PlacedGlyph {
            font: name.clone(),
            source: glyph.source,
            x,
            y,
            width,
            height,
        });
        if character != ' ' && width > 0.0 && height > 0.0 {
            if first {
                layout.left = x;
                layout.top = y;
                layout.right = x + width;
                layout.bottom = y + height;
                first = false;
            } else {
                layout.left = layout.left.min(x);
                layout.top = layout.top.min(y);
                layout.right = layout.right.max(x + width);
                layout.bottom = layout.bottom.max(y + height);
            }
        }
        layout.advance += glyph.advance * scale;
        previous = Some((name, character));
    }
    layout
}

/// Decode font metrics using the skin loader's existing permissive policy.
pub fn decode(data: &Value) -> Font {
    let mut glyphs = BTreeMap::new();
    if let Some(items) = data.get("glyphs").and_then(Value::as_object) {
        for (code, item) in items {
            let Ok(code) = code.parse::<u32>() else {
                continue;
            };
            let Some(character) = char::from_u32(code) else {
                continue;
            };
            let n = |key: &str| item.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            glyphs.insert(
                character,
                Glyph {
                    source: Rect {
                        x: n("x"),
                        y: n("y"),
                        w: n("width"),
                        h: n("height"),
                    },
                    x: n("xOffset"),
                    y: n("yOffset"),
                    advance: n("advance"),
                },
            );
        }
    }
    let mut kernings = BTreeMap::new();
    if let Some(items) = data.get("kernings").and_then(Value::as_object) {
        for (key, value) in items {
            let Some((left, right)) = key.split_once(':') else {
                continue;
            };
            let (Ok(left), Ok(right)) = (left.parse::<u32>(), right.parse::<u32>()) else {
                continue;
            };
            if let (Some(left), Some(right), Some(amount)) =
                (char::from_u32(left), char::from_u32(right), value.as_f64())
            {
                kernings.insert((left, right), amount as f32);
            }
        }
    }
    Font {
        size: data.get("fontSize").and_then(Value::as_f64).unwrap_or(16.0) as f32,
        line: data
            .get("lineHeight")
            .and_then(Value::as_f64)
            .unwrap_or(20.0) as f32,
        baseline: data.get("baseline").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        glyphs,
        kernings,
    }
}

#[cfg(test)]
mod tests {
    use super::{Font, Glyph, PlacedGlyph, Rect, TextLayout, decode, measure, placement};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn decodes_exact_metrics_unicode_and_signed_kerning() {
        let font = decode(&json!({
            "fontSize": 31.5, "lineHeight": 42, "baseline": -3.25,
            "glyphs": {"65": {"x": 1.25, "y": -2, "width": 8.5, "height": 9,
                "xOffset": -1.5, "yOffset": 2.25, "advance": 10.75},
                "9731": {"x": 0, "y": 0, "width": 0, "height": 0, "advance": 4}},
            "kernings": {"65:86": -1.75}
        }));
        assert_eq!(font.size, 31.5);
        assert_eq!(font.line, 42.0);
        assert_eq!(font.baseline, -3.25);
        assert_eq!(
            font.glyphs[&'A'],
            Glyph {
                source: Rect {
                    x: 1.25,
                    y: -2.0,
                    w: 8.5,
                    h: 9.0
                },
                x: -1.5,
                y: 2.25,
                advance: 10.75,
            }
        );
        assert_eq!(
            font.glyphs[&'☃'],
            Glyph {
                source: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0
                },
                x: 0.0,
                y: 0.0,
                advance: 4.0,
            }
        );
        assert_eq!(font.kernings.get(&('A', 'V')), Some(&-1.75));
    }

    #[test]
    fn defaults_and_empty_or_malformed_tables_remain_permissive() {
        let empty = Font {
            size: 16.0,
            line: 20.0,
            baseline: 0.0,
            glyphs: BTreeMap::new(),
            kernings: BTreeMap::new(),
        };
        for value in [
            json!({}),
            json!({"glyphs": null, "kernings": []}),
            json!([]),
        ] {
            assert_eq!(decode(&value), empty);
        }
        let font = decode(&json!({"glyphs": {
            "-1": {}, "4294967296": {}, "55296": {}, "not-a-number": {},
            "66": {"x": "bad", "y": false, "width": null, "height": [],
                "xOffset": "1", "yOffset": {}, "advance": "bad"}
        }, "kernings": {"bad": 1, "65:no": 2, "55296:65": 3, "65:66": "bad"}}));
        assert_eq!(font.glyphs.len(), 1);
        assert_eq!(
            font.glyphs[&'B'],
            Glyph {
                source: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0
                },
                x: 0.0,
                y: 0.0,
                advance: 0.0,
            }
        );
        assert!(font.kernings.is_empty());
    }

    fn glyph(x: f32, y: f32, w: f32, h: f32, advance: f32) -> Glyph {
        Glyph {
            source: Rect {
                x: 2.0,
                y: 3.0,
                w,
                h,
            },
            x,
            y,
            advance,
        }
    }

    fn font(size: f32, line: f32, baseline: f32, glyphs: &[(char, Glyph)]) -> Font {
        Font {
            size,
            line,
            baseline,
            glyphs: glyphs.iter().copied().collect(),
            kernings: BTreeMap::new(),
        }
    }

    fn placed(name: &str, character_glyph: Glyph, x: f32, y: f32, scale: f32) -> PlacedGlyph {
        PlacedGlyph {
            font: name.to_owned(),
            source: character_glyph.source,
            x,
            y,
            width: character_glyph.source.w * scale,
            height: character_glyph.source.h * scale,
        }
    }

    #[test]
    fn measure_snapshot_preserves_kerning_negative_bearing_and_baseline_scaling() {
        let mut primary = font(20.0, 24.0, 10.0, &[('A', glyph(-2.0, 3.0, 5.0, 6.0, 8.0))]);
        primary.kernings.insert(('A', 'A'), -1.5);
        let fonts = BTreeMap::from([("primary".into(), primary)]);
        assert_eq!(
            measure(&fonts, "AA", "primary"),
            TextLayout {
                glyphs: vec![
                    placed("primary", glyph(-2.0, 3.0, 5.0, 6.0, 8.0), -2.0, 3.0, 1.0),
                    placed("primary", glyph(-2.0, 3.0, 5.0, 6.0, 8.0), 4.5, 3.0, 1.0),
                ],
                advance: 14.5,
                line: 24.0,
                left: -2.0,
                top: 3.0,
                right: 9.5,
                bottom: 9.0,
            }
        );
    }

    #[test]
    fn measure_snapshot_prefers_primary_then_uses_lexicographic_fallback_without_cross_kerning() {
        let mut primary = font(20.0, 20.0, 0.0, &[('A', glyph(0.0, 0.0, 2.0, 2.0, 5.0))]);
        primary.kernings.insert(('A', 'X'), -3.0);
        let mut a = font(
            10.0,
            11.0,
            1.0,
            &[
                ('X', glyph(1.0, 2.0, 3.0, 4.0, 6.0)),
                ('Y', glyph(0.0, 0.0, 1.0, 1.0, 2.0)),
            ],
        );
        a.kernings.insert(('X', 'Y'), -1.0);
        let z = font(
            10.0,
            12.0,
            0.0,
            &[
                ('X', glyph(0.0, 0.0, 8.0, 8.0, 9.0)),
                ('?', glyph(0.0, 0.0, 1.0, 1.0, 1.0)),
            ],
        );
        let fonts = BTreeMap::from([
            ("primary".into(), primary),
            ("z".into(), z),
            ("a".into(), a),
        ]);
        let layout = measure(&fonts, "AXY", "primary");
        assert_eq!(
            layout.glyphs,
            vec![
                placed("primary", glyph(0.0, 0.0, 2.0, 2.0, 5.0), 0.0, 0.0, 1.0),
                placed("a", glyph(1.0, 2.0, 3.0, 4.0, 6.0), 7.0, 2.0, 2.0),
                placed("a", glyph(0.0, 0.0, 1.0, 1.0, 2.0), 15.0, -2.0, 2.0),
            ]
        );
        assert_eq!(layout.advance, 19.0);
    }

    #[test]
    fn measure_snapshot_covers_question_fallback_skips_unicode_whitespace_and_zero_area() {
        let primary = font(
            12.0,
            14.0,
            5.0,
            &[
                ('?', glyph(0.0, 0.0, 2.0, 3.0, 4.0)),
                (' ', glyph(0.0, 0.0, 0.0, 0.0, 3.0)),
                ('☃', glyph(-1.0, 1.0, 0.0, 4.0, 2.0)),
            ],
        );
        let fallback = font(12.0, 14.0, 5.0, &[('Ω', glyph(1.0, 2.0, 3.0, 4.0, 6.0))]);
        let fonts = BTreeMap::from([("main".into(), primary), ("other".into(), fallback)]);
        let layout = measure(&fonts, "! Ω☃", "main");
        assert_eq!(
            layout.glyphs,
            vec![
                placed("main", glyph(0.0, 0.0, 2.0, 3.0, 4.0), 0.0, 0.0, 1.0),
                placed("main", glyph(0.0, 0.0, 0.0, 0.0, 3.0), 4.0, 0.0, 1.0),
                placed("other", glyph(1.0, 2.0, 3.0, 4.0, 6.0), 8.0, 2.0, 1.0),
                placed("main", glyph(-1.0, 1.0, 0.0, 4.0, 2.0), 12.0, 1.0, 1.0),
            ]
        );
        assert_eq!(layout.advance, 15.0);
        assert_eq!(
            (layout.left, layout.top, layout.right, layout.bottom),
            (0.0, 0.0, 11.0, 6.0)
        );
        let whitespace = measure(&fonts, " ", "main");
        assert_eq!(whitespace.advance, 3.0);
        assert_eq!(
            (
                whitespace.left,
                whitespace.top,
                whitespace.right,
                whitespace.bottom
            ),
            (0.0, 0.0, 0.0, 0.0)
        );
        let no_question = BTreeMap::from([("main".into(), font(12.0, 14.0, 5.0, &[]))]);
        assert_eq!(measure(&no_question, "!", "main").advance, 0.0);
        assert!(measure(&no_question, "!", "main").glyphs.is_empty());
    }

    #[test]
    fn measure_snapshot_handles_missing_primary_and_limits_input_to_4096_chars() {
        let empty = measure(&BTreeMap::new(), "hello", "missing");
        assert_eq!(
            empty,
            TextLayout {
                glyphs: Vec::new(),
                advance: 0.0,
                line: 0.0,
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            }
        );
        let fonts = BTreeMap::from([(
            "p".into(),
            font(1.0, 7.0, 0.0, &[('A', glyph(0.0, 0.0, 1.0, 1.0, 1.0))]),
        )]);
        let input = "A".repeat(4097);
        let layout = measure(&fonts, &input, "p");
        assert_eq!(layout.glyphs.len(), 4096);
        assert_eq!(layout.advance, 4096.0);
    }

    #[test]
    fn placement_preserves_line_and_ink_anchor_math_and_glyph_source() {
        let layout = TextLayout {
            glyphs: Vec::new(),
            advance: 30.0,
            line: 12.0,
            left: -4.0,
            top: -3.0,
            right: 20.0,
            bottom: 9.0,
        };
        assert_eq!(
            placement(&layout, 50.0, 40.0, 100.0, true),
            super::TextPlacement {
                left: 35.0,
                top: 34.0,
                scale: 1.0,
            }
        );
        assert_eq!(
            placement(&layout, 50.0, 40.0, 12.0, false),
            super::TextPlacement {
                left: 46.0,
                top: 38.5,
                scale: 0.5,
            }
        );

        let glyph = PlacedGlyph {
            font: "fallback".into(),
            source: Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            },
            x: -2.0,
            y: 5.0,
            width: 6.0,
            height: 8.0,
        };
        let place = placement(&layout, 50.0, 40.0, 12.0, false);
        assert_eq!(
            place.glyph_rect(&glyph),
            Rect {
                x: 45.0,
                y: 41.0,
                w: 3.0,
                h: 4.0,
            }
        );
        assert_eq!(
            glyph.source,
            Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0
            }
        );
    }

    #[test]
    fn placement_keeps_empty_extent_no_upscale_and_nonfinite_arithmetic() {
        let empty = TextLayout {
            glyphs: Vec::new(),
            advance: 0.0,
            line: 0.0,
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        assert_eq!(
            placement(&empty, 10.0, 20.0, 100.0, true),
            super::TextPlacement {
                left: 10.0,
                top: 20.0,
                scale: 1.0,
            }
        );
        assert_eq!(placement(&empty, 10.0, 20.0, -4.0, false).scale, -4.0);
        let nonfinite = placement(&empty, f32::NAN, f32::INFINITY, f32::INFINITY, false);
        assert!(nonfinite.left.is_nan());
        assert!(nonfinite.top.is_infinite());
        assert_eq!(nonfinite.scale, 1.0);
    }
}
