//! Bus marker positioning and movement through persistent joints.
use super::*;

fn length(a: Point, b: Point) -> f32 {
    (b.x - a.x).hypot(b.y - a.y)
}

impl Wire {
    /// Resolve a normalized marker position, or the longest-segment midpoint.
    /// Returns its center and whether its segment is predominantly horizontal.
    /// Degenerate routes have no marker.
    pub fn bus_anchor(points: &[Point], position: Option<f32>) -> Option<(Point, bool)> {
        let segments: Vec<_> = points
            .windows(2)
            .filter(|p| length(p[0], p[1]) > 0.001)
            .collect();
        let (pair, fraction) = if let Some(position) = position {
            let total: f32 = segments.iter().map(|p| length(p[0], p[1])).sum();
            let mut remaining = position.clamp(0.0, 1.0) * total;
            let mut result = None;
            for pair in &segments {
                let size = length(pair[0], pair[1]);
                result = Some((*pair, (remaining / size).clamp(0.0, 1.0)));
                if remaining <= size {
                    break;
                }
                remaining -= size;
            }
            result?
        } else {
            (
                *segments
                    .iter()
                    .max_by(|a, b| length(a[0], a[1]).total_cmp(&length(b[0], b[1])))?,
                0.5,
            )
        };
        Some((
            Point::new(
                pair[0].x + (pair[1].x - pair[0].x) * fraction,
                pair[0].y + (pair[1].y - pair[0].y) * fraction,
            ),
            (pair[1].x - pair[0].x).abs() >= (pair[1].y - pair[0].y).abs(),
        ))
    }

    /// Project a point onto a route, returning distance and normalized position.
    /// Degenerate routes return `None`.
    pub fn project_bus_position(points: &[Point], point: Point) -> Option<(f32, f32)> {
        let total: f32 = points.windows(2).map(|p| length(p[0], p[1])).sum();
        if total < 0.001 {
            return None;
        }
        let mut travelled = 0.0;
        let mut best: Option<(f32, f32)> = None;
        for pair in points.windows(2) {
            let size = length(pair[0], pair[1]);
            if size < 0.001 {
                continue;
            }
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            let fraction = (((point.x - pair[0].x) * dx + (point.y - pair[0].y) * dy)
                / (size * size))
                .clamp(0.0, 1.0);
            let projected = Point::new(pair[0].x + dx * fraction, pair[0].y + dy * fraction);
            let distance = length(point, projected);
            if best.is_none_or(|(old, _)| distance < old) {
                best = Some((distance, (travelled + size * fraction) / total));
            }
            travelled += size;
        }
        best
    }
}

impl Circuit {
    /// Find wires reachable through joint nodes, excluding unrelated crossings
    /// and paths through component bodies. An unavailable index returns an empty set.
    pub fn joint_connected_wires(&self, wire: usize) -> BTreeSet<usize> {
        if wire >= self.wires.len() {
            return BTreeSet::new();
        }
        let mut adjacency: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        let joints: BTreeSet<_> = self
            .nodes
            .iter()
            .filter(|node| node.kind == Kind::Joint)
            .map(|node| node.id)
            .collect();
        for (index, edge) in self.wires.iter().enumerate() {
            for endpoint in [edge.from, edge.to] {
                if joints.contains(&endpoint.node) {
                    adjacency.entry(endpoint.node).or_default().push(index);
                }
            }
        }
        let mut seen = BTreeSet::from([wire]);
        let mut pending = vec![wire];
        while let Some(index) = pending.pop() {
            for endpoint in [self.wires[index].from, self.wires[index].to] {
                if let Some(neighbors) = adjacency.remove(&endpoint.node) {
                    for neighbor in neighbors {
                        if seen.insert(neighbor) {
                            pending.push(neighbor);
                        }
                    }
                }
            }
        }
        seen
    }

    /// Move a bus marker along its wire or onto a wire connected through joints.
    /// A matching destination annotation is merged; a different width is preserved.
    /// Connectivity and routes are unchanged.
    ///
    /// # Errors
    /// Returns an error without mutation for invalid positions, missing markers,
    /// disconnected destinations, or a conflicting destination annotation.
    pub fn move_bus_annotation(
        &mut self,
        source: usize,
        target: usize,
        position: f32,
    ) -> Result<(), String> {
        if !position.is_finite() || !(0.0..=1.0).contains(&position) {
            return Err("Bus position must be between 0 and 1.".into());
        }
        let width = self
            .wires
            .get(source)
            .and_then(|wire| wire.bus_width)
            .ok_or("No bus annotation on this wire.")?;
        if !self.joint_connected_wires(source).contains(&target) {
            return Err("Bus annotations can only move across connected joints.".into());
        }
        if self.wires[target]
            .bus_width
            .is_some_and(|other| other != width)
        {
            return Err("This wire already has a different bus annotation.".into());
        }
        self.wires[source].bus_width = None;
        self.wires[source].bus_position = None;
        self.wires[target].bus_width = Some(width);
        self.wires[target].bus_position = Some(position);
        Ok(())
    }
}
