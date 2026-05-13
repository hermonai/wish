use std::path::PathBuf;

use lsp::LanguageServerId;
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use super::{DiagnosticsState, PerFileSummary, SeverityCounts};

fn diag(line: u32, col: u32, severity: DiagnosticSeverity, message: &str) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col + 1,
            },
        },
        severity: Some(severity),
        message: message.to_string(),
        ..Default::default()
    }
}

fn server() -> LanguageServerId {
    LanguageServerId::new()
}

fn path(p: &str) -> PathBuf {
    PathBuf::from(p)
}

#[test]
fn empty_state_has_zero_counts() {
    let state = DiagnosticsState::default();
    assert_eq!(state.counts(), SeverityCounts::default());
    assert_eq!(state.entry_count(), 0);
    assert_eq!(state.file_count(), 0);
    assert!(state.entries_grouped_by_path().is_empty());
}

#[test]
fn update_then_clear_via_empty_returns_changed_signal() {
    let mut state = DiagnosticsState::default();
    let s = server();
    let p = path("src/main.rs");
    let changed = state.update_path(
        s,
        p.clone(),
        vec![diag(1, 0, DiagnosticSeverity::ERROR, "x")],
    );
    assert!(changed);
    assert_eq!(state.entry_count(), 1);

    let cleared = state.update_path(s, p, vec![]);
    assert!(
        cleared,
        "empty publish should clear and return changed=true"
    );
    assert_eq!(state.entry_count(), 0);
}

#[test]
fn idempotent_republish_does_not_signal_change() {
    let mut state = DiagnosticsState::default();
    let s = server();
    let p = path("src/main.rs");
    let d = vec![diag(3, 7, DiagnosticSeverity::WARNING, "y")];
    let first = state.update_path(s, p.clone(), d.clone());
    let second = state.update_path(s, p, d);
    assert!(first);
    assert!(
        !second,
        "republishing identical diagnostics should be a no-op"
    );
}

#[test]
fn counts_track_severities_across_servers_and_files() {
    let mut state = DiagnosticsState::default();
    let s1 = server();
    let s2 = server();

    state.update_path(
        s1,
        path("a.rs"),
        vec![
            diag(0, 0, DiagnosticSeverity::ERROR, "e1"),
            diag(1, 0, DiagnosticSeverity::WARNING, "w1"),
        ],
    );
    state.update_path(
        s1,
        path("b.rs"),
        vec![diag(0, 0, DiagnosticSeverity::HINT, "h1")],
    );
    // Distinct server reports an extra diagnostic on the same file.
    state.update_path(
        s2,
        path("a.rs"),
        vec![diag(2, 0, DiagnosticSeverity::INFORMATION, "i1")],
    );

    let counts = state.counts();
    assert_eq!(
        counts,
        SeverityCounts {
            error: 1,
            warning: 1,
            info: 1,
            hint: 1,
        }
    );
    assert_eq!(counts.total(), 4);
    assert!(!counts.is_empty());

    assert_eq!(state.entry_count(), 4);
    assert_eq!(state.file_count(), 2);
}

#[test]
fn two_servers_on_same_file_do_not_clobber() {
    let mut state = DiagnosticsState::default();
    let rust_analyzer = server();
    let clippy = server();
    let p = path("src/main.rs");

    state.update_path(
        rust_analyzer,
        p.clone(),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "ra")],
    );
    state.update_path(
        clippy,
        p.clone(),
        vec![diag(5, 0, DiagnosticSeverity::WARNING, "c1")],
    );

    let merged = state.entries_for_path(&p);
    assert_eq!(merged.len(), 2, "both servers should be visible");

    // Clearing one server keeps the other.
    let changed = state.clear_server(clippy);
    assert!(changed);
    let after = state.entries_for_path(&p);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].1.message, "ra");
}

#[test]
fn grouped_view_sorts_by_severity_then_position() {
    let mut state = DiagnosticsState::default();
    let s = server();
    let p = path("src/main.rs");

    // Insert intentionally out of order.
    state.update_path(
        s,
        p.clone(),
        vec![
            diag(10, 0, DiagnosticSeverity::HINT, "h"),
            diag(2, 0, DiagnosticSeverity::ERROR, "e_late"),
            diag(0, 0, DiagnosticSeverity::WARNING, "w"),
            diag(0, 0, DiagnosticSeverity::ERROR, "e_early"),
        ],
    );

    let grouped = state.entries_grouped_by_path();
    assert_eq!(grouped.len(), 1);
    let (got_path, bucket) = &grouped[0];
    assert_eq!(got_path, &p);
    let messages: Vec<&str> = bucket.iter().map(|(_, d)| d.message.as_str()).collect();
    // Errors first, sorted by line; then warnings; then hints.
    assert_eq!(messages, vec!["e_early", "e_late", "w", "h"]);
}

#[test]
fn grouped_view_sorts_files_alphabetically() {
    let mut state = DiagnosticsState::default();
    let s = server();
    state.update_path(
        s,
        path("z.rs"),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "e")],
    );
    state.update_path(
        s,
        path("a.rs"),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "e")],
    );
    state.update_path(
        s,
        path("m.rs"),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "e")],
    );

    let grouped = state.entries_grouped_by_path();
    let paths: Vec<&std::path::Path> = grouped.iter().map(|(p, _)| p.as_path()).collect();
    assert_eq!(
        paths,
        vec![
            std::path::Path::new("a.rs"),
            std::path::Path::new("m.rs"),
            std::path::Path::new("z.rs")
        ]
    );
}

#[test]
fn clear_server_with_no_state_returns_unchanged() {
    let mut state = DiagnosticsState::default();
    let ghost_server = server();
    assert!(!state.clear_server(ghost_server));
}

#[test]
fn update_with_only_unknown_severity_is_counted_in_neither_bucket() {
    let mut state = DiagnosticsState::default();
    let s = server();
    let mut unknown = diag(0, 0, DiagnosticSeverity::ERROR, "x");
    unknown.severity = None;
    state.update_path(s, path("x.rs"), vec![unknown]);

    let counts = state.counts();
    assert_eq!(counts, SeverityCounts::default());
    // …but the entry is still tracked.
    assert_eq!(state.entry_count(), 1);
}

#[test]
fn summarize_empty_state_is_empty() {
    let state = DiagnosticsState::default();
    let summary = state.summarize(10);
    assert_eq!(summary.totals, SeverityCounts::default());
    assert_eq!(summary.file_count, 0);
    assert!(summary.by_file.is_empty());
    assert!(summary.top.is_empty());
}

#[test]
fn summarize_orders_files_by_severity_weight_descending() {
    let mut state = DiagnosticsState::default();
    let s = server();
    // a.rs: 1 hint
    state.update_path(
        s,
        path("a.rs"),
        vec![diag(0, 0, DiagnosticSeverity::HINT, "h")],
    );
    // b.rs: 2 errors (heaviest)
    state.update_path(
        s,
        path("b.rs"),
        vec![
            diag(0, 0, DiagnosticSeverity::ERROR, "e1"),
            diag(1, 0, DiagnosticSeverity::ERROR, "e2"),
        ],
    );
    // c.rs: 1 warning
    state.update_path(
        s,
        path("c.rs"),
        vec![diag(0, 0, DiagnosticSeverity::WARNING, "w")],
    );

    let summary = state.summarize(10);
    let paths: Vec<&std::path::Path> = summary.by_file.iter().map(|f| f.path.as_path()).collect();
    assert_eq!(
        paths,
        vec![
            std::path::Path::new("b.rs"),
            std::path::Path::new("c.rs"),
            std::path::Path::new("a.rs"),
        ]
    );

    // Per-file counts populated correctly.
    let b = summary
        .by_file
        .iter()
        .find(|f| f.path == path("b.rs"))
        .unwrap();
    assert_eq!(
        b,
        &PerFileSummary {
            path: path("b.rs"),
            counts: SeverityCounts {
                error: 2,
                ..Default::default()
            },
        }
    );
}

#[test]
fn summarize_top_n_caps_returned_entries() {
    let mut state = DiagnosticsState::default();
    let s = server();
    state.update_path(
        s,
        path("a.rs"),
        (0..20)
            .map(|i| diag(i, 0, DiagnosticSeverity::ERROR, "e"))
            .collect(),
    );

    let summary = state.summarize(5);
    assert_eq!(summary.top.len(), 5);
    assert_eq!(summary.totals.error, 20);

    // Top entries are sorted ascending by (severity, line) — errors at lines 0..=4.
    let lines: Vec<u32> = summary.top.iter().map(|t| t.line).collect();
    assert_eq!(lines, vec![0, 1, 2, 3, 4]);
}

#[test]
fn summarize_serializes_to_json() {
    let mut state = DiagnosticsState::default();
    let s = server();
    state.update_path(
        s,
        path("src/main.rs"),
        vec![{
            let mut d = diag(11, 4, DiagnosticSeverity::ERROR, "expected `;`");
            d.source = Some("rustc".to_string());
            d
        }],
    );
    let summary = state.summarize(5);
    let json = serde_json::to_string(&summary).expect("serializes");
    assert!(json.contains("\"error\":1"));
    assert!(json.contains("\"path\":\"src/main.rs\""));
    assert!(json.contains("\"severity\":\"error\""));
    assert!(json.contains("\"line\":11"));
    assert!(json.contains("\"source\":\"rustc\""));
}

#[test]
fn format_for_agent_context_empty_state() {
    let state = DiagnosticsState::default();
    let text = state.format_for_agent_context(10, 10);
    assert_eq!(
        text,
        "Workspace diagnostics: clean (no errors or warnings)."
    );
}

#[test]
fn format_for_agent_context_groups_by_file_with_line_columns_one_based() {
    let mut state = DiagnosticsState::default();
    let s = server();
    let mut d1 = diag(11, 4, DiagnosticSeverity::ERROR, "cannot find value `x`");
    d1.source = Some("rust-analyzer".to_string());
    state.update_path(s, path("src/main.rs"), vec![d1]);
    state.update_path(
        s,
        path("src/lib.rs"),
        vec![diag(
            3,
            7,
            DiagnosticSeverity::WARNING,
            "unused variable: `tmp`",
        )],
    );

    let text = state.format_for_agent_context(10, 10);
    // 1-based line/col rendering.
    assert!(
        text.contains("12:5"),
        "expected 1-based 12:5 in agent output, got: {text}"
    );
    assert!(text.contains("cannot find value `x`"));
    assert!(text.contains("[rust-analyzer]"));
    assert!(text.contains("4:8"));
    assert!(text.contains("unused variable: `tmp`"));
    // Header summary appears.
    assert!(text.contains("1 error"));
    assert!(text.contains("1 warning"));
    assert!(text.contains("in 2 files"));
}

#[test]
fn format_for_agent_context_caps_files_and_per_file_entries() {
    let mut state = DiagnosticsState::default();
    let s = server();
    for i in 0..5 {
        state.update_path(
            s,
            path(&format!("file_{i}.rs")),
            (0..6)
                .map(|j| diag(j, 0, DiagnosticSeverity::ERROR, "e"))
                .collect(),
        );
    }
    let text = state.format_for_agent_context(2, 3);
    // Two files shown.
    assert!(text.contains("…and 3 more files"));
    // Three entries per file shown.
    assert!(text.contains("…and 3 more"));
}

#[test]
fn format_for_agent_context_singular_grammar() {
    let mut state = DiagnosticsState::default();
    let s = server();
    state.update_path(
        s,
        path("a.rs"),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "boom")],
    );
    let text = state.format_for_agent_context(10, 10);
    assert!(text.contains("1 error in 1 file"));
    assert!(!text.contains("1 errors"));
    assert!(!text.contains("1 files"));
}

#[test]
fn entries_for_path_returns_only_matching_file() {
    let mut state = DiagnosticsState::default();
    let s = server();
    state.update_path(
        s,
        path("a.rs"),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "a")],
    );
    state.update_path(
        s,
        path("b.rs"),
        vec![diag(0, 0, DiagnosticSeverity::ERROR, "b")],
    );

    let a = state.entries_for_path(&path("a.rs"));
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].1.message, "a");

    let none = state.entries_for_path(&path("c.rs"));
    assert!(none.is_empty());
}
