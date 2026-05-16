//! Interactive HTML viewer for a Wish world or repo canvas.
//!
//! Used by:
//! - `wish-world-cli`'s `view` subcommand (browser pop-out).
//! - The Wish desktop app's `OpenRepoCanvas` workspace action.
//!
//! The output is a single self-contained HTML file (no external CDNs):
//! dark themed, pan + zoom, sidebar of entities, click-to-select
//! across canvas and sidebar via `data-semantic-id`, and a WorldLine
//! summary if one is present on disk next to the world.

use std::path::Path;
use wish_canvas_core::types::Canvas;
use wish_world_model::WishWorld;

/// Render a world + an already-laid-out canvas SVG to a complete HTML
/// document. `world_dir` is the optional path to the `.wishworld/`
/// directory on disk — when set, the viewer pulls in the WorldLine
/// summary.
pub fn world_html(world: &WishWorld, svg: &str, world_dir: Option<&Path>) -> String {
    render(
        &world.name,
        &format!("{:?}", world.kind),
        world.entities.len(),
        world.agents.len(),
        entities_html(world),
        agents_html(world),
        worldline_html(world_dir),
        svg,
    )
}

/// Render a generic, world-less canvas (e.g., the repo map) as HTML.
/// The header reads `<title> — Repo Canvas`, the sidebar lists every
/// node in the canvas grouped by kind.
pub fn canvas_html(title: &str, canvas: &Canvas, svg: &str) -> String {
    use std::collections::BTreeMap;
    let mut by_kind: BTreeMap<String, Vec<&wish_canvas_core::types::CanvasNode>> = BTreeMap::new();
    for n in canvas.nodes.values() {
        by_kind
            .entry(format!("{:?}", n.kind))
            .or_default()
            .push(n);
    }

    let mut entity_html = String::new();
    for (kind, mut nodes) in by_kind {
        nodes.sort_by(|a, b| a.label.cmp(&b.label));
        for n in nodes {
            entity_html.push_str(&format!(
                r#"<li class="entity" data-sid="{sid}"><span class="kind">[{kind}]</span> <span class="name">{name}</span></li>"#,
                sid = escape(&n.semantic_id.to_string()),
                kind = escape(&kind),
                name = escape(&n.label),
            ));
        }
    }

    render(
        title,
        &format!("{} nodes · {} edges", canvas.nodes.len(), canvas.edges.len()),
        canvas.nodes.len(),
        0,
        entity_html,
        String::new(),
        String::new(),
        svg,
    )
}

fn entities_html(world: &WishWorld) -> String {
    let mut s = String::new();
    let mut entries: Vec<_> = world.entities.values().collect();
    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    for e in entries {
        s.push_str(&format!(
            r#"<li class="entity" data-sid="{sid}"><span class="kind">[{kind:?}]</span> <span class="name">{name}</span></li>"#,
            sid = escape(&e.id.to_string()),
            kind = e.kind,
            name = escape(&e.display_name),
        ));
    }
    s
}

fn agents_html(world: &WishWorld) -> String {
    let mut s = String::new();
    for a in world.agents.values() {
        s.push_str(&format!(
            r#"<li class="agent" data-sid="{sid}"><span class="kind">[agent]</span> <span class="name">{name}</span> <span class="role">— {role}</span></li>"#,
            sid = escape(&a.id.to_string()),
            name = escape(&a.display_name),
            role = escape(&a.role),
        ));
    }
    s
}

fn worldline_html(world_dir: Option<&Path>) -> String {
    let Some(dir) = world_dir else { return String::new() };
    let wl_path = dir.join("provenance").join("worldline.jsonl");
    if !wl_path.is_file() {
        return String::new();
    }
    let Ok(wl) = wish_provenance::WorldLine::open(wl_path) else {
        return String::new();
    };
    let mut s = format!(
        r#"<details open><summary>WorldLine ({} events · merkle {})</summary><ol class="wl">"#,
        wl.len(),
        short_hex(&wl.merkle_root(wish_provenance::DEFAULT_BRANCH))
    );
    for ev in wl.iter() {
        let actor = match &ev.actor {
            wish_world_model::Actor::Agent { agent_id } => format!("agent:{agent_id}"),
            wish_world_model::Actor::Human { user_id } => format!("human:{user_id}"),
            wish_world_model::Actor::System => "system".into(),
        };
        s.push_str(&format!(
            r#"<li><span class="risk">risk={:.2}</span> <span class="approval">{:?}</span> <span class="actor">{}</span><br><span class="intent">{}</span></li>"#,
            ev.risk_score,
            ev.approval,
            escape(&actor),
            escape(&ev.intent),
        ));
    }
    s.push_str("</ol></details>");
    s
}

fn render(
    title: &str,
    meta: &str,
    n_entities: usize,
    n_agents: usize,
    entity_rows: String,
    agent_rows: String,
    worldline_block: String,
    svg: &str,
) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Wish — {title}</title>
<style>
  :root {{
    --bg: #0e1116;
    --panel: #161b22;
    --fg: #e6edf3;
    --muted: #8b949e;
    --accent: #61afef;
    --border: #2d333b;
  }}
  * {{ box-sizing: border-box; }}
  html, body {{ margin: 0; height: 100%; background: var(--bg); color: var(--fg); font: 13px/1.4 -apple-system, "SF Pro Text", Segoe UI, sans-serif; }}
  header {{ padding: 12px 16px; border-bottom: 1px solid var(--border); display: flex; align-items: baseline; gap: 16px; }}
  header h1 {{ margin: 0; font-size: 18px; }}
  header .meta {{ color: var(--muted); }}
  main {{ display: grid; grid-template-columns: 300px 1fr; height: calc(100% - 48px); }}
  aside {{ background: var(--panel); border-right: 1px solid var(--border); padding: 12px; overflow: auto; }}
  aside h2 {{ font-size: 11px; text-transform: uppercase; color: var(--muted); margin: 16px 0 6px; letter-spacing: 0.05em; }}
  aside ul {{ list-style: none; padding: 0; margin: 0; }}
  aside li {{ padding: 4px 6px; border-radius: 4px; cursor: pointer; font-size: 12px; }}
  aside li:hover {{ background: rgba(97, 175, 239, 0.15); }}
  aside li.active {{ background: rgba(97, 175, 239, 0.30); }}
  aside .kind {{ color: var(--muted); font-size: 11px; }}
  aside .role {{ color: var(--muted); font-size: 11px; }}
  #stage {{ position: relative; overflow: hidden; cursor: grab; }}
  #stage:active {{ cursor: grabbing; }}
  #stage svg {{ width: 100%; height: 100%; display: block; }}
  #stage svg g {{ cursor: pointer; }}
  #stage svg g.selected rect {{ stroke: var(--accent); stroke-width: 2.5; }}
  #toolbar {{ position: absolute; top: 12px; right: 12px; background: rgba(22, 27, 34, 0.85); border: 1px solid var(--border); border-radius: 6px; padding: 4px; display: flex; gap: 2px; }}
  #toolbar button {{ background: none; border: none; color: var(--fg); padding: 4px 10px; cursor: pointer; font: inherit; border-radius: 4px; }}
  #toolbar button:hover {{ background: rgba(255,255,255,0.06); }}
  details summary {{ cursor: pointer; color: var(--muted); margin-top: 12px; padding: 6px 0; }}
  .wl {{ list-style: none; padding-left: 0; }}
  .wl li {{ padding: 4px 0; border-top: 1px dashed var(--border); cursor: default; font-size: 11px; }}
  .wl li:first-child {{ border-top: none; }}
  .risk {{ color: #d29922; }}
  .approval {{ color: #3fb950; }}
  .actor {{ color: var(--muted); }}
  .intent {{ color: var(--fg); }}
  footer {{ position: fixed; bottom: 0; right: 0; padding: 6px 10px; background: rgba(22,27,34,0.85); border-top-left-radius: 6px; color: var(--muted); font-size: 11px; }}
</style>
</head>
<body>
<header>
  <h1>{title}</h1>
  <span class="meta">{meta}</span>
</header>
<main>
  <aside>
    <h2>Entities ({n_entities})</h2>
    <ul id="entities">{entity_rows}</ul>
    <h2>Agents ({n_agents})</h2>
    <ul id="agents">{agent_rows}</ul>
    {worldline_block}
  </aside>
  <section id="stage">
    <div id="toolbar">
      <button id="zoom-in" title="Zoom in">+</button>
      <button id="zoom-out" title="Zoom out">−</button>
      <button id="fit" title="Fit to view">Fit</button>
    </div>
    {svg}
  </section>
</main>
<footer>wish · v0.5.0 World Model IDE</footer>
<script>
(() => {{
  const stage = document.getElementById('stage');
  const svg = stage.querySelector('svg');
  if (!svg) return;
  const ns = svg.namespaceURI;
  const wrap = document.createElementNS(ns, 'g');
  while (svg.firstChild) wrap.appendChild(svg.firstChild);
  svg.appendChild(wrap);
  let tx = 0, ty = 0, scale = 1;
  const apply = () => {{ wrap.setAttribute('transform', `translate(${{tx}} ${{ty}}) scale(${{scale}})`); }};
  apply();
  let dragging = false, sx = 0, sy = 0;
  stage.addEventListener('mousedown', (e) => {{ dragging = true; sx = e.clientX - tx; sy = e.clientY - ty; }});
  window.addEventListener('mouseup', () => {{ dragging = false; }});
  window.addEventListener('mousemove', (e) => {{ if (!dragging) return; tx = e.clientX - sx; ty = e.clientY - sy; apply(); }});
  stage.addEventListener('wheel', (e) => {{
    e.preventDefault();
    const factor = Math.exp(-e.deltaY * 0.002);
    const rect = svg.getBoundingClientRect();
    const px = e.clientX - rect.left, py = e.clientY - rect.top;
    tx = px - (px - tx) * factor;
    ty = py - (py - ty) * factor;
    scale *= factor;
    scale = Math.max(0.05, Math.min(20, scale));
    apply();
  }}, {{ passive: false }});
  document.getElementById('zoom-in').onclick = () => {{ scale *= 1.2; apply(); }};
  document.getElementById('zoom-out').onclick = () => {{ scale /= 1.2; apply(); }};
  document.getElementById('fit').onclick = () => {{ tx = 0; ty = 0; scale = 1; apply(); }};
  const items = [...document.querySelectorAll('aside li[data-sid]')];
  const select = (sid) => {{
    items.forEach((el) => el.classList.toggle('active', el.dataset.sid === sid));
    svg.querySelectorAll('g[data-semantic-id]').forEach((g) => {{
      g.classList.toggle('selected', g.dataset.semanticId === sid);
    }});
    const target = svg.querySelector(`g[data-semantic-id="${{cssEscape(sid)}}"]`);
    if (target) target.scrollIntoView({{ behavior: 'smooth', block: 'center', inline: 'center' }});
  }};
  items.forEach((el) => el.addEventListener('click', () => select(el.dataset.sid)));
  svg.querySelectorAll('g[data-semantic-id]').forEach((g) => {{
    g.addEventListener('click', () => select(g.dataset.semanticId));
  }});
  function cssEscape(s) {{ return s.replace(/(["\\\\])/g, '\\\\$1'); }}
}})();
</script>
</body>
</html>
"##
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn short_hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    if s.len() > 12 {
        format!("{}…", &s[..12])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wish_world_model::{WishWorld, WorldKind};

    #[test]
    fn world_html_is_valid_html_with_svg_and_sidebar() {
        let w = WishWorld::new("test", WorldKind::EducationWorld);
        let html = world_html(&w, "<svg></svg>", None);
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<title>Wish — test</title>"));
        assert!(html.contains("<svg></svg>"));
        assert!(html.contains("Entities (0)"));
        assert!(html.contains("Agents (0)"));
    }

    #[test]
    fn canvas_html_groups_nodes_by_kind() {
        let canvas = Canvas::new();
        let html = canvas_html("repo", &canvas, "<svg></svg>");
        assert!(html.contains("Wish — repo"));
        assert!(html.contains("0 nodes"));
    }
}
