use std::path::PathBuf;

use chrono::NaiveDate;

use super::{
    compose_message_with_context, find_active_project, format_active_project_block,
    format_git_status_block, format_open_files_block, format_recent_commands_block,
    NamedContextBlock, RecentCommand, RepoGitSummary,
};
use crate::persistence::model::Project;

fn project(
    path: &str,
    last_opened_year: i32,
    last_opened_month: u32,
    last_opened_day: u32,
) -> Project {
    let dt = NaiveDate::from_ymd_opt(last_opened_year, last_opened_month, last_opened_day)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    Project {
        path: path.to_string(),
        added_ts: dt,
        last_opened_ts: Some(dt),
    }
}

#[test]
fn no_blocks_returns_message_unchanged() {
    let composed = compose_message_with_context("hello", &[]);
    assert_eq!(composed, "hello");
}

#[test]
fn one_block_wraps_with_tags_and_user_message() {
    let blocks = vec![NamedContextBlock {
        name: "workspace_diagnostics",
        body:
            "Workspace diagnostics: 1 error in 1 file.\nsrc/main.rs (1 error):\n  error 12:5 — boom"
                .to_string(),
    }];
    let composed = compose_message_with_context("fix the errors", &blocks);
    let expected = "\
<workspace_diagnostics>
Workspace diagnostics: 1 error in 1 file.
src/main.rs (1 error):
  error 12:5 — boom
</workspace_diagnostics>

<user_message>
fix the errors
</user_message>";
    assert_eq!(composed, expected);
}

#[test]
fn multiple_blocks_are_concatenated_in_order() {
    let blocks = vec![
        NamedContextBlock {
            name: "workspace_diagnostics",
            body: "Workspace diagnostics: 1 error in 1 file.".to_string(),
        },
        NamedContextBlock {
            name: "git_status",
            body: "branch: main\n2 modified files".to_string(),
        },
    ];
    let composed = compose_message_with_context("what changed?", &blocks);
    assert!(composed.contains("<workspace_diagnostics>"));
    assert!(composed.contains("</workspace_diagnostics>"));
    assert!(composed.contains("<git_status>"));
    assert!(composed.contains("</git_status>"));
    // Diagnostics block precedes git_status block (collect order is preserved).
    let diag_idx = composed.find("<workspace_diagnostics>").unwrap();
    let git_idx = composed.find("<git_status>").unwrap();
    assert!(diag_idx < git_idx, "blocks must appear in input order");
    // The user message comes last, exactly once.
    let user_msg_count = composed.matches("<user_message>").count();
    assert_eq!(user_msg_count, 1);
    assert!(composed.ends_with("</user_message>"));
}

#[test]
fn trailing_whitespace_in_block_body_is_trimmed() {
    // Avoids `\n</tag>` with stray blank lines that look like the model's own output.
    let blocks = vec![NamedContextBlock {
        name: "workspace_diagnostics",
        body: "hello\n\n\n".to_string(),
    }];
    let composed = compose_message_with_context("hi", &blocks);
    assert!(composed.contains("hello\n</workspace_diagnostics>"));
    assert!(!composed.contains("\n\n\n</workspace_diagnostics>"));
}

#[test]
fn empty_user_message_is_preserved() {
    // The agent shouldn't have its empty-message handling silently changed
    // when context injection is on. Producing `<user_message>\n\n</user_message>`
    // keeps that signal intact.
    let blocks = vec![NamedContextBlock {
        name: "workspace_diagnostics",
        body: "hello".to_string(),
    }];
    let composed = compose_message_with_context("", &blocks);
    assert!(composed.contains("<user_message>\n\n</user_message>"));
}

#[test]
fn block_body_with_inner_newlines_is_preserved_verbatim() {
    let blocks = vec![NamedContextBlock {
        name: "workspace_diagnostics",
        body: "line one\nline two\nline three".to_string(),
    }];
    let composed = compose_message_with_context("ok", &blocks);
    assert!(composed.contains("line one\nline two\nline three"));
}

#[test]
fn open_files_block_empty_returns_none() {
    assert_eq!(format_open_files_block(&[], 8), None);
}

#[test]
fn open_files_block_single_file_uses_singular_label() {
    let body = format_open_files_block(&[PathBuf::from("src/main.rs")], 8).unwrap();
    assert!(
        body.starts_with("Files the user has recently opened (1 file):"),
        "got: {body}"
    );
    assert!(body.contains("src/main.rs"));
}

#[test]
fn open_files_block_plural_within_limit() {
    let paths: Vec<PathBuf> = ["a.rs", "b.rs", "c.rs"].iter().map(PathBuf::from).collect();
    let body = format_open_files_block(&paths, 8).unwrap();
    assert!(body.starts_with("Files the user has recently opened (3 files):"));
    assert!(body.contains("a.rs"));
    assert!(body.contains("b.rs"));
    assert!(body.contains("c.rs"));
    assert!(!body.contains("…and"));
}

#[test]
fn open_files_block_truncates_with_and_n_more_when_over_limit() {
    let paths: Vec<PathBuf> = (0..10).map(|i| PathBuf::from(format!("f{i}.rs"))).collect();
    let body = format_open_files_block(&paths, 3).unwrap();
    assert!(body.starts_with("Files the user has recently opened (3 of 10 shown):"));
    assert!(body.contains("f0.rs"));
    assert!(body.contains("f1.rs"));
    assert!(body.contains("f2.rs"));
    assert!(!body.contains("f3.rs"));
    assert!(body.ends_with("…and 7 more"));
}

#[test]
fn open_files_block_preserves_input_order() {
    // The caller is responsible for sorting (typically by recency desc); the
    // formatter must not reorder.
    let paths: Vec<PathBuf> = vec!["z.rs", "a.rs", "m.rs"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let body = format_open_files_block(&paths, 8).unwrap();
    let z = body.find("z.rs").unwrap();
    let a = body.find("a.rs").unwrap();
    let m = body.find("m.rs").unwrap();
    assert!(z < a && a < m, "formatter must preserve input order");
}

#[test]
fn open_files_block_with_repo_prefixed_paths_renders_full_path() {
    let paths = vec![PathBuf::from("/home/dev/proj/src/main.rs")];
    let body = format_open_files_block(&paths, 8).unwrap();
    assert!(
        body.contains("/home/dev/proj/src/main.rs"),
        "expected absolute path verbatim, got: {body}"
    );
}

#[test]
fn active_project_block_empty_path_returns_none() {
    assert_eq!(format_active_project_block(""), None);
    assert_eq!(format_active_project_block("   "), None);
}

#[test]
fn active_project_block_renders_path_verbatim() {
    let body = format_active_project_block("/Users/dev/proj").unwrap();
    assert_eq!(body, "The user is working in: /Users/dev/proj");
}

#[test]
fn find_active_project_returns_none_for_no_projects() {
    assert!(find_active_project(&[]).is_none());
}

#[test]
fn find_active_project_picks_most_recent_last_opened() {
    let projects = vec![
        project("/a", 2025, 1, 1),
        project("/b", 2026, 5, 13), // most recent
        project("/c", 2026, 1, 1),
    ];
    let picked = find_active_project(&projects).unwrap();
    assert_eq!(picked.path, "/b");
}

#[test]
fn find_active_project_falls_back_to_added_ts_when_never_opened() {
    // Project that's been added but never opened: last_opened_ts is None;
    // `last_used_at` returns `added_ts`. find_active_project should still
    // consider it.
    let dt_old = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let dt_new = NaiveDate::from_ymd_opt(2026, 5, 13)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let projects = vec![
        Project {
            path: "/old-opened".to_string(),
            added_ts: dt_old,
            last_opened_ts: Some(dt_old),
        },
        Project {
            path: "/new-only-added".to_string(),
            added_ts: dt_new,
            last_opened_ts: None,
        },
    ];
    let picked = find_active_project(&projects).unwrap();
    assert_eq!(picked.path, "/new-only-added");
}

fn cmd(c: &str, exit: Option<i32>) -> RecentCommand {
    RecentCommand {
        command: c.to_string(),
        exit_code: exit,
    }
}

#[test]
fn recent_commands_block_empty_returns_none() {
    assert_eq!(format_recent_commands_block(&[], 10), None);
}

#[test]
fn recent_commands_block_success_elides_exit_code() {
    let body = format_recent_commands_block(&[cmd("ls", Some(0))], 10).unwrap();
    assert!(body.contains("✓ ls"), "got: {body}");
    assert!(
        !body.contains("(exit 0)"),
        "exit 0 should be elided for noise reduction, got: {body}"
    );
}

#[test]
fn recent_commands_block_failure_shows_exit_code_and_x_marker() {
    let body = format_recent_commands_block(&[cmd("cargo test", Some(1))], 10).unwrap();
    assert!(body.contains("✗ cargo test (exit 1)"), "got: {body}");
}

#[test]
fn recent_commands_block_running_shows_dot_marker() {
    let body = format_recent_commands_block(&[cmd("cargo build", None)], 10).unwrap();
    assert!(
        body.contains("• cargo build (still running)"),
        "got: {body}"
    );
}

#[test]
fn recent_commands_block_preserves_input_order() {
    // Caller is responsible for sorting most-recent-first; formatter must not reorder.
    let commands = vec![
        cmd("first", Some(0)),
        cmd("second", Some(1)),
        cmd("third", Some(0)),
    ];
    let body = format_recent_commands_block(&commands, 10).unwrap();
    let f = body.find("first").unwrap();
    let s = body.find("second").unwrap();
    let t = body.find("third").unwrap();
    assert!(f < s && s < t, "formatter must preserve order");
}

#[test]
fn recent_commands_block_truncates_long_lines() {
    let long_cmd = "echo ".to_string() + &"x".repeat(500);
    let body = format_recent_commands_block(&[cmd(&long_cmd, Some(0))], 10).unwrap();
    assert!(
        body.contains("…"),
        "long commands should be truncated, got: {body}"
    );
    // Body should not contain the full 500-char tail.
    assert!(!body.contains(&"x".repeat(500)));
}

#[test]
fn recent_commands_block_collapses_multiline_command() {
    // Multi-line pastes happen for heredocs and shell loops. Keep the block
    // legible by replacing internal newlines with a visible marker.
    let body = format_recent_commands_block(&[cmd("git commit -m \"line1\nline2\"", Some(0))], 10)
        .unwrap();
    assert!(body.contains("⏎"), "multi-line should be marked: {body}");
    // The block must not break into a new bullet for the inner newline.
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected header + 1 command line, got: {body}"
    );
}

#[test]
fn recent_commands_block_caps_to_limit_with_count_in_header() {
    let commands: Vec<RecentCommand> = (0..20).map(|i| cmd(&format!("c{i}"), Some(0))).collect();
    let body = format_recent_commands_block(&commands, 5).unwrap();
    assert!(
        body.starts_with("Recent terminal commands (5 most recent first):"),
        "got: {body}"
    );
    assert!(body.contains("c0"));
    assert!(body.contains("c4"));
    assert!(!body.contains("c5"), "anything past limit should be elided");
}

#[test]
fn recent_commands_block_negative_exit_code_renders_as_failure() {
    let body = format_recent_commands_block(&[cmd("./bad", Some(-9))], 10).unwrap();
    assert!(body.contains("✗ ./bad (exit -9)"), "got: {body}");
}

fn repo(
    path: &str,
    branch: &str,
    main: &str,
    files_changed: usize,
    additions: usize,
    deletions: usize,
) -> RepoGitSummary {
    RepoGitSummary {
        path: PathBuf::from(path),
        branch: branch.to_string(),
        main_branch: main.to_string(),
        files_changed,
        total_additions: additions,
        total_deletions: deletions,
    }
}

#[test]
fn git_status_block_empty_returns_none() {
    assert_eq!(format_git_status_block(&[]), None);
}

#[test]
fn git_status_block_on_main_clean_renders_main_branch_label() {
    let body =
        format_git_status_block(&[repo("/Users/dev/proj", "main", "main", 0, 0, 0)]).unwrap();
    assert!(
        body.contains("/Users/dev/proj on `main` (main branch): clean"),
        "got: {body}"
    );
    // Don't duplicate the main-branch annotation.
    assert!(!body.contains("(main: `main`)"));
}

#[test]
fn git_status_block_off_main_clean_shows_main_pointer() {
    let body = format_git_status_block(&[repo("/Users/dev/proj", "feature/foo", "main", 0, 0, 0)])
        .unwrap();
    assert!(
        body.contains("/Users/dev/proj on `feature/foo` (main: `main`): clean"),
        "got: {body}"
    );
}

#[test]
fn git_status_block_dirty_renders_modified_files_and_line_deltas() {
    let body = format_git_status_block(&[repo("/Users/dev/proj", "feature/foo", "main", 3, 42, 8)])
        .unwrap();
    assert!(body.contains("3 modified files (+42 −8)"), "got: {body}");
}

#[test]
fn git_status_block_singular_modified_file() {
    let body =
        format_git_status_block(&[repo("/Users/dev/proj", "main", "main", 1, 5, 0)]).unwrap();
    assert!(body.contains("1 modified file (+5 −0)"), "got: {body}");
}

#[test]
fn git_status_block_multi_repo_renders_one_bullet_each() {
    let body = format_git_status_block(&[
        repo("/Users/dev/a", "main", "main", 0, 0, 0),
        repo("/Users/dev/b", "feature/x", "main", 2, 10, 3),
    ])
    .unwrap();
    assert!(body.contains("/Users/dev/a on `main`"));
    assert!(body.contains("/Users/dev/b on `feature/x`"));
    assert!(body.contains("clean"));
    assert!(body.contains("2 modified files (+10 −3)"));
    // Header appears exactly once.
    assert_eq!(body.matches("Workspace git status:").count(), 1);
    // Repos appear in the input order (caller is responsible for sorting).
    let a_idx = body.find("/Users/dev/a").unwrap();
    let b_idx = body.find("/Users/dev/b").unwrap();
    assert!(a_idx < b_idx);
}

#[test]
fn git_status_block_detached_head_renders_sha_as_branch() {
    // The git layer already returns a short SHA via detect_current_branch_display
    // for detached-HEAD state. The formatter renders it verbatim — no special case.
    let body =
        format_git_status_block(&[repo("/Users/dev/proj", "abc1234", "main", 1, 1, 1)]).unwrap();
    assert!(body.contains("on `abc1234`"), "got: {body}");
    assert!(body.contains("(main: `main`)"));
}
