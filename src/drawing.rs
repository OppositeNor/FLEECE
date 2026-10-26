//! Scene primitives, font outlines and document-style diagram geometry.
//!
//! A scene is expressed in world coordinates and can be painted to the editor,
//! SVG or PNG. [`wire_points`] uses interactive pins; [`diagram_wire_points`] uses
//! export pins. Both delegate orthogonal routing to [`crate::model::Wire`]. Keep
//! those geometry choices separate when changing symbols or UI proportions.
//!
//! Static scenes contain only diagram geometry and annotations.

mod fsm;
pub use fsm::{STATE_RADIUS, TransitionGeometry, build_fsm_scene, transition_geometry};

use crate::model::{Circuit, Endpoint, Kind, Node, Point};
use ab_glyph::{Font, FontRef, ScaleFont};
use std::sync::OnceLock;

/// Embedded sans-serif font for editor labels and UI text.
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
/// Embedded regular serif font used for document labels.
pub const SERIF_BYTES: &[u8] = include_bytes!("../assets/DejaVuSerif.ttf");
/// Embedded italic serif font used for mathematical notation.
pub const MATH_BYTES: &[u8] = include_bytes!("../assets/DejaVuSerif-Italic.ttf");
/// Embedded bold serif font used for diagram headings.
pub const HEADING_BYTES: &[u8] = include_bytes!("../assets/DejaVuSerif-Bold.ttf");

/// Font role used for measuring and outlining scene text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextStyle {
    /// Sans-serif editor and UI labels.
    Ui,
    /// Regular serif document text.
    Serif,
    /// Italic mathematical labels and symbols.
    #[default]
    Math,
    /// Bold serif diagram headings.
    Heading,
}
impl TextStyle {
    /// Return the egui font-family key registered for this text role.
    pub fn family(self) -> &'static str {
        match self {
            Self::Ui => "circuit-ui",
            Self::Serif => "circuit-serif",
            Self::Math => "circuit-math",
            Self::Heading => "circuit-heading",
        }
    }
}

/// Borrow the lazily initialized embedded italic serif math font.
pub fn font() -> &'static FontRef<'static> {
    styled_font(TextStyle::Math)
}
fn styled_font(style: TextStyle) -> &'static FontRef<'static> {
    static SERIF: OnceLock<FontRef<'static>> = OnceLock::new();
    static UI: OnceLock<FontRef<'static>> = OnceLock::new();
    static MATH: OnceLock<FontRef<'static>> = OnceLock::new();
    static HEADING: OnceLock<FontRef<'static>> = OnceLock::new();
    let (slot, bytes) = match style {
        TextStyle::Ui => (&UI, FONT_BYTES),
        TextStyle::Serif => (&SERIF, SERIF_BYTES),
        TextStyle::Math => (&MATH, MATH_BYTES),
        TextStyle::Heading => (&HEADING, HEADING_BYTES),
    };
    slot.get_or_init(|| FontRef::try_from_slice(bytes).expect("bundled diagram font"))
}

/// Measure a mathematical label using the embedded italic serif font.
pub fn text_width(text: &str, size: f32) -> f32 {
    styled_text_width(text, size, TextStyle::Math)
}

/// Measure text using the font associated with `style`, including kerning.
pub fn styled_text_width(text: &str, size: f32, style: TextStyle) -> f32 {
    let f = styled_font(style).as_scaled(size);
    let mut width = 0.0;
    let mut previous = None;
    for ch in text.chars() {
        let id = f.glyph_id(ch);
        if let Some(p) = previous {
            width += f.kern(p, id);
        }
        width += f.h_advance(id);
        previous = Some(id);
    }
    width
}

/// Absolute world-space commands in a vector path.
#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    /// Move the path cursor to a point without drawing a segment.
    Move(Point),
    /// Draw a straight segment from the cursor to the supplied endpoint.
    Line(Point),
    /// Draw a quadratic Bézier segment with a control point and endpoint.
    Quad(Point, Point),
    /// Draw a cubic Bézier segment with two control points and an endpoint.
    Cubic(Point, Point, Point),
    /// Close the current subpath back to its starting point.
    Close,
}
/// Ordered vector-path commands, reusable across native painting and image exports.
#[derive(Clone, Debug, Default)]
pub struct PathData(
    /// Vector commands in drawing order.
    pub Vec<Cmd>,
);
impl PathData {
    /// Create an open path through the supplied points; an empty slice produces no commands.
    pub fn polyline(points: &[Point]) -> Self {
        let mut commands = vec![];
        for (i, &p) in points.iter().enumerate() {
            commands.push(if i == 0 { Cmd::Move(p) } else { Cmd::Line(p) });
        }
        Self(commands)
    }
    /// Create a closed rectangle from its top-left position and width/height.
    pub fn rect(p: Point, size: Point) -> Self {
        let mut path = Self::polyline(&[
            p,
            Point::new(p.x + size.x, p.y),
            Point::new(p.x + size.x, p.y + size.y),
            Point::new(p.x, p.y + size.y),
        ]);
        path.0.push(Cmd::Close);
        path
    }
    /// Approximate a circle at center `c` and radius `r` with cubic Bézier segments.
    pub fn circle(c: Point, r: f32) -> Self {
        let k = r * 0.5522848;
        Self(vec![
            Cmd::Move(Point::new(c.x + r, c.y)),
            Cmd::Cubic(
                Point::new(c.x + r, c.y + k),
                Point::new(c.x + k, c.y + r),
                Point::new(c.x, c.y + r),
            ),
            Cmd::Cubic(
                Point::new(c.x - k, c.y + r),
                Point::new(c.x - r, c.y + k),
                Point::new(c.x - r, c.y),
            ),
            Cmd::Cubic(
                Point::new(c.x - r, c.y - k),
                Point::new(c.x - k, c.y - r),
                Point::new(c.x, c.y - r),
            ),
            Cmd::Cubic(
                Point::new(c.x + k, c.y - r),
                Point::new(c.x + r, c.y - k),
                Point::new(c.x + r, c.y),
            ),
            Cmd::Close,
        ])
    }
    /// Approximate curves by line segments for painting, bounds and hit testing.
    /// This preserves point order and repeats the subpath start when closing it.
    pub fn flattened(&self) -> Vec<Point> {
        let mut result = vec![];
        let mut current = Point::default();
        for cmd in &self.0 {
            match *cmd {
                Cmd::Move(p) | Cmd::Line(p) => {
                    result.push(p);
                    current = p;
                }
                Cmd::Quad(a, b) => {
                    let start = current;
                    for step in 1..=20 {
                        let t = step as f32 / 20.0;
                        let u = 1.0 - t;
                        result.push(Point::new(
                            u * u * start.x + 2.0 * u * t * a.x + t * t * b.x,
                            u * u * start.y + 2.0 * u * t * a.y + t * t * b.y,
                        ));
                    }
                    current = b;
                }
                Cmd::Cubic(a, b, c) => {
                    let start = current;
                    for step in 1..=24 {
                        let t = step as f32 / 24.0;
                        let u = 1.0 - t;
                        result.push(Point::new(
                            u * u * u * start.x
                                + 3.0 * u * u * t * a.x
                                + 3.0 * u * t * t * b.x
                                + t * t * t * c.x,
                            u * u * u * start.y
                                + 3.0 * u * u * t * a.y
                                + 3.0 * u * t * t * b.y
                                + t * t * t * c.y,
                        ));
                    }
                    current = c;
                }
                Cmd::Close => {}
            }
        }
        result
    }
    /// Translate all path endpoints/control points by a world-space offset.
    pub fn translated(mut self, p: Point) -> Self {
        let add = |v: &mut Point| {
            v.x += p.x;
            v.y += p.y;
        };
        for cmd in &mut self.0 {
            match cmd {
                Cmd::Move(a) | Cmd::Line(a) => add(a),
                Cmd::Quad(a, b) => {
                    add(a);
                    add(b)
                }
                Cmd::Cubic(a, b, c) => {
                    add(a);
                    add(b);
                    add(c)
                }
                Cmd::Close => {}
            }
        }
        self
    }
}

/// Straight-alpha RGBA color channels, each in the range 0–255.
pub type Color = [u8; 4];
/// Default near-black ink for static document diagrams.
pub const INK: Color = [12, 12, 12, 255];
/// Opaque white paper color for static diagram backgrounds.
pub const WHITE: Color = [255, 255, 255, 255];

/// Resolve a persisted net-color choice to document/export RGBA ink.
pub fn net_color(color: crate::model::NetColor) -> Color {
    use crate::model::NetColor::*;
    match color {
        Black => INK,
        Red => [230, 35, 32, 255],
        Green => [0, 145, 55, 255],
        Blue => [0, 90, 235, 255],
        Purple => [123, 29, 182, 255],
        Amber => [152, 108, 0, 255],
    }
}
/// A backend-independent path or center-anchored text item.
#[derive(Clone)]
pub enum Primitive {
    /// A vector path with optional fill and stroke.
    Path {
        /// World-space drawing commands.
        path: PathData,
        /// Fill color, or `None` for an unfilled path.
        fill: Option<Color>,
        /// Optional stroke color and width in world units.
        stroke: Option<(Color, f32)>,
    },
    /// Text anchored at the center of its supplied position.
    Text {
        /// Center anchor in world coordinates.
        position: Point,
        /// Text content to paint or convert to font outlines.
        text: String,
        /// Font size in world units before viewport/export scaling.
        size: f32,
        /// Text fill color.
        color: Color,
        /// Font role used to measure and outline the text.
        style: TextStyle,
    }, // center anchor
}
/// Axis-aligned world-space bounds used for fitting and export cropping.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// Smallest included x/y coordinates.
    pub min: Point,
    /// Largest included x/y coordinates.
    pub max: Point,
}
impl Bounds {
    /// Horizontal extent in world units.
    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }
    /// Vertical extent in world units.
    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }
    /// Expand the box in place to include one world-space point.
    pub fn include(&mut self, p: Point) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }
    /// Return a copy padded by `n` world units on each side.
    pub fn expanded(self, n: f32) -> Self {
        Self {
            min: Point::new(self.min.x - n, self.min.y - n),
            max: Point::new(self.max.x + n, self.max.y + n),
        }
    }
}
/// Ordered world-space primitives. No viewport, selection or theme is stored here.
#[derive(Clone, Default)]
pub struct Scene {
    /// Primitives in back-to-front painting order.
    pub items: Vec<Primitive>,
}
impl Scene {
    /// Append one vector primitive with optional fill and stroke.
    pub fn path(&mut self, path: PathData, fill: Option<Color>, stroke: Option<(Color, f32)>) {
        self.items.push(Primitive::Path { path, fill, stroke });
    }
    /// Lay out a center-anchored mathematical label, expanding scripts and overbars
    /// into text/path primitives before appending them.
    pub fn text(&mut self, p: Point, text: impl Into<String>, size: f32, color: Color) {
        let text = text.into();
        let label = crate::notation::layout(&text, size);
        let left = p.x - label.width / 2.0;
        for run in label.runs {
            self.styled_text(
                Point::new(
                    left + run.x + text_width(&run.text, run.size) / 2.0,
                    p.y + run.y,
                ),
                run.text,
                run.size,
                color,
                TextStyle::Math,
            );
        }
        for bar in label.bars {
            self.path(
                PathData::polyline(&[
                    Point::new(left + bar.0, p.y + bar.2),
                    Point::new(left + bar.1, p.y + bar.2),
                ]),
                None,
                Some((color, 1.2)),
            );
        }
    }
    /// Append one center-anchored text run with an explicit font role.
    pub fn styled_text(
        &mut self,
        p: Point,
        text: impl Into<String>,
        size: f32,
        color: Color,
        style: TextStyle,
    ) {
        self.items.push(Primitive::Text {
            position: p,
            text: text.into(),
            size,
            color,
            style,
        });
    }
    /// Compute primitive bounds, including stroke and text extents.
    /// An empty scene receives a finite default box suitable for fitting.
    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds {
            min: Point::new(f32::INFINITY, f32::INFINITY),
            max: Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        };
        for item in &self.items {
            match item {
                Primitive::Path { path, stroke, .. } => {
                    let pad = stroke.map_or(0.0, |(_, w)| w / 2.0);
                    for p in path.flattened() {
                        b.include(Point::new(p.x - pad, p.y - pad));
                        b.include(Point::new(p.x + pad, p.y + pad));
                    }
                }
                Primitive::Text {
                    position,
                    text,
                    size,
                    style,
                    ..
                } => {
                    let half = styled_text_width(text, *size, *style) / 2.0;
                    b.include(Point::new(position.x - half - 3.0, position.y - size));
                    b.include(Point::new(position.x + half + 3.0, position.y + size));
                }
            }
        }
        if !b.min.x.is_finite() {
            b = Bounds {
                min: Point::default(),
                max: Point::new(160.0, 100.0),
            };
        }
        b
    }
}

/// Resolve a wire with interactive pin geometry; missing endpoints yield no path.
pub fn wire_points(circuit: &Circuit, wire: &crate::model::Wire) -> Vec<Point> {
    route_wire(circuit, wire, false)
}

/// Editable routing joints, including the elbow of an automatic wire.
pub fn wire_joints(circuit: &Circuit, wire: &crate::model::Wire) -> Vec<Point> {
    if !wire.waypoints.is_empty() {
        return wire.waypoints.clone();
    }
    let mut route = wire_points(circuit, wire);
    route.dedup();
    if route.len() > 2 {
        route[1..route.len() - 1].to_vec()
    } else {
        vec![]
    }
}

/// Resolve the same routing controls using document/export pin geometry.
pub fn diagram_wire_points(circuit: &Circuit, wire: &crate::model::Wire) -> Vec<Point> {
    route_wire(circuit, wire, true)
}

fn route_wire(circuit: &Circuit, wire: &crate::model::Wire, document: bool) -> Vec<Point> {
    let Some(from) = circuit.node(wire.from.node) else {
        return vec![];
    };
    let Some(to) = circuit.node(wire.to.node) else {
        return vec![];
    };
    let start = if document {
        diagram_pin(from, true, wire.from.port)
    } else {
        from.pin(true, wire.from.port)
    };
    let end = if document {
        diagram_pin(to, false, wire.to.port)
    } else {
        to.pin(false, wire.to.port)
    };
    wire.points_between(start, end)
}

/// Resolve the driving net through joints and return its persisted color.
/// Missing source nodes use the default document ink.
pub fn endpoint_color(circuit: &Circuit, endpoint: Endpoint) -> Color {
    let endpoint = circuit.net_source(endpoint);
    net_color(
        circuit
            .node(endpoint.node)
            .map(|node| node.net_color(endpoint.port))
            .unwrap_or_default(),
    )
}

/// Export proportions are independent of the editor's interactive block sizes.
pub fn diagram_size(node: &Node) -> Point {
    match node.kind {
        Kind::Joint => Point::default(),
        Kind::Register => Point::new(144.0, 112.0 + node.outputs as f32 * 28.0),
        kind if kind.module() => Point::new(
            180.0,
            80.0_f32.max((node.inputs.max(node.outputs) + 1) as f32 * 24.0),
        ),
        Kind::Input | Kind::Output | Kind::High | Kind::Low => Point::new(96.0, 40.0),
        Kind::Not | Kind::Buffer => Point::new(64.0, 64.0),
        _ => Point::new(84.0, 72.0_f32.max((node.inputs + 1) as f32 * 20.0)),
    }
}

/// Locate a zero-based document/export pin independently of interactive node size.
pub fn diagram_pin(node: &Node, output: bool, port: usize) -> Point {
    if node.kind == Kind::Joint {
        return node.position;
    }
    let size = diagram_size(node);
    let count = if output { node.outputs } else { node.inputs };
    let y = if node.kind == Kind::Register {
        if !output && port == node.outputs {
            size.y - 52.0
        } else if !output && port == node.outputs + 1 {
            size.y - 24.0
        } else {
            36.0 + port as f32 * 28.0
        }
    } else {
        size.y * (port + 1) as f32 / (count + 1) as f32
    };
    Point::new(
        node.position.x + if output { size.x + 16.0 } else { -16.0 },
        node.position.y + y,
    )
}

/// Build document-style symbols and wire paths, dispatching FSM documents to
/// the FSM renderer. Scenes contain static symbols and annotations only.
pub fn build_scene(circuit: &Circuit) -> Scene {
    if let Some(fsm) = &circuit.fsm {
        return build_fsm_scene(fsm, INK, WHITE, INK);
    }
    use std::collections::BTreeMap;
    let mut scene = Scene::default();
    let mut routes: BTreeMap<Endpoint, Vec<Vec<Point>>> = BTreeMap::new();
    let drivers: BTreeMap<_, _> = circuit
        .wires
        .iter()
        .map(|wire| (wire.to, wire.from))
        .collect();
    for wire in &circuit.wires {
        let points = diagram_wire_points(circuit, wire);
        scene.path(
            PathData::polyline(&points),
            None,
            Some((endpoint_color(circuit, wire.from), 2.1)),
        );
        if let Some(width) = wire.bus_width {
            draw_positioned_bus_annotation(
                &mut scene,
                &points,
                wire.bus_position,
                width,
                endpoint_color(circuit, wire.from),
                TextStyle::Math,
            );
        }
        routes.entry(wire.from).or_default().push(points);
    }
    for (source, paths) in routes {
        if paths.len() < 2 {
            continue;
        }
        for point in junction_points(&paths) {
            scene.path(
                PathData::circle(point, 3.4),
                Some(endpoint_color(circuit, source)),
                None,
            );
        }
    }
    for node in &circuit.nodes {
        draw_node(&mut scene, circuit, node, &drivers);
    }
    scene
}

/// Add a bus slash and width near the midpoint of the longest nonzero segment.
/// This is a display annotation, never a routing joint or connectivity change.
/// Empty/coincident paths have no space for a marker and are left unchanged.
pub fn draw_bus_annotation(
    scene: &mut Scene,
    points: &[Point],
    width: usize,
    color: Color,
    style: TextStyle,
) {
    draw_positioned_bus_annotation(scene, points, None, width, color, style);
}

/// Draw a bus marker at a saved normalized position along the route.
/// Missing positions retain the longest-segment default.
pub fn draw_positioned_bus_annotation(
    scene: &mut Scene,
    points: &[Point],
    position: Option<f32>,
    width: usize,
    color: Color,
    style: TextStyle,
) {
    let Some((center, horizontal)) = crate::model::Wire::bus_anchor(points, position) else {
        return;
    };
    scene.path(
        PathData::polyline(&[
            Point::new(center.x - 5.0, center.y + 7.0),
            Point::new(center.x + 5.0, center.y - 7.0),
        ]),
        None,
        Some((color, 2.0)),
    );
    let text = width.to_string();
    let size = if style == TextStyle::Ui { 12.0 } else { 14.0 };
    let position = bus_label_position(center, horizontal, width, style);
    scene.styled_text(position, text, size, color, style);
}

/// Center the bus width above horizontal segments or beside vertical segments.
pub fn bus_label_position(
    center: Point,
    horizontal: bool,
    width: usize,
    style: TextStyle,
) -> Point {
    if horizontal {
        Point::new(center.x, center.y - 16.0)
    } else {
        let size = if style == TextStyle::Ui { 12.0 } else { 14.0 };
        Point::new(
            center.x + 12.0 + styled_text_width(&width.to_string(), size, style) / 2.0,
            center.y,
        )
    }
}

/// Dots mark actual branches within one net. Crossings of different nets never
/// acquire a dot. Merged axis intervals keep large fan-outs inexpensive to draw.
pub fn junction_points(paths: &[Vec<Point>]) -> Vec<Point> {
    use std::collections::{BTreeMap, BTreeSet};
    let key = |value: f32| (value * 1000.0).round() as i64;
    let mut axes: BTreeMap<(bool, i64), Vec<(i64, i64)>> = BTreeMap::new();
    let mut candidates = BTreeSet::new();
    for path in paths {
        for point in path {
            candidates.insert((key(point.x), key(point.y)));
        }
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let vertical = (a.x - b.x).abs() < 0.001;
            let (axis, start, end) = if vertical {
                (key(a.x), key(a.y), key(b.y))
            } else {
                (key(a.y), key(a.x), key(b.x))
            };
            axes.entry((vertical, axis))
                .or_default()
                .push((start.min(end), start.max(end)));
        }
    }
    for intervals in axes.values_mut() {
        intervals.sort_unstable();
        let mut merged: Vec<(i64, i64)> = vec![];
        for &(start, end) in intervals.iter() {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        *intervals = merged;
    }
    candidates
        .into_iter()
        .filter_map(|(x, y)| {
            let mut directions = 0;
            for (vertical, axis, position) in [(false, y, x), (true, x, y)] {
                if let Some(intervals) = axes.get(&(vertical, axis)) {
                    let index = intervals.partition_point(|(start, _)| *start <= position);
                    if let Some(&(start, end)) =
                        index.checked_sub(1).and_then(|index| intervals.get(index))
                        && position <= end
                    {
                        directions += usize::from(start < position) + usize::from(position < end);
                    }
                }
            }
            (directions >= 3).then_some(Point::new(x as f32 / 1000.0, y as f32 / 1000.0))
        })
        .collect()
}

fn body_path(node: &Node) -> PathData {
    let size = diagram_size(node);
    let h = size.y;
    let inverted = matches!(node.kind, Kind::Not | Kind::Nand | Kind::Nor | Kind::Xnor);
    let w = size.x - if inverted { 12.0 } else { 0.0 };
    let p = Point::new;
    match node.kind {
        Kind::And | Kind::Nand => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Line(p(w - h / 2.0, 0.0)),
            Cmd::Cubic(p(w - h * 0.224, 0.0), p(w, h * 0.224), p(w, h / 2.0)),
            Cmd::Cubic(p(w, h * 0.776), p(w - h * 0.224, h), p(w - h / 2.0, h)),
            Cmd::Line(p(0.0, h)),
            Cmd::Close,
        ]),
        Kind::Or | Kind::Nor | Kind::Xor | Kind::Xnor => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Cubic(p(w * 0.45, 0.0), p(w * 0.8, h * 0.08), p(w, h / 2.0)),
            Cmd::Cubic(p(w * 0.8, h * 0.92), p(w * 0.45, h), p(0.0, h)),
            Cmd::Quad(p(28.0, h / 2.0), p(0.0, 0.0)),
            Cmd::Close,
        ]),
        Kind::Not | Kind::Buffer => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Line(p(w, h / 2.0)),
            Cmd::Line(p(0.0, h)),
            Cmd::Close,
        ]),
        Kind::Mux => PathData(vec![
            Cmd::Move(p(0.0, 0.0)),
            Cmd::Line(p(w, 18.0)),
            Cmd::Line(p(w, h - 18.0)),
            Cmd::Line(p(0.0, h)),
            Cmd::Close,
        ]),
        _ => PathData::rect(Point::default(), size),
    }
}

/// Export markers hide joints that simply join two wire segments.
pub fn draw_joint(scene: &mut Scene, position: Point, degree: usize, color: Color) {
    if degree != 2 {
        scene.path(PathData::circle(position, 3.4), Some(color), None);
    }
}

fn draw_node(
    scene: &mut Scene,
    circuit: &Circuit,
    node: &Node,
    drivers: &std::collections::BTreeMap<Endpoint, Endpoint>,
) {
    if node.kind == Kind::Joint {
        draw_joint(
            scene,
            node.position,
            circuit.joint_degree(node.id),
            endpoint_color(
                circuit,
                Endpoint {
                    node: node.id,
                    port: 0,
                },
            ),
        );
        return;
    }
    let size = diagram_size(node);
    let p = |x, y| Point::new(node.position.x + x, node.position.y + y);
    if matches!(node.kind, Kind::Input | Kind::Output) {
        let input = node.kind == Kind::Input;
        let endpoint = Endpoint {
            node: node.id,
            port: 0,
        };
        let pin = diagram_pin(node, input, 0);
        let outer = p(if input { 0.0 } else { size.x }, size.y / 2.0);
        let color = if input {
            endpoint_color(circuit, endpoint)
        } else {
            drivers
                .get(&endpoint)
                .map(|source| endpoint_color(circuit, *source))
                .unwrap_or(INK)
        };
        scene.path(PathData::polyline(&[outer, pin]), None, Some((color, 2.1)));
        let center = Point::new((outer.x + pin.x) / 2.0, pin.y);
        scene.text(
            Point::new(center.x, center.y - 18.0),
            &node.label,
            21.0,
            INK,
        );
        return;
    }
    scene.path(
        body_path(node).translated(node.position),
        Some(WHITE),
        Some((INK, 2.2)),
    );
    if matches!(node.kind, Kind::Xor | Kind::Xnor) {
        scene.path(
            PathData(vec![
                Cmd::Move(p(-7.0, 0.0)),
                Cmd::Quad(p(21.0, size.y / 2.0), p(-7.0, size.y)),
            ]),
            None,
            Some((INK, 2.2)),
        );
    }
    if matches!(node.kind, Kind::Not | Kind::Nand | Kind::Nor | Kind::Xnor) {
        scene.path(
            PathData::circle(p(size.x - 6.0, size.y / 2.0), 6.0),
            Some(WHITE),
            Some((INK, 2.2)),
        );
    }
    for output in [false, true] {
        for port in 0..if output { node.outputs } else { node.inputs } {
            let endpoint = Endpoint {
                node: node.id,
                port,
            };
            let pin = diagram_pin(node, output, port);
            let mut edge = Point::new(node.position.x + if output { size.x } else { 0.0 }, pin.y);
            if !output && matches!(node.kind, Kind::Or | Kind::Nor | Kind::Xor | Kind::Xnor) {
                let t = (pin.y - node.position.y) / size.y;
                edge.x += 56.0 * t * (1.0 - t);
            }
            let color = if output {
                endpoint_color(circuit, endpoint)
            } else {
                drivers
                    .get(&endpoint)
                    .map(|source| endpoint_color(circuit, *source))
                    .unwrap_or(INK)
            };
            if !output && node.bubble(false, port) {
                if matches!(node.kind, Kind::Xor | Kind::Xnor) {
                    edge.x -= 7.0;
                }
                scene.path(
                    PathData::circle(Point::new(edge.x - 6.0, edge.y), 6.0),
                    Some(WHITE),
                    Some((INK, 2.2)),
                );
                edge.x -= 12.0;
            }
            scene.path(PathData::polyline(&[edge, pin]), None, Some((color, 2.1)));
            let clock_pin = node.kind == Kind::Register && !output && port == node.outputs;
            if clock_pin {
                // The edge triangle is the conventional positive-edge clock marker.
                scene.path(
                    PathData::polyline(&[
                        Point::new(edge.x, edge.y - 9.0),
                        Point::new(edge.x + 13.0, edge.y),
                        Point::new(edge.x, edge.y + 9.0),
                    ]),
                    None,
                    Some((INK, 2.0)),
                );
            }
            if node.kind.module() {
                let name = node.port_name(output, port);
                let font_size = if node.kind == Kind::Register {
                    20.0
                } else {
                    15.0
                };
                let half = crate::notation::layout(&name, font_size).width / 2.0;
                // Reserve the triangle's 13-unit width, then the same 10-unit
                // padding used by other pins. CLK shares their math text style.
                let inset = if clock_pin { 23.0 } else { 10.0 };
                scene.text(
                    Point::new(
                        edge.x + if output { -half - inset } else { half + inset },
                        edge.y,
                    ),
                    name,
                    font_size,
                    INK,
                );
            }
        }
    }
    let center = p(size.x / 2.0, size.y / 2.0);
    if node.kind.module() {
        scene.styled_text(
            p(size.x / 2.0, -34.0),
            &node.label,
            20.0,
            INK,
            TextStyle::Heading,
        );
    } else {
        scene.text(p(size.x / 2.0, -20.0), &node.label, 20.0, INK);
    }
    match node.kind {
        Kind::High | Kind::Low => scene.text(
            center,
            if node.kind == Kind::High { "1" } else { "0" },
            23.0,
            INK,
        ),
        Kind::Register => {}
        kind if kind.module() => scene.styled_text(
            center,
            match kind {
                Kind::Mux => "MUX",
                Kind::HalfAdder => "HA",
                Kind::FullAdder => "FA",
                _ => "f",
            },
            15.0,
            INK,
            TextStyle::Serif,
        ),
        _ => {}
    }
}

/// Font outlines avoid depending on an installed font in exported SVGs.
pub fn text_paths(position: Point, text: &str, size: f32) -> Vec<PathData> {
    styled_text_paths(position, text, size, TextStyle::Math)
}
/// Convert text into positioned glyph-outline paths using the selected embedded font.
pub fn styled_text_paths(
    position: Point,
    text: &str,
    size: f32,
    style: TextStyle,
) -> Vec<PathData> {
    use ab_glyph::OutlineCurve;
    let font = styled_font(style);
    let f = font.as_scaled(size);
    let factor = f.scale_factor();
    let mut x = position.x - styled_text_width(text, size, style) / 2.0;
    let baseline = position.y + (f.ascent() + f.descent()) / 2.0;
    let mut previous = None;
    let mut paths = vec![];
    for ch in text.chars() {
        let id = f.glyph_id(ch);
        if let Some(prev) = previous {
            x += f.kern(prev, id);
        }
        previous = Some(id);
        if let Some(outline) = font.outline(id) {
            let convert = |p: ab_glyph::Point| {
                Point::new(
                    x + p.x * factor.horizontal,
                    baseline - p.y * factor.vertical,
                )
            };
            let mut commands = vec![];
            let mut end = None;
            for curve in outline.curves {
                let start = match curve {
                    OutlineCurve::Line(a, _)
                    | OutlineCurve::Quad(a, _, _)
                    | OutlineCurve::Cubic(a, _, _, _) => a,
                };
                if end != Some(start) {
                    if end.is_some() {
                        commands.push(Cmd::Close);
                    }
                    commands.push(Cmd::Move(convert(start)));
                }
                match curve {
                    OutlineCurve::Line(_, b) => {
                        commands.push(Cmd::Line(convert(b)));
                        end = Some(b);
                    }
                    OutlineCurve::Quad(_, b, c) => {
                        commands.push(Cmd::Quad(convert(b), convert(c)));
                        end = Some(c);
                    }
                    OutlineCurve::Cubic(_, b, c, d) => {
                        commands.push(Cmd::Cubic(convert(b), convert(c), convert(d)));
                        end = Some(d);
                    }
                }
            }
            commands.push(Cmd::Close);
            paths.push(PathData(commands));
        }
        x += f.h_advance(id);
    }
    paths
}
