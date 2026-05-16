//! Layout engines. v0.5.0 ships layered + (stub) force-directed.

use crate::types::{Canvas, CanvasEdge, CanvasNodeId, LayoutMode};
use std::collections::{HashMap, HashSet};

pub fn run(canvas: &mut Canvas) {
    match canvas.layout {
        LayoutMode::Manual => {}
        LayoutMode::Layered => layered(canvas),
        LayoutMode::ForceDirected => force_directed_stub(canvas),
        LayoutMode::Grid => grid(canvas),
    }
}

/// A simple layered (Sugiyama-style, simplified) layout for DAG-shaped
/// canvases. Topologically sorts nodes into layers, then evenly spaces
/// each layer horizontally.
///
/// Falls back to a grid if cycles are detected.
fn layered(canvas: &mut Canvas) {
    let ids: Vec<CanvasNodeId> = canvas.nodes.keys().copied().collect();
    if ids.is_empty() {
        return;
    }

    let mut in_degree: HashMap<CanvasNodeId, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut succ: HashMap<CanvasNodeId, Vec<CanvasNodeId>> =
        ids.iter().map(|id| (*id, Vec::new())).collect();
    for CanvasEdge { from, to, .. } in canvas.edges.values() {
        if let Some(s) = succ.get_mut(from) {
            s.push(*to);
        }
        if let Some(d) = in_degree.get_mut(to) {
            *d += 1;
        }
    }

    // Kahn's algorithm with layer tracking.
    let mut layers: Vec<Vec<CanvasNodeId>> = Vec::new();
    let mut placed: HashSet<CanvasNodeId> = HashSet::new();
    let mut current: Vec<CanvasNodeId> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    current.sort();

    while !current.is_empty() {
        layers.push(current.clone());
        let mut next = Vec::new();
        for n in &current {
            placed.insert(*n);
            if let Some(s) = succ.get(n) {
                for m in s {
                    if let Some(d) = in_degree.get_mut(m) {
                        if *d > 0 {
                            *d -= 1;
                            if *d == 0 {
                                next.push(*m);
                            }
                        }
                    }
                }
            }
        }
        next.sort();
        current = next;
    }

    // If we have a cycle, append the rest as one final layer.
    let leftover: Vec<CanvasNodeId> = ids.iter().copied().filter(|id| !placed.contains(id)).collect();
    if !leftover.is_empty() {
        layers.push(leftover);
    }

    let layer_height = 100.0_f32;
    let column_width = 180.0_f32;
    for (layer_idx, layer) in layers.iter().enumerate() {
        let total_w = layer.len() as f32 * column_width;
        let start_x = -total_w * 0.5;
        for (i, id) in layer.iter().enumerate() {
            if let Some(node) = canvas.nodes.get_mut(id) {
                node.bounds.x = start_x + i as f32 * column_width;
                node.bounds.y = layer_idx as f32 * layer_height;
            }
        }
    }
}

/// A trivial grid layout, used as a safe fallback.
fn grid(canvas: &mut Canvas) {
    let n = canvas.nodes.len() as f32;
    let cols = n.sqrt().ceil() as usize;
    let mut ids: Vec<CanvasNodeId> = canvas.nodes.keys().copied().collect();
    ids.sort();
    let cell_w = 160.0_f32;
    let cell_h = 80.0_f32;
    for (idx, id) in ids.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        if let Some(node) = canvas.nodes.get_mut(id) {
            node.bounds.x = col as f32 * cell_w;
            node.bounds.y = row as f32 * cell_h;
        }
    }
}

/// Fruchterman-Reingold force-directed layout.
///
/// Iteratively pushes nodes apart (repulsion) and pulls connected
/// nodes together (attraction), with a cooling schedule that shrinks
/// max displacement each iteration. Deterministic seeding (based on
/// node id) keeps results stable between runs.
fn force_directed_stub(canvas: &mut Canvas) {
    let ids: Vec<CanvasNodeId> = canvas.nodes.keys().copied().collect();
    let n = ids.len();
    if n == 0 {
        return;
    }
    if n == 1 {
        if let Some(node) = canvas.nodes.get_mut(&ids[0]) {
            node.bounds.x = 0.0;
            node.bounds.y = 0.0;
        }
        return;
    }

    let area: f32 = (n as f32) * 30000.0;
    let k = (area / n as f32).sqrt();
    let iterations: usize = if n < 100 { 80 } else { 40 };
    let mut temperature: f32 = (area).sqrt() / 10.0;

    // Deterministic initial placement on a circle (fallback for nodes
    // that have not been positioned yet).
    let circle_radius = (n as f32).sqrt() * 40.0 + 40.0;
    let mut positions: HashMap<CanvasNodeId, (f32, f32)> = HashMap::with_capacity(n);
    for (i, id) in ids.iter().enumerate() {
        let theta = (i as f32) * std::f32::consts::TAU / (n as f32);
        let x = circle_radius * theta.cos();
        let y = circle_radius * theta.sin();
        positions.insert(*id, (x, y));
    }

    let edges: Vec<(CanvasNodeId, CanvasNodeId)> = canvas
        .edges
        .values()
        .map(|e| (e.from, e.to))
        .filter(|(a, b)| positions.contains_key(a) && positions.contains_key(b))
        .collect();

    for _ in 0..iterations {
        let mut disp: HashMap<CanvasNodeId, (f32, f32)> = ids.iter().map(|id| (*id, (0.0, 0.0))).collect();

        // Repulsive forces: O(n^2) — fine for canvas sizes we ship.
        for i in 0..n {
            for j in (i + 1)..n {
                let (vi, vj) = (ids[i], ids[j]);
                let (xi, yi) = positions[&vi];
                let (xj, yj) = positions[&vj];
                let mut dx = xi - xj;
                let mut dy = yi - yj;
                let mut d2 = dx * dx + dy * dy;
                if d2 < 0.0001 {
                    // Deterministic jitter so coincident nodes separate.
                    let jitter = ((vi.wrapping_add(vj)) & 0xFF) as f32 / 255.0 - 0.5;
                    dx = jitter * 0.5;
                    dy = (1.0 - jitter) * 0.5;
                    d2 = dx * dx + dy * dy + 0.0001;
                }
                let dist = d2.sqrt();
                let force = (k * k) / dist;
                let fx = dx / dist * force;
                let fy = dy / dist * force;
                let e = disp.get_mut(&vi).unwrap();
                e.0 += fx;
                e.1 += fy;
                let e = disp.get_mut(&vj).unwrap();
                e.0 -= fx;
                e.1 -= fy;
            }
        }

        // Attractive forces along edges.
        for (a, b) in &edges {
            let (xa, ya) = positions[a];
            let (xb, yb) = positions[b];
            let dx = xa - xb;
            let dy = ya - yb;
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
            let force = (dist * dist) / k;
            let fx = dx / dist * force;
            let fy = dy / dist * force;
            let e = disp.get_mut(a).unwrap();
            e.0 -= fx;
            e.1 -= fy;
            let e = disp.get_mut(b).unwrap();
            e.0 += fx;
            e.1 += fy;
        }

        // Apply displacement, capped by temperature.
        for id in &ids {
            let (dx, dy) = disp[id];
            let dlen = (dx * dx + dy * dy).sqrt().max(0.0001);
            let capped = dlen.min(temperature);
            let p = positions.get_mut(id).unwrap();
            p.0 += dx / dlen * capped;
            p.1 += dy / dlen * capped;
        }

        // Cool.
        temperature *= 0.95;
    }

    // Write back to canvas bounds, preserving each node's existing size.
    for id in &ids {
        if let Some(node) = canvas.nodes.get_mut(id) {
            let (x, y) = positions[id];
            // Position is node center; bounds.x/y is top-left.
            node.bounds.x = x - node.bounds.w * 0.5;
            node.bounds.y = y - node.bounds.h * 0.5;
        }
    }
}
