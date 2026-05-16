//! Architecture-view exporter.
//!
//! Distills a [`RepoGraph`] to its top-level skeleton — one node per
//! crate or top-level package, edges for inter-crate dependencies,
//! per-crate counts (files + public functions + total functions) as
//! the node sublabel — and emits it as **Mermaid flowchart** notation.
//!
//! This is Wish's *post-UML* answer to the classic "system
//! architecture diagram": instead of hand-drawn class diagrams that
//! drift out of sync with code, the architecture view is **generated
//! from the live codegraph**. It's the same data the Canvas pane
//! shows, just collapsed to the highest-altitude level.

use crate::{RepoCrate, RepoGraph};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// Compute and render a top-level architecture view as a Mermaid
/// `flowchart TD` document. Suitable for pasting into a README, a
/// docs site, or `wish-world canvas repo --format architecture-mermaid`.
pub fn to_mermaid(graph: &RepoGraph) -> String {
    let summary = summarize(graph);
    let mut out = String::from("flowchart TD\n");

    // Group crates by trust-tier-equivalent (the first segment of the
    // crate name, if any). Wish's own crates split into `wish-*` vs
    // `wishui`, `app`, etc. This produces nice visual clusters even
    // for big repos.
    let mut by_group: BTreeMap<String, Vec<&CrateSummary>> = BTreeMap::new();
    for c in summary.crates.values() {
        let group = group_of(&c.name);
        by_group.entry(group).or_default().push(c);
    }
    for (group, crates) in &by_group {
        let _ = writeln!(out, "  subgraph {}", mermaid_id(&format!("g_{group}")));
        let _ = writeln!(out, "    direction TB");
        for c in crates {
            let _ = writeln!(
                out,
                "    {}[\"{}<br/><sub>{} files · {} fns · {} pub</sub>\"]",
                mermaid_id(&c.name),
                escape_mermaid_label(&c.name),
                c.file_count,
                c.fn_count,
                c.pub_fn_count
            );
        }
        let _ = writeln!(out, "  end");
    }

    // Edges between crates.
    for (from, to) in &summary.edges {
        let _ = writeln!(out, "  {} --> {}", mermaid_id(from), mermaid_id(to));
    }
    out
}

/// Per-crate summary used by [`to_mermaid`] and by the architecture
/// canvas. Keeps the architecture view dependency-light.
#[derive(Debug, Clone)]
pub struct CrateSummary {
    pub name: String,
    pub file_count: usize,
    pub fn_count: usize,
    pub pub_fn_count: usize,
}

/// Top-level repo summary.
#[derive(Debug, Clone)]
pub struct ArchitectureSummary {
    pub crates: BTreeMap<String, CrateSummary>,
    /// Inter-crate edges, deduplicated.
    pub edges: BTreeSet<(String, String)>,
}

/// Roll up the per-file / per-function tables in a `RepoGraph` into a
/// crate-level summary suitable for a top-level architecture diagram.
pub fn summarize(graph: &RepoGraph) -> ArchitectureSummary {
    let mut crates: BTreeMap<String, CrateSummary> = BTreeMap::new();
    for c in &graph.crates {
        crates.insert(
            c.name.clone(),
            CrateSummary {
                name: c.name.clone(),
                file_count: 0,
                fn_count: 0,
                pub_fn_count: 0,
            },
        );
    }
    for f in &graph.files {
        if let Some(c) = f.crate_name.as_ref().and_then(|n| crates.get_mut(n)) {
            c.file_count += 1;
        }
    }
    for func in &graph.functions {
        if let Some(c) = func.crate_name.as_ref().and_then(|n| crates.get_mut(n)) {
            c.fn_count += 1;
            if func.is_pub {
                c.pub_fn_count += 1;
            }
        }
    }
    // Crates we never saw a file for (rare, but possible if Cargo.toml
    // exists with no src/) still appear with zero counts — that's the
    // truth, surface it.
    let _ = RepoCrate::default; // future-proof: keep RepoCrate referenced

    let edges: BTreeSet<(String, String)> = graph.deps.iter().cloned().collect();
    ArchitectureSummary { crates, edges }
}

fn group_of(crate_name: &str) -> String {
    // `wish-*` → `wish`, `wishui*` → `wishui`, anything else → `other`.
    if crate_name.starts_with("wish-") {
        return "wish".into();
    }
    if let Some(idx) = crate_name.find(|c: char| c == '_' || c == '-') {
        return crate_name[..idx].to_string();
    }
    crate_name.to_string()
}

fn mermaid_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn escape_mermaid_label(s: &str) -> String {
    s.replace('"', "\\\"")
}

// Implement Default for RepoCrate so the unused-binding above stays
// safe under future refactors that move the type around.
impl Default for RepoCrate {
    fn default() -> Self {
        RepoCrate {
            name: String::new(),
            path: std::path::PathBuf::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RepoFile, RepoFunction};
    use std::path::PathBuf;

    fn sample() -> RepoGraph {
        RepoGraph {
            root: PathBuf::from("/r"),
            crates: vec![
                RepoCrate {
                    name: "wish-canvas-core".into(),
                    path: PathBuf::from("/r/canvas-core"),
                },
                RepoCrate {
                    name: "wish-codegraph".into(),
                    path: PathBuf::from("/r/codegraph"),
                },
                RepoCrate {
                    name: "wishui".into(),
                    path: PathBuf::from("/r/wishui"),
                },
            ],
            files: vec![
                RepoFile {
                    path: PathBuf::from("/r/canvas-core/src/lib.rs"),
                    crate_name: Some("wish-canvas-core".into()),
                },
                RepoFile {
                    path: PathBuf::from("/r/codegraph/src/lib.rs"),
                    crate_name: Some("wish-codegraph".into()),
                },
            ],
            deps: vec![("wish-codegraph".into(), "wish-canvas-core".into())],
            functions: vec![
                RepoFunction {
                    crate_name: Some("wish-canvas-core".into()),
                    file_path: PathBuf::from("/r/canvas-core/src/lib.rs"),
                    name: "alpha".into(),
                    line: 1,
                    is_test: false,
                    is_pub: true,
                    is_async: false,
                },
                RepoFunction {
                    crate_name: Some("wish-canvas-core".into()),
                    file_path: PathBuf::from("/r/canvas-core/src/lib.rs"),
                    name: "private".into(),
                    line: 10,
                    is_test: false,
                    is_pub: false,
                    is_async: false,
                },
                RepoFunction {
                    crate_name: Some("wish-codegraph".into()),
                    file_path: PathBuf::from("/r/codegraph/src/lib.rs"),
                    name: "beta".into(),
                    line: 1,
                    is_test: false,
                    is_pub: true,
                    is_async: false,
                },
            ],
            calls: vec![],
        }
    }

    #[test]
    fn summarize_rolls_up_per_crate_counts() {
        let s = summarize(&sample());
        assert_eq!(s.crates.len(), 3);
        let cc = s.crates.get("wish-canvas-core").unwrap();
        assert_eq!(cc.file_count, 1);
        assert_eq!(cc.fn_count, 2);
        assert_eq!(cc.pub_fn_count, 1);
        let cg = s.crates.get("wish-codegraph").unwrap();
        assert_eq!(cg.fn_count, 1);
        assert_eq!(cg.pub_fn_count, 1);
        assert!(s
            .edges
            .contains(&("wish-codegraph".to_string(), "wish-canvas-core".to_string())));
    }

    #[test]
    fn to_mermaid_emits_flowchart_with_crates_and_edges() {
        let m = to_mermaid(&sample());
        assert!(m.starts_with("flowchart TD\n"));
        assert!(m.contains("wish_canvas_core"));
        assert!(m.contains("wish_codegraph"));
        assert!(m.contains("wishui"));
        assert!(m.contains("wish_codegraph --> wish_canvas_core"));
        // Sub-labels render the counts.
        assert!(m.contains("1 files · 2 fns · 1 pub"));
    }

    #[test]
    fn group_of_buckets_wish_crates_together() {
        assert_eq!(group_of("wish-canvas-core"), "wish");
        assert_eq!(group_of("wish-codegraph"), "wish");
        assert_eq!(group_of("wishui"), "wishui");
        assert_eq!(group_of("app"), "app");
    }
}
