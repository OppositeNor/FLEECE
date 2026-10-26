//! Static PNG/SVG export using document geometry.
//!
//! Exports exclude viewport, theme, grid, selection and unfinished editor gestures.
//! Both formats consume the same scene and crop bounds, and text is emitted as
//! font outlines so the result does not depend on fonts installed on the viewer's
//! machine. Externally constructed diagrams should be validated before export.
//!
//! The library's [`save`] replaces files atomically. Command-line image output
//! uses replacement semantics; the editor file dialog confirms overwrites.

use crate::{
    drawing::*,
    model::{Circuit, atomic_write},
};
use std::{fmt::Write, path::Path};

/// Static diagram export settings, independent of UI preferences and viewport.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExportOptions {
    /// PNG raster scale, an integer from 1 through 6; SVG remains vector geometry.
    pub scale: u32,
    /// Omit the opaque paper background when true.
    pub transparent: bool,
    /// Include the circuit title above the cropped diagram when true.
    pub title: bool,
    /// Padding around the diagram in world units, from 0 through 200.
    pub padding: f32,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            scale: 3,
            transparent: false,
            title: false,
            padding: 28.0,
        }
    }
}
/// Build a static document scene and padded crop bounds.
///
/// Rejects empty diagrams and invalid scale/padding.
///
/// # Errors
///
/// Returns an error for an empty diagram or unsupported scale/padding values.
pub fn export_scene(circuit: &Circuit, options: ExportOptions) -> Result<(Scene, Bounds), String> {
    if circuit.nodes.is_empty() && circuit.fsm.as_ref().is_none_or(|fsm| fsm.states.is_empty()) {
        return Err("Add a block or state before exporting a diagram.".into());
    }
    if !(1..=6).contains(&options.scale)
        || !options.padding.is_finite()
        || !(0.0..=200.0).contains(&options.padding)
    {
        return Err("Invalid export scale or padding.".into());
    }
    let mut scene = build_scene(circuit);
    if options.title {
        let b = scene.bounds();
        scene.styled_text(
            crate::model::Point::new((b.min.x + b.max.x) / 2.0, b.min.y - 32.0),
            &circuit.title,
            22.0,
            INK,
            TextStyle::Heading,
        );
    }
    let bounds = scene.bounds().expanded(options.padding);
    Ok((scene, bounds))
}
/// Render a self-contained SVG with outlined text. PNG scale does not affect
/// vector geometry; explicit net colors and diagram labels remain intact.
///
/// # Errors
///
/// Returns scene construction errors, including empty diagrams or invalid options.
pub fn svg(circuit: &Circuit, options: ExportOptions) -> Result<String, String> {
    let (scene, b) = export_scene(circuit, options)?;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"{} {} {} {}\" role=\"img\"><title>{}</title>\n",
        b.width().ceil(),
        b.height().ceil(),
        b.min.x,
        b.min.y,
        b.width(),
        b.height(),
        xml(&circuit.title)
    );
    if !options.transparent {
        let _ = writeln!(
            s,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\"/>",
            b.min.x,
            b.min.y,
            b.width(),
            b.height()
        );
    }
    for item in scene.items {
        match item {
            Primitive::Path { path, fill, stroke } => svg_path(&mut s, &path, fill, stroke),
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                let _ = writeln!(s, "<g aria-label=\"{}\">", xml(&text));
                for path in styled_text_paths(position, &text, size, style) {
                    svg_path(&mut s, &path, Some(color), None);
                }
                s.push_str("</g>\n");
            }
        }
    }
    s.push_str("</svg>\n");
    Ok(s)
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn svg_path(s: &mut String, path: &PathData, fill: Option<Color>, stroke: Option<(Color, f32)>) {
    s.push_str("<path d=\"");
    for cmd in &path.0 {
        let _ = match cmd {
            Cmd::Move(p) => write!(s, "M{:.3},{:.3} ", p.x, p.y),
            Cmd::Line(p) => write!(s, "L{:.3},{:.3} ", p.x, p.y),
            Cmd::Quad(a, b) => write!(s, "Q{:.3},{:.3} {:.3},{:.3} ", a.x, a.y, b.x, b.y),
            Cmd::Cubic(a, b, c) => write!(
                s,
                "C{:.3},{:.3} {:.3},{:.3} {:.3},{:.3} ",
                a.x, a.y, b.x, b.y, c.x, c.y
            ),
            Cmd::Close => write!(s, "Z "),
        };
    }
    let _ = write!(
        s,
        "\" fill=\"{}\"",
        fill.map(hex).unwrap_or_else(|| "none".into())
    );
    if let Some((color, width)) = stroke {
        let _ = write!(
            s,
            " stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
            hex(color),
            width
        );
    }
    s.push_str("/>\n");
}
fn skia_path(path: &PathData) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for cmd in &path.0 {
        match cmd {
            Cmd::Move(p) => pb.move_to(p.x, p.y),
            Cmd::Line(p) => pb.line_to(p.x, p.y),
            Cmd::Quad(a, b) => pb.quad_to(a.x, a.y, b.x, b.y),
            Cmd::Cubic(a, b, c) => pb.cubic_to(a.x, a.y, b.x, b.y, c.x, c.y),
            Cmd::Close => pb.close(),
        }
    }
    pb.finish()
}
/// Rasterize the shared document scene at the requested integer scale.
/// Allocation limits reject oversized images before allocating the pixel buffer.
///
/// # Errors
///
/// Returns scene construction, oversized-image, pixel-buffer allocation or PNG
/// encoding errors.
pub fn png(circuit: &Circuit, options: ExportOptions) -> Result<Vec<u8>, String> {
    let (scene, b) = export_scene(circuit, options)?;
    let scale = options.scale as f32;
    let width = (b.width() * scale).ceil() as u32;
    let height = (b.height() * scale).ceil() as u32;
    if width > 16384 || height > 16384 || u64::from(width) * u64::from(height) > 40_000_000 {
        return Err("Image would be too large. Reduce the scale or move the blocks closer together (maximum 40 megapixels).".into());
    }
    rasterize(scene, b, scale, width, height, options.transparent)?
        .encode_png()
        .map_err(|e| e.to_string())
}

/// Render a bounded thumbnail using exactly the export paths, fonts and crop.
/// The full-resolution export is never allocated just to display its preview.
pub(crate) fn preview(
    circuit: &Circuit,
    options: ExportOptions,
) -> Result<tiny_skia::Pixmap, String> {
    let (scene, bounds) = export_scene(circuit, options)?;
    let scale = (960.0 / bounds.width())
        .min(480.0 / bounds.height())
        .min(options.scale as f32);
    let width = (bounds.width() * scale).ceil().max(1.0) as u32;
    let height = (bounds.height() * scale).ceil().max(1.0) as u32;
    rasterize(scene, bounds, scale, width, height, options.transparent)
}

fn rasterize(
    scene: Scene,
    b: Bounds,
    scale: f32,
    width: u32,
    height: u32,
    transparent: bool,
) -> Result<tiny_skia::Pixmap, String> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("Cannot allocate image.")?;
    if !transparent {
        pixmap.fill(tiny_skia::Color::WHITE);
    }
    let transform =
        tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, -b.min.x * scale, -b.min.y * scale);
    let draw = |pixmap: &mut tiny_skia::Pixmap,
                path: &PathData,
                fill: Option<Color>,
                stroke: Option<(Color, f32)>| {
        let Some(path) = skia_path(path) else {
            return;
        };
        let mut paint = tiny_skia::Paint {
            anti_alias: true,
            ..Default::default()
        };
        if let Some(c) = fill {
            paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
        }
        if let Some((c, width)) = stroke {
            paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
            let stroke = tiny_skia::Stroke {
                width,
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
    };
    for item in scene.items {
        match item {
            Primitive::Path { path, fill, stroke } => draw(&mut pixmap, &path, fill, stroke),
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                for path in styled_text_paths(position, &text, size, style) {
                    draw(&mut pixmap, &path, Some(color), None);
                }
            }
        }
    }
    Ok(pixmap)
}
/// Choose PNG/SVG from the destination extension and replace it atomically.
/// Returns an error for unsupported extensions or failed rendering/file writes.
///
/// # Errors
///
/// Returns rendering errors, an unsupported-extension error, or atomic file
/// write errors. Library callers must enforce their own overwrite policy.
pub fn save(circuit: &Circuit, path: &Path, options: ExportOptions) -> Result<(), String> {
    atomic_write(path, &encode(circuit, path, options)?)
}

/// Encode PNG or SVG according to the file name's extension, without accessing a file.
///
/// # Errors
/// Returns an error for unsupported extensions, invalid export settings or rendering failure.
pub fn encode(circuit: &Circuit, name: &Path, options: ExportOptions) -> Result<Vec<u8>, String> {
    match name
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("svg") => Ok(svg(circuit, options)?.into_bytes()),
        Some("png") => png(circuit, options),
        _ => Err("Choose a .png or .svg file name.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_matches_png_pixels_and_updates_export_settings() {
        let mut circuit = Circuit::empty();
        circuit.add(crate::model::Kind::Input, crate::model::Point::default());
        let options = ExportOptions {
            scale: 1,
            ..Default::default()
        };
        let thumbnail = preview(&circuit, options).unwrap();
        let exported = tiny_skia::Pixmap::decode_png(&png(&circuit, options).unwrap()).unwrap();
        assert_eq!(thumbnail, exported);
        let transparent = preview(
            &circuit,
            ExportOptions {
                transparent: true,
                ..options
            },
        )
        .unwrap();
        assert_eq!(transparent.pixels()[0].alpha(), 0);
        assert_eq!(thumbnail.pixels()[0].alpha(), 255);
        let titled = preview(
            &circuit,
            ExportOptions {
                title: true,
                ..options
            },
        )
        .unwrap();
        assert!(titled.height() > thumbnail.height());
        let padded = preview(
            &circuit,
            ExportOptions {
                padding: 60.0,
                ..options
            },
        )
        .unwrap();
        assert!(padded.width() > thumbnail.width());
        assert!(preview(&Circuit::empty(), options).is_err());
    }

    #[test]
    fn preview_is_bounded_even_when_full_png_is_too_large() {
        let mut circuit = Circuit::empty();
        circuit.add(crate::model::Kind::Input, crate::model::Point::default());
        circuit.add(
            crate::model::Kind::Output,
            crate::model::Point::new(90_000.0, 90_000.0),
        );
        let thumbnail = preview(&circuit, ExportOptions::default()).unwrap();
        assert!(thumbnail.width() <= 960 && thumbnail.height() <= 480);
        assert!(png(&circuit, ExportOptions::default()).is_err());
    }
}
