//! Licensed, full SVG flag assets embedded from rust/assets/flags.
use super::{Canvas, Color, FRect, Window, fill, panel};
use sdl3::{
    pixels::PixelFormat,
    render::{BlendMode, ScaleMode, Texture, TextureCreator},
    surface::Surface,
    video::WindowContext,
};

pub(super) use crate::frontend::launcher_scene::Region;

pub(super) struct Assets {
    japan: Texture,
    europe: Texture,
    usa: Texture,
}

impl Assets {
    pub(super) fn new(creator: &TextureCreator<WindowContext>) -> Result<Self, String> {
        Ok(Self {
            japan: load_svg(creator, include_bytes!("../../../assets/flags/jp.svg"))?,
            europe: load_svg(creator, include_bytes!("../../../assets/flags/eu.svg"))?,
            usa: load_svg(creator, include_bytes!("../../../assets/flags/us.svg"))?,
        })
    }

    pub(super) fn draw(&self, canvas: &mut Canvas<Window>, region: Region, rect: FRect) {
        fill(
            canvas,
            FRect::new(rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0),
            Color::RGB(105, 124, 140),
        );

        let asset = match region {
            Region::Japan => Some(&self.japan),
            Region::Europe => Some(&self.europe),
            Region::Usa => Some(&self.usa),
            Region::Unknown => None,
        };
        if let Some(asset) = asset {
            let _ = canvas.copy(asset, None, rect);
        } else {
            // Unknown is intentionally neutral UI, not an invented flag.
            panel(canvas, rect, Color::RGB(57, 73, 89), 0.0);
            fill(
                canvas,
                FRect::new(
                    rect.x + rect.w * 0.30,
                    rect.y + rect.h * 0.46,
                    rect.w * 0.40,
                    rect.h * 0.08,
                ),
                Color::RGB(173, 188, 201),
            );
        }
    }
}

fn load_svg(creator: &TextureCreator<WindowContext>, data: &[u8]) -> Result<Texture, String> {
    const WIDTH: u32 = 128;
    const HEIGHT: u32 = 96;
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &options).map_err(|error| error.to_string())?;
    let scale = (WIDTH as f32 / tree.size().width()).min(HEIGHT as f32 / tree.size().height());
    let mut pixmap = resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT)
        .ok_or_else(|| "Could not allocate flag texture".to_owned())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.data().to_vec();
    let surface = Surface::from_data(&mut pixels, WIDTH, HEIGHT, WIDTH * 4, PixelFormat::RGBA32)
        .map_err(|error| error.to_string())?;
    let mut texture = creator
        .create_texture_from_surface(surface)
        .map_err(|error| error.to_string())?;
    texture.set_blend_mode(BlendMode::Blend);
    texture.set_scale_mode(ScaleMode::Linear);
    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_preserve_region_semantics_and_unknown_is_neutral() {
        for (marker, expected, label) in [
            ("JPN", Region::Japan, "Japan"),
            ("Japan", Region::Japan, "Japan"),
            ("EUR", Region::Europe, "Europe"),
            ("Europe", Region::Europe, "Europe"),
            ("USA", Region::Usa, "United States"),
            ("IMG", Region::Unknown, "Unknown"),
            ("ALL", Region::Unknown, "Unknown"),
            ("", Region::Unknown, "Unknown"),
        ] {
            assert_eq!(Region::from_marker(marker), expected);
            assert_eq!(expected.label(), label);
        }
    }

    #[test]
    fn sourced_flags_parse_and_render_expected_colors() {
        for (svg, expected) in [
            (
                include_bytes!("../../../assets/flags/jp.svg").as_slice(),
                [188, 0, 45, 255],
            ),
            (
                include_bytes!("../../../assets/flags/eu.svg").as_slice(),
                [0, 51, 153, 255],
            ),
            (
                include_bytes!("../../../assets/flags/us.svg").as_slice(),
                [25, 47, 93, 255],
            ),
        ] {
            let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
                .expect("MIT-licensed source SVG should parse");
            let width = 128;
            let height = 96;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
            let scale = width as f32 / tree.size().width();
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            let pixel = match Region::from_marker(if expected[0] == 188 {
                "JPN"
            } else if expected[0] == 0 {
                "EUR"
            } else {
                "USA"
            }) {
                Region::Japan => pixmap.pixel(64, 48),
                Region::Europe => pixmap.pixel(2, 2),
                Region::Usa => pixmap.pixel(2, 2),
                Region::Unknown => None,
            }
            .expect("expected flag pixel");
            assert_eq!(
                [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()],
                expected
            );
        }
    }
}
