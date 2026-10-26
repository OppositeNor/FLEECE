use fleece::{
    app::Editor,
    drawing,
    export::{self, ExportOptions},
    model::*,
};

fn document() -> Document {
    Document::from_toml(include_str!("../examples/fsm.toml")).unwrap()
}
#[test]
fn fsm_documents_roundtrip_validate_and_preserve_topology_on_deletion() {
    let mut doc = document();
    let original = doc.clone();
    let fsm = doc.circuit.fsm.as_ref().unwrap();
    assert_eq!(fsm.states.len(), 2);
    assert_eq!(fsm.transitions.len(), 3);
    assert_eq!(fsm.initial, Some(1));
    assert!(fsm.state(2).unwrap().accepting);
    assert_eq!(Document::from_toml(&doc.to_toml().unwrap()).unwrap(), doc);
    doc.circuit
        .fsm
        .as_mut()
        .unwrap()
        .delete_states(&[1].into_iter().collect());
    let fsm = doc.circuit.fsm.as_ref().unwrap();
    assert_eq!(fsm.states.len(), 1);
    assert!(fsm.transitions.is_empty());
    assert_eq!(fsm.initial, Some(2));
    let mut mixed = original;
    mixed.circuit.add(Kind::Input, Point::default());
    assert!(mixed.validate().is_err());
}

#[test]
fn fsm_exports_include_loops_and_reciprocal_arrows_and_ignore_editor_settings() {
    let mut doc = document();
    let fsm = doc.circuit.fsm.as_ref().unwrap();
    let forward = drawing::transition_geometry(fsm, &fsm.transitions[0]).unwrap();
    let reverse = drawing::transition_geometry(fsm, &fsm.transitions[1]).unwrap();
    let loop_edge = drawing::transition_geometry(fsm, &fsm.transitions[2]).unwrap();
    assert_ne!(forward.label, reverse.label);
    assert!(loop_edge.label.y < fsm.state(1).unwrap().position.y - drawing::STATE_RADIUS);
    let svg = export::svg(&doc.circuit, ExportOptions::default()).unwrap();
    let png = export::png(
        &doc.circuit,
        ExportOptions {
            scale: 1,
            ..Default::default()
        },
    )
    .unwrap();
    for label in ["Idle", "Run", "start = 1", "stop = 1", "start = 0", "y = 0"] {
        assert!(svg.contains(label));
    }
    doc.editor.pan = Point::new(900.0, -50.0);
    doc.editor.zoom = 0.25;
    doc.editor.theme = Some("tokyo-night".into());
    doc.editor.grid = false;
    assert_eq!(
        export::svg(&doc.circuit, ExportOptions::default()).unwrap(),
        svg
    );
    assert_eq!(
        export::png(
            &doc.circuit,
            ExportOptions {
                scale: 1,
                ..Default::default()
            }
        )
        .unwrap(),
        png
    );
    assert!(export::svg(&Circuit::new_fsm(), ExportOptions::default()).is_err());
}
#[test]
fn external_fsm_edits_reload_clean_documents_and_preserve_unsaved_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("machine.toml");
    document().save(&path).unwrap();
    let doc = Document::load(&path).unwrap();
    let mut editor = Editor::new(doc.clone(), Some(path.clone()));
    let mut updated = doc.clone();
    updated
        .circuit
        .fsm
        .as_mut()
        .unwrap()
        .state_mut(1)
        .unwrap()
        .name = "Updated".into();
    updated.save(&path).unwrap();
    editor.check_external_file();
    assert_eq!(editor.document.circuit, updated.circuit);
    assert!(!editor.dirty());
    editor
        .document
        .circuit
        .fsm
        .as_mut()
        .unwrap()
        .state_mut(1)
        .unwrap()
        .output = "Unsaved".into();
    doc.save(&path).unwrap();
    editor.check_external_file();
    assert_eq!(
        editor
            .document
            .circuit
            .fsm
            .as_ref()
            .unwrap()
            .state(1)
            .unwrap()
            .output,
        "Unsaved"
    );
    assert!(editor.dirty());
}

#[test]
fn fsm_clipboard_rejects_invalid_geometry_and_foreign_endpoints_atomically() {
    let mut target = document().circuit;
    let original = target.clone();
    let fragment = target.selection(&[1, 2].into_iter().collect());
    assert!(
        target
            .paste_fragment(&fragment, Point::new(100000.0, 0.0))
            .is_err()
    );
    assert_eq!(target, original);
    let mut invalid = fragment.clone();
    invalid.fsm.as_mut().unwrap().transitions[0].to = 999;
    assert!(target.paste_fragment(&invalid, Point::default()).is_err());
    assert_eq!(target, original);
    assert!(
        target
            .paste_fragment(&Circuit::half_adder(), Point::default())
            .is_err()
    );
    assert_eq!(target, original);
}
