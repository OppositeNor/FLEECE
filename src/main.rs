//! Open the native editor or export a document with `-o`.
#[cfg(not(target_arch = "wasm32"))]
mod desktop;
#[cfg(not(target_arch = "wasm32"))]
mod launch;

#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), String> {
    match launch::parse(std::env::args_os().skip(1))? {
        launch::Launch::Desktop(path) => desktop::run(path),
        launch::Launch::Export { input, output } => {
            let document = fleece::model::Document::load(&input)?;
            fleece::export::save(
                &document.circuit,
                &output,
                fleece::export::ExportOptions::default(),
            )
        }
        launch::Launch::Help => {
            print!("{}", launch::HELP);
            Ok(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("FLEECE: {error}");
        std::process::exit(1);
    }
}

// Browser startup is exported by the library cdylib.
#[cfg(target_arch = "wasm32")]
fn main() {}
