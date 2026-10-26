use fleece::{app::Editor, model::*};

#[test]
fn external_changes_reload_clean_documents_and_preserve_local_edits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared.toml");
    let mut document = Document::new(Circuit::half_adder());
    document.save(&path).unwrap();
    let mut editor = Editor::new(document.clone(), Some(path.clone()));
    editor.document.editor.zoom = 0.5;
    document.circuit.title = "Edited externally".into();
    document.save(&path).unwrap();
    editor.check_external_file();
    assert_eq!(editor.document.circuit.title, "Edited externally");
    assert_eq!(editor.document.editor.zoom, 0.5);
    assert!(!editor.dirty());
    editor.document.circuit.title = "Unsaved user work".into();
    document.circuit.title = "Another change made externally".into();
    document.save(&path).unwrap();
    editor.check_external_file();
    assert_eq!(editor.document.circuit.title, "Unsaved user work");
    assert!(editor.dirty());
}
