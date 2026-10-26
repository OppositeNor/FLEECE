//! Persistent save/export history, separate from theme configuration.
//!
//! Native hosts resolve an OS state directory; embedded editors/tests inject a
//! path or keep preferences in memory. Writes create parent directories and replace the
//! state file atomically.
//!
//! Remember file paths/options only after successful save/export. Theme choices
//! persist immediately after a successful load. Constructing dialogs or cancelling
//! operations must not replace remembered values. Loading a preference never writes
//! it back merely because an editor was constructed.

use super::*;
use crate::model::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Native application state, separate from saved circuit/FSM documents.
/// Optional fields keep earlier state files readable as new preferences are added.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    /// Absolute path of the last successfully saved document, if any.
    #[serde(default)]
    pub last_save_path: Option<PathBuf>,
    /// Absolute path of the last successful image export, including its extension.
    #[serde(default)]
    pub last_export_path: Option<PathBuf>,
    /// Options from the last successful export, restored for future export dialogs.
    #[serde(default)]
    pub export_options: ExportOptions,
}
impl Preferences {
    /// Load bounded TOML state, defaulting on missing/malformed files. Invalid
    /// export options reset independently so remembered paths are preserved.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .filter(|s| s.len() <= 16 * 1024)
            .and_then(|s| toml::from_str(&s).ok())
            .map(|mut state: Self| {
                let options = state.export_options;
                if !(1..=6).contains(&options.scale)
                    || !options.padding.is_finite()
                    || !(0.0..=200.0).contains(&options.padding)
                {
                    state.export_options = ExportOptions::default();
                }
                state
            })
            .unwrap_or_default()
    }
    /// Create the state directory if necessary, then atomically replace the file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization, parent-directory creation, or atomic file
    /// replacement fails.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        atomic_write(
            path,
            toml::to_string_pretty(self)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
    }
}
pub(super) fn absolute_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

fn xdg_state_root(state: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    state.filter(|p| p.is_absolute()).or_else(|| {
        home.filter(|p| p.is_absolute())
            .map(|p| p.join(".local/state"))
    })
}

fn state_root() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        absolute_env("LOCALAPPDATA").or_else(|| absolute_env("APPDATA"))
    } else if cfg!(target_os = "macos") {
        absolute_env("HOME").map(|p| p.join("Library/Application Support"))
    } else {
        xdg_state_root(absolute_env("XDG_STATE_HOME"), absolute_env("HOME"))
    }
}

/// Recent file locations are runtime state, rather than document data or cache.
/// Respect XDG_STATE_HOME on Unix and the corresponding native locations elsewhere.
pub fn state_path() -> Option<PathBuf> {
    state_root().map(|p| p.join("fleece/state.toml"))
}

impl Editor {
    /// Native hosts persist history. Ordinary library editors stay isolated.
    pub fn with_native_preferences(document: Document, path: Option<PathBuf>) -> Self {
        let mut editor = Self::with_state_paths(document, path, state_path());
        editor.initialize_theme_store(configuration::config_path());
        editor
    }

    /// Inject a state file without touching native paths, useful for embedded
    /// editors and tests. A provided path gets a sibling `config.toml` and managed
    /// `themes/` directory; use [`Self::with_storage_paths`] for separate locations.
    /// Passing `None` keeps all state in memory.
    pub fn with_preferences_path(
        document: Document,
        path: Option<PathBuf>,
        preferences_path: Option<PathBuf>,
    ) -> Self {
        let config_path = preferences_path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.join("config.toml"));
        Self::with_storage_paths(document, path, preferences_path, config_path)
    }
    /// Inject separate history and theme-config files without reading native paths.
    pub fn with_storage_paths(
        document: Document,
        path: Option<PathBuf>,
        state_path: Option<PathBuf>,
        config_path: Option<PathBuf>,
    ) -> Self {
        let mut editor = Self::with_state_paths(document, path, state_path);
        editor.initialize_theme_store(config_path);
        editor
    }
    fn initialize_theme_store(&mut self, path: Option<PathBuf>) {
        let Some(path) = path else {
            return;
        };
        let store = configuration::ThemeStore::new(path);
        let fallback = self.theme_reference();
        match store.initialize(&fallback) {
            Ok((reference, theme)) => {
                self.document.editor.theme = Some(reference.clone());
                self.theme_key = reference.clone();
                self.remembered_theme = Some(reference);
                self.theme = theme;
            }
            Err(error) => self.status = format!("Cannot load theme configuration: {error}"),
        }
        self.theme_store = Some(store);
    }
    fn with_state_paths(
        document: Document,
        path: Option<PathBuf>,
        state_path: Option<PathBuf>,
    ) -> Self {
        let mut editor = Self::new(document, path);
        if let Some(path) = &state_path {
            editor.preferences = Preferences::load(path);
            editor.export_options = editor.preferences.export_options;
        }
        editor.preferences_path = state_path;
        editor.apply_remembered_theme();
        editor
    }
    pub(super) fn apply_remembered_theme(&mut self) {
        let Some(reference) = self.remembered_theme.clone() else {
            return;
        };
        if reference == self.theme_key {
            self.document.editor.theme = Some(reference);
            return;
        }
        match Theme::load_reference(&reference) {
            Ok(theme) => {
                self.document.editor.theme = Some(reference.clone());
                self.theme_key = reference;
                self.theme = theme;
            }
            Err(error) => self.status = format!("Cannot load remembered theme: {error}"),
        }
    }
    pub(super) fn save_dialog(&self) -> FileDialog {
        if let Some(path) = &self.path {
            return FileDialog::new(FileMode::Save, Some(path));
        }
        let mut dialog = FileDialog::new(FileMode::Save, self.preferences.last_save_path.as_ref());
        dialog.name = if self.circuit().fsm.is_some() {
            "fsm.toml"
        } else {
            "circuit.toml"
        }
        .into();
        dialog
    }
    pub(super) fn export_dialog(&self) -> FileDialog {
        if let Some(path) = &self.preferences.last_export_path {
            return FileDialog::new(FileMode::Export, Some(path));
        }
        let mut dialog = FileDialog::new(
            FileMode::Export,
            self.path
                .as_ref()
                .or(self.preferences.last_save_path.as_ref()),
        );
        dialog.name = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|stem| format!("{}.png", stem.to_string_lossy()))
            .unwrap_or_else(|| {
                if self.circuit().fsm.is_some() {
                    "fsm.png"
                } else {
                    "circuit.png"
                }
                .into()
            });
        dialog
    }
    pub(super) fn persist_preferences(&self) {
        if let Some(path) = &self.preferences_path {
            let _ = self.preferences.save(path);
        }
    }
    pub(super) fn remember_save_path(&mut self, path: &Path) {
        self.preferences.last_save_path = Some(absolute_path(path));
        self.persist_preferences();
    }
    pub(super) fn export_image(&mut self, path: &Path) -> Result<(), String> {
        self.finish_edit();
        self.finish_drag();

        export::save(self.circuit(), path, self.export_options)?;
        self.preferences.last_export_path = Some(absolute_path(path));
        self.preferences.export_options = self.export_options;
        self.persist_preferences();
        Ok(())
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::Editor,
        model::{Circuit, Document},
    };

    #[test]
    fn save_location_survives_new_documents_and_a_new_editor_session() {
        let dir = tempfile::tempdir().unwrap();
        let drawings = dir.path().join("drawings");
        std::fs::create_dir(&drawings).unwrap();
        let saved = drawings.join("saved.toml");
        let settings = dir.path().join("config/preferences.toml");
        let mut editor = Editor::with_preferences_path(
            Document::new(Circuit::empty()),
            Some(saved.clone()),
            Some(settings.clone()),
        );
        editor.save();
        assert!(saved.exists());
        assert_eq!(
            Preferences::load(&settings).last_save_path.as_deref(),
            Some(saved.as_path())
        );
        editor.new_fsm();
        let dialog = editor.save_dialog();
        assert_eq!(dialog.directory, drawings);
        assert_eq!(dialog.name, "fsm.toml");
        let fresh = Editor::with_preferences_path(
            Document::new(Circuit::empty()),
            None,
            Some(settings.clone()),
        );
        assert_eq!(fresh.save_dialog().directory, drawings);
        assert_eq!(fresh.save_dialog().name, "circuit.toml");
        // An open file takes precedence over the remembered location.
        let current = dir.path().join("current.toml");
        let current_editor = Editor::with_preferences_path(
            Document::new(Circuit::empty()),
            Some(current.clone()),
            Some(settings),
        );
        assert_eq!(current_editor.save_dialog().directory, dir.path());
        assert_eq!(current_editor.save_dialog().name, "current.toml");
    }

    #[test]
    fn unsuccessful_saves_and_dialog_creation_do_not_change_the_remembered_path() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("preferences.toml");
        let remembered = dir.path().join("previous.toml");
        Preferences {
            last_save_path: Some(remembered.clone()),
            ..Preferences::default()
        }
        .save(&settings)
        .unwrap();
        let mut editor = Editor::with_preferences_path(
            Document::new(Circuit::empty()),
            Some(dir.path().join("missing/file.toml")),
            Some(settings.clone()),
        );
        let _dialog = editor.save_dialog();
        editor.save();
        assert_eq!(
            Preferences::load(&settings).last_save_path,
            Some(remembered)
        );
    }
    #[test]
    fn save_and_export_history_and_export_settings_survive_restart_independently() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state/fleece/state.toml");
        let saved = dir.path().join("circuit.toml");
        let images = dir.path().join("images");
        std::fs::create_dir(&images).unwrap();
        let image = images.join("diagram.svg");
        let mut editor = Editor::with_preferences_path(
            Document::new(Circuit::half_adder()),
            Some(saved.clone()),
            Some(state.clone()),
        );
        assert!(!state.exists());
        editor.save();
        editor.export_options = ExportOptions {
            scale: 4,
            transparent: true,
            title: true,
            padding: 40.0,
        };
        editor.export_image(&image).unwrap();
        assert!(image.exists());
        let stored = Preferences::load(&state);
        assert_eq!(stored.last_save_path, Some(saved.clone()));
        assert_eq!(stored.last_export_path, Some(image));
        assert_eq!(stored.export_options, editor.export_options);
        let reopened = Editor::with_preferences_path(
            Document::new(Circuit::new_fsm()),
            None,
            Some(state.clone()),
        );
        assert_eq!(reopened.export_options, editor.export_options);
        assert_eq!(reopened.export_dialog().directory, images);
        assert_eq!(reopened.export_dialog().name, "diagram.svg");
        assert_eq!(reopened.save_dialog().directory, dir.path());
        assert_eq!(reopened.save_dialog().name, "fsm.toml");
        // Saving later cannot replace the last successful export settings.
        editor.export_options.scale = 2;
        editor.save();
        assert_eq!(
            Preferences::load(&state).export_options,
            stored.export_options
        );
    }

    #[test]
    fn failed_exports_and_unconfirmed_export_settings_do_not_replace_history() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.toml");
        let image = dir.path().join("previous.svg");
        let mut editor = Editor::with_preferences_path(
            Document::new(Circuit::half_adder()),
            None,
            Some(state.clone()),
        );
        editor.export_image(&image).unwrap();
        let original = std::fs::read(&state).unwrap();
        editor.export_options.transparent = true;
        let _dialog = editor.export_dialog();
        assert_eq!(std::fs::read(&state).unwrap(), original);
        assert!(
            editor
                .export_image(&dir.path().join("missing/failure.svg"))
                .is_err()
        );
        assert_eq!(std::fs::read(&state).unwrap(), original);
        assert_eq!(editor.preferences.last_export_path, Some(image));
        editor.export_options.scale = 0;
        assert!(
            editor
                .export_image(&dir.path().join("invalid.svg"))
                .is_err()
        );
        assert_eq!(std::fs::read(&state).unwrap(), original);
        assert!(!dir.path().join("invalid.svg").exists());
    }

    #[test]
    fn xdg_state_paths_respect_absolute_overrides_and_ignore_relative_values() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let custom = dir.path().join("custom-state");
        assert_eq!(
            xdg_state_root(None, Some(home.clone())),
            Some(home.join(".local/state"))
        );
        assert_eq!(
            xdg_state_root(Some(custom.clone()), Some(home.clone())),
            Some(custom.clone())
        );
        assert_eq!(xdg_state_root(Some(custom.clone()), None), Some(custom));
        assert_eq!(
            xdg_state_root(Some(PathBuf::from("relative")), Some(home.clone())),
            Some(home.join(".local/state"))
        );
        assert_eq!(
            xdg_state_root(Some(PathBuf::from("")), Some(home.clone())),
            Some(home.join(".local/state"))
        );
        assert_eq!(xdg_state_root(None, None), None);
    }

    #[test]
    fn invalid_export_options_fall_back_without_losing_saved_locations() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.toml");
        let saved = dir.path().join("saved.toml");
        let invalid = Preferences {
            last_save_path: Some(saved.clone()),
            export_options: ExportOptions {
                scale: 99,
                ..ExportOptions::default()
            },
            ..Preferences::default()
        };
        invalid.save(&state).unwrap();
        let loaded = Preferences::load(&state);
        assert_eq!(loaded.last_save_path, Some(saved));
        assert_eq!(loaded.export_options, ExportOptions::default());
    }
    #[test]
    fn successful_theme_choices_persist_in_config_across_new_documents_and_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state/state.toml");
        let config = dir.path().join("config/fleece/config.toml");
        let mut editor = Editor::with_storage_paths(
            Document::new(Circuit::empty()),
            None,
            Some(state.clone()),
            Some(config.clone()),
        );
        assert_eq!(editor.active_theme(), &Theme::original(true));
        assert!(!state.exists());
        for key in ["vscode", "tokyo-night", "original-light", "original-dark"] {
            editor.select_theme(key).unwrap();
            let selected = configuration::ThemeConfig::load(&config).unwrap().theme;
            assert_eq!(
                selected,
                config.parent().unwrap().join(format!("themes/{key}.toml"))
            );
            editor.new_fsm();
            assert_eq!(editor.active_theme(), &Theme::load(&selected).unwrap());
            assert_eq!(editor.document.editor.theme.as_deref(), selected.to_str());
            editor.new_logic_circuit();
            assert_eq!(editor.document.editor.theme.as_deref(), selected.to_str());
            let reopened = Editor::with_storage_paths(
                Document::new(Circuit::empty()),
                None,
                Some(state.clone()),
                Some(config.clone()),
            );
            assert_eq!(reopened.active_theme(), editor.active_theme());
        }
        let bytes = std::fs::read(&config).unwrap();
        let palette = editor.active_theme().clone();
        assert!(
            editor
                .select_theme(&dir.path().join("missing-theme.toml").to_string_lossy())
                .is_err()
        );
        assert_eq!(editor.active_theme(), &palette);
        assert_eq!(std::fs::read(&config).unwrap(), bytes);
        assert!(
            !state.exists(),
            "Theme choices must not be written to runtime history"
        );
    }

    #[test]
    fn custom_themes_are_imported_and_missing_managed_themes_preserve_config() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.toml");
        let config = dir.path().join("config.toml");
        let source = dir.path().join("custom.toml");
        std::fs::write(&source, "theme_version=1\nname='Custom'\nbase='light'\n").unwrap();
        let mut editor = Editor::with_preferences_path(
            Document::new(Circuit::empty()),
            None,
            Some(state.clone()),
        );
        editor.select_theme(&source.to_string_lossy()).unwrap();
        let managed = configuration::ThemeConfig::load(&config).unwrap().theme;
        assert_eq!(managed, dir.path().join("themes/custom.toml"));
        assert_eq!(
            std::fs::read(&managed).unwrap(),
            std::fs::read(&source).unwrap()
        );
        std::fs::remove_file(&source).unwrap();
        let reopened = Editor::with_preferences_path(
            Document::new(Circuit::empty()),
            None,
            Some(state.clone()),
        );
        assert_eq!(reopened.active_theme().name, "Custom");
        let bytes = std::fs::read(&config).unwrap();
        std::fs::remove_file(&managed).unwrap();
        let reopened =
            Editor::with_preferences_path(Document::new(Circuit::empty()), None, Some(state));
        assert_eq!(reopened.active_theme(), &Theme::original(true));
        assert_eq!(std::fs::read(&config).unwrap(), bytes);
    }
}
