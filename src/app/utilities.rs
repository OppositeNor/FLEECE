//! Shared canvas setup, coordinates, navigation and controls.
use super::*;

impl Editor {
    pub(super) fn screen(&self, p: Point) -> Pos2 {
        Pos2::new(
            self.canvas_rect.min.x + self.document.editor.pan.x + p.x * self.document.editor.zoom,
            self.canvas_rect.min.y + self.document.editor.pan.y + p.y * self.document.editor.zoom,
        )
    }
    pub(super) fn world(&self, p: Pos2) -> Point {
        let z = self.document.editor.zoom;
        Point::new(
            (p.x - self.canvas_rect.min.x - self.document.editor.pan.x) / z,
            (p.y - self.canvas_rect.min.y - self.document.editor.pan.y) / z,
        )
    }
    pub(super) fn pan_by(&mut self, delta: Vec2) {
        if delta.is_finite() {
            self.document.editor.pan.x =
                (self.document.editor.pan.x + delta.x).clamp(-100_000.0, 100_000.0);
            self.document.editor.pan.y =
                (self.document.editor.pan.y + delta.y).clamp(-100_000.0, 100_000.0);
        }
    }
    pub(super) fn zoom_at(&mut self, factor: f32, p: Pos2) {
        let world = self.world(p);
        self.document.editor.zoom = (self.document.editor.zoom * factor).clamp(0.15, 4.0);
        let z = self.document.editor.zoom;
        self.document.editor.pan = Point::new(
            p.x - self.canvas_rect.min.x - world.x * z,
            p.y - self.canvas_rect.min.y - world.y * z,
        );
    }
    pub(super) fn fit(&mut self) {
        let scene = build_editor_scene(self.circuit(), &self.theme.canvas);
        let b = scene.bounds().expanded(60.0);
        let z = (self.canvas_rect.width() / b.width())
            .min(self.canvas_rect.height() / b.height())
            .clamp(0.15, 2.0);
        self.document.editor.zoom = z;
        self.document.editor.pan = Point::new(
            (self.canvas_rect.width() - b.width() * z) / 2.0 - b.min.x * z,
            (self.canvas_rect.height() - b.height() * z) / 2.0 - b.min.y * z,
        );
    }

    pub(super) fn canvas_frame(
        &mut self,
        ui: &mut egui::Ui,
    ) -> (
        egui::Response,
        egui::Painter,
        egui::Context,
        Option<Pos2>,
        bool,
    ) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        self.canvas_rect = response.rect;
        if self.fit_requested {
            self.fit();
            self.fit_requested = false;
        }
        let bg = self.theme.canvas.background.egui();
        painter.rect_filled(response.rect, 0.0, bg);
        let ctx = ui.ctx().clone();
        let mouse = ctx.input(|i| i.pointer.hover_pos());
        let modal = self.file_dialog.is_some()
            || self.palette
            || self.confirm_unsaved
            || self.connect_dialog
            || self.help
            || self.new_dialog;
        if let Some(p) = mouse {
            if response.rect.contains(p) && !modal && self.pointer_last != Some(p) {
                self.cursor = self.world(p);
            }
            self.pointer_last = Some(p);
            if response.hovered() && !modal {
                let navigation: Vec<_> = ctx.input(|i| {
                    i.raw
                        .events
                        .iter()
                        .filter(|event| {
                            matches!(event, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_))
                        })
                        .cloned()
                        .collect()
                });
                let navigated = !navigation.is_empty();
                for event in navigation {
                    match event {
                        egui::Event::MouseWheel {
                            unit,
                            mut delta,
                            modifiers,
                            ..
                        } => {
                            match unit {
                                egui::MouseWheelUnit::Point => {}
                                egui::MouseWheelUnit::Line => delta *= 40.0,
                                egui::MouseWheelUnit::Page => {
                                    delta = Vec2::new(
                                        delta.x * response.rect.width(),
                                        delta.y * response.rect.height(),
                                    );
                                }
                            }
                            if modifiers.ctrl {
                                let amount = if delta.y != 0.0 { delta.y } else { delta.x };
                                self.zoom_at((amount * 0.002).exp(), p);
                            } else {
                                if modifiers.shift && delta.x == 0.0 {
                                    delta = Vec2::new(delta.y, 0.0);
                                }
                                self.pan_by(delta);
                            }
                        }
                        egui::Event::Zoom(factor) => self.zoom_at(factor, p),
                        _ => unreachable!(),
                    }
                }
                if navigated {
                    self.cursor = self.world(p);
                }
            }
        }
        if self.document.editor.grid {
            let z = self.document.editor.zoom;
            let step = GRID * z;
            if step >= 7.0 {
                let first = self.screen(Point::default());
                let x0 = response.rect.left() + (first.x - response.rect.left()).rem_euclid(step);
                let y0 = response.rect.top() + (first.y - response.rect.top()).rem_euclid(step);
                let color = self.theme.canvas.grid.egui();
                let mut x = x0;
                while x < response.rect.right() {
                    let mut y = y0;
                    while y < response.rect.bottom() {
                        painter.circle_filled(Pos2::new(x, y), 1.0, color);
                        y += step;
                    }
                    x += step;
                }
            }
        }
        (response, painter, ctx, mouse, modal)
    }
    pub(super) fn canvas_controls(&mut self, ctx: &egui::Context, rect: Rect) {
        egui::Area::new(egui::Id::unique("canvas_controls"))
            .fixed_pos(rect.right_bottom() - Vec2::new(230.0, 42.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("−").clicked() {
                            self.zoom_at(0.8, rect.center());
                        }
                        ui.label(format!("{:.0}%", self.document.editor.zoom * 100.0));
                        if ui.small_button("+").clicked() {
                            self.zoom_at(1.25, rect.center());
                        }
                        if ui.small_button("Fit F").clicked() {
                            self.fit_requested = true;
                        }
                        ui.separator();
                        ui.checkbox(&mut self.document.editor.snap, "Snap");
                    });
                });
            });
    }
    pub(super) fn paint_selection_box(&self, painter: &egui::Painter, mouse: Option<Pos2>) {
        if let Some(Drag::Box { start, .. }) = &self.drag
            && let Some(p) = mouse
        {
            let rect = Rect::from_two_pos(self.screen(*start), p);
            painter.rect_filled(
                rect,
                0.0,
                self.theme.canvas.selection.egui().gamma_multiply(0.1),
            );
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, self.theme.canvas.selection.egui()),
                egui::StrokeKind::Inside,
            );
        }
    }
}
