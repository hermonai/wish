//! Canvas exports — SVG and Mermaid.

use crate::types::Canvas;
use std::fmt::Write;

pub fn to_svg(canvas: &Canvas) -> String {
    let (min_x, min_y, max_x, max_y) = bounding_box(canvas);
    let w = (max_x - min_x).max(1.0);
    let h = (max_y - min_y).max(1.0);
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.2} {:.2} {:.2} {:.2}">"#,
        min_x, min_y, w, h
    );

    for edge in canvas.edges.values() {
        let from = canvas.nodes.get(&edge.from);
        let to = canvas.nodes.get(&edge.to);
        if let (Some(a), Some(b)) = (from, to) {
            let (ax, ay) = a.bounds.center();
            let (bx, by) = b.bounds.center();
            let [r, g, bl, al] = edge.style.color;
            let _ = writeln!(
                s,
                r#"  <line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="rgba({},{},{},{})" stroke-width="{:.2}" />"#,
                ax,
                ay,
                bx,
                by,
                r,
                g,
                bl,
                al as f32 / 255.0,
                edge.style.width
            );
        }
    }

    for node in canvas.nodes.values() {
        let [r, g, bl, _] = node.style.fill;
        let [br, bg, bb, _] = node.style.border;
        let _ = writeln!(
            s,
            r#"  <g data-semantic-id="{}">
    <rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="{:.2}" fill="rgb({},{},{})" stroke="rgb({},{},{})" stroke-width="{:.2}" />
    <text x="{:.2}" y="{:.2}" font-family="sans-serif" font-size="11" fill="white">{}</text>
  </g>"#,
            node.semantic_id,
            node.bounds.x,
            node.bounds.y,
            node.bounds.w,
            node.bounds.h,
            node.style.corner_radius,
            r,
            g,
            bl,
            br,
            bg,
            bb,
            node.style.border_width,
            node.bounds.x + 6.0,
            node.bounds.y + node.bounds.h * 0.6,
            escape_xml(&node.label),
        );
    }

    let _ = writeln!(s, "</svg>");
    s
}

pub fn to_mermaid(canvas: &Canvas) -> String {
    let mut s = String::from("graph TD\n");
    for node in canvas.nodes.values() {
        let _ = writeln!(s, "  N{}[{}]", node.id, escape_mermaid(&node.label));
    }
    for edge in canvas.edges.values() {
        let _ = writeln!(s, "  N{} --> N{}", edge.from, edge.to);
    }
    s
}

fn bounding_box(canvas: &Canvas) -> (f32, f32, f32, f32) {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for n in canvas.nodes.values() {
        min_x = min_x.min(n.bounds.x);
        min_y = min_y.min(n.bounds.y);
        max_x = max_x.max(n.bounds.x + n.bounds.w);
        max_y = max_y.max(n.bounds.y + n.bounds.h);
    }
    if !min_x.is_finite() {
        return (0.0, 0.0, 1.0, 1.0);
    }
    (min_x - 8.0, min_y - 8.0, max_x + 8.0, max_y + 8.0)
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_mermaid(s: &str) -> String {
    s.replace(['[', ']'], "_")
}
