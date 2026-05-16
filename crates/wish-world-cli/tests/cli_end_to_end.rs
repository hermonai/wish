//! End-to-end CLI tests. Spawn the actual `wish-world` binary and
//! exercise every subcommand against a fresh temp dir.
//!
//! This is the *proof* that the v0.5.0 World Model IDE seed runs as a
//! product: one binary, one command, one demo world materialized on
//! disk with a WorldLine, mission, and renderable canvas.

use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    // CARGO_BIN_EXE_<name> is set by cargo for integration tests.
    PathBuf::from(env!("CARGO_BIN_EXE_wish-world"))
}

fn tmp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("wish_world_cli_{label}_{nanos}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn version_prints() {
    let out = Command::new(binary()).arg("version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("wish-world"), "got: {stdout}");
}

#[test]
fn help_when_no_args() {
    let out = Command::new(binary()).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("USAGE"));
    assert!(stdout.contains("World Model IDE"));
}

#[test]
fn demo_shanhai_builds_world_inspectable_via_cli() {
    let out_dir = tmp_dir("demo");
    // 1. Build the demo world.
    let build = Command::new(binary())
        .arg("demo")
        .arg("shanhai")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "demo failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let world_dir = out_dir.join("shanhai-fintech-harbor.wishworld");
    assert!(world_dir.is_dir(), "world dir not created");
    assert!(world_dir.join("world.json").is_file());
    assert!(world_dir.join("provenance/worldline.jsonl").is_file());

    // 2. Inspect it via the CLI.
    let inspect = Command::new(binary())
        .arg("inspect")
        .arg(&world_dir)
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let inspect_out = String::from_utf8(inspect.stdout).unwrap();
    assert!(inspect_out.contains("Shan Hai Fintech Harbor"));
    assert!(inspect_out.contains("Dragon Temple"));
    assert!(inspect_out.contains("Merchant Liu"));
    assert!(inspect_out.contains("World Architect"));
    assert!(inspect_out.contains("WorldLine:"));

    // 3. Show worldline.
    let wl = Command::new(binary())
        .arg("worldline")
        .arg(&world_dir)
        .output()
        .unwrap();
    assert!(wl.status.success());
    let wl_out = String::from_utf8(wl.stdout).unwrap();
    assert!(wl_out.contains("events: 7"));
    assert!(wl_out.contains("AutoApproved"));

    // 4. Render canvas as Mermaid.
    let mermaid = Command::new(binary())
        .arg("canvas")
        .arg("world")
        .arg(&world_dir)
        .arg("--format")
        .arg("mermaid")
        .output()
        .unwrap();
    assert!(mermaid.status.success());
    let mermaid_out = String::from_utf8(mermaid.stdout).unwrap();
    assert!(mermaid_out.starts_with("graph TD"));
    assert!(mermaid_out.contains("Dragon Temple"));
    // 1 root + 1 architect + 5 entities = 7 lines of node defs.
    let node_lines = mermaid_out
        .lines()
        .filter(|l| l.contains('[') && l.contains(']'))
        .count();
    assert!(node_lines >= 7, "expected ≥7 nodes, got {node_lines}");

    // 5. Render canvas as SVG.
    let svg = Command::new(binary())
        .arg("canvas")
        .arg("world")
        .arg(&world_dir)
        .arg("--format")
        .arg("svg")
        .output()
        .unwrap();
    assert!(svg.status.success());
    let svg_out = String::from_utf8(svg.stdout).unwrap();
    assert!(svg_out.starts_with("<svg "));
    assert!(svg_out.contains("</svg>"));
}

#[test]
fn canvas_repo_walks_this_repo_and_recovers_dep_edges() {
    // Walk from the test's CARGO_MANIFEST_DIR up to the repo root.
    let mut repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    repo_root.pop(); // crates/wish-world-cli -> crates
    repo_root.pop(); // crates -> repo root

    let out = Command::new(binary())
        .arg("canvas")
        .arg("repo")
        .arg(&repo_root)
        .arg("--format")
        .arg("text")
        .output()
        .unwrap();
    assert!(out.status.success(), "canvas repo failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8(out.stderr).unwrap();
    // stderr line: "wish-world: walked <root> (N crates, M files, K dep edges)"
    assert!(stderr.contains("crates"), "got stderr: {stderr}");
    assert!(stderr.contains("dep edges"));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("Canvas:"));
}
