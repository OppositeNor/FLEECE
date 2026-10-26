//! Cached export thumbnails shared by desktop and browser export dialogs.
use super::*;

pub(super) struct ExportPreview {
    circuit: Circuit,
    options: ExportOptions,
    image: Result<egui::TextureHandle, String>,
}

impl Editor {
    pub(super) fn export_settings(&mut self, ui: &mut egui::Ui) {
        ui.label("Use .png or .svg; export includes the whole diagram.");
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.export_options.scale, 1..=6).text("PNG scale"));
            ui.checkbox(
                &mut self.export_options.transparent,
                "Transparent background",
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.export_options.title, "Include title");
            ui.add(
                egui::Slider::new(&mut self.export_options.padding, 0.0..=200.0).text("Padding"),
            );
        });
        ui.separator();
        ui.label("Export preview");
        if self.export_preview.as_ref().is_none_or(|preview| {
            preview.options != self.export_options || preview.circuit != *self.circuit()
        }) {
            let image = export::preview(self.circuit(), self.export_options).map(|pixmap| {
                // tiny-skia stores premultiplied RGBA, as does egui's texture.
                let image = egui::ColorImage::from_rgba_premultiplied(
                    [pixmap.width() as usize, pixmap.height() as usize],
                    pixmap.data(),
                );
                ui.ctx()
                    .load_texture("export_preview", image, egui::TextureOptions::LINEAR)
            });
            self.export_preview = Some(ExportPreview {
                circuit: self.circuit().clone(),
                options: self.export_options,
                image,
            });
        }
        match &self.export_preview.as_ref().unwrap().image {
            Ok(texture) => {
                let available = Vec2::new(
                    ui.available_width(),
                    (ui.ctx().content_rect().height() * 0.27).clamp(60.0, 220.0),
                );
                let size = texture.size_vec2();
                let size = size * (available.x / size.x).min(available.y / size.y).min(1.0);
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(available.x, size.y), Sense::hover());
                let image_rect = Rect::from_center_size(rect.center(), size);
                let painter = ui.painter().with_clip_rect(image_rect);
                // Checkerboard makes the actual alpha channel visible.
                for y in 0..(size.y / 12.0).ceil() as usize {
                    for x in 0..(size.x / 12.0).ceil() as usize {
                        let color = if (x + y) % 2 == 0 { 210 } else { 245 };
                        painter.rect_filled(
                            Rect::from_min_size(
                                image_rect.min + Vec2::new(x as f32 * 12.0, y as f32 * 12.0),
                                Vec2::splat(12.0),
                            ),
                            0.0,
                            Color32::from_gray(color),
                        );
                    }
                }
                painter.image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            Err(error) => {
                ui.colored_label(self.theme.ui.error.egui(), error);
            }
        }
    }
}
