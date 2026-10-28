//! File browser, component search, wiring, and shortcut dialogs.
use super::*;

impl Editor {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if self.file_dialog.is_some() {
            self.file_modal(ctx);
            return;
        }
        if self.confirm_unsaved {
            let mut action = 0;
            egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading("Save your changes?");
                ui.label("This document has unsaved changes.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        action = 1;
                    }
                    if ui.button("Discard changes").clicked() {
                        action = 2;
                    }
                    if ui.button("Cancel").clicked() {
                        action = 3;
                    }
                });
            });
            match action {
                1 => self.save(),
                2 => {
                    self.confirm_unsaved = false;
                    if let Some(p) = self.pending.take() {
                        self.perform(p);
                    }
                }
                3 => {
                    self.confirm_unsaved = false;
                    self.pending = None;
                }
                _ => {}
            }
            return;
        }
        if self.new_dialog {
            let mut choice = None;
            let response = egui::Modal::new(egui::Id::unique("new_document")).show(ctx, |ui| {
                ui.heading("New document");
                ui.label("Choose the diagram you want to create.");
                if ui.button("Logic Circuit").clicked() {
                    choice = Some(false);
                }
                if ui.button("FSM (Finite State Machine)").clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    self.new_dialog = false;
                }
            });
            if response.should_close() {
                self.new_dialog = false;
            }
            if let Some(fsm) = choice {
                self.new_dialog = false;
                self.request(Pending::New(fsm));
            }
            return;
        }
        if self.palette {
            let mut chosen = None;
            let mut cancel = false;
            let response =
                egui::Modal::new(egui::Id::unique("component_palette")).show(ctx, |ui| {
                    ui.set_width(410.0);
                    ui.heading("Add a component");
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.palette_query)
                            .hint_text("Search gates, inputs, modules…")
                            .desired_width(f32::INFINITY),
                    );
                    if self.palette_focus {
                        edit.request_focus();
                        self.palette_focus = false;
                    }
                    if edit.changed() {
                        self.palette_index = 0;
                    }
                    let kinds: Vec<_> = Kind::ALL
                        .into_iter()
                        .filter(|k| {
                            k.name()
                                .to_lowercase()
                                .contains(&self.palette_query.to_lowercase())
                        })
                        .collect();
                    if !kinds.is_empty() {
                        if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                            self.palette_index = (self.palette_index + 1) % kinds.len();
                        }
                        if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                            self.palette_index =
                                (self.palette_index + kinds.len() - 1) % kinds.len();
                        }
                        if ctx.input(|i| i.key_pressed(Key::Enter)) {
                            chosen = Some(kinds[self.palette_index.min(kinds.len() - 1)]);
                        }
                    }
                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            for (i, kind) in kinds.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        i == self.palette_index,
                                        format!("{:<22} {}", kind.name(), kind.shortcut()),
                                    )
                                    .clicked()
                                {
                                    chosen = Some(*kind);
                                }
                            }
                        });
                    ui.label(
                        RichText::new("↑ ↓ choose · Enter select · Esc cancel")
                            .small()
                            .weak(),
                    );
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            if response.should_close() || cancel {
                self.palette = false;
            }
            if let Some(kind) = chosen {
                self.palette = false;
                ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(egui::Id::NULL)));
                self.start_placing(kind);
            }
            return;
        }
        if self.connect_dialog {
            let mut connect = false;
            let mut cancel = false;
            let nodes = self.circuit().nodes.clone();
            let response = egui::Modal::new(egui::Id::unique("connect_dialog")).show(ctx, |ui| {
                ui.set_width(400.0);
                ui.heading("Connect pins");
                ui.label("Output → input. Connecting to a driven input replaces its wire.");
                for (output, endpoint) in [
                    (true, &mut self.connect_from),
                    (false, &mut self.connect_to),
                ] {
                    ui.add_space(8.0);
                    ui.label(if output {
                        "Source block"
                    } else {
                        "Destination block"
                    });
                    egui::ComboBox::from_id_salt(("connect_node", output))
                        .selected_text(
                            nodes
                                .iter()
                                .find(|n| n.id == endpoint.node)
                                .map(|n| format!("{} #{}", n.label, n.id))
                                .unwrap_or_else(|| "Choose a block".into()),
                        )
                        .show_ui(ui, |ui| {
                            for n in nodes
                                .iter()
                                .filter(|n| if output { n.outputs > 0 } else { n.inputs > 0 })
                            {
                                if ui
                                    .selectable_value(
                                        &mut endpoint.node,
                                        n.id,
                                        format!("{} #{}", n.label, n.id),
                                    )
                                    .changed()
                                {
                                    endpoint.port = 0;
                                }
                            }
                        });
                    if let Some(n) = nodes.iter().find(|n| n.id == endpoint.node) {
                        egui::ComboBox::from_id_salt(("connect_port", output))
                            .selected_text(n.port_name(output, endpoint.port))
                            .show_ui(ui, |ui| {
                                for port in 0..if output { n.outputs } else { n.inputs } {
                                    ui.selectable_value(
                                        &mut endpoint.port,
                                        port,
                                        n.port_name(output, port),
                                    );
                                }
                            });
                    }
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Connect").clicked() || ctx.input(|i| i.key_pressed(Key::Enter)) {
                        connect = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
            if response.should_close() || cancel {
                self.connect_dialog = false;
            }
            if connect {
                let from = self.connect_from;
                let to = self.connect_to;
                let mut result = Ok(());
                self.change(|c| result = c.connect(from, to));
                match result {
                    Ok(()) => {
                        self.connect_dialog = false;
                        self.status = "Connected".into();
                    }
                    Err(e) => self.status = e,
                }
            }
            return;
        }
        if self.help {
            self.help_modal(ctx);
        }
    }

    fn help_modal(&mut self, ctx: &egui::Context) {
        if self.circuit().fsm.is_some() {
            let response = egui::Modal::new(egui::Id::unique("fsm_help")).show(ctx, |ui| {
                ui.heading("FSM keyboard & mouse");
                for (keys, description) in [
                    ("S, then click or Enter", "Repeatedly place states until Esc"),
                    ("W, then source/destination", "Wire transitions; same state creates a loop (T is an alias)"),
                    ("V / Esc", "Select and move / cancel"),
                    ("D", "Toggle Drag mode to pan anywhere"),
                    ("Drag empty canvas / Shift+drag", "Box select states / extend selection"),
                    ("Drag / arrow keys", "Move selected states"),
                    ("Right-click / Delete", "Remove item / remove selection"),
                    ("Ctrl+Z / Ctrl+Shift+Z", "Undo / redo"),
                    ("Ctrl+X / Ctrl+C / Ctrl+V / Ctrl+D", "Cut / copy / paste / duplicate selected states"),
                    ("Ctrl+scroll / pinch", "Zoom at the pointer"),
                    ("Shift+scroll", "Pan horizontally"),
                    ("F / Shift+L", "Fit / arrange states"),
                    ("Ctrl+N / Ctrl+S / Ctrl+O / Ctrl+E", "New / save / open / export"),
                ] { ui.horizontal(|ui| { ui.monospace(keys); ui.label(description); }); }
                ui.label("Edit names, initial/final states, transition endpoints, conditions and outputs in the inspector. Conditions and outputs are annotations.");
                if ui.button("Close").clicked() { self.help = false; }
            });
            if response.should_close() {
                self.help = false;
            }
            return;
        }
        const SHORTCUTS: &[(&str, &str)] = &[
            ("I / O", "Input / output"),
            ("A / R / N / X / B", "AND / OR / NOT / XOR / buffer"),
            ("Shift+A / Shift+R / Shift+E", "NAND / NOR / XNOR"),
            (
                "M / H / Shift+H / U",
                "Mux / half adder / full adder / custom module",
            ),
            ("1 / 0", "Constant high / low"),
            ("Shift+D", "D-flip-flop"),
            ("Ctrl+K", "Search component library"),
            (
                "Arrows / Shift+Arrows",
                "Move placement or selected blocks / larger steps",
            ),
            ("Enter / Esc", "Place component / cancel"),
            ("W / D / V / Esc", "Wire / Drag / Select / Select mode"),
            ("Shift+W", "Connect pins using keyboard dialog"),
            ("Tab / Shift+Tab", "Select next / previous block"),
            (
                "Wire mode: click opposite pins",
                "Connect wire (fan-out supported)",
            ),
            ("Drag / Shift-click", "Move / select blocks"),
            (
                "Wire mode: click connected pin",
                "Start another branch; existing wires stay connected",
            ),
            ("Alt-click gate pin", "Toggle inversion bubble"),
            (
                "Right-click component / joint",
                "Remove node (Ctrl+Z to undo)",
            ),
            (
                "Drag blank space / Shift+drag",
                "Box select / extend selection",
            ),
            ("Wire mode: click wire", "Add joint or start a branch"),
            ("Wire mode: click joint, then pin", "Add a wire branch"),
            (
                "Wire mode: click empty space",
                "Start or extend suspended wires",
            ),
            (
                "Drag joint / Delete",
                "Move all attached wires / remove joint",
            ),
            (
                "Scroll / Ctrl+scroll / pinch",
                "Pan / zoom / zoom at the pointer",
            ),
            ("Shift+scroll", "Pan canvas horizontally"),
            ("Middle drag / Space+drag", "Pan canvas"),
            ("Ctrl+scroll / F", "Zoom at pointer / fit circuit"),
            ("G / L / Shift+L", "Grid / cycle panels / arrange circuit"),
            ("Ctrl+A / Ctrl+D / Del", "Select all / duplicate / delete"),
            (
                "Ctrl+X / Ctrl+C / Ctrl+V",
                "Cut / copy / paste blocks and internal wires",
            ),
            ("Ctrl+Z / Shift+Z / Y", "Undo / redo / redo"),
            ("Ctrl+N / O / S / Shift+S", "New / open / save / save as"),
            ("Ctrl+E", "Export circuit as PNG or SVG"),
            ("Ctrl+Shift+R", "Reload external changes"),
        ];
        let response = egui::Modal::new(egui::Id::unique("help")).show(ctx, |ui| {
            ui.set_width(580.0);
            ui.heading("Keyboard & mouse");
            egui::ScrollArea::vertical().max_height(550.0).show(ui, |ui| {
                egui::Grid::new("shortcuts").striped(true).min_col_width(190.0).show(ui, |ui| {
                    for (keys, description) in SHORTCUTS {
                        ui.monospace(*keys);
                        ui.label(*description);
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.label("On macOS use Command for Ctrl shortcuts.");
                ui.label("FLEECE edits circuit and FSM diagrams. Component pin counts, register widths, inversion bubbles and wire colors are diagram properties.");
                ui.label("Documents and panel settings are saved as TOML.");
            });
            if ui.button("Close").clicked() {
                self.help = false;
            }
        });
        if response.should_close() {
            self.help = false;
        }
    }

    fn file_modal(&mut self, ctx: &egui::Context) {
        if self.external_files.is_some() {
            self.external_file_modal(ctx);
        } else {
            self.native_file_modal(ctx);
        }
    }

    fn native_file_modal(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.file_dialog.take() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("file_dialog")).show(ctx, |ui| {
            ui.set_width(590.0);
            ui.heading(match dialog.mode {
                FileMode::Open => "Open circuit",
                FileMode::Save => "Save circuit",
                FileMode::Export => "Export circuit image",
                FileMode::Theme => "Load UI theme",
            });
            ui.horizontal(|ui| {
                if ui.button("↑ Parent").clicked()
                    && let Some(p) = dialog.directory.parent().map(PathBuf::from)
                {
                    dialog.directory_text = p.display().to_string();
                    dialog.directory = p;
                    dialog.overwrite = false;
                }
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut dialog.directory_text).desired_width(390.0),
                );
                if ui.button("Go").clicked()
                    || (edit.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter)))
                {
                    let p = PathBuf::from(&dialog.directory_text);
                    if p.is_dir() {
                        dialog.directory = p;
                        dialog.error.clear();
                    } else {
                        dialog.error = "Folder does not exist.".into();
                    }
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("file_list")
                .max_height(200.0)
                .show(ui, |ui| match std::fs::read_dir(&dialog.directory) {
                    Ok(entries) => {
                        let mut paths: Vec<_> = entries
                            .filter_map(Result::ok)
                            .map(|e| e.path())
                            .filter(|p| {
                                p.is_dir()
                                    || p.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                                        if dialog.mode == FileMode::Export {
                                            matches!(s, "png" | "svg")
                                        } else {
                                            s == "toml"
                                        }
                                    })
                            })
                            .collect();
                        paths.sort_by_key(|p| {
                            (
                                !p.is_dir(),
                                p.file_name().map(|n| n.to_string_lossy().to_lowercase()),
                            )
                        });
                        for path in paths.into_iter().take(400) {
                            let name = path.file_name().unwrap().to_string_lossy();
                            let is_directory = path.is_dir();
                            // Stable path IDs keep clicks on different entries
                            // distinct when navigating to a new directory.
                            let response = ui
                                .push_id(&path, |ui| {
                                    ui.selectable_label(
                                        !is_directory && dialog.name == name.as_ref(),
                                        format!(
                                            "{} {}",
                                            if is_directory { "▸" } else { "·" },
                                            name
                                        ),
                                    )
                                })
                                .inner;
                            if response.clicked() || response.double_clicked() {
                                if is_directory {
                                    dialog.directory = path.clone();
                                    dialog.directory_text = path.display().to_string();
                                } else {
                                    dialog.name = name.into_owned();
                                    if response.double_clicked() && dialog.mode == FileMode::Open {
                                        submit = true;
                                    }
                                }
                                dialog.overwrite = false;
                            }
                        }
                    }
                    Err(e) => {
                        ui.colored_label(self.theme.ui.error.egui(), e.to_string());
                    }
                });
            ui.separator();
            let mut name_submitted = false;
            ui.horizontal(|ui| {
                ui.label("File name");
                let name =
                    ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(460.0));
                if dialog.focus_name {
                    name.request_focus();
                    dialog.focus_name = false;
                }
                name_submitted = (name.has_focus() || name.lost_focus())
                    && ctx.input(|input| input.key_pressed(Key::Enter));
                if name.changed() {
                    dialog.overwrite = false;
                }
            });
            if dialog.mode == FileMode::Export {
                self.export_settings(ui);
            }
            if !dialog.error.is_empty() {
                ui.colored_label(self.theme.ui.error.egui(), &dialog.error);
            }
            if dialog.overwrite {
                ui.colored_label(
                    self.theme.ui.warning.egui(),
                    "This file exists. Click Replace file to overwrite it.",
                );
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if dialog.overwrite {
                    "Replace file"
                } else {
                    match dialog.mode {
                        FileMode::Open => "Open",
                        FileMode::Save => "Save",
                        FileMode::Export => "Export",
                        FileMode::Theme => "Load theme",
                    }
                };
                if ui.button(label).clicked() || name_submitted {
                    submit = true;
                }
                if ui.button("Cancel").clicked() || ctx.input(|i| i.key_pressed(Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if cancel {
            return;
        }
        if submit {
            if dialog.name.trim().is_empty() {
                dialog.error = "Choose a file name.".into();
            } else {
                let mut path = dialog.directory.join(dialog.name.trim());
                if path.extension().is_none() {
                    path.set_extension(if dialog.mode == FileMode::Export {
                        "png"
                    } else {
                        "toml"
                    });
                }
                if dialog.mode == FileMode::Theme {
                    match self.select_theme(&path.to_string_lossy()) {
                        Ok(()) => return,
                        Err(error) => dialog.error = error,
                    }
                    self.file_dialog = Some(dialog);
                    return;
                }
                if dialog.mode == FileMode::Open {
                    self.request(Pending::Open(path));
                    return;
                }
                if path.exists() && !dialog.overwrite {
                    dialog.overwrite = true;
                } else {
                    self.finish_edit();
                    let result = if dialog.mode == FileMode::Save {
                        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                            Err("Use a .toml extension for diagram files.".into())
                        } else {
                            self.document.save(&path)
                        }
                    } else {
                        self.export_image(&path)
                    };
                    match result {
                        Ok(()) => {
                            if dialog.mode == FileMode::Save {
                                self.path = Some(path.clone());
                                self.saved = self.circuit().clone();
                                self.remember_save_path(&path);
                                self.file_stamp = file_stamp(&path);
                                self.external_changed = false;
                                self.after_save();
                            }
                            self.status = format!(
                                "{} {}",
                                if dialog.mode == FileMode::Save {
                                    "Saved"
                                } else {
                                    "Exported"
                                },
                                path.display()
                            );
                            return;
                        }
                        Err(e) => {
                            dialog.error = e;
                            dialog.overwrite = false;
                        }
                    }
                }
            }
        }
        self.file_dialog = Some(dialog);
    }
}
