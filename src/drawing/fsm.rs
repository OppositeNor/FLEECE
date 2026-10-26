//! State circles and directed transitions shared by the canvas and exports.
use super::*;
use crate::model::{Fsm, Transition};

/// State-circle radius in world units, shared by rendering and editor hit testing.
pub const STATE_RADIUS: f32 = 44.0;

/// A curved edge plus label/arrow geometry shared by painting and hit testing.
pub struct TransitionGeometry {
    /// Curved world-space path from the source boundary to the destination.
    pub path: PathData,
    /// Center anchor for the transition condition/output annotation.
    pub label: Point,
    /// Arrow tip at the destination-state boundary.
    pub tip: Point,
    /// Direction vector used to orient the destination arrowhead.
    pub tangent: Point,
}

/// Route an edge around state circles, separating parallel/reciprocal edges
/// and self-loops. Missing endpoint states return `None`.
pub fn transition_geometry(fsm: &Fsm, transition: &Transition) -> Option<TransitionGeometry> {
    let a = fsm.state(transition.from)?.position;
    let b = fsm.state(transition.to)?.position;
    let siblings: Vec<_> = fsm
        .transitions
        .iter()
        .filter(|t| t.from == transition.from && t.to == transition.to)
        .collect();
    let index = siblings
        .iter()
        .position(|t| t.id == transition.id)
        .unwrap_or(0) as f32;
    if transition.from == transition.to {
        let height = 92.0 + index * 34.0;
        let start = Point::new(a.x - 28.0, a.y - 34.0);
        let tip = Point::new(a.x + 28.0, a.y - 34.0);
        let left = Point::new(a.x - 85.0, a.y - height);
        let right = Point::new(a.x + 85.0, a.y - height);
        return Some(TransitionGeometry {
            path: PathData(vec![Cmd::Move(start), Cmd::Cubic(left, right, tip)]),
            label: Point::new(a.x, a.y - 0.75 * height - 16.0),
            tip,
            tangent: Point::new(tip.x - right.x, tip.y - right.y),
        });
    }
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy).max(1.0);
    let (ux, uy) = if length <= 1.0 {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    };
    let reverse = fsm
        .transitions
        .iter()
        .any(|t| t.from == transition.to && t.to == transition.from);
    let offset = if reverse {
        60.0 + index * 40.0
    } else {
        (index - (siblings.len() as f32 - 1.0) / 2.0) * 50.0
    };
    let control = Point::new(
        (a.x + b.x) / 2.0 - uy * offset,
        (a.y + b.y) / 2.0 + ux * offset,
    );
    let boundary = |center: Point, toward: Point| {
        let dx = toward.x - center.x;
        let dy = toward.y - center.y;
        let d = dx.hypot(dy).max(1.0);
        Point::new(
            center.x + dx / d * STATE_RADIUS,
            center.y + dy / d * STATE_RADIUS,
        )
    };
    let start = boundary(a, control);
    let tip = boundary(b, control);
    Some(TransitionGeometry {
        path: PathData(vec![Cmd::Move(start), Cmd::Quad(control, tip)]),
        label: Point::new(
            (start.x + 2.0 * control.x + tip.x) / 4.0,
            (start.y + 2.0 * control.y + tip.y) / 4.0 - 14.0,
        ),
        tip,
        tangent: Point::new(tip.x - control.x, tip.y - control.y),
    })
}

fn arrow(scene: &mut Scene, tip: Point, tangent: Point, color: Color) {
    let len = tangent.x.hypot(tangent.y).max(1.0);
    let (x, y) = (tangent.x / len, tangent.y / len);
    scene.path(
        PathData(vec![
            Cmd::Move(tip),
            Cmd::Line(Point::new(
                tip.x - x * 12.0 - y * 5.0,
                tip.y - y * 12.0 + x * 5.0,
            )),
            Cmd::Line(Point::new(
                tip.x - x * 12.0 + y * 5.0,
                tip.y - y * 12.0 - x * 5.0,
            )),
            Cmd::Close,
        ]),
        Some(color),
        None,
    );
}

/// Build state circles, initial/accepting markers, transitions and annotations.
/// Colors are supplied by the caller so the same geometry serves editor and export.
pub fn build_fsm_scene(fsm: &Fsm, ink: Color, fill: Color, text: Color) -> Scene {
    let mut scene = Scene::default();
    for transition in &fsm.transitions {
        let Some(g) = transition_geometry(fsm, transition) else {
            continue;
        };
        scene.path(g.path, None, Some((ink, 2.0)));
        arrow(&mut scene, g.tip, g.tangent, ink);
        let label = if transition.output.is_empty() {
            transition.condition.clone()
        } else {
            format!("{} / {}", transition.condition, transition.output)
        };
        scene.styled_text(g.label, label, 14.0, text, TextStyle::Ui);
    }
    for state in &fsm.states {
        let c = state.position;
        if fsm.initial == Some(state.id) {
            let tip = Point::new(c.x - STATE_RADIUS, c.y);
            scene.path(
                PathData::polyline(&[Point::new(tip.x - 42.0, tip.y), tip]),
                None,
                Some((ink, 2.0)),
            );
            arrow(&mut scene, tip, Point::new(1.0, 0.0), ink);
        }
        scene.path(
            PathData::circle(c, STATE_RADIUS),
            Some(fill),
            Some((ink, 2.0)),
        );
        if state.accepting {
            scene.path(
                PathData::circle(c, STATE_RADIUS - 6.0),
                None,
                Some((ink, 1.5)),
            );
        }
        scene.styled_text(
            Point::new(c.x, c.y - if state.output.is_empty() { 0.0 } else { 11.0 }),
            &state.name,
            16.0,
            text,
            TextStyle::Ui,
        );
        if !state.output.is_empty() {
            scene.styled_text(
                Point::new(c.x, c.y + 13.0),
                &state.output,
                13.0,
                text,
                TextStyle::Ui,
            );
        }
    }
    scene
}
