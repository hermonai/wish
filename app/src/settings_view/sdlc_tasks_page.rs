//! Settings → SDLC Tasks page.
//!
//! Renders the live contents of [`AgentTaskRegistryModel`] as two
//! sections — **Running** and **Completed** — with one chip per
//! task. Mirrors the Tasks-panel surface in the Claude-Code-style
//! UX from the project goals.
//!
//! # Why a settings page (for now)
//!
//! Long-term this view should be a workspace-level panel docked to
//! the right edge of the window, identical to the Code Review pane.
//! That requires wiring into the workspace layout (`render_panels`,
//! `vertical_tabs`, etc.) — significant infrastructure for a single
//! feature. Until that's worth the cost, the same UI lives in
//! Settings → SDLC Tasks, which:
//!
//! - Uses the existing settings page infrastructure (no new layout)
//! - Is immediately visible to users
//! - Validates the `AgentTaskRegistryModel` contract end-to-end
//! - Can be promoted to a real workspace panel later (the render
//!   logic moves verbatim; only the surrounding chrome changes)
//!
//! # Surfaces rendered
//!
//! - **Header**: title, subtitle, "Clear completed" button
//! - **Status row**: live counts ("3 running · 12 completed")
//! - **Demo button** (`Add demo tasks`): for visual verification
//!   while the runtime adapters aren't yet wired. Will be removed
//!   once the Hermon + local-LLM adapters land.
//! - **Running section**: chips for active tasks
//! - **Completed section**: chips for terminal tasks (newest first)
//!
//! # Chip layout (one per task)
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │ <title>                       [Tool badge]    [✕]   │
//! │ <last annotation one_liner>                         │
//! │ <duration> · <status>                                │
//! └──────────────────────────────────────────────────────┘
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use wishui::{
    elements::{
        Container, CrossAxisAlignment, Element, Flex, MainAxisAlignment, MouseStateHandle,
        ParentElement,
    },
    scene::{CornerRadius, Radius},
    ui_components::{
        button::ButtonVariant,
        components::{UiComponent, UiComponentStyles},
    },
    AppContext, Entity, SingletonEntity, View, ViewContext, ViewHandle,
};

use crate::ai::agent_tasks::{
    AgentTask, AgentTaskEvent, AgentTaskRegistryModel, TaskAnnotation, TaskId, TaskStatus,
};
use crate::appearance::Appearance;
use crate::workspace::WorkspaceAction;

use super::{
    settings_page::{
        MatchData, PageType, SettingsPageEvent, SettingsPageMeta, SettingsPageViewHandle,
        SettingsWidget,
    },
    SettingsSection,
};

// ── View ───────────────────────────────────────────────────────────────

/// Settings → "SDLC Tasks" page view. Subscribes to
/// [`AgentTaskEvent`] and re-renders on any change.
pub struct SdlcTasksPageView {
    page: PageType<Self>,
}

impl SdlcTasksPageView {
    pub fn new(ctx: &mut ViewContext<SdlcTasksPageView>) -> Self {
        let registry = AgentTaskRegistryModel::handle(ctx);
        ctx.subscribe_to_model(&registry, |_view, _, _event: &AgentTaskEvent, ctx| {
            // Coarse subscription: any registry change re-renders.
            // The registry's events are already granular; the view
            // doesn't need finer filtering for this MVP.
            ctx.notify();
        });

        Self {
            page: PageType::new_monolith(SdlcTasksPageWidget::default(), None, false),
        }
    }
}

impl Entity for SdlcTasksPageView {
    type Event = SettingsPageEvent;
}

impl View for SdlcTasksPageView {
    fn ui_name() -> &'static str {
        "SdlcTasksPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for SdlcTasksPageView {
    fn section() -> SettingsSection {
        SettingsSection::SdlcTasks
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<SdlcTasksPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<SdlcTasksPageView>) -> Self {
        SettingsPageViewHandle::SdlcTasks(view_handle)
    }
}

// ── Widget ────────────────────────────────────────────────────────────

/// Per-task mouse states. Lazy-populated as tasks appear.
#[derive(Default)]
struct ChipMouseStates {
    /// State for the "✕" dismiss button on this chip.
    dismiss: MouseStateHandle,
}

#[derive(Default)]
struct SdlcTasksPageWidget {
    /// Mouse state for the "Clear completed" header button.
    clear_completed_state: MouseStateHandle,
    /// Mouse state for the "Add demo tasks" button (debug helper).
    demo_button_state: MouseStateHandle,
    /// Per-task dismiss-button mouse states. RefCell because
    /// `SettingsWidget::render` takes `&self` but we need to
    /// lazy-insert as tasks appear.
    chip_states: RefCell<HashMap<TaskId, ChipMouseStates>>,
}

impl SdlcTasksPageWidget {
    /// Get (or lazily create) the mouse-state bundle for a task chip.
    fn chip_states_for(&self, id: &TaskId) -> ChipMouseStates {
        let mut map = self.chip_states.borrow_mut();
        let entry = map.entry(id.clone()).or_default();
        ChipMouseStates {
            dismiss: entry.dismiss.clone(),
        }
    }
}

impl SettingsWidget for SdlcTasksPageWidget {
    type View = SdlcTasksPageView;

    fn search_terms(&self) -> &str {
        "sdlc tasks agents running completed bash edit read tool background"
    }

    fn render(
        &self,
        _view: &SdlcTasksPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let registry = AgentTaskRegistryModel::handle(app);
        let registry_ref = registry.as_ref(app);
        let active = registry_ref.active_tasks();
        let completed = registry_ref.completed_tasks();
        let total = active.len() + completed.len();

        let ui_builder = appearance.ui_builder();

        // ── Header (title + Clear completed button) ──────────────
        let title = ui_builder
            .paragraph("SDLC Tasks")
            .with_style(UiComponentStyles {
                font_size: Some(24.),
                ..Default::default()
            })
            .build()
            .finish();

        let subtitle = ui_builder
            .paragraph(
                "Live view of every tool the agents are running. Each chip is one tool \
                 invocation — file edit, shell command, search, sub-agent, etc.",
            )
            .with_style(UiComponentStyles {
                font_size: Some(13.),
                ..Default::default()
            })
            .build()
            .with_margin_top(4.)
            .with_margin_bottom(16.)
            .finish();

        // "Clear completed" — disabled when there's nothing to clear.
        let mut clear_button = ui_builder
            .button(ButtonVariant::Secondary, self.clear_completed_state.clone())
            .with_text_label(format!("Clear completed ({})", completed.len()))
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            });
        if completed.is_empty() {
            clear_button = clear_button.disabled();
        }
        let clear_button = clear_button
            .build()
            .on_click(|ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceAction::ClearCompletedTasks);
            })
            .finish();

        let header_row = Flex::row()
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_child(title)
                    .with_child(subtitle)
                    .finish(),
            )
            .with_child(clear_button)
            .finish();

        // ── Status row ───────────────────────────────────────────
        let status_text = format_status_line(active.len(), completed.len());
        let status_widget = ui_builder
            .span(status_text)
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            })
            .build()
            .with_margin_bottom(12.)
            .finish();

        // ── Demo helper button (will be removed when adapters land) ─
        let demo_button = ui_builder
            .button(ButtonVariant::Text, self.demo_button_state.clone())
            .with_text_label("+ Add demo tasks (debug)".to_string())
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                ..Default::default()
            })
            .build()
            .on_click(|ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceAction::CreateDemoTasks);
            })
            .finish();
        let demo_row = Container::new(demo_button).with_margin_bottom(20.).finish();

        // ── Build the column ─────────────────────────────────────
        let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
        column = column.with_child(header_row);
        column = column.with_child(status_widget);
        column = column.with_child(demo_row);

        // True empty state — no tasks at all.
        if total == 0 {
            let empty = ui_builder
                .paragraph(
                    "No agent tasks yet. They'll appear here as soon as a built-in or \
                     custom agent invokes a tool.",
                )
                .build()
                .with_margin_top(16.)
                .finish();
            column = column.with_child(empty);
        } else {
            // Running section
            if !active.is_empty() {
                column =
                    column.with_child(render_section_header(appearance, "Running", active.len()));
                for task in &active {
                    let chip_states = self.chip_states_for(&task.id);
                    column = column.with_child(render_task_chip(task, appearance, &chip_states));
                }
            }
            // Completed section
            if !completed.is_empty() {
                column = column.with_child(render_section_header(
                    appearance,
                    "Completed",
                    completed.len(),
                ));
                for task in &completed {
                    let chip_states = self.chip_states_for(&task.id);
                    column = column.with_child(render_task_chip(task, appearance, &chip_states));
                }
            }
        }

        Container::new(column.finish())
            .with_padding_left(24.)
            .with_padding_right(24.)
            .with_padding_top(24.)
            .with_padding_bottom(24.)
            .finish()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Format the live-counts status line. Singular/plural-aware.
fn format_status_line(active: usize, completed: usize) -> String {
    let active_str = match active {
        1 => "1 running".to_string(),
        n => format!("{n} running"),
    };
    let completed_str = match completed {
        1 => "1 completed".to_string(),
        n => format!("{n} completed"),
    };
    format!("{active_str} · {completed_str}")
}

fn render_section_header(appearance: &Appearance, label: &str, count: usize) -> Box<dyn Element> {
    let ui_builder = appearance.ui_builder();
    ui_builder
        .span(format!("{label} ({count})"))
        .with_style(UiComponentStyles {
            font_size: Some(11.),
            ..Default::default()
        })
        .build()
        .with_margin_top(8.)
        .with_margin_bottom(6.)
        .finish()
}

/// Render a single task chip.
fn render_task_chip(
    task: &AgentTask,
    appearance: &Appearance,
    states: &ChipMouseStates,
) -> Box<dyn Element> {
    let ui_builder = appearance.ui_builder();
    let theme = appearance.theme();

    // Title row: title + tool badge + dismiss button
    let title_widget = ui_builder
        .span(task.title.clone())
        .with_style(UiComponentStyles {
            font_size: Some(14.),
            ..Default::default()
        })
        .build()
        .finish();

    let badge = render_tool_badge(appearance, &task.tool.badge_label());

    let dismiss_label = "✕".to_string();
    let task_id_for_click = task.id.clone();
    let dismiss_button = ui_builder
        .button(ButtonVariant::Text, states.dismiss.clone())
        .with_text_label(dismiss_label)
        .with_style(UiComponentStyles {
            font_size: Some(11.),
            ..Default::default()
        })
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::DismissTask {
                task_id: task_id_for_click.0.clone(),
            });
        })
        .finish();

    let title_row = Flex::row()
        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Container::new(title_widget).with_padding_right(8.).finish())
                .with_child(badge)
                .finish(),
        )
        .with_child(dismiss_button)
        .finish();

    // Last annotation one-liner (if any) — the rolling progress
    // indicator the user sees in Claude Code's "Edited X +7 -1".
    let last_annotation: Box<dyn Element> = match task.annotations.last() {
        Some(a) => ui_builder
            .span(a.one_liner())
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            })
            .build()
            .with_margin_top(2.)
            .finish(),
        None => Container::new(Flex::row().finish()).finish(),
    };

    // Status footer line
    let status_line = format_chip_status(task);
    let status_widget = ui_builder
        .span(status_line)
        .with_style(UiComponentStyles {
            font_size: Some(11.),
            ..Default::default()
        })
        .build()
        .with_margin_top(4.)
        .finish();

    let body = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Start)
        .with_child(title_row)
        .with_child(last_annotation)
        .with_child(status_widget)
        .finish();

    Container::new(body)
        .with_padding_top(12.)
        .with_padding_bottom(12.)
        .with_padding_left(16.)
        .with_padding_right(16.)
        .with_margin_bottom(8.)
        .with_background(theme.surface_2())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
        .finish()
}

fn render_tool_badge(appearance: &Appearance, label: &str) -> Box<dyn Element> {
    let ui_builder = appearance.ui_builder();
    let theme = appearance.theme();
    let span = ui_builder
        .span(label.to_string())
        .with_style(UiComponentStyles {
            font_size: Some(10.),
            ..Default::default()
        })
        .build()
        .finish();
    Container::new(span)
        .with_padding_left(6.)
        .with_padding_right(6.)
        .with_padding_top(2.)
        .with_padding_bottom(2.)
        .with_background(theme.surface_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .finish()
}

/// Compose the per-chip status footer line: duration + status.
fn format_chip_status(task: &AgentTask) -> String {
    let dur = humanize_duration(task.duration());
    match &task.status {
        TaskStatus::Pending => format!("Pending · {dur}"),
        TaskStatus::Running => {
            if task.background {
                format!("Running (background) · {dur}")
            } else {
                format!("Running · {dur}")
            }
        }
        TaskStatus::Completed => format!("Completed · {dur}"),
        TaskStatus::Failed { error } => format!("Failed: {error} · {dur}"),
        TaskStatus::Cancelled => format!("Cancelled · {dur}"),
    }
}

/// Human-readable duration: "<1s", "5s", "2m", "1h 5m".
///
/// Pure function, exposed for testing. Same intent as
/// `format_relative_time` in the Built-in Agents page but
/// formatted differently because here we're showing *elapsed*
/// time on a chip rather than "X ago" relative timestamps.
fn humanize_duration(d: std::time::Duration) -> String {
    let total_secs = d.as_secs();
    if total_secs < 1 {
        "<1s".to_string()
    } else if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let m = total_secs / 60;
        let s = total_secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        }
    } else {
        let h = total_secs / 3600;
        let m = (total_secs % 3600) / 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

// ── Demo task fixture ─────────────────────────────────────────────────

/// Construct a small set of demo tasks for visual verification.
/// Called by [`WorkspaceAction::CreateDemoTasks`].
///
/// Exposed at module scope so the workspace handler can call it
/// without needing to know any of the page's internals.
pub fn populate_demo_tasks(
    registry: &mut AgentTaskRegistryModel,
    ctx: &mut wishui::ModelContext<AgentTaskRegistryModel>,
) {
    use crate::ai::agent_tasks::ToolKind;

    // Two completed, two running, one failed. Mixed tool kinds so
    // every badge variant is represented.
    let id_test = registry.create("Run all tests", ToolKind::Bash, false, ctx);
    registry.set_status(&id_test, TaskStatus::Running, ctx);
    registry.add_annotation(
        &id_test,
        TaskAnnotation::CommandRun {
            description: "cargo test -p wish --lib".into(),
            exit_code: Some(0),
        },
        ctx,
    );
    registry.set_status(&id_test, TaskStatus::Completed, ctx);

    let id_edit = registry.create("Update local_llm.rs", ToolKind::Edit, false, ctx);
    registry.set_status(&id_edit, TaskStatus::Running, ctx);
    registry.add_annotation(
        &id_edit,
        TaskAnnotation::FileEdit {
            path: "app/src/ai/local_llm.rs".into(),
            additions: 7,
            deletions: 1,
        },
        ctx,
    );
    registry.set_status(&id_edit, TaskStatus::Completed, ctx);

    let id_search = registry.create("Find TODO markers", ToolKind::Search, false, ctx);
    registry.set_status(&id_search, TaskStatus::Running, ctx);
    registry.add_annotation(
        &id_search,
        TaskAnnotation::Search {
            query: "TODO".into(),
            match_count: 17,
        },
        ctx,
    );

    let id_dev = registry.create("npm run dev", ToolKind::Bash, true, ctx);
    registry.set_status(&id_dev, TaskStatus::Running, ctx);
    registry.add_annotation(
        &id_dev,
        TaskAnnotation::Note {
            text: "Listening on http://localhost:3000".into(),
        },
        ctx,
    );

    let id_failed = registry.create("Lint check", ToolKind::Bash, false, ctx);
    registry.set_status(&id_failed, TaskStatus::Running, ctx);
    registry.add_annotation(
        &id_failed,
        TaskAnnotation::CommandRun {
            description: "cargo clippy".into(),
            exit_code: Some(101),
        },
        ctx,
    );
    registry.set_status(
        &id_failed,
        TaskStatus::Failed {
            error: "clippy reported 3 warnings".into(),
        },
        ctx,
    );
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn status_line_handles_singular_and_plural() {
        assert_eq!(format_status_line(0, 0), "0 running · 0 completed");
        assert_eq!(format_status_line(1, 0), "1 running · 0 completed");
        assert_eq!(format_status_line(1, 1), "1 running · 1 completed");
        assert_eq!(format_status_line(5, 12), "5 running · 12 completed");
    }

    #[test]
    fn humanize_duration_under_one_second() {
        assert_eq!(humanize_duration(Duration::from_millis(0)), "<1s");
        assert_eq!(humanize_duration(Duration::from_millis(999)), "<1s");
    }

    #[test]
    fn humanize_duration_seconds() {
        assert_eq!(humanize_duration(Duration::from_secs(1)), "1s");
        assert_eq!(humanize_duration(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn humanize_duration_minutes() {
        assert_eq!(humanize_duration(Duration::from_secs(60)), "1m");
        assert_eq!(humanize_duration(Duration::from_secs(125)), "2m 5s");
        assert_eq!(humanize_duration(Duration::from_secs(3599)), "59m 59s");
    }

    #[test]
    fn humanize_duration_hours() {
        assert_eq!(humanize_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(humanize_duration(Duration::from_secs(3900)), "1h 5m");
        assert_eq!(humanize_duration(Duration::from_secs(7260)), "2h 1m");
    }

    #[test]
    fn format_chip_status_running_includes_duration() {
        let mut task = AgentTask {
            id: TaskId::new("a"),
            title: "x".into(),
            tool: crate::ai::agent_tasks::ToolKind::Bash,
            status: TaskStatus::Running,
            started_at: std::time::Instant::now(),
            completed_at: None,
            annotations: vec![],
            background: false,
            metadata: Default::default(),
        };
        let s = format_chip_status(&task);
        assert!(s.starts_with("Running · "));

        task.background = true;
        let s2 = format_chip_status(&task);
        assert!(s2.starts_with("Running (background) · "));
    }

    #[test]
    fn format_chip_status_failed_includes_error() {
        let task = AgentTask {
            id: TaskId::new("a"),
            title: "x".into(),
            tool: crate::ai::agent_tasks::ToolKind::Bash,
            status: TaskStatus::Failed {
                error: "boom".into(),
            },
            started_at: std::time::Instant::now(),
            completed_at: Some(std::time::Instant::now()),
            annotations: vec![],
            background: false,
            metadata: Default::default(),
        };
        let s = format_chip_status(&task);
        assert!(s.starts_with("Failed: boom · "));
    }

    #[test]
    fn format_chip_status_pending() {
        let task = AgentTask {
            id: TaskId::new("a"),
            title: "x".into(),
            tool: crate::ai::agent_tasks::ToolKind::Bash,
            status: TaskStatus::Pending,
            started_at: std::time::Instant::now(),
            completed_at: None,
            annotations: vec![],
            background: false,
            metadata: Default::default(),
        };
        let s = format_chip_status(&task);
        assert!(s.starts_with("Pending · "));
    }
}
