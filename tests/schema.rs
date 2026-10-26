use fleece::{app::Editor, export::ExportOptions, model::*, theme::Theme};

#[test]
fn loading_current_documents_never_changes_names_pins_or_connectivity() {
    let mut circuit = Circuit::empty();
    let id = circuit.add(Kind::Register, Point::new(100.0, 100.0));
    circuit.node_mut(id).unwrap().label = format!("State register {id}");
    circuit.resize_register(id, 3).unwrap();
    circuit
        .connect(
            Endpoint { node: id, port: 0 },
            Endpoint { node: id, port: 0 },
        )
        .unwrap();
    let document = Document::new(circuit);
    let text = document.to_toml().unwrap();
    assert_eq!(Document::from_toml(&text).unwrap(), document);
    for field in ["value =", "truth_table =", "last_clock", "dark ="] {
        assert!(!text.contains(field));
    }
    for example in ["half-adder", "state-register", "fsm"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(format!("{example}.toml"));
        let document = Document::load(&path).unwrap();
        let mut browser = Editor::with_external_files(Document::new(Circuit::empty()));
        browser
            .import_document(
                &format!("{example}.toml"),
                &std::fs::read_to_string(&path).unwrap(),
            )
            .unwrap();
        assert_eq!(browser.document.circuit, document.circuit);
        assert_eq!(
            Document::from_toml(&document.to_toml().unwrap()).unwrap(),
            document
        );
    }
}

#[test]
fn retired_fields_and_unsupported_pin_layouts_are_rejected() {
    let mut circuit = Circuit::empty();
    let id = circuit.add(Kind::Register, Point::default());
    let document = Document::new(circuit);
    let value: toml::Value = toml::from_str(&document.to_toml().unwrap()).unwrap();
    for (key, retired) in [
        ("value", toml::Value::Boolean(true)),
        ("truth_table", toml::Value::Array(vec![])),
        ("register", toml::Value::Table(Default::default())),
    ] {
        let mut invalid = value.clone();
        invalid["circuit"]["nodes"].as_array_mut().unwrap()[0]
            .as_table_mut()
            .unwrap()
            .insert(key.into(), retired);
        assert!(
            Document::from_toml(&toml::to_string(&invalid).unwrap()).is_err(),
            "{key}"
        );
    }
    let mut invalid = document.clone();
    invalid.circuit.node_mut(id).unwrap().inputs -= 1;
    assert!(invalid.validate().is_err());
    let text = document.to_toml().unwrap();
    assert!(
        Document::from_toml(&text.replace("kind = \"register\"", "kind = \"d_flip_flop\""))
            .is_err()
    );
    let mut invalid = value;
    invalid["editor"]
        .as_table_mut()
        .unwrap()
        .insert("dark".into(), toml::Value::Boolean(true));
    assert!(Document::from_toml(&toml::to_string(&invalid).unwrap()).is_err());
    let mut circuit = Circuit::empty();
    let id = circuit.add(Kind::Xor, Point::default());
    circuit.node_mut(id).unwrap().inputs = 3;
    assert!(Document::new(circuit).validate().is_err());
}

#[test]
fn configuration_and_exports_accept_only_current_fields() {
    assert!(toml::from_str::<ExportOptions>("signals = true").is_err());
    for key in ["signal_low", "signal_high", "signal_unknown"] {
        let text = format!("theme_version = 1\nname = 'Unsupported'\n[canvas]\n{key} = '#123456'");
        assert!(Theme::from_toml(&text).is_err());
    }
    let document = Document::new(Circuit::empty());
    let mut view = document.clone();
    view.editor.theme = None;
    assert_eq!(
        Editor::new(view, None).active_theme(),
        &Theme::original(true)
    );
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("logic-circuit-editor/state.toml");
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    std::fs::write(&old, "last_save_path = '/obsolete/file.toml'\n").unwrap();
    let state = directory.path().join("fleece/state.toml");
    let editor = Editor::with_storage_paths(document, None, Some(state.clone()), None);
    assert!(!state.exists());
    assert_eq!(editor.active_theme(), &Theme::original(true));
    assert_eq!(
        std::fs::read_to_string(old).unwrap(),
        "last_save_path = '/obsolete/file.toml'\n"
    );
}
