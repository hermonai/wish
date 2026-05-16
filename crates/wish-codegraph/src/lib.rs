//! Wish Codegraph — extract repository structure into the Wish World Model.
//!
//! v0.5.0 scope: a multi-language, dependency-light extractor that walks
//! a directory tree, identifies Cargo crates, source files, and
//! function-level declarations across **Rust, Python, TypeScript,
//! JavaScript, and Go**, producing a [`RepoGraph`] that can be
//! projected into a [`Canvas`] or a [`WishWorld`] via [`to_world_patch`].
//!
//! Heavy LSP / tree-sitter integration (real cross-file call
//! resolution) lands in v0.6.0.

pub mod languages;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wish_canvas_core::{
    layout,
    types::{Canvas, CanvasEdge, CanvasNode, CanvasNodeKind, EdgeKind, LayoutMode, Rect},
};
use wish_world_model::{Actor, EntityKind, PatchOp, SemanticId, WorldEntity, WorldPatch};

pub use languages::Language;

pub mod architecture;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoGraph {
    pub root: PathBuf,
    pub crates: Vec<RepoCrate>,
    pub files: Vec<RepoFile>,
    pub deps: Vec<(String, String)>, // (from_crate, to_crate)
    /// Function-level extraction. Populated by `extract_functions_into`
    /// or by passing `ExtractOptions::with_functions(true)` to
    /// `extract_repo_graph_with`. Defaults to empty (lazy).
    #[serde(default)]
    pub functions: Vec<RepoFunction>,
    /// Heuristic call edges between functions: (from_qualified_name, to_qualified_name).
    /// Currently derived from textual name references inside function bodies.
    #[serde(default)]
    pub calls: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCrate {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoFile {
    pub path: PathBuf,
    pub crate_name: Option<String>,
}

/// A function discovered inside a `.rs` file. We capture enough to
/// uniquely identify it for SemanticId derivation: crate name + file
/// path + short qualifier (`mod1::mod2`) inferred from the file, plus
/// the function's own name and the line it was declared on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoFunction {
    pub crate_name: Option<String>,
    pub file_path: PathBuf,
    pub name: String,
    pub line: u32,
    /// Whether the function is annotated `#[test]`.
    pub is_test: bool,
    /// Whether the function is `pub` or `pub(...)`.
    pub is_pub: bool,
    /// Whether the function is `async`.
    pub is_async: bool,
}

impl RepoFunction {
    /// `crate::file_stem::fn_name` — best-effort qualifier.
    pub fn qualified_name(&self) -> String {
        let stem = self
            .file_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let crate_prefix = self.crate_name.as_deref().unwrap_or("?");
        if stem == "lib" || stem == "main" || stem == "mod" {
            format!("{crate_prefix}::{}", self.name)
        } else {
            format!("{crate_prefix}::{stem}::{}", self.name)
        }
    }
}

/// Optional knobs for repo extraction.
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    pub extract_functions: bool,
    /// Cap on functions per file (prevents very large generated files
    /// from dominating the graph). 0 means no cap.
    pub max_functions_per_file: usize,
    /// Skip the function-extraction step on files larger than this
    /// many bytes. Defaults to 256 KiB.
    pub max_file_bytes: u64,
    /// When extracting `Calls` edges, restrict to caller and callee
    /// living in the same file. Faster (O(files × intra-file
    /// functions²) instead of O(functions × global names)) and avoids
    /// false positives from common names across the repo. Cross-file
    /// call edges arrive in v0.6.0 via real LSP / tree-sitter
    /// integration.
    pub same_file_calls_only: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            extract_functions: false,
            max_functions_per_file: 200,
            max_file_bytes: 256 * 1024,
            same_file_calls_only: true,
        }
    }
}

impl ExtractOptions {
    pub fn with_functions(mut self, on: bool) -> Self {
        self.extract_functions = on;
        self
    }
    pub fn with_same_file_calls_only(mut self, on: bool) -> Self {
        self.same_file_calls_only = on;
        self
    }
}

/// Walk `root`, return a [`RepoGraph`].
///
/// v0.5.0 implementation: detects Cargo crates by `Cargo.toml` files and
/// captures `*.rs` files inside each crate's `src/`. Deeper integration
/// (Cargo metadata, tree-sitter, git churn) lands in later steps.
pub fn extract_repo_graph(root: &Path) -> RepoGraph {
    extract_repo_graph_with(root, &ExtractOptions::default())
}

/// Like `extract_repo_graph`, but with extraction knobs (function
/// scanning, file-size cap, etc.).
pub fn extract_repo_graph_with(root: &Path, opts: &ExtractOptions) -> RepoGraph {
    let mut graph = RepoGraph {
        root: root.to_path_buf(),
        ..Default::default()
    };

    if !root.exists() {
        return graph;
    }

    walk(root, &mut graph, 0);

    // Filter recorded deps to only those that actually correspond to
    // workspace-local crates.
    let crate_names: HashSet<String> = graph.crates.iter().map(|c| c.name.clone()).collect();
    graph.deps.retain(|(from, to)| {
        if crate_names.contains(to) {
            return true;
        }
        let alt = to.replace('-', "_");
        if crate_names.contains(&alt) {
            return true;
        }
        let alt = to.replace('_', "-");
        if crate_names.contains(&alt) {
            return true;
        }
        let _ = from;
        false
    });

    // Optional function-level extraction.
    if opts.extract_functions {
        extract_functions_into(&mut graph, opts);
    }

    graph
}

/// Walk every `RepoFile` (only `.rs` files within size budget) and
/// extract function declarations. Populates `graph.functions` and a
/// heuristic `graph.calls` edge set.
pub fn extract_functions_into(graph: &mut RepoGraph, opts: &ExtractOptions) {
    use std::collections::HashMap;

    // Cache file text + start indices of each function in the file so
    // the call-edge pass doesn't re-read or re-scan anything.
    let mut file_text: HashMap<PathBuf, String> = HashMap::new();
    // For each function index in graph.functions, the index of its file
    // in `graph.files` (Some) or None if unknown.
    let mut name_to_qname: HashMap<String, Vec<String>> = HashMap::new();
    let mut fn_per_file: HashMap<PathBuf, Vec<usize>> = HashMap::new();

    for file in &graph.files {
        // Dispatch by language detected from the file extension.
        let Some(lang) = languages::Language::from_path(&file.path) else {
            continue;
        };
        if let Ok(meta) = std::fs::metadata(&file.path) {
            if opts.max_file_bytes > 0 && meta.len() > opts.max_file_bytes {
                continue;
            }
        }
        let Ok(text) = std::fs::read_to_string(&file.path) else {
            continue;
        };
        let mut funcs = languages::extract_functions(lang, &text);
        if opts.max_functions_per_file > 0 && funcs.len() > opts.max_functions_per_file {
            funcs.truncate(opts.max_functions_per_file);
        }
        for f in funcs {
            let rf = RepoFunction {
                crate_name: file.crate_name.clone(),
                file_path: file.path.clone(),
                name: f.name.clone(),
                line: f.line,
                is_test: f.is_test,
                is_pub: f.is_pub,
                is_async: f.is_async,
            };
            let qn = rf.qualified_name();
            name_to_qname
                .entry(rf.name.clone())
                .or_default()
                .push(qn.clone());
            fn_per_file
                .entry(file.path.clone())
                .or_default()
                .push(graph.functions.len());
            graph.functions.push(rf);
        }
        file_text.insert(file.path.clone(), text);
    }

    if opts.same_file_calls_only {
        // Fast path: for each file, collect its function names; for
        // each function in the file, body-scan for sibling names.
        for (path, indices) in &fn_per_file {
            let Some(text) = file_text.get(path) else {
                continue;
            };
            // Build the {name → qname} map for this file (handles dup names).
            let mut local: HashMap<&str, &str> = HashMap::new();
            for &i in indices {
                let f = &graph.functions[i];
                // First-wins on collision — that's fine for the
                // heuristic.
                local.entry(&f.name).or_insert("");
            }
            // Materialize the per-fn qnames separately to satisfy borrow rules.
            let mut name_to_qname_local: HashMap<String, String> = HashMap::new();
            for &i in indices {
                let f = &graph.functions[i];
                name_to_qname_local
                    .entry(f.name.clone())
                    .or_insert_with(|| f.qualified_name());
            }
            for &i in indices {
                let caller_qn = graph.functions[i].qualified_name();
                let caller_line = graph.functions[i].line as usize;
                if let Some(body) = body_of_function(text, caller_line) {
                    for (callee_name, callee_qn) in &name_to_qname_local {
                        if callee_qn == &caller_qn {
                            continue;
                        }
                        if mentions_function(&body, callee_name) {
                            graph.calls.push((caller_qn.clone(), callee_qn.clone()));
                        }
                    }
                }
            }
        }
    } else {
        // Global-unique heuristic (slower, O(functions × global names)).
        let global: HashMap<String, String> = name_to_qname
            .into_iter()
            .filter(|(_, qns)| qns.len() == 1)
            .map(|(name, qns)| (name, qns.into_iter().next().unwrap()))
            .collect();
        for caller in &graph.functions {
            let Some(text) = file_text.get(&caller.file_path) else {
                continue;
            };
            if let Some(body) = body_of_function(text, caller.line as usize) {
                let caller_qn = caller.qualified_name();
                for (callee_name, callee_qname) in &global {
                    if callee_qname == &caller_qn {
                        continue;
                    }
                    if mentions_function(&body, callee_name) {
                        graph.calls.push((caller_qn.clone(), callee_qname.clone()));
                    }
                }
            }
        }
    }
}

// The Rust function extractor now lives in `languages::extract_rust`.
// We keep tiny back-compat aliases so existing in-crate tests that
// referenced the old internal names keep compiling.
#[cfg(test)]
type ParsedFn = languages::ParsedFn;

#[cfg(test)]
fn extract_functions_from_text(text: &str) -> Vec<ParsedFn> {
    languages::extract_functions(languages::Language::Rust, text)
}

/// Grab the textual body of a function starting at `decl_line` (1-indexed)
/// by counting balanced braces. Returns the slice from the opening `{`
/// to its matching `}`. Strings and `//` line comments are respected.
fn body_of_function(text: &str, decl_line: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let mut line_idx = 0usize;
    let mut offset = 0usize;
    while line_idx + 1 < decl_line && offset < bytes.len() {
        if bytes[offset] == b'\n' {
            line_idx += 1;
        }
        offset += 1;
    }
    // Find the first '{' from `offset` ignoring strings / line comments.
    let mut i = offset;
    let mut in_string = false;
    let mut string_delim = b'"';
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if c == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if c == string_delim {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            in_string = true;
            string_delim = c;
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'{' {
            break;
        }
        if c == b';' {
            // declaration without body (trait method).
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let body_start = i;
    let mut depth: i32 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if c == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if c == string_delim {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            in_string = true;
            string_delim = c;
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return std::str::from_utf8(&bytes[body_start..=i])
                    .ok()
                    .map(|s| s.to_string());
            }
        }
        i += 1;
    }
    None
}

/// Word-boundary check: does `body` mention `name` as a word (not as
/// a substring of another identifier)?
fn mentions_function(body: &str, name: &str) -> bool {
    let target = name.as_bytes();
    let bytes = body.as_bytes();
    if target.len() > bytes.len() {
        return false;
    }
    let mut i = 0;
    while i + target.len() <= bytes.len() {
        if &bytes[i..i + target.len()] == target {
            let prev_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let next_ok =
                i + target.len() == bytes.len() || !is_ident_char(bytes[i + target.len()]);
            if prev_ok && next_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn walk(dir: &Path, graph: &mut RepoGraph, depth: usize) {
    if depth > 12 {
        return;
    }
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip(&name) {
            continue;
        }
        if path.is_dir() {
            walk(&path, graph, depth + 1);
        } else if name == "Cargo.toml" {
            // Treat the parent dir as a crate. Parse a minimal subset of
            // the manifest to recover `package.name` and the dep keys
            // pointing at workspace-local crates.
            if let Some(parent) = path.parent() {
                let (crate_name, deps) = parse_cargo_toml(&path).unwrap_or_else(|| {
                    let fallback = parent
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    (fallback, Vec::new())
                });
                graph.crates.push(RepoCrate {
                    name: crate_name.clone(),
                    path: parent.to_path_buf(),
                });
                for dep in deps {
                    graph.deps.push((crate_name.clone(), dep));
                }
            }
        } else if languages::Language::from_path(&path).is_some() {
            // Any path with a recognized source extension becomes a
            // RepoFile. Multi-language support: .rs / .py / .ts / .js /
            // .tsx / .jsx / .mjs / .cjs / .go / .pyi.
            let crate_name = nearest_crate_name(&path);
            graph.files.push(RepoFile {
                path: path.clone(),
                crate_name,
            });
        }
    }
}

fn nearest_crate_name(file: &Path) -> Option<String> {
    let mut dir = file.parent()?.to_path_buf();
    loop {
        if dir.join("Cargo.toml").exists() {
            return dir.file_name().map(|n| n.to_string_lossy().to_string());
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Parse a `Cargo.toml` and return `(package_name, dep_names)`.
///
/// Minimal hand-rolled parser — we deliberately don't add the `toml`
/// crate as a workspace dep here. We extract:
///   * the `[package] name = "..."` value
///   * top-level table headers `[dependencies]`, `[dev-dependencies]`,
///     `[build-dependencies]`, `[target.…dependencies]`, and the key
///     names beneath them (the dep names, not their versions).
///
/// Workspace inheritance (`dep = { workspace = true }`) keeps the dep
/// name; we don't try to resolve version metadata.
fn parse_cargo_toml(path: &Path) -> Option<(String, Vec<String>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut package_name: Option<String> = None;
    let mut deps: Vec<String> = Vec::new();
    let mut section: Section = Section::Other;

    enum Section {
        Package,
        Deps,
        Other,
    }

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let h = header.trim();
            section = if h == "package" {
                Section::Package
            } else if h == "dependencies"
                || h == "dev-dependencies"
                || h == "build-dependencies"
                || (h.starts_with("target.") && h.ends_with("dependencies"))
            {
                Section::Deps
            } else if h.starts_with("dependencies.")
                || h.starts_with("dev-dependencies.")
                || h.starts_with("build-dependencies.")
            {
                // Sub-table like [dependencies.foo]. The dep name is the
                // last component.
                if let Some(name) = h.rsplit('.').next() {
                    deps.push(strip_quotes(name).to_string());
                }
                Section::Other
            } else {
                Section::Other
            };
            continue;
        }
        match section {
            Section::Package => {
                if let Some((key, value)) = line.split_once('=') {
                    if key.trim() == "name" {
                        package_name = Some(strip_quotes(value).to_string());
                    }
                }
            }
            Section::Deps => {
                if let Some((key, _rest)) = line.split_once('=') {
                    // Cargo accepts dotted-key shorthand, e.g.
                    // `wish-world-model.workspace = true`. The dep name
                    // is the head of the dotted path.
                    let head = key
                        .trim()
                        .trim_matches('"')
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('"');
                    if !head.is_empty() && !head.contains(char::is_whitespace) {
                        deps.push(head.to_string());
                    }
                }
            }
            Section::Other => {}
        }
    }
    package_name.map(|name| (name, deps))
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    s.trim_matches('"').trim_matches('\'').trim()
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        "target"
            | "node_modules"
            | ".git"
            | "dist"
            | "build"
            | "out"
            | ".cargo"
            | ".idea"
            | ".vscode"
    )
}

/// Project a [`RepoGraph`] as a layered [`Canvas`] suitable for the
/// "Open Repo Canvas" action.
pub fn to_canvas(graph: &RepoGraph) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.layout = LayoutMode::Layered;

    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 140.0,
        h: 36.0,
    };

    for c in &graph.crates {
        let id = SemanticId::code_crate(&c.name);
        let node = CanvasNode::new(id, &c.name, CanvasNodeKind::Crate, bounds);
        canvas.upsert_node(node);
    }
    for f in &graph.files {
        let rel = f.path.strip_prefix(&graph.root).unwrap_or(&f.path);
        let path_str = rel.to_string_lossy().to_string();
        let id = SemanticId::code_file(&path_str);
        let label = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(path_str.clone());
        let node = CanvasNode::new(id, label, CanvasNodeKind::File, bounds);
        let file_node_id = node.id;
        canvas.upsert_node(node);
        if let Some(cn) = &f.crate_name {
            let crate_id = wish_canvas_core::types::CanvasNode::new(
                SemanticId::code_crate(cn),
                cn,
                CanvasNodeKind::Crate,
                bounds,
            )
            .id;
            canvas.upsert_edge(CanvasEdge::new(crate_id, file_node_id, EdgeKind::DependsOn));
        }
    }

    // Inter-crate dependency edges, recovered from Cargo manifests.
    for (from, to) in &graph.deps {
        let from_id = wish_canvas_core::types::CanvasNode::new(
            SemanticId::code_crate(from),
            from,
            CanvasNodeKind::Crate,
            bounds,
        )
        .id;
        let to_id = wish_canvas_core::types::CanvasNode::new(
            SemanticId::code_crate(to),
            to,
            CanvasNodeKind::Crate,
            bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(from_id, to_id, EdgeKind::DependsOn));
    }

    // Function-level nodes + call edges (only when the graph was
    // extracted with `ExtractOptions::with_functions(true)`).
    for func in &graph.functions {
        let qname = func.qualified_name();
        let id = SemanticId::code_function(&qname);
        let label = if func.is_test {
            format!("test {}", func.name)
        } else if func.is_async {
            format!("async {}", func.name)
        } else {
            func.name.clone()
        };
        let kind = if func.is_test {
            CanvasNodeKind::Test
        } else {
            CanvasNodeKind::Function
        };
        let node = CanvasNode::new(id.clone(), label, kind, bounds);
        let fn_node_id = node.id;
        canvas.upsert_node(node);

        // Edge: containing file → function.
        let rel = func
            .file_path
            .strip_prefix(&graph.root)
            .unwrap_or(&func.file_path);
        let file_id = wish_canvas_core::types::CanvasNode::new(
            SemanticId::code_file(&rel.to_string_lossy()),
            rel.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            CanvasNodeKind::File,
            bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(file_id, fn_node_id, EdgeKind::DependsOn));
    }
    for (from_q, to_q) in &graph.calls {
        let from_id = wish_canvas_core::types::CanvasNode::new(
            SemanticId::code_function(from_q),
            from_q,
            CanvasNodeKind::Function,
            bounds,
        )
        .id;
        let to_id = wish_canvas_core::types::CanvasNode::new(
            SemanticId::code_function(to_q),
            to_q,
            CanvasNodeKind::Function,
            bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(from_id, to_id, EdgeKind::Calls));
    }

    layout::run(&mut canvas);
    canvas
}

/// **Architecture View** — crates only, with inter-crate `DependsOn`
/// edges. The architectural unit of a Rust workspace is the crate; a
/// 3000-file canvas drowns this signal in noise. This view shows the
/// 50–200-node shape of the workspace so the user can actually read
/// the dependency structure.
///
/// Each crate node carries a multi-line label with its file count to
/// give a sense of crate "weight." Layered layout based on the
/// inter-crate dep graph.
pub fn to_canvas_architecture(graph: &RepoGraph) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.layout = LayoutMode::Layered;

    // Count files per crate so the label can show "name\n(N files)".
    let mut file_count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for f in &graph.files {
        if let Some(cn) = &f.crate_name {
            *file_count.entry(cn.as_str()).or_insert(0) += 1;
        }
    }

    // Crate nodes have larger bounds than files so labels stay
    // readable when fit-to-view zooms out.
    let crate_bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 220.0,
        h: 56.0,
    };
    for c in &graph.crates {
        let count = file_count.get(c.name.as_str()).copied().unwrap_or(0);
        let label = if count > 0 {
            format!(
                "{}\n{} file{}",
                c.name,
                count,
                if count == 1 { "" } else { "s" }
            )
        } else {
            c.name.clone()
        };
        let id = SemanticId::code_crate(&c.name);
        let node = CanvasNode::new(id, label, CanvasNodeKind::Crate, crate_bounds);
        canvas.upsert_node(node);
    }
    for (from, to) in &graph.deps {
        let from_id = CanvasNode::new(
            SemanticId::code_crate(from),
            from.clone(),
            CanvasNodeKind::Crate,
            crate_bounds,
        )
        .id;
        let to_id = CanvasNode::new(
            SemanticId::code_crate(to),
            to.clone(),
            CanvasNodeKind::Crate,
            crate_bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(from_id, to_id, EdgeKind::DependsOn));
    }
    layout::run(&mut canvas);
    canvas
}

/// **Function Graph View** — functions with `Calls` edges only. Drops
/// crates and files entirely, and **filters out orphan functions**
/// (no incoming or outgoing calls) since they're just noise at the
/// function-level zoom. The user wants to see the *call graph*, not
/// a phone book.
///
/// Requires the graph to have been extracted with
/// `ExtractOptions::with_functions(true)`. Returns an empty canvas
/// if `graph.functions` is empty.
pub fn to_canvas_function_graph(graph: &RepoGraph) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.layout = LayoutMode::ForceDirected;

    if graph.functions.is_empty() {
        return canvas;
    }

    // Identify functions that participate in at least one Calls edge.
    let mut participants: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (from, to) in &graph.calls {
        participants.insert(from.clone());
        participants.insert(to.clone());
    }

    // Smaller bounds — function nodes can pack denser than crates.
    let fn_bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 160.0,
        h: 36.0,
    };
    for func in &graph.functions {
        let qname = func.qualified_name();
        if !participants.contains(&qname) {
            continue; // orphan — skip
        }
        let label = if func.is_test {
            format!("test {}", func.name)
        } else if func.is_async {
            format!("async {}", func.name)
        } else {
            func.name.clone()
        };
        let kind = if func.is_test {
            CanvasNodeKind::Test
        } else {
            CanvasNodeKind::Function
        };
        canvas.upsert_node(CanvasNode::new(
            SemanticId::code_function(&qname),
            label,
            kind,
            fn_bounds,
        ));
    }
    for (from_q, to_q) in &graph.calls {
        let from_id = CanvasNode::new(
            SemanticId::code_function(from_q),
            from_q.clone(),
            CanvasNodeKind::Function,
            fn_bounds,
        )
        .id;
        let to_id = CanvasNode::new(
            SemanticId::code_function(to_q),
            to_q.clone(),
            CanvasNodeKind::Function,
            fn_bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(from_id, to_id, EdgeKind::Calls));
    }
    layout::run(&mut canvas);
    canvas
}

/// **Repo Canvas (Engineering)** — crates + their *top-level* source
/// roots (`lib.rs` / `main.rs`), grouped by `DependsOn` edges from
/// crate → root file. Skips the per-file dump that produced 3000-node
/// noise; the user reads architecture at the crate scale + sees where
/// each crate's entry-point lives. To drill into specific files, the
/// user opens them in the editor; the canvas annotates, it doesn't
/// duplicate the file tree.
///
/// Falls back to [`to_canvas_architecture`] when no source roots are
/// discoverable.
pub fn to_canvas_repo(graph: &RepoGraph) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.layout = LayoutMode::Layered;

    let crate_bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 220.0,
        h: 56.0,
    };
    let file_bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 180.0,
        h: 36.0,
    };

    // Crates first (same as architecture view).
    for c in &graph.crates {
        canvas.upsert_node(CanvasNode::new(
            SemanticId::code_crate(&c.name),
            c.name.clone(),
            CanvasNodeKind::Crate,
            crate_bounds,
        ));
    }
    // Inter-crate deps.
    for (from, to) in &graph.deps {
        let from_id = CanvasNode::new(
            SemanticId::code_crate(from),
            from.clone(),
            CanvasNodeKind::Crate,
            crate_bounds,
        )
        .id;
        let to_id = CanvasNode::new(
            SemanticId::code_crate(to),
            to.clone(),
            CanvasNodeKind::Crate,
            crate_bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(from_id, to_id, EdgeKind::DependsOn));
    }
    // Per-crate source roots — `lib.rs` and `main.rs` only.
    for f in &graph.files {
        let Some(cn) = &f.crate_name else {
            continue;
        };
        let name = f
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name != "lib.rs" && name != "main.rs" {
            continue;
        }
        let rel = f.path.strip_prefix(&graph.root).unwrap_or(&f.path);
        let path_str = rel.to_string_lossy().to_string();
        let file_node = CanvasNode::new(
            SemanticId::code_file(&path_str),
            name,
            CanvasNodeKind::File,
            file_bounds,
        );
        let file_id = file_node.id;
        canvas.upsert_node(file_node);
        // crate → root file edge.
        let crate_id = CanvasNode::new(
            SemanticId::code_crate(cn),
            cn.clone(),
            CanvasNodeKind::Crate,
            crate_bounds,
        )
        .id;
        canvas.upsert_edge(CanvasEdge::new(crate_id, file_id, EdgeKind::DependsOn));
    }
    layout::run(&mut canvas);
    canvas
}

/// Translate a [`RepoGraph`] into a [`WorldPatch`] that adds a
/// `Crate` and `File` entity per node.
/// Project a [`RepoGraph`] as a Universal Reality Engine
/// [`wish_world_model::Primitive::Graph`]. Bridges Wish's code-domain
/// extractor to the URE substrate — the same JSON form a chemistry
/// `BondGraph` or a finance `ExposureGraph` would emit.
///
/// The returned `Graph` carries:
///   - one node per crate (SemanticId `code:crate:<name>`)
///   - one node per file (SemanticId `code:file:<rel-path>`)
///   - inter-crate `depends_on` edges
///   - file→crate `belongs_to` edges
///   - function→function `calls` edges if function extraction ran
///
/// See strategic frame
/// `wish-design/.../01-strategy/11-universal-reality-engine.md`.
pub fn to_ure_graph(graph: &RepoGraph) -> wish_world_model::Graph {
    use wish_world_model::{Graph as UreGraph, GraphEdge as UreEdge, Realm};
    let root_id = wish_world_model::SemanticId::new(
        Realm::Code,
        "graph",
        graph
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string()),
    );
    let mut g = UreGraph::new(root_id, "rust_repo_graph");
    for c in &graph.crates {
        g.add_node(wish_world_model::SemanticId::code_crate(&c.name));
    }
    for f in &graph.files {
        let rel = f.path.strip_prefix(&graph.root).unwrap_or(&f.path);
        let id = wish_world_model::SemanticId::code_file(&rel.to_string_lossy());
        g.add_node(id.clone());
        if let Some(cn) = &f.crate_name {
            g.add_edge(UreEdge {
                from: id,
                to: wish_world_model::SemanticId::code_crate(cn),
                kind: "belongs_to".to_string(),
                weight: None,
            });
        }
    }
    for (from, to) in &graph.deps {
        g.add_edge(UreEdge {
            from: wish_world_model::SemanticId::code_crate(from),
            to: wish_world_model::SemanticId::code_crate(to),
            kind: "depends_on".to_string(),
            weight: None,
        });
    }
    for (from_q, to_q) in &graph.calls {
        g.add_edge(UreEdge {
            from: wish_world_model::SemanticId::code_function(from_q),
            to: wish_world_model::SemanticId::code_function(to_q),
            kind: "calls".to_string(),
            weight: None,
        });
    }
    g
}

pub fn to_world_patch(graph: &RepoGraph) -> WorldPatch {
    let mut ops: Vec<PatchOp> = Vec::with_capacity(graph.crates.len() + graph.files.len());
    for c in &graph.crates {
        let id = SemanticId::code_crate(&c.name);
        ops.push(PatchOp::AddEntity(WorldEntity::stub(
            id,
            &c.name,
            EntityKind::Crate,
        )));
    }
    for f in &graph.files {
        let rel = f.path.strip_prefix(&graph.root).unwrap_or(&f.path);
        let path_str = rel.to_string_lossy().to_string();
        let id = SemanticId::code_file(&path_str);
        ops.push(PatchOp::AddEntity(WorldEntity::stub(
            id,
            &path_str,
            EntityKind::File,
        )));
    }
    WorldPatch::new(Actor::System, "wish-codegraph: extract repo", ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_extract_empty() {
        let g = extract_repo_graph(Path::new("/this/path/does/not/exist"));
        assert!(g.crates.is_empty());
        assert!(g.files.is_empty());
    }

    #[test]
    fn parse_cargo_toml_extracts_name_and_deps() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("cg_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        let cargo = tmp.join("Cargo.toml");
        std::fs::write(
            &cargo,
            r#"[package]
name = "demo-crate"
version = "0.1.0"

[dependencies]
serde = "1"
wish-world-model = { workspace = true }
wish-canvas-core = { path = "../wish-canvas-core" }

[dev-dependencies]
tempfile = "3"
"#,
        )
        .unwrap();
        let (name, deps) = parse_cargo_toml(&cargo).unwrap();
        assert_eq!(name, "demo-crate");
        assert!(deps.contains(&"serde".to_string()));
        assert!(deps.contains(&"wish-world-model".to_string()));
        assert!(deps.contains(&"wish-canvas-core".to_string()));
        assert!(deps.contains(&"tempfile".to_string()));
    }

    #[test]
    fn extract_repo_graph_recovers_workspace_deps_on_this_repo() {
        // Walk from this crate up to the repo root, then extract.
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop(); // crates/wish-codegraph -> crates
        p.pop(); // crates -> repo root
        let graph = extract_repo_graph(&p);
        // Plenty of crates in this repo.
        assert!(
            graph.crates.len() >= 50,
            "expected ≥50 crates, got {}",
            graph.crates.len()
        );
        // Some inter-crate workspace edges must have been recovered.
        assert!(
            !graph.deps.is_empty(),
            "expected at least one workspace-local dep edge"
        );
        // Our own crate should depend on wish-canvas-core and wish-world-model.
        let edges_from_codegraph: Vec<&String> = graph
            .deps
            .iter()
            .filter(|(from, _)| from == "wish-codegraph")
            .map(|(_, to)| to)
            .collect();
        assert!(
            edges_from_codegraph
                .iter()
                .any(|d| d.as_str() == "wish-canvas-core"),
            "wish-codegraph should depend on wish-canvas-core; got {:?}",
            edges_from_codegraph
        );
        assert!(
            edges_from_codegraph
                .iter()
                .any(|d| d.as_str() == "wish-world-model"),
            "wish-codegraph should depend on wish-world-model; got {:?}",
            edges_from_codegraph
        );
    }

    #[test]
    fn extract_functions_from_text_finds_pub_async_test_and_plain_fns() {
        let src = r#"
//! A doc comment
use std::fmt;

pub fn alpha() {}

async fn beta(x: i32) -> i32 { x }

pub(crate) async fn gamma<T>(t: T) -> T { t }

#[test]
fn delta_test() {
    assert_eq!(1, 1);
}

// pub fn commented_out() {}     <-- must NOT be detected

fn epsilon(
    arg: i32,
) -> i32 {
    arg
}
"#;
        let funcs = extract_functions_from_text(src);
        let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["alpha", "beta", "gamma", "delta_test", "epsilon"]
        );

        let alpha = funcs.iter().find(|f| f.name == "alpha").unwrap();
        assert!(alpha.is_pub);
        assert!(!alpha.is_async);
        assert!(!alpha.is_test);

        let beta = funcs.iter().find(|f| f.name == "beta").unwrap();
        assert!(!beta.is_pub);
        assert!(beta.is_async);

        let gamma = funcs.iter().find(|f| f.name == "gamma").unwrap();
        assert!(gamma.is_pub);
        assert!(gamma.is_async);

        let delta = funcs.iter().find(|f| f.name == "delta_test").unwrap();
        assert!(delta.is_test);
    }

    #[test]
    fn mentions_function_respects_word_boundaries() {
        let body = "fn outer() { inner_call(1); not_inner_call(2); inner_call(3); }";
        assert!(mentions_function(body, "inner_call"));
        // `not_inner_call` should not count as a mention of `inner_call`.
        let body2 = "fn outer() { not_inner_call(2); }";
        assert!(!mentions_function(body2, "inner_call"));
    }

    #[test]
    fn body_of_function_finds_matched_braces() {
        let src = "fn x() { let s = \"} not body\"; if true { return; } }\n";
        let body = body_of_function(src, 1).expect("body");
        assert!(body.starts_with("{"));
        assert!(body.ends_with("}"));
        assert!(body.contains("\"} not body\""));
    }

    #[test]
    fn function_extraction_finds_real_functions_in_this_crate() {
        // Scope the live-repo test to a single sibling crate so the
        // O(N²)-per-file call-edge step finishes in milliseconds. The
        // whole-workspace path is exercised manually via `wish-world
        // canvas repo --functions`.
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop(); // crates/wish-codegraph -> crates
        p.push("wish-canvas-core");
        assert!(p.exists(), "missing sibling crate: {}", p.display());
        let graph = extract_repo_graph_with(&p, &ExtractOptions::default().with_functions(true));
        assert!(
            graph.functions.len() > 10,
            "expected ≥10 functions in wish-canvas-core, found {}",
            graph.functions.len()
        );
        // Same-file calls only by default → still emits useful edges.
        assert!(!graph.calls.is_empty(), "expected at least one call edge");
    }

    #[test]
    fn smoke_to_canvas_and_patch() {
        let g = RepoGraph {
            root: PathBuf::from("/tmp/x"),
            crates: vec![RepoCrate {
                name: "x".into(),
                path: PathBuf::from("/tmp/x"),
            }],
            files: vec![RepoFile {
                path: PathBuf::from("/tmp/x/src/lib.rs"),
                crate_name: Some("x".into()),
            }],
            deps: vec![],
            functions: Vec::new(),
            calls: Vec::new(),
        };
        let c = to_canvas(&g);
        assert_eq!(c.nodes.len(), 2);
        let p = to_world_patch(&g);
        assert_eq!(p.ops.len(), 2);
    }

    #[test]
    fn ure_graph_adapter_lifts_crates_files_deps() {
        let mut g = RepoGraph::default();
        g.root = std::path::PathBuf::from("/tmp/repo");
        g.crates.push(RepoCrate {
            name: "alpha".to_string(),
            path: "/tmp/repo/crates/alpha".into(),
        });
        g.crates.push(RepoCrate {
            name: "beta".to_string(),
            path: "/tmp/repo/crates/beta".into(),
        });
        g.files.push(RepoFile {
            path: "/tmp/repo/crates/alpha/src/lib.rs".into(),
            crate_name: Some("alpha".to_string()),
        });
        g.deps.push(("alpha".to_string(), "beta".to_string()));
        let ure = to_ure_graph(&g);
        // 2 crates + 1 file = 3 nodes (dedup happens in add_node).
        assert_eq!(ure.nodes.len(), 3);
        // 1 file→crate belongs_to edge + 1 crate→crate depends_on.
        assert_eq!(ure.edges.len(), 2);
        let kinds: Vec<&str> = ure.edges.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains(&"belongs_to"));
        assert!(kinds.contains(&"depends_on"));
    }
}
