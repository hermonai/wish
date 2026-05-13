//! Workspace-wide LSP diagnostics aggregator.
//!
//! This is the storage of record for two parallel consumers:
//!
//! 1. **The Problems panel UI** — needs a flat, file-grouped, severity-sorted
//!    view of every diagnostic in the workspace right now.
//! 2. **The AI agent** — needs the *same* state in a stable, serializable
//!    shape so prompts can include live workspace context without an LSP
//!    roundtrip. This is the core of Wish's "AI-native" stance: the agent
//!    never sees a divergent view from the human.
//!
//! Architecture:
//! - [`DiagnosticsState`] is the pure (non-`wishui`) storage. Keyed by
//!   `(LanguageServerId, PathBuf)` so two servers that report on the same
//!   file (`rust-analyzer` + `clippy`, `pyright` + `ruff`) do not clobber
//!   each other. Exposes:
//!     - `entries_grouped_by_path()` — input for the Problems panel rows.
//!     - `summarize(top_n)` → [`DiagnosticsSummary`] — `serde::Serialize`
//!       snapshot suitable for the panel header *and* the agent context tray.
//!     - `format_for_agent_context(...)` — deterministic plain-text rendering
//!       with 1-based line/column suitable for embedding in an LLM prompt.
//! - [`DiagnosticsAggregatorModel`] is a `SingletonEntity` that wraps the
//!   state, subscribes to [`LspManagerModel`] for server lifecycle, and
//!   subscribes to each [`LspServerModel`] for `DiagnosticsUpdated`. It
//!   emits [`DiagnosticsAggregatorEvent::Changed`] for downstream views.
//!
//! No UI lives in this module — the Problems panel view, status-bar badge,
//! and agent context tray are separate slices that read this state.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lsp::{LanguageServerId, LspEvent, LspManagerModel, LspManagerModelEvent, LspServerModel};
use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde::Serialize;
use wishui::{Entity, ModelContext, ModelHandle, SingletonEntity};

/// Counts of diagnostics by severity. Useful for status-bar badges and
/// for tests.
#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel UI / status bar badge"
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SeverityCounts {
    pub error: usize,
    pub warning: usize,
    pub info: usize,
    pub hint: usize,
}

#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel UI / status bar badge"
)]
impl SeverityCounts {
    pub fn total(&self) -> usize {
        self.error + self.warning + self.info + self.hint
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Pure storage of workspace-wide diagnostics. Has no dependency on `wishui`,
/// so it's covered by ordinary unit tests.
#[derive(Debug, Default)]
pub struct DiagnosticsState {
    by_server: HashMap<(LanguageServerId, PathBuf), Vec<Diagnostic>>,
}

#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel UI; tests cover this surface today"
)]
impl DiagnosticsState {
    /// Replace the set of diagnostics a single server has for a single file.
    /// An empty `diagnostics` is treated as a removal.
    /// Returns `true` if the stored state changed (i.e. callers should notify).
    pub fn update_path(
        &mut self,
        server_id: LanguageServerId,
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
    ) -> bool {
        let key = (server_id, path);
        if diagnostics.is_empty() {
            self.by_server.remove(&key).is_some()
        } else {
            // Compare lengths first as a cheap fast-path; if they match,
            // serde-compare the diagnostics so an idempotent re-publish from
            // the server doesn't generate a spurious "Changed" event.
            let unchanged = match self.by_server.get(&key) {
                Some(prev) if prev.len() == diagnostics.len() => {
                    diagnostics_equal(prev, &diagnostics)
                }
                _ => false,
            };
            self.by_server.insert(key, diagnostics);
            !unchanged
        }
    }

    /// Drop all diagnostics a specific server has published. Called when a
    /// server is removed from the manager.
    pub fn clear_server(&mut self, server_id: LanguageServerId) -> bool {
        let before = self.by_server.len();
        self.by_server.retain(|(sid, _), _| *sid != server_id);
        self.by_server.len() != before
    }

    /// Severity tallies across the whole workspace.
    pub fn counts(&self) -> SeverityCounts {
        let mut c = SeverityCounts::default();
        for diags in self.by_server.values() {
            for d in diags {
                match d.severity {
                    Some(DiagnosticSeverity::ERROR) => c.error += 1,
                    Some(DiagnosticSeverity::WARNING) => c.warning += 1,
                    Some(DiagnosticSeverity::INFORMATION) => c.info += 1,
                    Some(DiagnosticSeverity::HINT) => c.hint += 1,
                    _ => {}
                }
            }
        }
        c
    }

    /// Total number of diagnostic entries across every file and every server.
    pub fn entry_count(&self) -> usize {
        self.by_server.values().map(|v| v.len()).sum()
    }

    /// Number of distinct files with at least one diagnostic.
    pub fn file_count(&self) -> usize {
        self.by_server
            .keys()
            .map(|(_, p)| p)
            .collect::<HashSet<_>>()
            .len()
    }

    /// Diagnostics for a single path, merged across every server that has
    /// reported on it. Order matches `entries_grouped_by_path` (severity then
    /// position).
    pub fn entries_for_path(&self, path: &Path) -> Vec<(LanguageServerId, Diagnostic)> {
        let mut out: Vec<(LanguageServerId, Diagnostic)> = self
            .by_server
            .iter()
            .filter(|((_, p), _)| p.as_path() == path)
            .flat_map(|((sid, _), diags)| diags.iter().map(move |d| (*sid, d.clone())))
            .collect();
        out.sort_by(|(_, a), (_, b)| diagnostic_order(a).cmp(&diagnostic_order(b)));
        out
    }

    /// All entries grouped by file, file paths sorted ascending, each file's
    /// diagnostics sorted by severity (errors first) then line/column.
    /// Suitable input for the Problems panel.
    pub fn entries_grouped_by_path(&self) -> Vec<(PathBuf, Vec<(LanguageServerId, Diagnostic)>)> {
        let mut by_path: HashMap<PathBuf, Vec<(LanguageServerId, Diagnostic)>> = HashMap::new();
        for ((sid, path), diags) in &self.by_server {
            let bucket = by_path.entry(path.clone()).or_default();
            for d in diags {
                bucket.push((*sid, d.clone()));
            }
        }
        let mut out: Vec<_> = by_path.into_iter().collect();
        for (_, bucket) in out.iter_mut() {
            bucket.sort_by(|(_, a), (_, b)| diagnostic_order(a).cmp(&diagnostic_order(b)));
        }
        out.sort_by(|(a, _), (b, _)| a.cmp(b));
        out
    }
}

/// Per-file rollup used by the Problems panel header and the agent context tray.
#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel header + agent context tray"
)]
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PerFileSummary {
    pub path: PathBuf,
    pub counts: SeverityCounts,
}

/// Compact snapshot of the workspace's diagnostic state. Intended for two
/// consumers:
///
/// 1. The Problems panel header (`12 errors, 3 warnings in 4 files`).
/// 2. The AI agent context tray — the agent should see the same authoritative
///    state the human does, in a stable serializable shape.
///
/// Deliberately a snapshot, not a live view: the agent consumes it once per
/// turn, so it should be cheap to produce and not hold borrows.
#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel header + agent context tray"
)]
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticsSummary {
    pub totals: SeverityCounts,
    pub file_count: usize,
    /// Per-file rollups, files sorted by descending severity weight then by path.
    pub by_file: Vec<PerFileSummary>,
    /// Top-N actionable diagnostics, severity-first then file/line. Capped to
    /// the `limit` requested by the caller (typically a small number suitable
    /// for an agent prompt window).
    pub top: Vec<TopDiagnostic>,
}

/// A single high-priority entry suitable for an agent prompt or a status tooltip.
/// Fields are deliberately denormalized for legibility on the wire.
#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel header + agent context tray"
)]
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TopDiagnostic {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
    pub severity: &'static str,
    pub message: String,
    pub source: Option<String>,
}

#[allow(
    dead_code,
    reason = "consumed by upcoming Problems panel header + agent context tray"
)]
impl DiagnosticsState {
    /// Produce a snapshot of the current state suitable for the Problems panel
    /// header and for handing to the agent as machine-readable context.
    ///
    /// `top_n` caps the number of `top` entries so this is cheap on huge
    /// workspaces. The ordering matches `entries_grouped_by_path()`: errors
    /// first, then by file path, then by line/column.
    pub fn summarize(&self, top_n: usize) -> DiagnosticsSummary {
        // Per-file rollup.
        let mut per_file: HashMap<PathBuf, SeverityCounts> = HashMap::new();
        for ((_, path), diags) in &self.by_server {
            let bucket = per_file.entry(path.clone()).or_default();
            for d in diags {
                match d.severity {
                    Some(DiagnosticSeverity::ERROR) => bucket.error += 1,
                    Some(DiagnosticSeverity::WARNING) => bucket.warning += 1,
                    Some(DiagnosticSeverity::INFORMATION) => bucket.info += 1,
                    Some(DiagnosticSeverity::HINT) => bucket.hint += 1,
                    _ => {}
                }
            }
        }
        let mut by_file: Vec<PerFileSummary> = per_file
            .into_iter()
            .map(|(path, counts)| PerFileSummary { path, counts })
            .collect();
        // Sort by descending severity weight (errors heavier than warnings…) then path.
        by_file.sort_by(|a, b| {
            file_severity_weight(&b.counts)
                .cmp(&file_severity_weight(&a.counts))
                .then_with(|| a.path.cmp(&b.path))
        });

        // Top-N actionable items.
        let mut all: Vec<(PathBuf, &Diagnostic)> = self
            .by_server
            .iter()
            .flat_map(|((_, path), diags)| diags.iter().map(move |d| (path.clone(), d)))
            .collect();
        all.sort_by(|(pa, a), (pb, b)| {
            diagnostic_order(a)
                .cmp(&diagnostic_order(b))
                .then_with(|| pa.cmp(pb))
        });
        let top: Vec<TopDiagnostic> = all
            .into_iter()
            .take(top_n)
            .map(|(path, d)| TopDiagnostic {
                path,
                line: d.range.start.line,
                column: d.range.start.character,
                severity: severity_label(d.severity),
                message: d.message.clone(),
                source: d.source.clone(),
            })
            .collect();

        DiagnosticsSummary {
            totals: self.counts(),
            file_count: self.file_count(),
            by_file,
            top,
        }
    }

    /// Format the workspace's diagnostics as plain text suitable for embedding
    /// in an agent prompt. Deliberately compact and deterministic so the model
    /// sees the same string every time the state is the same.
    ///
    /// Example output:
    /// ```text
    /// Workspace diagnostics: 3 errors, 1 warning in 2 files.
    /// src/main.rs (2 errors):
    ///   error 12:5 — cannot find value `x` in this scope [rust-analyzer]
    ///   error 28:1 — expected `;`
    /// src/lib.rs (1 error, 1 warning):
    ///   error 4:8 — mismatched types
    ///   warning 22:1 — unused variable: `tmp`
    /// ```
    pub fn format_for_agent_context(&self, max_files: usize, max_per_file: usize) -> String {
        if self.entry_count() == 0 {
            return "Workspace diagnostics: clean (no errors or warnings).".to_string();
        }

        let totals = self.counts();
        let mut out = String::new();
        let _ = write!(
            out,
            "Workspace diagnostics: {} in {} {}.",
            describe_totals(&totals),
            self.file_count(),
            if self.file_count() == 1 {
                "file"
            } else {
                "files"
            }
        );

        let grouped = self.entries_grouped_by_path();
        // Order files by severity-weight desc, like `summarize`, so the most
        // actionable file shows first.
        let mut grouped = grouped;
        grouped.sort_by(|a, b| {
            let aw = bucket_severity_weight(&a.1);
            let bw = bucket_severity_weight(&b.1);
            bw.cmp(&aw).then_with(|| a.0.cmp(&b.0))
        });

        for (path, bucket) in grouped.iter().take(max_files) {
            let mut counts = SeverityCounts::default();
            for (_, d) in bucket {
                match d.severity {
                    Some(DiagnosticSeverity::ERROR) => counts.error += 1,
                    Some(DiagnosticSeverity::WARNING) => counts.warning += 1,
                    Some(DiagnosticSeverity::INFORMATION) => counts.info += 1,
                    Some(DiagnosticSeverity::HINT) => counts.hint += 1,
                    _ => {}
                }
            }
            let _ = write!(out, "\n{} ({}):", path.display(), describe_totals(&counts));
            for (_, d) in bucket.iter().take(max_per_file) {
                let label = severity_label(d.severity);
                let src = d
                    .source
                    .as_ref()
                    .map(|s| format!(" [{s}]"))
                    .unwrap_or_default();
                let _ = write!(
                    out,
                    "\n  {} {}:{} — {}{}",
                    label,
                    d.range.start.line + 1,
                    d.range.start.character + 1,
                    d.message.lines().next().unwrap_or(""),
                    src
                );
            }
            if bucket.len() > max_per_file {
                let _ = write!(out, "\n  …and {} more", bucket.len() - max_per_file);
            }
        }
        if grouped.len() > max_files {
            let _ = write!(out, "\n…and {} more files", grouped.len() - max_files);
        }
        out
    }
}

fn severity_label(s: Option<DiagnosticSeverity>) -> &'static str {
    match s {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "diagnostic",
    }
}

/// Higher = more severe. Used to sort files for the Problems panel + agent.
fn file_severity_weight(c: &SeverityCounts) -> u64 {
    // Errors dominate; warnings next; then info/hint as tie-breakers.
    (c.error as u64) * 10_000 + (c.warning as u64) * 100 + (c.info as u64) * 10 + (c.hint as u64)
}

fn bucket_severity_weight(bucket: &[(LanguageServerId, Diagnostic)]) -> u64 {
    let mut c = SeverityCounts::default();
    for (_, d) in bucket {
        match d.severity {
            Some(DiagnosticSeverity::ERROR) => c.error += 1,
            Some(DiagnosticSeverity::WARNING) => c.warning += 1,
            Some(DiagnosticSeverity::INFORMATION) => c.info += 1,
            Some(DiagnosticSeverity::HINT) => c.hint += 1,
            _ => {}
        }
    }
    file_severity_weight(&c)
}

fn describe_totals(c: &SeverityCounts) -> String {
    let mut parts: Vec<String> = Vec::new();
    if c.error > 0 {
        parts.push(format!("{} {}", c.error, pluralize("error", c.error)));
    }
    if c.warning > 0 {
        parts.push(format!("{} {}", c.warning, pluralize("warning", c.warning)));
    }
    if c.info > 0 {
        parts.push(format!("{} {}", c.info, pluralize("info", c.info)));
    }
    if c.hint > 0 {
        parts.push(format!("{} {}", c.hint, pluralize("hint", c.hint)));
    }
    if parts.is_empty() {
        "no diagnostics".to_string()
    } else {
        parts.join(", ")
    }
}

fn pluralize(word: &str, n: usize) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

/// Stable ordering used by the Problems panel.
///
/// Tuple is `(severity_rank, line, column)`. Severity 0 = error, 1 = warning,
/// 2 = information, 3 = hint, 4 = unknown.
fn diagnostic_order(d: &Diagnostic) -> (u8, u32, u32) {
    (
        severity_rank(d.severity),
        d.range.start.line,
        d.range.start.character,
    )
}

fn severity_rank(s: Option<DiagnosticSeverity>) -> u8 {
    match s {
        Some(DiagnosticSeverity::ERROR) => 0,
        Some(DiagnosticSeverity::WARNING) => 1,
        Some(DiagnosticSeverity::INFORMATION) => 2,
        Some(DiagnosticSeverity::HINT) => 3,
        _ => 4,
    }
}

/// Compare diagnostic vectors element-wise on the user-visible fields. We
/// deliberately don't compare every LSP-protocol field; this is for change
/// detection, not protocol equality.
fn diagnostics_equal(a: &[Diagnostic], b: &[Diagnostic]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.range == y.range
            && x.severity == y.severity
            && x.code == y.code
            && x.source == y.source
            && x.message == y.message
    })
}

// =====================================================================
// WishUI model wrapper.
// =====================================================================

#[derive(Debug)]
pub enum DiagnosticsAggregatorEvent {
    /// State changed in a way that warrants a re-render.
    Changed,
}

/// Singleton owner of [`DiagnosticsState`] plus the subscriptions that keep
/// it in sync with every running language server.
pub struct DiagnosticsAggregatorModel {
    state: DiagnosticsState,
    subscribed_servers: HashSet<LanguageServerId>,
}

impl Entity for DiagnosticsAggregatorModel {
    type Event = DiagnosticsAggregatorEvent;
}

impl SingletonEntity for DiagnosticsAggregatorModel {}

impl DiagnosticsAggregatorModel {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let mut me = Self {
            state: DiagnosticsState::default(),
            subscribed_servers: HashSet::new(),
        };

        let manager = LspManagerModel::handle(ctx);

        // React to server lifecycle: subscribe to newly started servers and
        // purge state when servers are removed. `ModelContext::subscribe_to_model`
        // takes a 3-arg callback `(self, event, ctx)`.
        let manager_for_closure = manager.clone();
        ctx.subscribe_to_model(&manager, move |this, event, ctx| match event {
            LspManagerModelEvent::ServerStarted(path) => {
                this.subscribe_to_servers_at(&manager_for_closure, path.clone(), ctx);
            }
            LspManagerModelEvent::ServerStopped(_) => {
                // Keep the last-published diagnostics visible while a server
                // is stopped; ServerRemoved is the signal to purge.
            }
            LspManagerModelEvent::ServerRemoved { server_id, .. } => {
                let id = *server_id;
                if this.state.clear_server(id) {
                    ctx.emit(DiagnosticsAggregatorEvent::Changed);
                }
                this.subscribed_servers.remove(&id);
            }
        });

        // Catch up with servers that registered before this aggregator was
        // constructed (e.g. the user opened a workspace before flipping the
        // ProblemsPanel feature flag).
        let known_roots: Vec<PathBuf> = manager.as_ref(ctx).workspace_roots().cloned().collect();
        for path in known_roots {
            me.subscribe_to_servers_at(&manager, path, ctx);
        }

        me
    }

    /// Subscribe to every server registered at `workspace_root` that we don't
    /// already track, and pull its current diagnostic snapshot.
    fn subscribe_to_servers_at(
        &mut self,
        manager: &ModelHandle<LspManagerModel>,
        workspace_root: PathBuf,
        ctx: &mut ModelContext<Self>,
    ) {
        let server_handles: Vec<ModelHandle<LspServerModel>> = manager
            .as_ref(ctx)
            .servers_for_workspace(&workspace_root)
            .cloned()
            .unwrap_or_default();

        for server_handle in server_handles {
            let server_id = server_handle.as_ref(ctx).id();
            if !self.subscribed_servers.insert(server_id) {
                continue;
            }

            // Seed initial state from anything the server has already
            // published before we subscribed.
            let mut changed = false;
            let initial: Vec<(PathBuf, Vec<Diagnostic>)> = server_handle
                .as_ref(ctx)
                .iter_diagnostics()
                .map(|(p, d)| (p.to_path_buf(), d.diagnostics.clone()))
                .collect();
            for (path, diags) in initial {
                if self.state.update_path(server_id, path, diags) {
                    changed = true;
                }
            }
            if changed {
                ctx.emit(DiagnosticsAggregatorEvent::Changed);
            }

            // Track future updates from this server.
            let captured_server_id = server_id;
            let captured_server = server_handle.clone();
            ctx.subscribe_to_model(&server_handle, move |this, event, ctx| {
                if let LspEvent::DiagnosticsUpdated { path } = event {
                    let snapshot = captured_server
                        .as_ref(ctx)
                        .diagnostics_for_path(path.as_path())
                        .ok()
                        .flatten()
                        .map(|d| d.diagnostics.clone())
                        .unwrap_or_default();
                    if this
                        .state
                        .update_path(captured_server_id, path.clone(), snapshot)
                    {
                        ctx.emit(DiagnosticsAggregatorEvent::Changed);
                    }
                }
            });
        }
    }

    /// Borrow the underlying state for read-only access (UI render path).
    #[allow(dead_code, reason = "consumed by upcoming Problems panel UI")]
    pub fn state(&self) -> &DiagnosticsState {
        &self.state
    }

    /// Convenience: produce the same snapshot a downstream view or agent
    /// context call would want, without forcing callers to reach through
    /// `state()`.
    #[allow(
        dead_code,
        reason = "consumed by upcoming Problems panel header + agent context tray"
    )]
    pub fn summary(&self, top_n: usize) -> DiagnosticsSummary {
        self.state.summarize(top_n)
    }

    /// Convenience: deterministic plain-text rendering for embedding in an
    /// agent prompt. See `DiagnosticsState::format_for_agent_context` for the
    /// exact shape.
    #[allow(dead_code, reason = "consumed by upcoming agent context tray")]
    pub fn format_for_agent_context(&self, max_files: usize, max_per_file: usize) -> String {
        self.state.format_for_agent_context(max_files, max_per_file)
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
