//! `wish-world` — the v0.5.0 CLI for the Wish World Model IDE.
//!
//! Subcommands:
//!   inspect <world-dir>
//!       Read a `.wishworld/` directory and print its semantic summary.
//!   canvas repo <root> [--format svg|mermaid|json]
//!       Walk a repo, project to a Canvas, print to stdout.
//!   canvas world <world-dir> [--format svg|mermaid|json]
//!       Project a `.wishworld/` to a Canvas, print to stdout.
//!   worldline <world-dir>
//!       Print the WorldLine summary + Merkle root.
//!   demo shanhai <out-dir>
//!       Run the deterministic Shan Hai world builder. Writes the
//!       result to `<out-dir>/shanhai-fintech-harbor.wishworld/`.
//!   agent-dag <run-json> [--format svg|mermaid|json]
//!       Read an AgentRun JSON file, build a DAG canvas, print.
//!   view <world-dir> [--out <path>] [--no-open]
//!       Generate an interactive HTML viewer (pan / zoom / entity
//!       sidebar) and open it in the system browser.
//!
//! This CLI is the *visible proof* that the v0.5.0 world model, the
//! canvas, the codegraph, the provenance ledger, and the world-studio
//! deterministic builder all compose into something a user can run.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use wish_canvas_core::{
    export,
    tensor::TensorSpec,
    types::{Canvas, CanvasNode, CanvasNodeKind, Rect as CanvasRect},
};
use wish_provenance::{WorldLine, DEFAULT_BRANCH};
use wish_world_model::{read_world_dir, write_world_dir, WishWorld, WishWorldBundle, WorldKind};
use wish_world_studio::{build_shanhai_harbor, world_to_canvas};

const HELP: &str = r#"wish-world — the World Model IDE CLI (v0.5.0)

USAGE:
    wish-world inspect <world-dir>
    wish-world canvas repo <root> [--format svg|mermaid|json]
    wish-world canvas world <world-dir> [--format svg|mermaid|json]
    wish-world worldline <world-dir>
    wish-world demo shanhai <out-dir>
    wish-world view <world-dir> [--out <path>] [--no-open]   (browser)
    wish-world render world <world-dir> [--perspective <p>] [--reveal <id>] (native)
    wish-world render repo <root>       [--perspective <p>] [--reveal <id>] (native)
    wish-world render demo              [--perspective <p>] [--reveal <id>] (native)
    wish-world render tensor            [--perspective <p>] [--reveal <id>] (native)
        End-to-end smoke test for the URE × wishUI tensor substrate.
        Builds a canvas of golden tensors (eye, linspace, ripple,
        Gaussian, sine plane) and renders them as inline heatmaps
        inside the native viewer.
        Domain perspectives (8):
            engineering, architecture, spatial, financial,
            education, scientific, design, analytic
        Science / Tensorium perspectives (7):
            math, geometry, chemistry, physics,
            linguistic, geologic, biologic
        --reveal: pan and highlight a node by its SemanticId. Canonical form
                  `realm:kind:stable_key[#instance]`, e.g.
                      code:function:my_mod::my_fn
                      code:file:src/main.rs
                      terminal:block:cargo-test#01HXYZ
    wish-world build "<intent>" [--live] [--out <dir>] [--step-ms <n>]
    wish-world timetravel <world-dir>                         (native, scrub)
    wish-world watch <world-dir> [--poll-ms <n>]              (native, hot-reload)
    wish-world agent "<intent>" --target <world-dir> [--step-ms <n>] [--fresh]
    wish-world swarm "<intent>" --target <world-dir> --count <n> [--step-ms <n>] [--fresh]
    wish-world branches <world-dir>
    wish-world branch <world-dir> <new-branch> [--from <event-id>]
    wish-world block "<shell-cmd>" --target <world-dir>
    wish-world repo-watch <root> --target <world-dir> [--poll-ms <n>] [--functions]
    wish-world canvas repo <root> [--format ...] [--functions]
    wish-world tour [--out <dir>] [--no-open]
    wish-world agent-dag <run-json> [--format svg|mermaid|json]
    wish-world version

INTENT → WORLD
    `wish-world build "<intent>"` plans a sequence of WorldPatches from
    a natural-language prompt and applies them. Add `--live` to open
    a native window that animates the build step-by-step. Templates
    match keywords like "harbor / merchant / stablecoin", "temple /
    dragon / sacred", "service / topology / kubernetes", or
    "education / teacher / student". Anything else falls back to a
    starter world.

INVOCATION:
    From the wish repo root, use one of:
        cargo run --bin wish-world -- <subcommand> …
        cargo run -p wish-world-cli -- <subcommand> …
        ./target/debug/wish-world <subcommand> …
    Or `cargo install --path crates/wish-world-cli` to put it on $PATH.

Every world is a `.wishworld/` directory; see
wish-design/wish-plan-20260514/04-data-model/02-wishworld-format.md.

Wish is the World Model IDE — and a world has a model, a ledger, a
runtime, a finance layer, and a trust layer. That is the moat.
"#;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("wish-world: error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next();
    match cmd.as_deref() {
        None | Some("--help") | Some("-h") | Some("help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("version") | Some("--version") | Some("-V") => {
            println!("wish-world {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("inspect") => {
            let dir = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world inspect <world-dir>"))?;
            cmd_inspect(Path::new(&dir))
        }
        Some("canvas") => {
            let sub = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world canvas repo|world <path>"))?;
            let path = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world canvas {sub} <path>"))?;
            let (format, extra) = parse_format_and_extras(&mut args)?;
            let functions = extra.contains("functions");
            match sub.as_str() {
                "repo" => cmd_canvas_repo(Path::new(&path), format, functions),
                "world" => cmd_canvas_world(Path::new(&path), format),
                other => bail!("unknown canvas subcommand: {other}"),
            }
        }
        Some("worldline") => {
            let dir = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world worldline <world-dir>"))?;
            cmd_worldline(Path::new(&dir))
        }
        Some("demo") => {
            let sub = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world demo shanhai <out-dir>"))?;
            let out = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world demo {sub} <out-dir>"))?;
            match sub.as_str() {
                "shanhai" => cmd_demo_shanhai(Path::new(&out)),
                other => bail!("unknown demo: {other}"),
            }
        }
        Some("agent-dag") => {
            let path = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world agent-dag <run-json>"))?;
            let format = parse_format_flag(&mut args)?;
            cmd_agent_dag(Path::new(&path), format)
        }
        Some("view") => {
            let dir = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world view <world-dir> [--out <path>] [--no-open]")
            })?;
            let (out, open) = parse_view_flags(&mut args)?;
            cmd_view(Path::new(&dir), out, open)
        }
        Some("tour") => {
            let (out, open) = parse_view_flags(&mut args)?;
            cmd_tour(out, open)
        }
        Some("build") => {
            let intent = args.next().ok_or_else(|| {
                anyhow!(
                    "usage: wish-world build \"<intent>\" [--live] [--out <dir>] [--step-ms <n>]"
                )
            })?;
            let (live, out, step_ms) = parse_build_flags(&mut args)?;
            cmd_build(&intent, live, out, step_ms)
        }
        Some("timetravel") => {
            let dir = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world timetravel <world-dir>"))?;
            cmd_timetravel(Path::new(&dir))
        }
        Some("block") => {
            let cmd_str = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world block \"<shell-cmd>\" --target <world-dir>")
            })?;
            let mut target: Option<PathBuf> = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--target" => {
                        target = Some(PathBuf::from(
                            args.next()
                                .ok_or_else(|| anyhow!("--target needs a path"))?,
                        ));
                    }
                    s if s.starts_with("--target=") => {
                        target = Some(PathBuf::from(s.trim_start_matches("--target=")));
                    }
                    other => bail!("unknown flag: {other}"),
                }
            }
            let target =
                target.ok_or_else(|| anyhow!("block: --target <world-dir> is required"))?;
            cmd_block(&cmd_str, &target)
        }
        Some("repo-watch") => {
            let root = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world repo-watch <root> --target <world-dir> [--poll-ms <n>] [--functions]"))?;
            let mut target: Option<PathBuf> = None;
            let mut poll_ms: u64 = 1500;
            let mut functions = false;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--target" => {
                        target = Some(PathBuf::from(
                            args.next()
                                .ok_or_else(|| anyhow!("--target needs a path"))?,
                        ));
                    }
                    s if s.starts_with("--target=") => {
                        target = Some(PathBuf::from(s.trim_start_matches("--target=")));
                    }
                    "--poll-ms" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--poll-ms needs a value"))?;
                        poll_ms = v.parse().map_err(|e| anyhow!("--poll-ms: {e}"))?;
                    }
                    s if s.starts_with("--poll-ms=") => {
                        poll_ms = s
                            .trim_start_matches("--poll-ms=")
                            .parse()
                            .map_err(|e| anyhow!("--poll-ms: {e}"))?;
                    }
                    "--functions" => functions = true,
                    other => bail!("unknown flag: {other}"),
                }
            }
            let target =
                target.ok_or_else(|| anyhow!("repo-watch: --target <world-dir> is required"))?;
            cmd_repo_watch(Path::new(&root), &target, poll_ms, functions)
        }
        Some("branches") => {
            let dir = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world branches <world-dir>"))?;
            cmd_branches(Path::new(&dir))
        }
        Some("branch") => {
            let dir = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world branch <world-dir> <new-branch> [--from <event-id>]")
            })?;
            let new_branch = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world branch <world-dir> <new-branch> [--from <event-id>]")
            })?;
            let mut from: Option<String> = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--from" => {
                        from = Some(args.next().ok_or_else(|| anyhow!("--from needs a value"))?);
                    }
                    s if s.starts_with("--from=") => {
                        from = Some(s.trim_start_matches("--from=").to_string());
                    }
                    other => bail!("unknown flag: {other}"),
                }
            }
            cmd_branch(Path::new(&dir), &new_branch, from.as_deref())
        }
        Some("agent") => {
            let intent = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world agent \"<intent>\" --target <world-dir> [--step-ms <n>] [--fresh]")
            })?;
            let mut target: Option<PathBuf> = None;
            let mut step_ms: u64 = 1500;
            let mut fresh = false;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--target" => {
                        target = Some(PathBuf::from(
                            args.next()
                                .ok_or_else(|| anyhow!("--target needs a path"))?,
                        ));
                    }
                    s if s.starts_with("--target=") => {
                        target = Some(PathBuf::from(s.trim_start_matches("--target=")));
                    }
                    "--step-ms" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--step-ms needs a value"))?;
                        step_ms = v.parse().map_err(|e| anyhow!("--step-ms: {e}"))?;
                    }
                    s if s.starts_with("--step-ms=") => {
                        step_ms = s
                            .trim_start_matches("--step-ms=")
                            .parse()
                            .map_err(|e| anyhow!("--step-ms: {e}"))?;
                    }
                    "--fresh" => fresh = true,
                    other => bail!("unknown flag: {other}"),
                }
            }
            let target =
                target.ok_or_else(|| anyhow!("agent: --target <world-dir> is required"))?;
            cmd_agent(&intent, &target, step_ms, fresh)
        }
        Some("swarm") => {
            let intent = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world swarm \"<intent>\" --target <world-dir> --count <n> [--step-ms <n>] [--fresh]")
            })?;
            let mut target: Option<PathBuf> = None;
            let mut count: usize = 3;
            let mut step_ms: u64 = 1200;
            let mut fresh = false;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--target" => {
                        target = Some(PathBuf::from(
                            args.next()
                                .ok_or_else(|| anyhow!("--target needs a path"))?,
                        ));
                    }
                    s if s.starts_with("--target=") => {
                        target = Some(PathBuf::from(s.trim_start_matches("--target=")));
                    }
                    "--count" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--count needs a value"))?;
                        count = v.parse().map_err(|e| anyhow!("--count: {e}"))?;
                    }
                    s if s.starts_with("--count=") => {
                        count = s
                            .trim_start_matches("--count=")
                            .parse()
                            .map_err(|e| anyhow!("--count: {e}"))?;
                    }
                    "--step-ms" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--step-ms needs a value"))?;
                        step_ms = v.parse().map_err(|e| anyhow!("--step-ms: {e}"))?;
                    }
                    s if s.starts_with("--step-ms=") => {
                        step_ms = s
                            .trim_start_matches("--step-ms=")
                            .parse()
                            .map_err(|e| anyhow!("--step-ms: {e}"))?;
                    }
                    "--fresh" => fresh = true,
                    other => bail!("unknown flag: {other}"),
                }
            }
            let target =
                target.ok_or_else(|| anyhow!("swarm: --target <world-dir> is required"))?;
            if count == 0 {
                bail!("swarm: --count must be ≥ 1");
            }
            cmd_swarm(&intent, &target, count, step_ms, fresh)
        }
        Some("watch") => {
            let dir = args
                .next()
                .ok_or_else(|| anyhow!("usage: wish-world watch <world-dir> [--poll-ms <n>]"))?;
            let mut poll_ms: u64 = 500;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--poll-ms" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--poll-ms needs a value"))?;
                        poll_ms = v.parse().map_err(|e| anyhow!("--poll-ms: {e}"))?;
                    }
                    s if s.starts_with("--poll-ms=") => {
                        poll_ms = s
                            .trim_start_matches("--poll-ms=")
                            .parse()
                            .map_err(|e| anyhow!("--poll-ms: {e}"))?;
                    }
                    other => bail!("unknown flag: {other}"),
                }
            }
            cmd_watch(Path::new(&dir), poll_ms)
        }
        Some("render") => {
            let sub = args.next().ok_or_else(|| {
                anyhow!("usage: wish-world render world|repo|demo [--perspective <name>] <path>")
            })?;
            // Separate positional args from flags so `--perspective`
            // can appear before or after the path (or, for `demo`,
            // without a path at all).
            let mut path: Option<String> = None;
            let mut perspective = wish_render::Perspective::default();
            let mut reveal: Option<wish_world_model::SemanticId> = None;
            let parse_reveal = |v: &str| -> Result<wish_world_model::SemanticId> {
                v.parse().map_err(|e: wish_world_model::ParseSemanticIdError| {
                    anyhow!(
                        "--reveal: {}\n  expected canonical form realm:kind:stable_key[#instance], e.g. code:function:my_mod::my_fn",
                        e
                    )
                })
            };
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--perspective" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--perspective needs a value"))?;
                        perspective = wish_render::Perspective::from_slug(&v).ok_or_else(|| {
                            anyhow!(
                                "unknown perspective: {v}. valid: {}",
                                wish_render::Perspective::ALL
                                    .iter()
                                    .map(|p| p.slug())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })?;
                    }
                    s if s.starts_with("--perspective=") => {
                        let v = s.trim_start_matches("--perspective=");
                        perspective = wish_render::Perspective::from_slug(v)
                            .ok_or_else(|| anyhow!("unknown perspective: {v}"))?;
                    }
                    "--reveal" => {
                        let v = args
                            .next()
                            .ok_or_else(|| anyhow!("--reveal needs a SemanticId value"))?;
                        reveal = Some(parse_reveal(&v)?);
                    }
                    s if s.starts_with("--reveal=") => {
                        let v = s.trim_start_matches("--reveal=");
                        reveal = Some(parse_reveal(v)?);
                    }
                    s if s.starts_with("--") => {
                        bail!("unknown flag: {s}");
                    }
                    other if path.is_none() => {
                        path = Some(other.to_string());
                    }
                    other => bail!("unexpected positional arg: {other}"),
                }
            }
            match sub.as_str() {
                "world" => {
                    let dir = path.ok_or_else(|| {
                        anyhow!("usage: wish-world render world <world-dir> [--perspective <name>] [--reveal <semantic-id>]")
                    })?;
                    cmd_render_world(Path::new(&dir), perspective, reveal)
                }
                "repo" => {
                    let root = path.ok_or_else(|| {
                        anyhow!("usage: wish-world render repo <root> [--perspective <name>] [--reveal <semantic-id>]")
                    })?;
                    cmd_render_repo(Path::new(&root), perspective, reveal)
                }
                "demo" => cmd_render_demo(perspective, reveal),
                "tensor" => cmd_render_tensor(perspective, reveal),
                other => bail!("unknown render target: {other}"),
            }
        }
        Some(other) => bail!("unknown subcommand: {other}\n\n{HELP}"),
    }
}

#[derive(Debug, Clone, Copy)]
enum Format {
    Svg,
    Mermaid,
    Json,
    Text,
    /// Top-level architecture view in Mermaid flowchart notation.
    /// One node per crate, edges for inter-crate deps, sub-labels
    /// with file/fn counts. The post-UML kingdom.
    ArchitectureMermaid,
}

fn parse_format_flag(args: &mut impl Iterator<Item = String>) -> Result<Format> {
    let mut fmt = Format::Text;
    while let Some(arg) = args.next() {
        if let Some(val) = arg.strip_prefix("--format=") {
            fmt = parse_format(val)?;
        } else if arg == "--format" {
            let val = args
                .next()
                .ok_or_else(|| anyhow!("--format needs a value"))?;
            fmt = parse_format(&val)?;
        } else {
            bail!("unknown flag: {arg}");
        }
    }
    Ok(fmt)
}

/// `--format <…>` plus a set of boolean flags from a known whitelist
/// (`--functions`, etc.). Used by subcommands that take both.
fn parse_format_and_extras(
    args: &mut impl Iterator<Item = String>,
) -> Result<(Format, std::collections::HashSet<String>)> {
    let mut fmt = Format::Text;
    let mut flags: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(arg) = args.next() {
        if let Some(val) = arg.strip_prefix("--format=") {
            fmt = parse_format(val)?;
        } else if arg == "--format" {
            let val = args
                .next()
                .ok_or_else(|| anyhow!("--format needs a value"))?;
            fmt = parse_format(&val)?;
        } else if arg == "--functions" {
            flags.insert("functions".into());
        } else {
            bail!("unknown flag: {arg}");
        }
    }
    Ok((fmt, flags))
}

fn parse_format(v: &str) -> Result<Format> {
    Ok(match v {
        "svg" => Format::Svg,
        "mermaid" => Format::Mermaid,
        "json" => Format::Json,
        "text" => Format::Text,
        "architecture" | "arch" | "architecture-mermaid" | "arch-mermaid" => {
            Format::ArchitectureMermaid
        }
        _ => bail!("unknown format: {v}"),
    })
}

// -- subcommand implementations ----------------------------------------------

fn cmd_inspect(dir: &Path) -> Result<()> {
    let bundle = read_world_dir(dir).with_context(|| format!("read {}", dir.display()))?;
    let w = &bundle.world;
    println!("World: {}", w.name);
    println!("  id:        {}", w.id);
    println!("  kind:      {:?}", w.kind);
    if !w.intent.is_empty() {
        println!("  intent:    {}", w.intent);
    }
    println!("  entities:  {}", w.entities.len());
    println!("  scenes:    {}", w.scenes.len());
    println!("  agents:    {}", w.agents.len());
    println!("  assets:    {}", w.assets.len());
    println!("  rules:     {}", w.rules.len());
    println!("  missions:  {}", bundle.missions.len());
    println!("  artifacts: {}", bundle.artifacts.len());

    if !w.entities.is_empty() {
        println!("\nEntities:");
        let mut entries: Vec<_> = w.entities.values().collect();
        entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        for e in entries {
            println!("  • {:<32} [{:?}] {}", e.display_name, e.kind, e.id);
        }
    }
    if !w.agents.is_empty() {
        println!("\nAgents:");
        for a in w.agents.values() {
            println!(
                "  • {:<24} role={} tools={}",
                a.display_name,
                a.role,
                a.tools.len()
            );
        }
    }

    // Worldline summary if present.
    let wl_path = dir.join("provenance").join("worldline.jsonl");
    if wl_path.is_file() {
        let wl = WorldLine::open(wl_path).context("open worldline")?;
        println!("\nWorldLine:");
        println!("  events:        {}", wl.len());
        println!(
            "  merkle (main): {}",
            hex_lower(&wl.merkle_root(DEFAULT_BRANCH))
        );
    }
    Ok(())
}

fn cmd_canvas_repo(root: &Path, format: Format, functions: bool) -> Result<()> {
    // The architecture-mermaid format implicitly needs function counts
    // to render meaningful sub-labels. Auto-enable extraction.
    let want_functions = functions || matches!(format, Format::ArchitectureMermaid);
    let opts = wish_codegraph::ExtractOptions::default().with_functions(want_functions);
    let graph = wish_codegraph::extract_repo_graph_with(root, &opts);
    if want_functions {
        eprintln!(
            "wish-world: walked {} ({} crates, {} files, {} dep edges, {} functions, {} call edges)",
            root.display(),
            graph.crates.len(),
            graph.files.len(),
            graph.deps.len(),
            graph.functions.len(),
            graph.calls.len(),
        );
    } else {
        eprintln!(
            "wish-world: walked {} ({} crates, {} files, {} dep edges)",
            root.display(),
            graph.crates.len(),
            graph.files.len(),
            graph.deps.len()
        );
    }
    // Architecture view bypasses the Canvas projection — it's its own
    // exporter sitting directly on the RepoGraph.
    if matches!(format, Format::ArchitectureMermaid) {
        print!("{}", wish_codegraph::architecture::to_mermaid(&graph));
        return Ok(());
    }
    let canvas = wish_codegraph::to_canvas(&graph);
    emit_canvas(&canvas, format)
}

/// `wish-world block "<shell-cmd>" --target <world-dir>` — run a shell
/// command, capture stdout/stderr/exit, and write the resulting
/// `TerminalBlock` entity into the world's WorldLine. Your shell
/// history becomes part of the world model — every command is a
/// SemanticId-tagged WorldEvent.
fn cmd_block(shell_cmd: &str, target: &Path) -> Result<()> {
    use std::process::Command;
    use wish_provenance::WorldLine;
    use wish_world_model::{
        Actor, Component, EntityKind, PatchOp, Realm, SemanticId, WorldEntity, WorldPatch,
    };

    std::fs::create_dir_all(target).context("create world dir")?;
    // Seed the world skeleton if missing so the watcher can find it.
    let manifest = target.join("world.json");
    if !manifest.exists() {
        let world = wish_world_model::WishWorld::new(
            "Terminal World",
            wish_world_model::WorldKind::GenericProject,
        );
        let bundle = WishWorldBundle {
            world,
            missions: Default::default(),
            artifacts: Default::default(),
        };
        write_world_dir(target, &bundle).context("seed skeleton")?;
    }

    // Run via the user's shell so pipes, env, and aliases work.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    eprintln!("wish-world block: $ {}", shell_cmd);
    let started = chrono::Utc::now();
    let output = Command::new(&shell)
        .arg("-c")
        .arg(shell_cmd)
        .output()
        .with_context(|| format!("spawn {shell}"))?;
    let finished = chrono::Utc::now();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    // The block's stable key is a hash of cmd + started-ts so reruns
    // of the same command produce distinct SemanticIds.
    let stable_key = format!(
        "{}-{}",
        sanitize_key(shell_cmd),
        started.timestamp_nanos_opt().unwrap_or(0)
    );
    let sid = SemanticId::new(Realm::Terminal, "block", stable_key);

    // Truncate captured output so a giant command doesn't bloat the
    // ledger. The full output stays on the user's terminal.
    let trim = |s: String| {
        const MAX: usize = 4096;
        if s.len() > MAX {
            format!("{}…(truncated {} bytes)", &s[..MAX], s.len() - MAX)
        } else {
            s
        }
    };
    let stdout_t = trim(stdout);
    let stderr_t = trim(stderr);

    let mut entity = WorldEntity::stub(sid.clone(), shell_cmd, EntityKind::TerminalBlock);
    entity.agent_editable = false;
    // Stash captured output as Custom components so any downstream
    // tool can re-read it.
    entity.components.push(Component::Custom(serde_json::json!({
        "kind": "shell.command",
        "shell": shell,
        "argv": shell_cmd,
        "started_at": started.to_rfc3339(),
        "finished_at": finished.to_rfc3339(),
        "exit_code": code,
        "stdout": stdout_t,
        "stderr": stderr_t,
    })));

    let patch = WorldPatch::new(
        Actor::Human { user_id: whoami() },
        format!("$ {}", shell_cmd),
        vec![PatchOp::AddEntity(entity)],
    );
    let mut wl = WorldLine::open_in_world_dir(target).context("open worldline")?;
    let mut world = read_world_dir(target).context("read world")?.world;
    // wish-provenance owns the worldline; the WishWorld's slim
    // provenance tail is a separate type — clear it before writing
    // back so we don't double-serialize and confuse the JSONL reader.
    world.provenance.clear();
    let outcome = wl
        .apply_with_provenance(&mut world, patch, 0.30)
        .map_err(|e| anyhow!("apply: {e}"))?;

    // Persist the new world snapshot back to disk so the entity is
    // visible to non-watching viewers as well. world.provenance is
    // already cleared, so write_world_dir won't touch worldline.jsonl.
    let bundle = WishWorldBundle {
        world,
        missions: Default::default(),
        artifacts: Default::default(),
    };
    write_world_dir(target, &bundle).context("write world")?;

    eprintln!(
        "wish-world block: ✓ exit={} ({}) — event {:?} written",
        code,
        if code == 0 { "ok" } else { "fail" },
        outcome
    );
    Ok(())
}

fn sanitize_key(s: &str) -> String {
    s.chars()
        .take(40)
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "human".into())
}

/// `wish-world repo-watch <root> --target <world-dir>` — continuously
/// walk a codebase and emit a `repo_snapshot` patch every time the
/// recorded crate/file/function count changes. Pair with
/// `wish-world watch <world-dir>` to render code changes live in the
/// native viewer.
fn cmd_repo_watch(root: &Path, target: &Path, poll_ms: u64, functions: bool) -> Result<()> {
    use std::collections::HashMap;
    use wish_provenance::WorldLine;
    use wish_world_model::{Actor, PatchOp, WorldPatch};

    std::fs::create_dir_all(target).context("create world dir")?;
    // Seed skeleton.
    let manifest = target.join("world.json");
    if !manifest.exists() {
        let world = wish_world_model::WishWorld::new(
            "Repo Watch",
            wish_world_model::WorldKind::GenericProject,
        );
        let bundle = WishWorldBundle {
            world,
            missions: Default::default(),
            artifacts: Default::default(),
        };
        write_world_dir(target, &bundle).context("seed skeleton")?;
    }

    eprintln!(
        "wish-world repo-watch: watching {} (poll every {}ms, functions={})",
        root.display(),
        poll_ms,
        functions
    );
    eprintln!(
        "wish-world repo-watch: in another terminal, run:\n   cargo run --bin wish-world -- watch {}",
        target.display()
    );

    let opts = wish_codegraph::ExtractOptions::default().with_functions(functions);
    let mut last_signature: Option<HashMap<String, u64>> = None;

    loop {
        // Walk + extract.
        let graph = wish_codegraph::extract_repo_graph_with(root, &opts);
        let mut sig: HashMap<String, u64> = HashMap::new();
        sig.insert("crates".into(), graph.crates.len() as u64);
        sig.insert("files".into(), graph.files.len() as u64);
        sig.insert("deps".into(), graph.deps.len() as u64);
        sig.insert("functions".into(), graph.functions.len() as u64);
        sig.insert("calls".into(), graph.calls.len() as u64);

        let changed = last_signature
            .as_ref()
            .map(|prev| prev != &sig)
            .unwrap_or(true);

        if changed {
            let mut world = read_world_dir(target).context("read world")?.world;
            world.provenance.clear();
            let mut wl = WorldLine::open_in_world_dir(target).context("open worldline")?;
            let patch = wish_codegraph::to_world_patch(&graph);
            // Convert into a single bulk patch with a synthetic
            // human-friendly intent.
            let intent = format!(
                "repo snapshot: {} crates · {} files · {} deps{}",
                sig["crates"],
                sig["files"],
                sig["deps"],
                if functions {
                    format!(" · {} fns · {} calls", sig["functions"], sig["calls"])
                } else {
                    String::new()
                }
            );
            // Wrap the existing patch's ops into a fresh WorldPatch
            // with a better intent + actor.
            let ops: Vec<PatchOp> = patch.ops;
            let wrapped = WorldPatch::new(
                Actor::Agent {
                    agent_id: "wish-repo-watch".into(),
                },
                intent.clone(),
                ops,
            );
            // Apply at a slightly looser auto-approve so the bulk
            // snapshots don't pile up in the pending queue.
            let _ = wl.apply_with_provenance(&mut world, wrapped, 1.0);
            let bundle = WishWorldBundle {
                world,
                missions: Default::default(),
                artifacts: Default::default(),
            };
            write_world_dir(target, &bundle).context("write world")?;
            eprintln!("wish-world repo-watch: tick — {}", intent);
            last_signature = Some(sig);
        }

        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}

fn cmd_canvas_world(dir: &Path, format: Format) -> Result<()> {
    let bundle = read_world_dir(dir).context("read world dir")?;
    let canvas = world_to_canvas(&bundle.world);
    emit_canvas(&canvas, format)
}

fn cmd_worldline(dir: &Path) -> Result<()> {
    let wl_path = dir.join("provenance").join("worldline.jsonl");
    if !wl_path.is_file() {
        bail!("no worldline at {}", wl_path.display());
    }
    let wl = WorldLine::open(wl_path).context("open worldline")?;
    println!("WorldLine: {}", dir.display());
    println!("  events: {}", wl.len());
    println!(
        "  merkle root (main): {}",
        hex_lower(&wl.merkle_root(DEFAULT_BRANCH))
    );
    println!();
    for (i, ev) in wl.iter().enumerate() {
        println!(
            "  [{:>3}] {} risk={:.2} approval={:?}",
            i, ev.id, ev.risk_score, ev.approval
        );
        println!("        intent: {}", ev.intent);
        let actor = match &ev.actor {
            wish_world_model::Actor::Agent { agent_id } => format!("agent:{agent_id}"),
            wish_world_model::Actor::Human { user_id } => format!("human:{user_id}"),
            wish_world_model::Actor::System => "system".to_string(),
        };
        println!("        actor:  {actor}");
        if !ev.affected.is_empty() {
            println!(
                "        affected: [{}]",
                ev.affected
                    .iter()
                    .map(|sid| sid.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}

fn cmd_demo_shanhai(out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir).context("create out-dir")?;
    let world_dir = out_dir.join("shanhai-fintech-harbor.wishworld");
    std::fs::create_dir_all(&world_dir).context("create world dir")?;

    let mut world = WishWorld::new("Shan Hai Fintech Harbor", WorldKind::EducationWorld);
    let mut wl = WorldLine::open_in_world_dir(&world_dir).context("open worldline")?;

    eprintln!("wish-world: building Shan Hai Fintech Harbor world…");
    let build = build_shanhai_harbor(&mut world, &mut wl).context("build shanhai")?;
    let bundle = WishWorldBundle {
        world,
        missions: std::iter::once((build.mission.id.clone(), build.mission.clone())).collect(),
        artifacts: build.artifacts.clone(),
    };
    write_world_dir(&world_dir, &bundle).context("write world dir")?;

    println!("✨ Shan Hai Fintech Harbor built");
    println!("   world dir: {}", world_dir.display());
    println!(
        "   entities:  {}   agents: {}   scenes: {}   worldline events: {}   artifacts: {}",
        bundle.world.entities.len(),
        bundle.world.agents.len(),
        bundle.world.scenes.len(),
        wl.len(),
        bundle.artifacts.len()
    );
    println!(
        "   merkle root (main): {}",
        hex_lower(&wl.merkle_root(DEFAULT_BRANCH))
    );
    println!("\nNext (run from this repo root):");
    println!(
        "   cargo run --bin wish-world -- view {}",
        world_dir.display()
    );
    println!(
        "   cargo run --bin wish-world -- inspect {}",
        world_dir.display()
    );
    println!(
        "   cargo run --bin wish-world -- worldline {}",
        world_dir.display()
    );
    println!(
        "   cargo run --bin wish-world -- canvas world {} --format svg > harbor.svg",
        world_dir.display()
    );
    println!("\nTo put `wish-world` on $PATH:");
    println!("   cargo install --path crates/wish-world-cli");
    Ok(())
}

fn parse_view_flags(args: &mut impl Iterator<Item = String>) -> Result<(Option<PathBuf>, bool)> {
    let mut out: Option<PathBuf> = None;
    let mut open = true;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-open" => open = false,
            "--out" => {
                let v = args.next().ok_or_else(|| anyhow!("--out needs a path"))?;
                out = Some(PathBuf::from(v));
            }
            s if s.starts_with("--out=") => {
                out = Some(PathBuf::from(s.trim_start_matches("--out=")));
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    Ok((out, open))
}

fn cmd_view(dir: &Path, out: Option<PathBuf>, open: bool) -> Result<()> {
    let bundle = read_world_dir(dir).context("read world dir")?;
    let canvas = world_to_canvas(&bundle.world);
    let svg = export::to_svg(&canvas);
    let html = wish_world_studio::world_html(&bundle.world, &svg, Some(dir));

    let out_path = out.unwrap_or_else(|| dir.join("view.html"));
    std::fs::write(&out_path, &html).with_context(|| format!("write {}", out_path.display()))?;
    eprintln!("wish-world: viewer written to {}", out_path.display());

    if open {
        let url = format!(
            "file://{}",
            out_path
                .canonicalize()
                .unwrap_or(out_path.clone())
                .display()
        );
        if let Err(e) = open_in_browser(&url) {
            eprintln!("wish-world: could not auto-open browser ({e}). Open manually: {url}");
        } else {
            eprintln!("wish-world: opened in your default browser");
        }
    }
    Ok(())
}

fn cmd_tour(out: Option<PathBuf>, open: bool) -> Result<()> {
    let default_root = std::env::temp_dir().join("wish-tour");
    let root = out.unwrap_or(default_root);
    if root.exists() {
        std::fs::remove_dir_all(&root).ok();
    }
    std::fs::create_dir_all(&root).context("create tour root")?;
    eprintln!("wish-world tour: working in {}", root.display());

    // 1. Build the Shan Hai world.
    cmd_demo_shanhai(&root)?;
    let world_dir = root.join("shanhai-fintech-harbor.wishworld");

    // 2. Generate and open the interactive viewer.
    cmd_view(&world_dir, Some(root.join("view.html")), open)?;

    println!("\n🌸  Wish tour complete.");
    println!("   World:    {}", world_dir.display());
    println!("   Viewer:   {}", root.join("view.html").display());
    println!("\nTry next:");
    println!(
        "   cargo run --bin wish-world -- worldline {}",
        world_dir.display()
    );
    println!(
        "   cargo run --bin wish-world -- inspect {}",
        world_dir.display()
    );
    println!("   cargo run --bin wish-world -- canvas repo $(pwd) --format text | head -40");
    Ok(())
}

fn parse_build_flags(
    args: &mut impl Iterator<Item = String>,
) -> Result<(bool, Option<PathBuf>, u64)> {
    let mut live = false;
    let mut out: Option<PathBuf> = None;
    let mut step_ms: u64 = 600;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--live" => live = true,
            "--out" => {
                let v = args
                    .next()
                    .ok_or_else(|| anyhow!("--out needs a directory"))?;
                out = Some(PathBuf::from(v));
            }
            s if s.starts_with("--out=") => {
                out = Some(PathBuf::from(s.trim_start_matches("--out=")));
            }
            "--step-ms" => {
                let v = args
                    .next()
                    .ok_or_else(|| anyhow!("--step-ms needs a value"))?;
                step_ms = v.parse().map_err(|e| anyhow!("--step-ms: {e}"))?;
            }
            s if s.starts_with("--step-ms=") => {
                step_ms = s
                    .trim_start_matches("--step-ms=")
                    .parse()
                    .map_err(|e| anyhow!("--step-ms: {e}"))?;
            }
            other => bail!("unknown flag: {other}"),
        }
    }
    Ok((live, out, step_ms))
}

/// `wish-world build "<intent>"` — plan a world from a prompt, apply
/// it through a real WorldLine (in `out` or a fresh temp dir), and
/// optionally animate the build in a native window.
fn cmd_build(intent: &str, live: bool, out: Option<PathBuf>, step_ms: u64) -> Result<()> {
    let plan = wish_world_studio::plan_world(intent);
    eprintln!(
        "wish-world: intent matched template '{}' with {} patches",
        plan.template,
        plan.patches.len()
    );

    if live {
        wish_splash(wish_render::Perspective::default());
        // Animate in a native window. Patches flow through a
        // temporary WorldLine inside wish-render::run_live.
        wish_render::run_live(plan, std::time::Duration::from_millis(step_ms))
            .map_err(|e| anyhow!("wish-render exited: {e}"))?;
        return Ok(());
    }

    // Headless build: apply all patches synchronously through a real
    // WorldLine on disk (or in a temp dir if no --out given).
    use wish_provenance::WorldLine;
    let world_dir = match out {
        Some(d) => d,
        None => std::env::temp_dir().join(format!(
            "wish-build-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )),
    };
    std::fs::create_dir_all(&world_dir).context("create build dir")?;
    let mut world = plan.world.clone();
    let mut wl = WorldLine::open_in_world_dir(&world_dir).context("open worldline")?;
    let outcomes = wish_world_studio::apply_plan(&plan, &mut world, &mut wl)
        .map_err(|e| anyhow!("apply plan: {e}"))?;
    let bundle = WishWorldBundle {
        world: world.clone(),
        missions: Default::default(),
        artifacts: Default::default(),
    };
    write_world_dir(&world_dir, &bundle).context("write world dir")?;

    println!("✨ built '{}' from intent", plan.template);
    println!("   world dir: {}", world_dir.display());
    println!(
        "   entities:  {}   agents: {}   scenes: {}   worldline events: {}",
        world.entities.len(),
        world.agents.len(),
        world.scenes.len(),
        wl.len()
    );
    println!(
        "   applied {} patches ({} auto-approved, {} pending)",
        outcomes.len(),
        outcomes
            .iter()
            .filter(|o| matches!(o, wish_provenance::ApplyOutcome::Applied { .. }))
            .count(),
        outcomes
            .iter()
            .filter(|o| matches!(o, wish_provenance::ApplyOutcome::Pending { .. }))
            .count(),
    );
    println!(
        "   merkle root (main): {}",
        hex_lower(&wl.merkle_root(DEFAULT_BRANCH))
    );
    println!("\nNext:");
    println!(
        "   cargo run --bin wish-world -- render world {}",
        world_dir.display()
    );
    println!(
        "   cargo run --bin wish-world -- view {}",
        world_dir.display()
    );
    Ok(())
}

/// `wish-world agent "<intent>" --target <world-dir>` — run a self-driven
/// agent process that emits one WorldPatch per `step_ms` against the
/// target `.wishworld/`'s worldline. Pair with `wish-world watch
/// <world-dir>` in another terminal to see the world materialize live
/// across two cooperating processes.
///
/// `--fresh` wipes the target world dir before starting so each run
/// is reproducible.
fn cmd_agent(intent: &str, target: &Path, step_ms: u64, fresh: bool) -> Result<()> {
    use wish_provenance::WorldLine;

    if fresh && target.exists() {
        eprintln!(
            "wish-world agent: clearing {} for a fresh run",
            target.display()
        );
        std::fs::remove_dir_all(target).context("clear target")?;
    }
    std::fs::create_dir_all(target).context("create target")?;

    let plan = wish_world_studio::plan_world(intent);
    eprintln!(
        "wish-world agent: intent matched template '{}' with {} patches; emitting one every {}ms",
        plan.template,
        plan.patches.len(),
        step_ms,
    );

    // Seed the world skeleton (id / name / kind / intent) so the
    // watcher's first read picks up a sane manifest. The watcher only
    // needs world.json + provenance/worldline.jsonl to start.
    let world = plan.world.clone();
    let mut wl = WorldLine::open_in_world_dir(target).context("open worldline")?;
    {
        let bundle = WishWorldBundle {
            world: world.clone(),
            missions: Default::default(),
            artifacts: Default::default(),
        };
        write_world_dir(target, &bundle).context("write skeleton")?;
    }

    eprintln!("wish-world agent: target ready at {}", target.display());
    eprintln!(
        "wish-world agent: in another terminal, run:\n   cargo run --bin wish-world -- watch {}",
        target.display()
    );

    let mut world = world;
    let total = plan.patches.len();
    for (i, patch) in plan.patches.into_iter().enumerate() {
        std::thread::sleep(std::time::Duration::from_millis(step_ms));
        eprintln!(
            "wish-world agent: step {}/{} — {}",
            i + 1,
            total,
            patch.intent
        );
        let outcome = wl
            .apply_with_provenance(&mut world, patch, 0.30)
            .map_err(|e| anyhow!("apply: {e}"))?;
        if !matches!(outcome, wish_provenance::ApplyOutcome::Applied { .. }) {
            eprintln!("wish-world agent: patch held pending — stopping");
            break;
        }
    }

    // Persist the final world (entities / scenes / agents) so non-watching
    // viewers can `inspect` the result.
    let bundle = WishWorldBundle {
        world,
        missions: Default::default(),
        artifacts: Default::default(),
    };
    write_world_dir(target, &bundle).context("write final world")?;

    eprintln!(
        "wish-world agent: ✓ done. {} events written to {}",
        wl.len(),
        target.display()
    );
    Ok(())
}

/// `wish-world swarm "<intent>" --target <world-dir> --count N` — spawn
/// **N concurrent agents** that collaborate on building one world.
///
/// Patches from the intent template are partitioned round-robin
/// across the agents, so each agent owns a disjoint slice of the work.
/// All N threads share a single `Arc<Mutex<WorldLine>>` + `Arc<Mutex<WishWorld>>`,
/// so WorldLine writes are serialized and the world state stays
/// consistent under concurrent emission. Each agent prefixes its log
/// lines with `[agent-i]` so you can see who emitted what.
///
/// Pair with `wish-world watch <world-dir>` in another terminal to see
/// the swarm collaborate live.
fn cmd_swarm(intent: &str, target: &Path, count: usize, step_ms: u64, fresh: bool) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use wish_provenance::WorldLine;

    if fresh && target.exists() {
        eprintln!(
            "wish-world swarm: clearing {} for a fresh run",
            target.display()
        );
        std::fs::remove_dir_all(target).context("clear target")?;
    }
    std::fs::create_dir_all(target).context("create target")?;

    let plan = wish_world_studio::plan_world(intent);
    eprintln!(
        "wish-world swarm: intent matched template '{}' with {} patches; {} agents at {}ms cadence",
        plan.template,
        plan.patches.len(),
        count,
        step_ms,
    );

    // Seed the world skeleton so the watcher can `read_world_dir` it
    // before any patches land.
    let world = plan.world.clone();
    {
        let bundle = WishWorldBundle {
            world: world.clone(),
            missions: Default::default(),
            artifacts: Default::default(),
        };
        write_world_dir(target, &bundle).context("write skeleton")?;
    }

    let wl = Arc::new(Mutex::new(
        WorldLine::open_in_world_dir(target).context("open worldline")?,
    ));
    let world = Arc::new(Mutex::new(world));

    // Round-robin partition: agent i gets patches[i], patches[i+count], …
    let total = plan.patches.len();
    let mut groups: Vec<Vec<wish_world_model::WorldPatch>> =
        (0..count).map(|_| Vec::new()).collect();
    for (idx, patch) in plan.patches.into_iter().enumerate() {
        groups[idx % count].push(patch);
    }

    eprintln!("wish-world swarm: target ready at {}", target.display());
    eprintln!(
        "wish-world swarm: in another terminal, run:\n   cargo run --bin wish-world -- watch {}",
        target.display()
    );

    let handles: Vec<_> = groups
        .into_iter()
        .enumerate()
        .map(|(i, patches)| {
            let wl = wl.clone();
            let world = world.clone();
            let n_patches = patches.len();
            thread::spawn(move || -> anyhow::Result<()> {
                let agent_name = format!("agent-{i}");
                for (step, patch) in patches.into_iter().enumerate() {
                    // Stagger each agent by `step_ms` so the swarm
                    // interleaves cleanly instead of bursting on the
                    // same tick.
                    thread::sleep(Duration::from_millis(step_ms));
                    eprintln!(
                        "[{agent_name}] step {}/{} — {}",
                        step + 1,
                        n_patches,
                        patch.intent
                    );
                    // Acquire the locks in fixed order (worldline first,
                    // then world) to avoid deadlock.
                    let mut wl = wl.lock().unwrap();
                    let mut world = world.lock().unwrap();
                    let _ = wl
                        .apply_with_provenance(&mut world, patch, 0.30)
                        .map_err(|e| eprintln!("[{agent_name}] apply error: {e}"));
                }
                let _ = n_patches; // silence unused warning if count==0
                Ok(())
            })
        })
        .collect();

    for h in handles {
        let _ = h.join();
    }

    // Persist the final world.
    let world_snapshot = world.lock().unwrap().clone();
    let final_events = wl.lock().unwrap().len();
    let bundle = WishWorldBundle {
        world: world_snapshot,
        missions: Default::default(),
        artifacts: Default::default(),
    };
    write_world_dir(target, &bundle).context("write final world")?;

    eprintln!(
        "wish-world swarm: ✓ done. {} events written by {} agents to {}",
        final_events,
        count,
        target.display()
    );
    let _ = total;
    Ok(())
}

/// `wish-world branches <world-dir>` — list every branch in the
/// world's WorldLine plus the per-branch event count and the
/// currently-active branch.
fn cmd_branches(dir: &Path) -> Result<()> {
    use wish_provenance::WorldLine;
    let wl_path = dir.join("provenance").join("worldline.jsonl");
    if !wl_path.is_file() {
        bail!("no worldline at {}", wl_path.display());
    }
    let wl = WorldLine::open(wl_path).context("open worldline")?;
    let branches = wl.branches();
    println!("Branches in {}", dir.display());
    println!("  current: {}", wl.current_branch());
    println!();
    for b in &branches {
        let count = wl.count_on(b);
        let marker = if b == wl.current_branch() { "*" } else { " " };
        println!("  {marker} {:<24} {count} events", b);
    }
    Ok(())
}

/// `wish-world branch <world-dir> <new-branch> [--from <event-id>]` —
/// fork the WorldLine. Writes a `branch_from` marker event so the
/// fork point is provenance-anchored, then switches the active
/// branch so subsequent `wish-world agent` / `swarm` writes land on
/// the new branch.
fn cmd_branch(dir: &Path, new_branch: &str, from: Option<&str>) -> Result<()> {
    use wish_provenance::WorldLine;
    let wl_path = dir.join("provenance").join("worldline.jsonl");
    let mut wl = WorldLine::open(wl_path.clone())
        .with_context(|| format!("open worldline at {}", wl_path.display()))?;
    let marker_id = wl
        .branch_from(new_branch, from)
        .map_err(|e| anyhow!("branch_from: {e}"))?;
    println!(
        "✓ created branch '{}' (current: {})",
        new_branch,
        wl.current_branch()
    );
    println!("  marker event: {}", marker_id);
    if let Some(parent) = from {
        println!("  parent event: {}", parent);
    } else {
        let total = wl.len();
        if total >= 2 {
            // The marker is the last event; the parent is the one
            // before it. We don't have an indexer on WorldLine so
            // collect once to slice.
            let events: Vec<_> = wl.iter().collect();
            if let Some(prev) = events.get(total - 2) {
                println!("  parent event: {} (previous head)", prev.id);
            }
        }
    }
    Ok(())
}

/// `wish-world timetravel <world-dir>` — open the native viewer with
/// a scrub slider that replays any prefix of the WorldLine.
fn cmd_timetravel(dir: &Path) -> Result<()> {
    let bundle = read_world_dir(dir).context("read world dir")?;
    let wl_path = dir.join("provenance").join("worldline.jsonl");
    let title = format!("{}  ·  time travel", bundle.world.name);
    eprintln!(
        "wish-world: opening time-travel viewer for {}",
        dir.display()
    );
    wish_render::run_timetravel(&title, bundle.world, wl_path)
        .map_err(|e| anyhow!("wish-render exited: {e}"))?;
    Ok(())
}

/// `wish-world watch <world-dir>` — open the native viewer and
/// hot-reload as the worldline grows on disk.
fn cmd_watch(dir: &Path, poll_ms: u64) -> Result<()> {
    let bundle = read_world_dir(dir).context("read world dir")?;
    let title = format!("{}  ·  watching", bundle.world.name);
    eprintln!(
        "wish-world: watching {} (poll every {}ms)",
        dir.join("provenance/worldline.jsonl").display(),
        poll_ms
    );
    wish_render::run_watch(
        &title,
        dir.to_path_buf(),
        std::time::Duration::from_millis(poll_ms),
    )
    .map_err(|e| anyhow!("wish-render exited: {e}"))?;
    Ok(())
}

/// ASCII Wish banner printed to the terminal before a native viewer
/// opens. Sets the user expectation: this is the World Model IDE, and
/// it has 15 perspectives anchored in the Tensorium. Skip via the
/// `WISH_NO_SPLASH=1` env var (for CI / tests / quiet smoke runs).
fn wish_splash(active_perspective: wish_render::Perspective) {
    if std::env::var("WISH_NO_SPLASH").ok().as_deref() == Some("1") {
        return;
    }
    // 256-color ANSI escape codes. Falls back to readable text on
    // terminals that don't render colors.
    const C_ACCENT: &str = "\x1b[38;5;111m"; //  soft blue
    const C_WARM: &str = "\x1b[38;5;215m"; //   amber
    const C_MUTED: &str = "\x1b[38;5;245m"; //  grey
    const C_RESET: &str = "\x1b[0m";
    const C_BOLD: &str = "\x1b[1m";

    eprintln!();
    eprintln!("{C_ACCENT}{C_BOLD}  ✦  W I S H{C_RESET}    {C_WARM}the World Model IDE{C_RESET}");
    eprintln!("{C_MUTED}      v0.5.0 · 15 perspectives · The Tensorium{C_RESET}");
    eprintln!(
        "{C_MUTED}      domain: {}  ·  science (tensorium): {}{C_RESET}",
        "🛠 🏛 🌐 💰 📚 🧪 🎨 📊", "∑ △ ⚗ ⚛ 🗣 🪨 🧬"
    );
    eprintln!("{C_MUTED}      ─────────────────────────────────────────────────{C_RESET}");
    eprintln!(
        "{C_ACCENT}      lens:{C_RESET} {} {C_MUTED}— {}{C_RESET}",
        active_perspective.label(),
        active_perspective.tagline()
    );
    eprintln!();
}

fn cmd_render_world(
    dir: &Path,
    perspective: wish_render::Perspective,
    reveal: Option<wish_world_model::SemanticId>,
) -> Result<()> {
    wish_splash(perspective);
    let bundle = read_world_dir(dir).context("read world dir")?;
    let canvas = world_to_canvas(&bundle.world);
    let title = format!(
        "{}  ·  {} entities · {} agents",
        bundle.world.name,
        bundle.world.entities.len(),
        bundle.world.agents.len()
    );
    if let Some(id) = &reveal {
        eprintln!("wish-world: will reveal {id} after cinematic boot");
    }
    wish_render::run_with_perspective_and_reveal(
        &title,
        canvas,
        Some(bundle.world),
        perspective,
        reveal,
    )
    .map_err(|e| anyhow!("wish-render exited: {e}"))?;
    Ok(())
}

fn cmd_render_repo(
    root: &Path,
    perspective: wish_render::Perspective,
    reveal: Option<wish_world_model::SemanticId>,
) -> Result<()> {
    wish_splash(perspective);
    // Function extraction is expensive (every .rs file gets re-parsed).
    // Engineering + Architecture lenses don't need it — they live at
    // the crate / file scale. Only the Function Graph view consumes
    // function-level data. Default to off so the common case is fast.
    let want_functions = false;
    let graph = if want_functions {
        wish_codegraph::extract_repo_graph_with(
            root,
            &wish_codegraph::ExtractOptions::default().with_functions(true),
        )
    } else {
        wish_codegraph::extract_repo_graph(root)
    };
    eprintln!(
        "wish-world: walked {} ({} crates, {} files, {} dep edges{})",
        root.display(),
        graph.crates.len(),
        graph.files.len(),
        graph.deps.len(),
        if want_functions {
            format!(
                ", {} fns, {} calls",
                graph.functions.len(),
                graph.calls.len()
            )
        } else {
            String::new()
        }
    );
    // Dispatch to the per-perspective canvas builder so each lens
    // surfaces the *right* level of detail. v0.5.0 wave 22:
    //   - Engineering  → to_canvas_repo: crates + crate root files (~250 nodes)
    //   - Architecture → to_canvas_architecture: crates only (~80 nodes)
    //   - (Function graph is reached via the in-canvas perspective
    //      dropdown today; the CLI flag `--perspective engineering`
    //      with function extraction enabled gives the user the call
    //      graph through `to_canvas` as the legacy fallback.)
    let (canvas, lens_label) = match perspective {
        wish_render::Perspective::Architecture => (
            wish_codegraph::to_canvas_architecture(&graph),
            "Architecture View",
        ),
        _ => (wish_codegraph::to_canvas_repo(&graph), "Repo Canvas"),
    };
    eprintln!(
        "wish-world: canvas built — {} nodes, {} edges (lens: {})",
        canvas.nodes.len(),
        canvas.edges.len(),
        lens_label
    );
    let title = root
        .file_name()
        .map(|n| format!("{} — {}", n.to_string_lossy(), lens_label))
        .unwrap_or_else(|| lens_label.to_string());
    if let Some(id) = &reveal {
        eprintln!("wish-world: will reveal {id} after cinematic boot");
    }
    wish_render::run_with_perspective_and_reveal(&title, canvas, None, perspective, reveal)
        .map_err(|e| anyhow!("wish-render exited: {e}"))?;
    Ok(())
}

/// `wish-world render demo` — build the Shan Hai world in-memory, never
/// touch disk, and open it directly in the native viewer. This is the
/// fastest path from `cargo run` to "Wish is up and rendering."
fn cmd_render_demo(
    perspective: wish_render::Perspective,
    reveal: Option<wish_world_model::SemanticId>,
) -> Result<()> {
    wish_splash(perspective);
    use wish_provenance::WorldLine;
    let tmp = std::env::temp_dir().join(format!(
        "wish-render-demo-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&tmp).context("create demo tmp")?;
    let mut world = WishWorld::new("Shan Hai Fintech Harbor", WorldKind::EducationWorld);
    let mut wl = WorldLine::open_in_world_dir(&tmp).context("open worldline")?;
    let _ = build_shanhai_harbor(&mut world, &mut wl).context("build shanhai")?;
    let canvas = world_to_canvas(&world);
    let title = format!(
        "{}  ·  {} entities · {} agents",
        world.name,
        world.entities.len(),
        world.agents.len()
    );
    if let Some(id) = &reveal {
        eprintln!("wish-world: will reveal {id} after cinematic boot");
    }
    eprintln!(
        "wish-world: rendering Shan Hai natively in {} perspective (no browser, no disk)…",
        perspective.label()
    );
    wish_render::run_with_perspective_and_reveal(&title, canvas, Some(world), perspective, reveal)
        .map_err(|e| anyhow!("wish-render exited: {e}"))?;
    Ok(())
}

/// `wish-world render tensor` — end-to-end smoke test for the URE ×
/// wishUI tensor substrate. Builds a canvas of "golden" tensors that
/// exercise each rank and dtype path, then opens the native viewer so
/// the user can see them as heatmaps in one screen. No worldline, no
/// disk — everything is constructed in memory from the
/// `wish_canvas_core::tensor` constructors.
///
/// The chosen examples:
/// - `eye_f32(8)` — sanity-check that the stride math reads diagonals.
/// - `linspace_f32` — rank-1 gradient.
/// - `ripple` (from_fn_f32) — radial cosine, shows the color ramp's
///   midband contrast.
/// - `gaussian` (from_fn_f32) — single mode, useful for spotting
///   bilinear vs nearest visually if a renderer changes its mind.
/// - `sine_plane` rank-3 — exercises the "pin axes 2..rank to 0"
///   default path.
fn cmd_render_tensor(
    perspective: wish_render::Perspective,
    reveal: Option<wish_world_model::SemanticId>,
) -> Result<()> {
    wish_splash(perspective);
    use wish_world_model::SemanticId;

    let mut canvas = Canvas::new();

    // Each tensor sits in a 220×140 px tile with 60 px gutters. Five
    // tiles laid out as a 3×2 grid (last cell empty) — fits the
    // default fit-to-view window without overcrowding.
    let tile_w = 220.0_f32;
    let tile_h = 140.0_f32;
    let gutter = 60.0_f32;
    let mut place = |idx: usize, label: &str, semantic: SemanticId, spec: TensorSpec| {
        let col = (idx % 3) as f32;
        let row = (idx / 3) as f32;
        let bounds = CanvasRect {
            x: col * (tile_w + gutter),
            y: row * (tile_h + gutter),
            w: tile_w,
            h: tile_h,
        };
        let node = CanvasNode::new(semantic, label, CanvasNodeKind::Tensor(spec), bounds);
        canvas.upsert_node(node);
    };

    place(
        0,
        "eye(8) — identity",
        SemanticId::code_function("tensor::eye_8"),
        TensorSpec::eye_f32(8),
    );

    place(
        1,
        "linspace(0..1, 32) — gradient",
        SemanticId::code_function("tensor::linspace_32"),
        TensorSpec::linspace_f32(0.0, 1.0, 32),
    );

    let ripple = TensorSpec::from_fn_f32(vec![24, 24], |c| {
        let y = c[0] as f32 - 11.5;
        let x = c[1] as f32 - 11.5;
        let r = (x * x + y * y).sqrt();
        (r * 0.6).cos()
    })
    .expect("ripple shape OK");
    place(
        2,
        "ripple(24×24) — radial cos",
        SemanticId::code_function("tensor::ripple_24"),
        ripple,
    );

    let gaussian = TensorSpec::from_fn_f32(vec![24, 24], |c| {
        let y = c[0] as f32 - 11.5;
        let x = c[1] as f32 - 11.5;
        let d2 = (x * x + y * y) / 40.0;
        (-d2).exp()
    })
    .expect("gaussian shape OK");
    place(
        3,
        "gaussian(24×24)",
        SemanticId::code_function("tensor::gaussian_24"),
        gaussian,
    );

    // Rank-3 example: a "stack of sine planes". The renderer pins axis
    // 2 to 0 by default, so the user sees the first plane — a sine
    // grating.
    let sine_stack = TensorSpec::from_fn_f32(vec![24, 24, 4], |c| {
        let theta = c[1] as f32 * 0.45 + c[2] as f32 * 0.8;
        (theta).sin()
    })
    .expect("sine_stack shape OK");
    place(
        4,
        "sine_stack(24×24×4) — plane 0",
        SemanticId::code_function("tensor::sine_stack"),
        sine_stack,
    );

    let title = format!(
        "Wish Tensorium — {} tensors · {} perspective",
        canvas.nodes.len(),
        perspective.label()
    );
    if let Some(id) = &reveal {
        eprintln!("wish-world: will reveal {id} after cinematic boot");
    }
    eprintln!(
        "wish-world: rendering {} tensors as inline heatmaps…",
        canvas.nodes.len()
    );
    wish_render::run_with_perspective_and_reveal(&title, canvas, None, perspective, reveal)
        .map_err(|e| anyhow!("wish-render exited: {e}"))?;
    Ok(())
}

fn open_in_browser(url: &str) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .status()
        .with_context(|| format!("spawn {opener}"))?;
    Ok(())
}

// HTML rendering moved to `wish_world_studio::viewer` so the desktop
// app's `OpenRepoCanvas` action can reuse it. The functions below are
// removed; this is a delete-target sentinel that ensures lint catches
// any stragglers.
#[allow(dead_code)]
fn _viewer_lives_in_world_studio_now() {}

#[cfg(any())]
fn _old_render_html_unused(
    world: &wish_world_model::WishWorld,
    svg: &str,
    world_dir: &Path,
) -> String {
    let mut entity_rows = String::new();
    let mut entries: Vec<_> = world.entities.values().collect();
    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    for e in entries {
        entity_rows.push_str(&format!(
            r#"<li class="entity" data-sid="{sid}"><span class="kind">[{kind:?}]</span> <span class="name">{name}</span></li>"#,
            sid = escape_html(&e.id.to_string()),
            kind = e.kind,
            name = escape_html(&e.display_name),
        ));
    }
    let mut agent_rows = String::new();
    for a in world.agents.values() {
        agent_rows.push_str(&format!(
            r#"<li class="agent" data-sid="{sid}"><span class="kind">[agent]</span> <span class="name">{name}</span> <span class="role">— {role}</span></li>"#,
            sid = escape_html(&a.id.to_string()),
            name = escape_html(&a.display_name),
            role = escape_html(&a.role),
        ));
    }

    // Worldline summary (if present on disk).
    let mut worldline_html = String::new();
    let wl_path = world_dir.join("provenance").join("worldline.jsonl");
    if wl_path.is_file() {
        if let Ok(wl) = WorldLine::open(wl_path.clone()) {
            worldline_html.push_str(&format!(
                    r#"<details open><summary>WorldLine ({} events · merkle {})</summary><ol class="wl">"#,
                    wl.len(),
                    short_hex(&wl.merkle_root(DEFAULT_BRANCH))
                ));
            for ev in wl.iter() {
                let actor = match &ev.actor {
                    wish_world_model::Actor::Agent { agent_id } => format!("agent:{agent_id}"),
                    wish_world_model::Actor::Human { user_id } => format!("human:{user_id}"),
                    wish_world_model::Actor::System => "system".into(),
                };
                worldline_html.push_str(&format!(
                        r#"<li><span class="risk">risk={:.2}</span> <span class="approval">{:?}</span> <span class="actor">{}</span><br><span class="intent">{}</span></li>"#,
                        ev.risk_score,
                        ev.approval,
                        escape_html(&actor),
                        escape_html(&ev.intent),
                    ));
            }
            worldline_html.push_str("</ol></details>");
        }
    }

    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Wish — {world_name}</title>
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
  <h1>{world_name}</h1>
  <span class="meta">{kind} · {n_entities} entities · {n_agents} agents</span>
</header>
<main>
  <aside>
    <h2>Entities ({n_entities})</h2>
    <ul id="entities">{entity_rows}</ul>
    <h2>Agents ({n_agents})</h2>
    <ul id="agents">{agent_rows}</ul>
    {worldline_html}
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
<footer>wish-world view · Wish v0.5.0 World Model IDE</footer>
<script>
(() => {{
  const stage = document.getElementById('stage');
  const svg = stage.querySelector('svg');
  if (!svg) return;
  // Wrap content in a <g> we can pan/zoom.
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
    // Zoom around cursor.
    tx = px - (px - tx) * factor;
    ty = py - (py - ty) * factor;
    scale *= factor;
    scale = Math.max(0.05, Math.min(20, scale));
    apply();
  }}, {{ passive: false }});

  document.getElementById('zoom-in').onclick = () => {{ scale *= 1.2; apply(); }};
  document.getElementById('zoom-out').onclick = () => {{ scale /= 1.2; apply(); }};
  document.getElementById('fit').onclick = () => {{ tx = 0; ty = 0; scale = 1; apply(); }};

  // Sidebar ↔ canvas selection.
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
"##,
        world_name = escape_html(&world.name),
        kind = format!("{:?}", world.kind),
        n_entities = world.entities.len(),
        n_agents = world.agents.len(),
        entity_rows = entity_rows,
        agent_rows = agent_rows,
        worldline_html = worldline_html,
        svg = svg,
    )
}

fn cmd_agent_dag(path: &Path, format: Format) -> Result<()> {
    let text = std::fs::read_to_string(path).context("read run json")?;
    let run: wish_agent_visualizer::AgentRun =
        serde_json::from_str(&text).context("parse AgentRun")?;
    let canvas = wish_agent_visualizer::build_dag(&run);
    emit_canvas(&canvas, format)
}

fn emit_canvas(canvas: &Canvas, format: Format) -> Result<()> {
    match format {
        Format::Svg => print!("{}", export::to_svg(canvas)),
        Format::Mermaid => print!("{}", export::to_mermaid(canvas)),
        Format::Json => {
            let json = serde_json::to_string_pretty(canvas).context("serialize canvas")?;
            print!("{json}");
        }
        Format::Text => {
            println!(
                "Canvas: {} nodes, {} edges",
                canvas.nodes.len(),
                canvas.edges.len()
            );
            let mut entries: Vec<_> = canvas.nodes.values().collect();
            entries.sort_by(|a, b| a.label.cmp(&b.label));
            for n in entries {
                println!("  • {:<32} [{:?}] {}", n.label, n.kind, n.semantic_id);
            }
        }
        Format::ArchitectureMermaid => {
            // Reached only via `canvas world` (canvas repo branches
            // earlier). The world-canvas path doesn't have a
            // RepoGraph, so fall back to a Mermaid graph of the canvas.
            print!("{}", export::to_mermaid(canvas));
        }
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xF) as usize] as char);
    }
    s
}

/// Convenience: lets callers `use _ as PathBuf` without warnings if we
/// reshape the CLI later.
#[allow(dead_code)]
fn _typed_path(s: &str) -> PathBuf {
    PathBuf::from(s)
}
