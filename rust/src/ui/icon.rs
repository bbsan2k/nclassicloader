//! Original source-only Clover icon, shared with the macOS bundle generator.
use sdl3::{pixels::PixelFormat, surface::Surface, video::Window};
use serde_json::Value;

const DESIGN: &str = include_str!("../../assets/icon/design.json");

/// SDL copies the pixels. Some backends (notably macOS) prefer the bundle icon;
/// callers should treat unsupported window icons as nonfatal.
pub(super) fn install(window: &mut Window) -> Result<(), String> {
    let size = 128;
    let mut pixels = rasterize(size)?;
    let surface = Surface::from_data(&mut pixels, size, size, size * 4, PixelFormat::RGBA32)
        .map_err(|e| e.to_string())?;
    if window.set_icon(&surface) {
        Ok(())
    } else {
        Err(sdl3::get_error().to_string())
    }
}

fn distance(kind: &str, g: &[f64], x: f64, y: f64) -> f64 {
    match kind {
        "circle" => g[2] - (x - g[0]).hypot(y - g[1]),
        "rounded" => {
            let dx = (x - g[0] - g[2] / 2.0).abs() - g[2] / 2.0 + g[4];
            let dy = (y - g[1] - g[3] / 2.0).abs() - g[3] / 2.0 + g[4];
            g[4] - dx.max(0.0).hypot(dy.max(0.0)) - dx.max(dy).min(0.0)
        }
        "triangle" => (0..3)
            .map(|edge| {
                let next = (edge + 1) % 3;
                let (ax, ay) = (g[edge * 2], g[edge * 2 + 1]);
                let (bx, by) = (g[next * 2], g[next * 2 + 1]);
                ((bx - ax) * (y - ay) - (by - ay) * (x - ax)) / (bx - ax).hypot(by - ay)
            })
            .fold(f64::INFINITY, f64::min),
        _ => unreachable!("validated icon primitive"),
    }
}

fn rasterize(size: u32) -> Result<Vec<u8>, String> {
    let design: Value = serde_json::from_str(DESIGN).map_err(|e| e.to_string())?;
    let shapes = design["shapes"].as_array().ok_or("Missing icon shapes")?;
    let mut pixels = vec![0; size as usize * size as usize * 4];
    for shape in shapes {
        let kind = shape["kind"].as_str().ok_or("Missing icon primitive")?;
        let expected = match kind {
            "circle" => 3,
            "rounded" => 5,
            "triangle" => 6,
            _ => return Err("Unknown icon primitive".into()),
        };
        let geometry: Vec<f64> = shape["geometry"]
            .as_array()
            .ok_or("Missing icon geometry")?
            .iter()
            .map(|value| value.as_f64().ok_or("Invalid icon geometry"))
            .collect::<Result<_, _>>()?;
        let color = shape["color"]
            .as_array()
            .filter(|color| color.len() == 3)
            .ok_or("Invalid icon color")?;
        if geometry.len() != expected {
            return Err("Invalid icon geometry length".into());
        }
        let color: Vec<f64> = color
            .iter()
            .map(|v| v.as_f64().ok_or("Invalid icon channel"))
            .collect::<Result<_, _>>()?;
        for row in 0..size {
            for column in 0..size {
                let alpha = (distance(
                    kind,
                    &geometry,
                    (column as f64 + 0.5) / size as f64,
                    (row as f64 + 0.5) / size as f64,
                ) * size as f64
                    + 0.5)
                    .clamp(0.0, 1.0);
                if alpha == 0.0 {
                    continue;
                }
                let offset = ((row * size + column) * 4) as usize;
                let previous = pixels[offset + 3] as f64 / 255.0;
                let result = alpha + previous * (1.0 - alpha);
                for channel in 0..3 {
                    pixels[offset + channel] = ((color[channel] * alpha
                        + pixels[offset + channel] as f64 * previous * (1.0 - alpha))
                        / result)
                        .round() as u8;
                }
                pixels[offset + 3] = (result * 255.0).round() as u8;
            }
        }
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_renders_transparent_margin_opaque_leaves_and_play_aperture() {
        let size = 100;
        let pixels = rasterize(size).unwrap();
        let pixel = |x: usize, y: usize| &pixels[(y * size as usize + x) * 4..][..4];
        assert_eq!(pixel(0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(39, 39), [143, 249, 226, 255]);
        assert_eq!(pixel(50, 50), [236, 250, 247, 255]);
        assert_eq!(pixel(85, 50), [20, 34, 48, 255]);
    }
}
