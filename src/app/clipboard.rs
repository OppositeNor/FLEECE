//! Circuit selections travel through the same native clipboard as text edits.
use super::*;

const HEADER: &str = "# FLEECE selection\n";

impl Editor {
    pub(super) fn clipboard_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let (copy, cut, pastes) = ctx.input(|input| {
            let copy = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Copy))
                || (input.modifiers.command && input.key_pressed(Key::C));
            let cut = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Cut))
                || (input.modifiers.command && input.key_pressed(Key::X));
            let pastes: Vec<_> = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Paste(text) => Some(text.clone()),
                    _ => None,
                })
                .collect();
            (copy, cut, pastes)
        });
        // Label selections do not request keyboard focus like TextEdit does.
        // Leave Copy events intact for egui's selection plugin instead of
        // replacing selected status/error text with the diagram fragment.
        if (copy || cut)
            && ctx
                .plugin::<egui::text_selection::LabelSelectionState>()
                .lock()
                .has_selection()
        {
            return true;
        }
        if copy || cut {
            self.finish_edit();
            self.finish_drag();
            let fragment = self.circuit().selection(&self.selected);
            if !fragment.nodes.is_empty()
                || fragment
                    .fsm
                    .as_ref()
                    .is_some_and(|fsm| !fsm.states.is_empty())
            {
                match Document::new(fragment).to_toml() {
                    Ok(text) => {
                        ctx.copy_text(format!("{HEADER}{text}"));
                        self.last_paste.clear();
                        self.paste_count = 0;
                        let count = self.selected.len();
                        if cut {
                            // Cut exactly the nodes/states included in the fragment;
                            // unrelated selected wire controls are not clipboard data.
                            self.selected_bends.clear();
                            self.selected_wire = None;
                            self.delete();
                            self.set_mode(CanvasMode::Select);
                        }
                        self.status =
                            format!("{} {count} nodes", if cut { "Cut" } else { "Copied" });
                    }
                    Err(error) => {
                        self.status =
                            format!("{} failed: {error}", if cut { "Cut" } else { "Copy" })
                    }
                }
            }
        }
        let handled = copy || cut || !pastes.is_empty();
        for text in pastes {
            let anchor = ctx
                .input(|input| input.pointer.hover_pos())
                .filter(|point| self.canvas_rect.contains(*point))
                .map(|point| self.world(point))
                .unwrap_or(self.cursor);
            if let Err(error) = self.paste_selection(&text, anchor) {
                self.status = format!("Paste failed: {error}");
            }
        }
        if handled {
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                    )
                })
            });
        }
        handled
    }

    fn paste_selection(&mut self, text: &str, anchor: Point) -> Result<(), String> {
        if text.len() > 8 * 1024 * 1024 || !text.starts_with(HEADER) {
            return Err("Copy diagram nodes before pasting onto the canvas.".into());
        }
        let fragment = Document::from_toml(text)?.circuit;
        let anchor = if self.document.editor.snap {
            anchor.snapped()
        } else {
            anchor
        };
        let count = if self.last_paste == text && self.last_paste_anchor == Some(anchor) {
            self.paste_count + 1
        } else {
            1
        };
        let positions: Vec<_> = if let Some(fsm) = &fragment.fsm {
            fsm.states.iter().map(|state| state.position).collect()
        } else {
            fragment.nodes.iter().map(|node| node.position).collect()
        };
        let origin = positions
            .iter()
            .fold(Point::new(f32::INFINITY, f32::INFINITY), |min, p| {
                Point::new(min.x.min(p.x), min.y.min(p.y))
            });
        let distance = GRID * 2.0 * (count - 1) as f32;
        let offset = Point::new(
            anchor.x - origin.x + distance,
            anchor.y - origin.y + distance,
        );
        self.finish_drag();
        let mut pasted = self.circuit().clone();
        let selected = pasted.paste_fragment(&fragment, offset)?;
        pasted.connect_overlapping_pins(&selected)?;
        self.change(|circuit| *circuit = pasted);
        self.selected = selected;
        self.selected_wire = None;
        self.selected_bends.clear();
        self.pending_pin = None;
        self.pending_joint = None;
        self.set_mode(CanvasMode::Select);
        self.drag = None;
        self.last_paste = text.into();
        self.paste_count = count;
        self.last_paste_anchor = Some(anchor);
        self.status = format!(
            "Pasted {} nodes · drag or use arrows to move",
            self.selected.len()
        );
        Ok(())
    }
}
