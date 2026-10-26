//! Orthogonal routing shared by document edits and both diagram renderers.
use super::*;

impl Wire {
    /// Endpoint geometry is supplied by the caller so export geometry remains
    /// independent of interactive node sizes and viewport state.
    pub fn points_between(&self, start: Point, end: Point) -> Vec<Point> {
        if !self.waypoints.is_empty() {
            let mut points = vec![start];
            let mut previous = start;
            for point in self.waypoints.iter().copied().chain(std::iter::once(end)) {
                if (previous.x - point.x).abs() > 0.01 && (previous.y - point.y).abs() > 0.01 {
                    points.push(Point::new(point.x, previous.y));
                }
                points.push(point);
                previous = point;
            }
            return points;
        }
        let corner = if self.horizontal_first {
            Point::new(end.x, start.y)
        } else {
            Point::new(start.x, end.y)
        };
        let mut points = vec![start, corner, end];
        points.dedup();
        points
    }
}

/// Drop only the requested controls, retaining the other corners. If a removed
/// corner is required to keep a gap orthogonal, choose the other elbow instead
/// of silently recreating the deleted joint at the same position.
fn without_points(route: &[Point], removed: &[Point]) -> Vec<Point> {
    if route.len() < 2 {
        return vec![];
    }
    let mut kept = vec![route[0]];
    kept.extend(
        route[1..route.len() - 1]
            .iter()
            .copied()
            .filter(|p| !removed.contains(p)),
    );
    kept.push(*route.last().unwrap());
    kept.dedup();
    let mut result = vec![kept[0]];
    for point in kept.into_iter().skip(1) {
        let previous = *result.last().unwrap();
        if (previous.x - point.x).abs() > 0.01 && (previous.y - point.y).abs() > 0.01 {
            let corner = Point::new(point.x, previous.y);
            result.push(if removed.contains(&corner) {
                Point::new(previous.x, point.y)
            } else {
                corner
            });
        }
        result.push(point);
    }
    if result.len() > 2 {
        result[1..result.len() - 1].to_vec()
    } else {
        vec![]
    }
}

impl Circuit {
    /// Resolve a wire path using interactive pin geometry; missing nodes yield an
    /// empty path. Export callers instead supply document pins to [`Wire::points_between`].
    pub fn wire_path(&self, wire: &Wire) -> Vec<Point> {
        let Some(from) = self.node(wire.from.node) else {
            return vec![];
        };
        let Some(to) = self.node(wire.to.node) else {
            return vec![];
        };
        wire.points_between(from.pin(true, wire.from.port), to.pin(false, wire.to.port))
    }

    /// Set or remove the bus-width annotation on the wire feeding an input.
    /// Widths are diagram annotations and do not change endpoint port counts.
    ///
    /// # Errors
    ///
    /// Fails without mutation for widths outside 1–1024 or an input without a wire.
    pub fn set_wire_bus_width(&mut self, to: Endpoint, width: Option<usize>) -> Result<(), String> {
        if width.is_some_and(|width| !(1..=MAX_BUS_WIDTH).contains(&width)) {
            return Err("Bus width must be between 1 and 1024.".into());
        }
        let wire = self
            .wires
            .iter_mut()
            .find(|wire| wire.to == to)
            .ok_or("No wire feeds this input pin.")?;
        wire.bus_width = width;
        if width.is_none() {
            wire.bus_position = None;
        }
        Ok(())
    }

    /// Indices refer to explicit controls, or the single automatic elbow when
    /// there are no explicit controls. Preserve all unselected route corners.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing wire, stale control indices, or a resulting
    /// route exceeding the waypoint limit. The wire is not changed on failure.
    pub fn remove_wire_controls(
        &mut self,
        wire: usize,
        controls: &BTreeSet<usize>,
    ) -> Result<(), String> {
        let edge = self.wires.get(wire).ok_or("Wire is unavailable.")?;
        if controls.is_empty() {
            return Ok(());
        }
        let mut route = self.wire_path(edge);
        route.dedup();
        let points = if edge.waypoints.is_empty() {
            if route.len() > 2 {
                &route[1..route.len() - 1]
            } else {
                &[]
            }
        } else {
            &edge.waypoints
        };
        let removed: Vec<_> = controls
            .iter()
            .map(|index| {
                points
                    .get(*index)
                    .copied()
                    .ok_or_else(|| "Joint is unavailable.".to_owned())
            })
            .collect::<Result<_, _>>()?;
        let waypoints = without_points(&route, &removed);
        if waypoints.len() > 64 {
            return Err("Removing this joint exceeds the route limit.".into());
        }
        self.wires[wire].waypoints = waypoints;
        Ok(())
    }

    /// Concatenate incident routes without keeping an obsolete control at the
    /// deleted joint or dropping a different automatic corner on the branch.
    pub(super) fn splice_joint(&mut self, id: u64) {
        let Some(position) = self.node(id).map(|n| n.position) else {
            return;
        };
        let Some(incoming) = self.wires.iter().find(|w| w.to.node == id).cloned() else {
            return;
        };
        let prefix = self.wire_path(&incoming);
        let branches: Vec<_> = self
            .wires
            .iter()
            .enumerate()
            .filter(|(_, w)| w.from.node == id)
            .map(|(index, wire)| {
                let mut route = prefix.clone();
                route.extend(self.wire_path(wire).into_iter().skip(1));
                route.dedup();
                (index, without_points(&route, &[position]))
            })
            .collect();
        for (index, points) in branches {
            let annotated = if self.wires[index].bus_width.is_some() {
                &self.wires[index]
            } else {
                &incoming
            };
            let anchor = annotated
                .bus_position
                .and_then(|_| Wire::bus_anchor(&self.wire_path(annotated), annotated.bus_position));
            self.wires[index].from = incoming.from;
            self.wires[index].bus_width = self.wires[index].bus_width.or(incoming.bus_width);
            self.wires[index].waypoints = if points.len() <= 64 { points } else { vec![] };
            self.wires[index].bus_position = anchor.and_then(|(center, _)| {
                Wire::project_bus_position(&self.wire_path(&self.wires[index]), center)
                    .map(|(_, fraction)| fraction)
            });
        }
    }
}
