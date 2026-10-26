use fleece::{
    app::Editor,
    drawing::{Primitive, TextStyle, diagram_size},
    editor_drawing::build_editor_scene,
    export::{self, ExportOptions},
    model::*,
    theme::{BUILT_INS, Base, Color, Theme},
};

#[test]
fn shipped_themes_load_through_the_toml_system() {
    for (key, name) in BUILT_INS {
        let theme = Theme::load_reference(key).unwrap();
        assert_eq!(theme.name, name);
    }
    let tokyo = Theme::from_toml(include_str!("../themes/tokyo-night.toml")).unwrap();
    assert_eq!(tokyo.canvas.background, Color([0x1a, 0x1b, 0x26, 255]));
    assert_eq!(tokyo.ui.accent, Color([0x7a, 0xa2, 0xf7, 255]));
    assert_eq!(tokyo.style().visuals.panel_fill, tokyo.ui.panel.egui());
    assert_eq!(
        tokyo.style().visuals.selection.bg_fill,
        tokyo.ui.selection.egui()
    );
}

#[test]
fn new_documents_start_with_original_dark_and_preserve_saved_theme_choices() {
    {
        let document = Document::new(Circuit::empty());
        assert_eq!(document.editor.theme.as_deref(), Some("original-dark"));
        let mut editor = Editor::new(document, None);
        assert_eq!(editor.active_theme(), &Theme::original(true));
        assert!(editor.document.editor.grid);

        for key in ["vscode", "tokyo-night", "original-light"] {
            editor.select_theme(key).unwrap();
            let saved = Document::from_toml(&editor.document.to_toml().unwrap()).unwrap();
            assert_eq!(saved.editor.theme.as_deref(), Some(key));
            let reopened = Editor::new(saved, None);
            assert_eq!(reopened.active_theme(), editor.active_theme());
        }
    }
}

#[test]
fn partial_themes_inherit_the_selected_base_and_support_metrics() {
    let theme = Theme::from_toml(
        r##"
        name = "My light palette"
        base = "light"
        [ui]
        accent = "#123456"
        [canvas]
        grid = "#abcdef80"
        [metrics]
        font_size = 18.0
        corner_radius = 8
    "##,
    )
    .unwrap();
    assert_eq!(theme.base, Base::Light);
    assert_eq!(theme.ui.accent, Color([0x12, 0x34, 0x56, 255]));
    assert_eq!(theme.canvas.grid, Color([0xab, 0xcd, 0xef, 0x80]));
    assert_eq!(
        theme.canvas.background,
        Theme::original(false).canvas.background
    );
    let style = theme.style();
    assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 18.0);
    assert_eq!(
        style.visuals.window_corner_radius,
        egui::CornerRadius::same(8)
    );
}

#[test]
fn invalid_theme_fields_colors_versions_and_metrics_are_rejected() {
    for text in [
        "name='Bad'\n[ui]\naccent='not a color'",
        "name='Bad'\n[ui]\naccent='#12345é'",
        "name='Bad'\n[canvas]\ngird='#123456'",
        "name='Bad'\nunknown=true",
        "name='Bad'\nbase='unknown'",
        "name='Bad'\ntheme_version=99",
        "name=''",
        "name='Bad'\n[metrics]\nfont_size=nan",
        "name='Bad'\n[metrics]\nfont_size=30",
        "name='Bad'\n[metrics]\ncorner_radius=25",
    ] {
        assert!(
            Theme::from_toml(text).is_err(),
            "Accepted invalid theme: {text}"
        );
    }
}

#[test]
fn custom_theme_files_reload_and_failed_loads_preserve_the_active_palette() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("custom.toml");
    std::fs::write(&path, "name='Custom'\n[canvas]\nbackground='#112233'").unwrap();
    let mut editor = Editor::new(Document::new(Circuit::half_adder()), None);
    editor.select_theme(path.to_str().unwrap()).unwrap();
    assert_eq!(
        editor.active_theme().canvas.background,
        Color([0x11, 0x22, 0x33, 255])
    );
    let saved = Document::from_toml(&editor.document.to_toml().unwrap()).unwrap();
    assert_eq!(
        saved.editor.theme,
        Some(path.to_string_lossy().into_owned())
    );
    std::fs::write(&path, "name='Reloaded'\n[canvas]\nbackground='#445566'").unwrap();
    editor.reload_theme().unwrap();
    assert_eq!(editor.active_theme().name, "Reloaded");
    let previous = editor.active_theme().clone();
    std::fs::write(&path, "name='Bad'\n[ui]\naccent='bad'").unwrap();
    assert!(editor.reload_theme().is_err());
    assert_eq!(editor.active_theme(), &previous);
    assert!(editor.select_theme("/does/not/exist.toml").is_err());
    assert_eq!(editor.active_theme(), &previous);
}

#[test]
fn editor_symbols_use_grid_geometry_and_sans_serif_labels() {
    let mut circuit = Circuit::empty();
    let id = circuit.add(Kind::Input, Point::default());
    circuit.node_mut(id).unwrap().label = "s_1".into();
    let node = circuit.node(id).unwrap();
    assert_eq!(node.size(), Point::new(80.0, 40.0));
    assert_eq!(node.pin(true, 0), node.pin(true, 0).snapped());
    assert_eq!(diagram_size(node), Point::new(96.0, 40.0));
    let scene = build_editor_scene(&circuit, &Theme::original(true).canvas);
    assert!(
        scene
            .items
            .iter()
            .any(|item| matches!(item,Primitive::Text{text,style:TextStyle::Ui,..} if text=="s_1"))
    );
    assert!(scene.items.iter().all(|item| !matches!(
        item,
        Primitive::Text {
            style: TextStyle::Math | TextStyle::Heading,
            ..
        }
    )));
    let view = View::default();
    assert!(view.grid);
}

#[test]
fn changing_ui_themes_does_not_change_any_exported_diagram_bytes() {
    let mut editor = Editor::new(Document::new(Circuit::half_adder()), None);
    let options = ExportOptions {
        scale: 1,
        ..Default::default()
    };
    let png = export::png(&editor.document.circuit, options).unwrap();
    let svg = export::svg(&editor.document.circuit, options).unwrap();
    for (key, _) in BUILT_INS {
        editor.select_theme(key).unwrap();
        assert_eq!(export::png(&editor.document.circuit, options).unwrap(), png);
        assert_eq!(export::svg(&editor.document.circuit, options).unwrap(), svg);
    }
}

#[test]
fn original_dark_uses_dark_component_bodies_and_contrasting_outlines() {
    let dark = Theme::load_reference("original-dark").unwrap();
    assert_eq!(dark, Theme::original(true));
    assert_eq!(dark.canvas.gate_fill, Color([32, 42, 58, 255]));
    assert_eq!(dark.canvas.gate_outline, Color([184, 199, 221, 255]));
    assert_eq!(
        Theme::original(false).canvas.gate_fill,
        Color([255, 255, 255, 255])
    );
    for kind in Kind::ALL {
        let mut circuit = Circuit::empty();
        circuit.add(kind, Point::default());
        let scene = build_editor_scene(&circuit, &dark.canvas);
        assert!(
            scene.items.iter().any(|item| matches!(item,
            Primitive::Path {fill:Some(fill),stroke:Some((stroke,_)),..}
            if *fill == dark.canvas.gate_fill.0 && *stroke == dark.canvas.gate_outline.0)),
            "{kind:?} must use the dark palette for its body and outline"
        );
    }
    let mut machine = Circuit::new_fsm();
    machine.fsm.as_mut().unwrap().add_state(Point::default());
    let scene = build_editor_scene(&machine, &dark.canvas);
    assert!(scene.items.iter().any(|item| matches!(item,
        Primitive::Path {fill:Some(fill),stroke:Some((stroke,_)),..}
        if *fill == dark.canvas.gate_fill.0 && *stroke == dark.canvas.gate_outline.0)));
}
