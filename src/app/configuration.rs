//! Theme configuration and managed theme files, separate from file history.
//!
//! Native themes live next to `config.toml` in `themes/`. Built-in templates are
//! installed only when absent, preserving user edits. Custom themes are validated
//! before import; a filename collision chooses a new name instead of overwriting
//! another theme. Only a successful load/write changes the active config path.
use super::*;
use crate::model::atomic_write;
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

/// User-editable configuration. `theme` is the active TOML file's path.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    /// Path to the selected theme TOML file; relative paths resolve beside `config.toml`.
    pub theme: PathBuf,
}
impl ThemeConfig {
    /// Parse at most 16 KiB. Relative theme paths resolve beside this config file.
    ///
    /// # Errors
    ///
    /// Returns an error for missing/unreadable files, input over 16 KiB, malformed
    /// TOML or unknown config fields.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut text = String::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(16 * 1024 + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > 16 * 1024 {
            return Err("Config file exceeds 16 KiB.".into());
        }
        let mut config: Self =
            toml::from_str(&text).map_err(|e| format!("Invalid config TOML: {e}"))?;
        if config.theme.is_relative() {
            config.theme = path.parent().unwrap_or(Path::new(".")).join(&config.theme);
        }
        Ok(config)
    }
    fn save(&self, path: &Path) -> Result<(), String> {
        atomic_write(
            path,
            toml::to_string_pretty(self)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
    }
}

/// Resolve the native config path without creating directories or reading files.
pub fn config_path() -> Option<PathBuf> {
    use preferences::absolute_env;
    let root = if cfg!(target_os = "windows") {
        absolute_env("APPDATA")
    } else if cfg!(target_os = "macos") {
        absolute_env("HOME").map(|p| p.join("Library/Application Support"))
    } else {
        absolute_env("XDG_CONFIG_HOME").or_else(|| absolute_env("HOME").map(|p| p.join(".config")))
    };
    root.map(|p| p.join("fleece/config.toml"))
}

pub(super) struct ThemeStore {
    path: PathBuf,
}
impl ThemeStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self { path }
    }
    fn themes_dir(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).join("themes")
    }
    fn install_templates(&self) -> Result<(), String> {
        let directory = self.themes_dir();
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        for (key, _) in theme::BUILT_INS {
            let path = directory.join(format!("{key}.toml"));
            if !path.exists() {
                atomic_write(
                    &path,
                    theme::builtin_source(key)
                        .expect("Built-in key has a template")
                        .as_bytes(),
                )?;
            }
        }
        Ok(())
    }
    /// Load an existing choice, or install a default when config is absent.
    pub(super) fn initialize(&self, fallback: &str) -> Result<(String, Theme), String> {
        self.install_templates()?;
        if self.path.exists() {
            let config = ThemeConfig::load(&self.path)?;
            let selected = self.prepare(&config.theme.to_string_lossy())?;
            if Path::new(&selected.0) != config.theme {
                ThemeConfig {
                    theme: selected.0.clone().into(),
                }
                .save(&self.path)?;
            }
            Ok(selected)
        } else {
            self.select(fallback)
        }
    }
    /// Validate/import the file before atomically recording the new choice.
    pub(super) fn select(&self, reference: &str) -> Result<(String, Theme), String> {
        self.install_templates()?;
        let selected = self.prepare(reference)?;
        ThemeConfig {
            theme: selected.0.clone().into(),
        }
        .save(&self.path)?;
        Ok(selected)
    }
    fn prepare(&self, reference: &str) -> Result<(String, Theme), String> {
        let directory = std::fs::canonicalize(self.themes_dir()).map_err(|e| e.to_string())?;
        let source = if theme::builtin_source(reference).is_some() {
            directory.join(format!("{reference}.toml"))
        } else {
            std::fs::canonicalize(reference).map_err(|e| e.to_string())?
        };
        let text = Theme::read_source(&source)?;
        let palette = Theme::from_toml(&text)?;
        let path = if source.starts_with(&directory) {
            source
        } else {
            self.import(&directory, &source, &text)?
        };
        Ok((path.to_string_lossy().into_owned(), palette))
    }
    fn import(&self, directory: &Path, source: &Path, text: &str) -> Result<PathBuf, String> {
        let stem = source
            .file_stem()
            .ok_or("Choose a theme file.")?
            .to_string_lossy();
        for suffix in 0..=1000 {
            let name = if suffix == 0 {
                format!("{stem}.toml")
            } else {
                format!("{stem}-{suffix}.toml")
            };
            let target = directory.join(name);
            if !target.exists() {
                atomic_write(&target, text.as_bytes())?;
                return Ok(target);
            }
            if std::fs::read_to_string(&target).ok().as_deref() == Some(text) {
                return Ok(target);
            }
        }
        Err("Too many themes with the same file name.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_config_are_created_without_overwriting_user_theme_edits() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("fleece/config.toml");
        let store = ThemeStore::new(config.clone());
        let (reference, _) = store.initialize("original-dark").unwrap();
        assert_eq!(
            Path::new(&reference),
            config.parent().unwrap().join("themes/original-dark.toml")
        );
        for (key, _) in theme::BUILT_INS {
            assert!(
                config
                    .parent()
                    .unwrap()
                    .join(format!("themes/{key}.toml"))
                    .exists()
            );
        }
        let custom = "theme_version=1\nname='Personal dark'\nbase='dark'\n";
        std::fs::write(&reference, custom).unwrap();
        assert_eq!(store.initialize("vscode").unwrap().1.name, "Personal dark");
        assert_eq!(
            store.select("original-dark").unwrap().1.name,
            "Personal dark"
        );
        assert_eq!(std::fs::read_to_string(&reference).unwrap(), custom);
        assert_eq!(
            ThemeConfig::load(&config).unwrap().theme,
            PathBuf::from(reference)
        );
    }

    #[test]
    fn custom_imports_preserve_existing_themes_and_reuse_identical_copies() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config/config.toml");
        let store = ThemeStore::new(config.clone());
        store.initialize("original-dark").unwrap();
        let source = dir.path().join("original-dark.toml");
        std::fs::write(&source, "theme_version=1\nname='Custom'\nbase='light'\n").unwrap();
        let (imported, palette) = store.select(&source.to_string_lossy()).unwrap();
        assert!(imported.ends_with("original-dark-1.toml"));
        assert_eq!(palette.name, "Custom");
        assert_eq!(store.select(&source.to_string_lossy()).unwrap().0, imported);
        let original = store.themes_dir().join("original-dark.toml");
        assert_eq!(Theme::load(&original).unwrap().name, "Original dark");
        let before = std::fs::read(&config).unwrap();
        std::fs::write(&source, "invalid TOML [").unwrap();
        assert!(store.select(&source.to_string_lossy()).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert_eq!(Theme::load(Path::new(&imported)).unwrap().name, "Custom");
    }

    #[test]
    fn relative_paths_load_and_invalid_config_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let store = ThemeStore::new(config.clone());
        store.initialize("original-dark").unwrap();
        std::fs::write(&config, "theme='themes/tokyo-night.toml'\n").unwrap();
        assert_eq!(
            store.initialize("original-dark").unwrap().1.name,
            "Tokyo Night"
        );
        let invalid = "theme='themes/tokyo-night.toml'\nunknown=true\n";
        std::fs::write(&config, invalid).unwrap();
        assert!(store.initialize("original-dark").is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), invalid);
    }
}
