use std::{
    ffi::OsStr,
    process::{Command, Output},
};

fn run(arguments: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fleece"))
        .args(arguments)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap()
}

#[test]
fn exports_logic_and_fsm_documents_without_a_display_and_preserves_the_source() {
    let directory = tempfile::tempdir().unwrap();
    for example in ["half-adder", "state-register", "fsm"] {
        let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(format!("{example}.toml"));
        let before = std::fs::read(&input).unwrap();
        let document = fleece::model::Document::load(&input).unwrap();
        for extension in ["png", "svg"] {
            let output = directory.path().join(format!("{example}.{extension}"));
            // Compiler-style output replaces an existing destination atomically.
            std::fs::write(&output, "old image").unwrap();
            let result = run(&[input.as_os_str(), "-o".as_ref(), output.as_os_str()]);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
            let bytes = std::fs::read(&output).unwrap();
            let expected = match extension {
                "png" => fleece::export::png(&document.circuit, Default::default()).unwrap(),
                _ => fleece::export::svg(&document.circuit, Default::default())
                    .unwrap()
                    .into_bytes(),
            };
            assert_eq!(bytes, expected);
        }
        assert_eq!(std::fs::read(input).unwrap(), before);
    }
}

#[test]
fn invalid_arguments_or_documents_preserve_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("bad.toml");
    let output = directory.path().join("image.png");
    std::fs::write(&input, "invalid document").unwrap();
    std::fs::write(&output, "existing image").unwrap();
    for arguments in [
        vec![input.as_os_str(), "-o".as_ref(), output.as_os_str()],
        vec!["-o".as_ref(), output.as_os_str()],
        vec![input.as_os_str(), "-o".as_ref()],
        vec!["new".as_ref(), input.as_os_str()],
        vec!["--theme".as_ref(), "vscode".as_ref()],
    ] {
        let result = run(&arguments);
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).starts_with("FLEECE: "));
        assert_eq!(std::fs::read(&output).unwrap(), b"existing image");
        assert_eq!(std::fs::read(&input).unwrap(), b"invalid document");
    }
    let result = run(&["--help".as_ref()]);
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("-o"));
}
