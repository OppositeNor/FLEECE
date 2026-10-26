//! Paint diagram scenes with the editor viewport and UI palette.
use super::*;

pub(super) fn paint_scene(
    painter: &egui::Painter,
    scene: &Scene,
    screen: impl Fn(Point) -> Pos2,
    zoom: f32,
    opacity: f32,
) {
    for item in &scene.items {
        match item {
            Primitive::Path { path, fill, stroke } => {
                let mut points: Vec<_> = path.flattened().into_iter().map(&screen).collect();
                let closed = path.0.iter().any(|c| matches!(c, Cmd::Close));
                if closed && points.first() == points.last() {
                    points.pop();
                }
                if let Some(color) = fill
                    && closed
                    && points.len() >= 3
                {
                    painter.add(egui::Shape::mesh(polygon_mesh(
                        &points,
                        to_color(*color).gamma_multiply(opacity),
                    )));
                }
                if let Some((color, width)) = stroke {
                    let stroke =
                        Stroke::new(width * zoom, to_color(*color).gamma_multiply(opacity));
                    if closed {
                        painter.add(egui::Shape::closed_line(points, stroke));
                    } else {
                        painter.add(egui::Shape::line(points, stroke));
                    }
                }
            }
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                painter.text(
                    screen(*position),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::new(size * zoom, egui::FontFamily::Name(style.family().into())),
                    to_color(*color).gamma_multiply(opacity),
                );
            }
        }
    }
}
/// Ear clipping permits the concave left edge of an OR gate to be filled correctly.
fn polygon_mesh(points: &[Pos2], color: Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    for &p in points {
        mesh.colored_vertex(p, color);
    }
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    while remaining.len() > 2 {
        let mut ear = None;
        for i in 0..remaining.len() {
            let a = remaining[(i + remaining.len() - 1) % remaining.len()];
            let b = remaining[i];
            let c = remaining[(i + 1) % remaining.len()];
            if cross(points[a], points[b], points[c]) * sign <= 0.00001 {
                continue;
            }
            if remaining.iter().any(|&j| {
                j != a
                    && j != b
                    && j != c
                    && cross(points[a], points[b], points[j]) * sign >= -0.00001
                    && cross(points[b], points[c], points[j]) * sign >= -0.00001
                    && cross(points[c], points[a], points[j]) * sign >= -0.00001
            }) {
                continue;
            }
            mesh.add_triangle(a as u32, b as u32, c as u32);
            ear = Some(i);
            break;
        }
        if let Some(i) = ear {
            remaining.remove(i);
        } else {
            break;
        }
    }
    mesh
}
