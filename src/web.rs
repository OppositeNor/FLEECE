//! Browser host integration: async graphics, file transfers and local preferences.
#[path = "desktop.rs"]
mod host;
use crate::{
    app::{Editor, FileRequest},
    model::{Circuit, Document},
};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, prelude::*};

#[wasm_bindgen(module = "/web/platform.js")]
extern "C" {
    fn install(callback: &js_sys::Function);
    fn pick_file(theme: bool);
    #[wasm_bindgen(catch)]
    fn download(name: &str, mime: &str, bytes: &[u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn load_settings() -> Result<Option<String>, JsValue>;
    #[wasm_bindgen(catch)]
    fn save_settings(text: &str) -> Result<(), JsValue>;
    fn update_page(title: &str, dirty: bool);
    fn show_ready();
    fn show_error(error: &str);
    pub(super) fn is_macos() -> bool;
    pub(super) fn copy(text: &str);
}

thread_local! {
    static EVENTS: RefCell<Vec<(String, String, String)>> = const { RefCell::new(Vec::new()) };
    static SETTINGS: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Start the shared editor in the page's `fleece-canvas` element.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(|info| fatal(&info.to_string())));
    let callback = Closure::<dyn Fn(String, String, String)>::new(|kind, name, text| {
        EVENTS.with_borrow_mut(|events| events.push((kind, name, text)));
    });
    install(callback.as_ref().unchecked_ref());
    callback.forget(); // The page owns these listeners for its entire lifetime.
    let mut editor = Editor::with_external_files(Document::new(Circuit::empty()));
    match load_settings() {
        Ok(Some(text)) => {
            if let Err(error) = editor.restore_external_settings(&text) {
                editor.report_host_error(&format!("Cannot restore browser preferences: {error}"));
            }
        }
        Ok(None) => {}
        Err(_) => {
            editor.report_host_error("Browser storage unavailable; preferences remain in memory.")
        }
    }
    SETTINGS.with_borrow_mut(|cached| *cached = editor.external_settings().ok());
    host::run_web(editor).map_err(|error| {
        fatal(&error);
        JsValue::from_str(&error)
    })
}

pub(super) fn canvas() -> Result<web_sys::HtmlCanvasElement, String> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("fleece-canvas"))
        .and_then(|element| element.dyn_into().ok())
        .ok_or("Missing fleece-canvas element.".into())
}
pub(super) fn ready() {
    show_ready();
}
pub(super) fn fatal(error: &str) {
    show_error(error);
    web_sys::console::error_1(&error.into());
}

pub(super) fn receive(editor: &mut Editor, input: &mut egui::RawInput) {
    for (kind, name, text) in EVENTS.with_borrow_mut(std::mem::take) {
        let result = match kind.as_str() {
            "document" => editor.import_document(&name, &text),
            "theme" => editor.import_theme(&text),
            "paste" => {
                input.events.push(egui::Event::Paste(text));
                Ok(())
            }
            _ => Err(text),
        };
        if let Err(error) = result {
            editor.report_host_error(&error);
        }
    }
}

pub(super) fn transfers(editor: &mut Editor) {
    for request in editor.take_file_requests() {
        match request {
            FileRequest::OpenDocument => pick_file(false),
            FileRequest::OpenTheme => pick_file(true),
            FileRequest::Download {
                id,
                name,
                mime,
                bytes,
            } => {
                let result = download(&name, mime, &bytes).map_err(|error| format!("{error:?}"));
                editor.finish_download(id, result);
            }
        }
    }
    update_page(&editor.title(), editor.dirty());
    if let Ok(text) = editor.external_settings() {
        SETTINGS.with_borrow_mut(|cached| {
            if cached.as_ref() != Some(&text) {
                if save_settings(&text).is_err() {
                    editor.report_host_error(
                        "Browser storage unavailable; preferences remain in memory.",
                    );
                }
                *cached = Some(text);
            }
        });
    }
}
