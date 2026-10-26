//! UI theme files. These palettes never participate in document export.
use egui::{Color32, Stroke};
use serde::Deserialize;
use std::{collections::BTreeMap, io::Read, path::Path};

/// Built-in palette keys and human-readable names in theme-menu order.
pub const BUILT_INS: [(&str, &str); 4] = [
    ("vscode", "VSCode"),
    ("original-dark", "Original dark"),
    ("original-light", "Original light"),
    ("tokyo-night", "Tokyo Night"),
];

/// Straight-alpha RGBA color parsed from `#RRGGBB` or `#RRGGBBAA` theme text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Color(
    /// Red, green, blue and alpha channels, each ranging from 0 to 255.
    pub [u8; 4],
);

impl Color {
    /// Convert the stored straight-alpha channels to an egui color.
    pub fn egui(self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

impl From<Color32> for Color {
    fn from(value: Color32) -> Self {
        Self(value.to_srgba_unmultiplied())
    }
}

impl TryFrom<String> for Color {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let hex = value.strip_prefix('#').ok_or("Colors must start with #.")?;
        if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "Invalid color {value:?}; use #RRGGBB or #RRGGBBAA."
            ));
        }
        let mut rgba = [0, 0, 0, 255];
        for (index, byte) in rgba.iter_mut().enumerate().take(hex.len() / 2) {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|e| e.to_string())?;
        }
        Ok(Self(rgba))
    }
}

/// Fallback light/dark palette inherited by omitted theme fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base {
    /// Dark controls, canvas and component bodies.
    #[default]
    Dark,
    /// Light controls and canvas.
    Light,
}

/// Resolved colors for native editor panels and widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiPalette {
    /// Side-panel background.
    pub panel: Color,
    /// Window and modal background.
    pub window: Color,
    /// Text-input background.
    pub input: Color,
    /// Idle button background.
    pub button: Color,
    /// Hovered widget background.
    pub hover: Color,
    /// Pressed/active widget background.
    pub active: Color,
    /// Primary text color.
    pub text: Color,
    /// Secondary and helper text color.
    pub muted: Color,
    /// Widget and panel border color.
    pub border: Color,
    /// Accent/link color.
    pub accent: Color,
    /// Selected-widget fill color.
    pub selection: Color,
    /// Warning message color.
    pub warning: Color,
    /// Error message color.
    pub error: Color,
}

/// Resolved interactive canvas colors; static exports use document colors instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanvasPalette {
    /// Canvas background fill.
    pub background: Color,
    /// Grid point color.
    pub grid: Color,
    /// Component label color.
    pub label: Color,
    /// Component body fill color.
    pub gate_fill: Color,
    /// Component outlines and default neutral wire ink.
    pub gate_outline: Color,
    /// Selection outlines, handles and unfinished-wire preview color.
    pub selection: Color,
}

/// Optional editor-only sizing overrides validated when loading a theme.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metrics {
    /// Proportional UI font size, from 10 through 24; `None` keeps the default.
    pub font_size: Option<f32>,
    /// Widget corner radius, from 0 through 24; `None` keeps the default.
    pub corner_radius: Option<u8>,
}

/// Resolved UI palette and optional metrics. It does not affect exported diagrams.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// Human-readable theme name shown by the editor.
    pub name: String,
    /// Base light/dark palette used for omitted colors and widget defaults.
    pub base: Base,
    /// Resolved widget and panel colors.
    pub ui: UiPalette,
    /// Resolved editor canvas colors.
    pub canvas: CanvasPalette,
    /// Optional editor sizing overrides; static export sizes are unaffected.
    pub metrics: Metrics,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    #[serde(default = "version_one")]
    theme_version: u32,
    name: String,
    #[serde(default)]
    base: Base,
    #[serde(default)]
    ui: BTreeMap<String, Color>,
    #[serde(default)]
    canvas: BTreeMap<String, Color>,
    #[serde(default)]
    metrics: Metrics,
}
fn version_one() -> u32 {
    1
}

impl Theme {
    /// Construct the original dark/light palette without reading external files.
    pub fn original(dark: bool) -> Self {
        let visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let accent = Color([71, 210, 171, 255]);
        Self {
            name: if dark {
                "Original dark"
            } else {
                "Original light"
            }
            .into(),
            base: if dark { Base::Dark } else { Base::Light },
            ui: UiPalette {
                panel: visuals.panel_fill.into(),
                window: visuals.window_fill.into(),
                input: visuals.extreme_bg_color.into(),
                button: visuals.widgets.inactive.bg_fill.into(),
                hover: visuals.widgets.hovered.bg_fill.into(),
                active: visuals.widgets.active.bg_fill.into(),
                text: visuals.text_color().into(),
                muted: visuals.weak_text_color().into(),
                border: visuals.widgets.noninteractive.bg_stroke.color.into(),
                accent,
                selection: visuals.selection.bg_fill.into(),
                warning: visuals.warn_fg_color.into(),
                error: visuals.error_fg_color.into(),
            },
            canvas: CanvasPalette {
                background: Color(if dark {
                    [20, 26, 36, 255]
                } else {
                    [245, 248, 251, 255]
                }),
                grid: Color(if dark {
                    [47, 57, 73, 255]
                } else {
                    [210, 218, 227, 255]
                }),
                label: Color(if dark {
                    [184, 199, 221, 255]
                } else {
                    [32, 42, 58, 255]
                }),
                gate_fill: Color(if dark {
                    [32, 42, 58, 255]
                } else {
                    [255, 255, 255, 255]
                }),
                gate_outline: Color(if dark {
                    [184, 199, 221, 255]
                } else {
                    [32, 42, 58, 255]
                }),
                selection: accent,
            },
            metrics: Metrics::default(),
        }
    }

    /// Parse a strict theme definition and inherit omitted colors from its base.
    /// Rejects unsupported versions, unknown keys and out-of-range color/metric values.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed TOML, unknown fields, unsupported theme
    /// versions, invalid colors or out-of-range metrics.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let file: ThemeFile =
            toml::from_str(text).map_err(|error| format!("Invalid theme TOML: {error}"))?;
        if file.theme_version != 1 {
            return Err("Unsupported theme_version (expected 1).".into());
        }
        if file.name.trim().is_empty()
            || file.name.chars().count() > 80
            || file.name.chars().any(char::is_control)
        {
            return Err("Theme name must contain 1–80 printable characters.".into());
        }
        if file
            .metrics
            .font_size
            .is_some_and(|size| !size.is_finite() || !(10.0..=24.0).contains(&size))
        {
            return Err("metrics.font_size must be between 10 and 24.".into());
        }
        if file.metrics.corner_radius.is_some_and(|radius| radius > 24) {
            return Err("metrics.corner_radius must be between 0 and 24.".into());
        }
        let mut theme = Self::original(file.base == Base::Dark);
        theme.name = file.name;
        theme.metrics = file.metrics;
        for (name, color) in file.ui {
            let target = match name.as_str() {
                "panel" => &mut theme.ui.panel,
                "window" => &mut theme.ui.window,
                "input" => &mut theme.ui.input,
                "button" => &mut theme.ui.button,
                "hover" => &mut theme.ui.hover,
                "active" => &mut theme.ui.active,
                "text" => &mut theme.ui.text,
                "muted" => &mut theme.ui.muted,
                "border" => &mut theme.ui.border,
                "accent" => &mut theme.ui.accent,
                "selection" => &mut theme.ui.selection,
                "warning" => &mut theme.ui.warning,
                "error" => &mut theme.ui.error,
                _ => return Err(format!("Unknown theme color: ui.{name}")),
            };
            *target = color;
        }
        for (name, color) in file.canvas {
            let target = match name.as_str() {
                "background" => &mut theme.canvas.background,
                "grid" => &mut theme.canvas.grid,
                "label" => &mut theme.canvas.label,
                "gate_fill" => &mut theme.canvas.gate_fill,
                "gate_outline" => &mut theme.canvas.gate_outline,
                "selection" => &mut theme.canvas.selection,
                _ => return Err(format!("Unknown theme color: canvas.{name}")),
            };
            *target = color;
        }
        Ok(theme)
    }

    /// Load and validate at most 256 KiB of UTF-8 TOML theme source.
    ///
    /// # Errors
    /// Fails for missing/unreadable files, oversized input, malformed TOML, or invalid
    /// versions/colors/metrics. Callers decide how to retain a previous active theme.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::from_toml(&Self::read_source(path)?)
    }

    /// Read a bounded theme source for validation and managed-file import.
    pub(crate) fn read_source(path: &Path) -> Result<String, String> {
        let mut text = String::new();
        std::fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(256 * 1024 + 1)
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        if text.len() > 256 * 1024 {
            return Err("Theme file exceeds 256 KiB.".into());
        }
        Ok(text)
    }

    /// Resolve a built-in key or custom TOML path. Built-ins have embedded fallbacks
    /// so the executable does not require a checkout of the themes directory.
    ///
    /// # Errors
    ///
    /// Returns the validation/read errors from [`Self::load`] or [`Self::from_toml`].
    /// A custom path must name a readable UTF-8 TOML theme.
    pub fn load_reference(reference: &str) -> Result<Self, String> {
        let embedded = builtin_source(reference);
        if let Some(embedded) = embedded {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let local = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("themes")
                    .join(format!("{reference}.toml"));
                if local.exists() {
                    return Self::load(&local);
                }
            }
            Self::from_toml(embedded)
        } else {
            Self::load(Path::new(reference))
        }
    }

    /// Build an egui style using the resolved widget colors and sizing overrides.
    pub fn style(&self) -> egui::Style {
        let mut style = egui::Style {
            visuals: if self.base == Base::Dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            },
            ..Default::default()
        };
        let ui = &self.ui;
        let visuals = &mut style.visuals;
        visuals.panel_fill = ui.panel.egui();
        visuals.window_fill = ui.window.egui();
        visuals.extreme_bg_color = ui.input.egui();
        visuals.text_edit_bg_color = Some(ui.input.egui());
        visuals.faint_bg_color = ui.button.egui().gamma_multiply(0.4);
        visuals.code_bg_color = ui.input.egui();
        visuals.override_text_color = Some(ui.text.egui());
        visuals.weak_text_color = Some(ui.muted.egui());
        visuals.hyperlink_color = ui.accent.egui();
        visuals.warn_fg_color = ui.warning.egui();
        visuals.error_fg_color = ui.error.egui();
        visuals.window_stroke = Stroke::new(1.0, ui.border.egui());
        visuals.selection.bg_fill = ui.selection.egui();
        visuals.selection.stroke = Stroke::new(1.0, ui.accent.egui());
        visuals.text_cursor.stroke = Stroke::new(1.5, ui.accent.egui());
        for (widget, fill) in [
            (&mut visuals.widgets.inactive, ui.button),
            (&mut visuals.widgets.hovered, ui.hover),
            (&mut visuals.widgets.active, ui.active),
            (&mut visuals.widgets.open, ui.active),
        ] {
            widget.bg_fill = fill.egui();
            widget.weak_bg_fill = fill.egui();
            widget.bg_stroke = Stroke::new(1.0, ui.border.egui());
            widget.fg_stroke = Stroke::new(1.0, ui.text.egui());
        }
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, ui.border.egui());
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ui.text.egui());
        if let Some(radius) = self.metrics.corner_radius {
            let radius = egui::CornerRadius::same(radius);
            visuals.window_corner_radius = radius;
            visuals.menu_corner_radius = radius;
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
                &mut visuals.widgets.open,
            ] {
                widget.corner_radius = radius;
            }
        }
        if let Some(size) = self.metrics.font_size {
            let ratio = size / style.text_styles[&egui::TextStyle::Body].size;
            for font in style.text_styles.values_mut() {
                font.size *= ratio;
            }
        }
        style
    }
}

/// Embedded templates used for fallback loading and first-run theme installation.
pub(crate) fn builtin_source(reference: &str) -> Option<&'static str> {
    match reference {
        "vscode" => Some(include_str!("../themes/vscode.toml")),
        "original-dark" => Some(include_str!("../themes/original-dark.toml")),
        "original-light" => Some(include_str!("../themes/original-light.toml")),
        "tokyo-night" => Some(include_str!("../themes/tokyo-night.toml")),
        _ => None,
    }
}
