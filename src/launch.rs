//! Minimal desktop launch and static image-output arguments.
use std::{ffi::OsString, path::PathBuf};

pub(super) const HELP: &str = "FLEECE

Usage: fleece [DOCUMENT.toml] [-o IMAGE.png|IMAGE.svg]

  DOCUMENT.toml         Open a saved logic circuit or FSM
  -o, --output IMAGE    Export the document without opening a window
  -h, --help            Show this help
  --                    Treat remaining arguments as document paths

Without arguments, open an empty logic circuit.
Image format follows the output extension. Existing output images are replaced.
";

pub(super) enum Launch {
    Desktop(Option<PathBuf>),
    Export { input: PathBuf, output: PathBuf },
    Help,
}

pub(super) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Launch, String> {
    let mut arguments = arguments.into_iter();
    let mut input = None;
    let mut output = None;
    let mut positional = false;
    while let Some(argument) = arguments.next() {
        if !positional && argument == "--" {
            positional = true;
        } else if !positional && (argument == "-h" || argument == "--help") {
            return Ok(Launch::Help);
        } else if !positional && (argument == "-o" || argument == "--output") {
            if output.is_some() {
                return Err("Specify the output path only once.".into());
            }
            let path = arguments
                .next()
                .ok_or("Expected an image path after -o/--output.")?;
            if path.is_empty() || path.to_string_lossy().starts_with('-') {
                return Err(
                    "Expected an image path after -o/--output; use ./ for paths starting with '-'."
                        .into(),
                );
            }
            output = Some(PathBuf::from(path));
        } else if !positional && argument.to_string_lossy().starts_with('-') {
            return Err(format!(
                "Unknown option: {}. Use --help for usage.",
                argument.to_string_lossy()
            ));
        } else if argument.is_empty() || input.replace(PathBuf::from(argument)).is_some() {
            return Err("Expected one document path. Use --help for usage.".into());
        }
    }
    match (input, output) {
        (Some(input), Some(output)) => Ok(Launch::Export { input, output }),
        (None, Some(_)) => Err("An input document is required with -o/--output.".into()),
        (input, None) => Ok(Launch::Desktop(input)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Result<Launch, String> {
        parse(items.iter().map(OsString::from))
    }

    #[test]
    fn opens_documents_and_exports_with_options_before_or_after_the_input() {
        assert!(matches!(args(&[]).unwrap(), Launch::Desktop(None)));
        assert!(
            matches!(args(&["diagram.toml"]).unwrap(), Launch::Desktop(Some(path)) if path == std::path::Path::new("diagram.toml"))
        );
        for arguments in [
            vec!["diagram.toml", "-o", "image.png"],
            vec!["-o", "image.png", "diagram.toml"],
            vec!["diagram.toml", "--output", "image.png"],
        ] {
            assert!(
                matches!(args(&arguments).unwrap(), Launch::Export { input, output } if input == std::path::Path::new("diagram.toml") && output == std::path::Path::new("image.png"))
            );
        }
        assert!(
            matches!(args(&["--", "-diagram.toml"]).unwrap(), Launch::Desktop(Some(path)) if path == std::path::Path::new("-diagram.toml"))
        );
    }

    #[test]
    fn rejects_incomplete_or_ambiguous_arguments() {
        for arguments in [
            vec!["-o"],
            vec!["-o", "image.png"],
            vec!["diagram.toml", "-o", "image.png", "-o", "other.svg"],
            vec!["one.toml", "two.toml"],
            vec!["--theme", "vscode"],
            vec!["diagram.toml", "-o", "--output"],
        ] {
            assert!(args(&arguments).is_err(), "{arguments:?}");
        }
    }
}
