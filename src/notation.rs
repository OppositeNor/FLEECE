//! Small, deterministic math-label layout shared by the canvas and exports.
//! Supports subscripts, superscripts, primes and overbars, rather than a TeX engine.
use crate::drawing::text_width;

/// One text run in a laid-out mathematical label.
pub struct Run {
    /// Literal glyph content after notation expansion.
    pub text: String,
    /// Horizontal offset from the label's left edge.
    pub x: f32,
    /// Vertical offset from the label origin; scripts use shifted offsets.
    pub y: f32,
    /// Font size of this run in world units.
    pub size: f32,
}
/// Laid-out runs and overbars for a small mathematical label.
#[derive(Default)]
pub struct Label {
    /// Text runs in reading order.
    pub runs: Vec<Run>,
    /// Overbar segments as `(left_x, right_x, y)` offsets from the label origin.
    pub bars: Vec<(f32, f32, f32)>,
    /// Total advance width used to center or append the label.
    pub width: f32,
}
impl Label {
    fn append_text(&mut self, text: String, size: f32) {
        if text.is_empty() {
            return;
        }
        let width = text_width(&text, size);
        self.runs.push(Run {
            text,
            x: self.width,
            y: 0.0,
            size,
        });
        self.width += width;
    }
    fn append(&mut self, mut child: Label, y: f32) {
        for run in &mut child.runs {
            run.x += self.width;
            run.y += y;
        }
        for bar in &mut child.bars {
            bar.0 += self.width;
            bar.1 += self.width;
            bar.2 += y;
        }
        self.width += child.width;
        self.runs.extend(child.runs);
        self.bars.extend(child.bars);
    }
}
/// Parse supported scripts, primes, commands and overbars into positioned runs.
/// Unsupported commands remain literal; this is label formatting, not TeX.
///
/// # Examples
/// ```
/// let label = fleece::notation::layout("s_{10}'", 20.0);
/// assert!(label.width > 0.0);
/// assert!(!label.runs.is_empty());
/// ```
pub fn layout(text: &str, size: f32) -> Label {
    let chars: Vec<_> = text.chars().collect();
    parse(&chars, &mut 0, size, 0, false)
}
fn parse(chars: &[char], cursor: &mut usize, size: f32, depth: usize, grouped: bool) -> Label {
    let mut label = Label::default();
    let mut plain = String::new();
    while *cursor < chars.len() {
        let ch = chars[*cursor];
        if grouped && ch == '}' {
            *cursor += 1;
            break;
        }
        if matches!(ch, '_' | '^') && depth < 8 && *cursor + 1 < chars.len() {
            label.append_text(std::mem::take(&mut plain), size);
            *cursor += 1;
            let child = argument(chars, cursor, size * 0.68, depth + 1);
            label.append(child, if ch == '_' { size * 0.3 } else { -size * 0.42 });
        } else if ch == '\\' && depth < 8 {
            let start = *cursor;
            *cursor += 1;
            let command_start = *cursor;
            while chars
                .get(*cursor)
                .is_some_and(|ch| ch.is_ascii_alphabetic())
            {
                *cursor += 1;
            }
            let command: String = chars[command_start..*cursor].iter().collect();
            if matches!(command.as_str(), "bar" | "overline") && chars.get(*cursor) == Some(&'{') {
                label.append_text(std::mem::take(&mut plain), size);
                let mut child = argument(chars, cursor, size, depth + 1);
                child.bars.push((0.0, child.width, -size * 0.58));
                label.append(child, 0.0);
            } else if let Some(symbol) = match command.as_str() {
                "cdot" => Some('·'),
                "oplus" => Some('⊕'),
                "land" => Some('∧'),
                "lor" => Some('∨'),
                _ => None,
            } {
                plain.push(symbol);
            } else if *cursor == command_start {
                if let Some(&escaped) = chars.get(*cursor) {
                    plain.push(escaped);
                    *cursor += 1;
                } else {
                    plain.push('\\');
                }
            } else {
                plain.extend(&chars[start..*cursor]);
            }
        } else {
            plain.push(if ch == '\'' { '′' } else { ch });
            *cursor += 1;
        }
    }
    label.append_text(plain, size);
    label
}
fn argument(chars: &[char], cursor: &mut usize, size: f32, depth: usize) -> Label {
    if chars.get(*cursor) == Some(&'{') {
        *cursor += 1;
        parse(chars, cursor, size, depth, true)
    } else {
        let mut label = Label::default();
        if let Some(ch) = chars.get(*cursor) {
            label.append_text(ch.to_string(), size);
            *cursor += 1;
        }
        label
    }
}
