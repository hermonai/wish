//! Settings page that lists every agent currently in the
//! [`AgentRegistryModel`] — both Hermon-backed agents and the built-in
//! SDLC agents shipped with Wish.
//!
//! # Scope of this page (read-only)
//!
//! For now, this page is purely a **catalog**:
//!
//! - Renders one card per agent showing name, slug, type, source, model,
//!   description, tools, and capabilities.
//! - Shows the registry's current refresh status.
//! - Has a "Refresh" button that calls
//!   [`AgentRegistryModel::refresh`].
//!
//! It does **not** yet support: editing, deleting, creating, cloning, or
//! starting a conversation. Those flows have their own design challenges
//! (modal flows, permission checks, conversation routing) and are
//! tackled in follow-up turns.
//!
//! # Reactivity
//!
//! The page subscribes to [`AgentRegistryEvent`] and re-renders whenever
//! the underlying list or status changes. The subscription is set up in
//! [`BuiltInAgentsPageView::new`] and lives for the lifetime of the
//! page view.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use wishui::{
    elements::{
        Container, CrossAxisAlignment, Element, Flex, Hoverable, MainAxisAlignment,
        MouseStateHandle, ParentElement,
    },
    platform::Cursor,
    scene::{CornerRadius, Radius},
    ui_components::{
        button::ButtonVariant,
        components::{UiComponent, UiComponentStyles},
    },
    AppContext, Entity, SingletonEntity, View, ViewContext, ViewHandle,
};

use crate::ai::agent_registry::{
    AgentRegistryEvent, AgentRegistryModel, AgentSource, BuiltInAgentsUiState,
    BuiltInAgentsUiStateEvent, RegistryEntry, RegistryStatus,
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

/// Settings → "Built-in Agents" page view.
///
/// A thin wrapper around [`BuiltInAgentsPageWidget`] that owns the
/// settings infrastructure boilerplate (search filtering, page chrome).
pub struct BuiltInAgentsPageView {
    page: PageType<Self>,
}

impl BuiltInAgentsPageView {
    pub fn new(ctx: &mut ViewContext<BuiltInAgentsPageView>) -> Self {
        // Re-render whenever the registry changes. The list and the
        // status both come from the same model, so a single
        // subscription covers both.
        let registry = AgentRegistryModel::handle(ctx);
        ctx.subscribe_to_model(&registry, |_view, _, _event: &AgentRegistryEvent, ctx| {
            ctx.notify();
        });

        // Re-render whenever the user expands/collapses a card.
        // The UI state lives in its own singleton (separate from the
        // registry) so the registry's API isn't polluted with
        // presentation events.
        let ui_state = BuiltInAgentsUiState::handle(ctx);
        ctx.subscribe_to_model(
            &ui_state,
            |_view, _, _event: &BuiltInAgentsUiStateEvent, ctx| {
                ctx.notify();
            },
        );

        BuiltInAgentsPageView {
            page: PageType::new_monolith(BuiltInAgentsPageWidget::default(), None, false),
        }
    }
}

impl Entity for BuiltInAgentsPageView {
    type Event = SettingsPageEvent;
}

impl View for BuiltInAgentsPageView {
    fn ui_name() -> &'static str {
        "BuiltInAgentsPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for BuiltInAgentsPageView {
    fn section() -> SettingsSection {
        SettingsSection::BuiltInAgents
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

impl From<ViewHandle<BuiltInAgentsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<BuiltInAgentsPageView>) -> Self {
        SettingsPageViewHandle::BuiltInAgents(view_handle)
    }
}

// ── Widget (the actual rendered content) ──────────────────────────────

/// Per-card mouse state handles.
///
/// Each card on the page has two interactive elements that need their
/// own [`MouseStateHandle`]: the card body (clickable to toggle
/// expansion) and the "Copy slug" sub-button. Bundling them in a struct
/// keeps the per-card lookup map's value type explicit and makes
/// future additions (e.g., an "Edit" button) a trivial change.
#[derive(Default)]
struct CardMouseStates {
    /// Mouse state for the card-body click target (toggle expansion).
    body: MouseStateHandle,
    /// Mouse state for the small "Copy slug" button at the card's edge.
    copy_slug: MouseStateHandle,
}

/// The body of the page. Pulls live data from [`AgentRegistryModel`] on
/// every render — the registry is the source of truth for *what* agents
/// exist; the [`BuiltInAgentsUiState`] singleton is the source of truth
/// for *which card is expanded*. The widget itself only owns
/// hover/click tracking handles.
#[derive(Default)]
struct BuiltInAgentsPageWidget {
    /// Mouse state for the "Refresh" button. Persists across renders so
    /// hover/click tracking works correctly (per `WISH.md`'s guidance
    /// against creating these inline).
    refresh_button_mouse_state: MouseStateHandle,
    /// Per-agent (keyed by slug) mouse-state handles for the card
    /// body and Copy-slug sub-button. Lazy-populated on first render of
    /// each card, then reused for the lifetime of the page so hover/click
    /// tracking is consistent across renders.
    ///
    /// Wrapped in a [`RefCell`] because [`SettingsWidget::render`] takes
    /// `&self`, but we need to insert new entries when an agent first
    /// appears. The borrow is single-threaded (UI rendering is
    /// single-threaded) and lives only for the duration of one
    /// `render` call, so deadlock is not possible.
    card_mouse_states: RefCell<HashMap<String, CardMouseStates>>,
}

impl BuiltInAgentsPageWidget {
    /// Get (or lazily create) the mouse-state handles for the card with
    /// the given slug.
    ///
    /// Clones the handles before returning — `MouseStateHandle` is a
    /// cheap clonable handle (Arc-backed internally), so this is O(1)
    /// and doesn't break hover tracking.
    fn card_mouse_states_for(&self, slug: &str) -> CardMouseStates {
        let mut map = self.card_mouse_states.borrow_mut();
        let entry = map.entry(slug.to_string()).or_default();
        CardMouseStates {
            body: entry.body.clone(),
            copy_slug: entry.copy_slug.clone(),
        }
    }
}

impl SettingsWidget for BuiltInAgentsPageWidget {
    type View = BuiltInAgentsPageView;

    fn search_terms(&self) -> &str {
        // Searched against when the user types in the settings search
        // box. We deliberately keep this list short and focused on the
        // page topic — agent names themselves aren't indexed here
        // (each agent renders its own searchable text in the body).
        "agents builtin sdlc planner coder reviewer tester debugger \
         deployer documenter refactorer security orchestrator hermon"
    }

    fn render(
        &self,
        _view: &BuiltInAgentsPageView,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let registry = AgentRegistryModel::handle(app);
        let registry_ref = registry.as_ref(app);

        let entries = registry_ref.entries();
        let status = registry_ref.status();

        let ui_builder = appearance.ui_builder();

        // ── Header: title + refresh button ────────────────────────
        let title = ui_builder
            .paragraph("Built-in Agents")
            .with_style(UiComponentStyles {
                font_size: Some(24.),
                ..Default::default()
            })
            .build()
            .finish();

        // Help text under the title. We construct this inside the column
        // builder rather than as a free-standing variable because
        // `Box<dyn Element>` doesn't implement `Clone` — every element must
        // have exactly one parent.
        let subtitle_text =
            "All AI agents available to invoke from Wish. Built-in SDLC \
             agents are shipped with the app; agents fetched from your \
             Hermon backend (if configured) appear above them.";

        // The refresh button is disabled while a refresh is in flight.
        // The label also flips to "Refreshing…" so the visual state
        // matches the disabled state and gives feedback that the click
        // was registered. We coalesce duplicate clicks at the model
        // layer (`AgentRegistryModel::refresh`), so this is purely a
        // UX nicety, not a correctness requirement.
        let mut refresh_button_builder = ui_builder
            .button(
                ButtonVariant::Secondary,
                self.refresh_button_mouse_state.clone(),
            )
            .with_text_label(if status.is_refreshing() {
                "Refreshing…".to_string()
            } else {
                "Refresh".to_string()
            });
        if status.is_refreshing() {
            refresh_button_builder = refresh_button_builder.disabled();
        }
        let refresh_button = refresh_button_builder
            .build()
            .on_click(|ctx, _, _| {
                // The click closure receives an `EventContext`, which
                // doesn't implement the model-update traits we need to
                // call `refresh()` directly. Dispatch through the
                // workspace action system instead — `Workspace::view`
                // handles `RefreshAgentRegistry` by calling the
                // registry under a proper `ViewContext`.
                ctx.dispatch_typed_action(WorkspaceAction::RefreshAgentRegistry);
            })
            .finish();

        let subtitle_widget = ui_builder
            .paragraph(subtitle_text)
            .with_style(UiComponentStyles {
                font_size: Some(13.),
                ..Default::default()
            })
            .build()
            .with_margin_top(4.)
            .with_margin_bottom(16.)
            .finish();

        let header_row = Flex::row()
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_child(title)
                    .with_child(subtitle_widget)
                    .finish(),
            )
            .with_child(refresh_button)
            .finish();

        // ── Status row ────────────────────────────────────────────
        let status_text = format_status(status, entries.len());
        let status_widget = ui_builder
            .span(status_text)
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            })
            .build()
            .with_margin_bottom(20.)
            .finish();

        // ── Empty state ───────────────────────────────────────────
        // The registry is *always* seeded with built-ins, so a truly
        // empty list should never happen. We handle it defensively
        // for robustness in case future code paths drain the
        // registry.
        if entries.is_empty() {
            let empty_msg = ui_builder
                .paragraph("No agents available.")
                .build()
                .finish();
            return Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(header_row)
                .with_child(status_widget)
                .with_child(empty_msg)
                .finish();
        }

        // ── Agent cards ───────────────────────────────────────────
        let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        column = column.with_child(header_row);
        column = column.with_child(status_widget);

        // Read the expansion state once up-front so each card can
        // check `is_expanded` without re-fetching from the singleton.
        let ui_state_handle = BuiltInAgentsUiState::handle(app);
        let ui_state = ui_state_handle.as_ref(app);

        for entry in entries {
            let mouse_states = self.card_mouse_states_for(&entry.agent.slug);
            let is_expanded = ui_state.is_expanded(&entry.agent.slug);
            column = column.with_child(render_agent_card(
                entry,
                appearance,
                &mouse_states,
                is_expanded,
            ));
        }

        // Wrap in a container with horizontal padding so the cards
        // don't touch the edge of the settings pane.
        Container::new(column.finish())
            .with_padding_left(24.)
            .with_padding_right(24.)
            .with_padding_top(24.)
            .with_padding_bottom(24.)
            .finish()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Format the registry status into a single human-readable line.
fn format_status(status: &RegistryStatus, count: usize) -> String {
    match status {
        RegistryStatus::Idle => format!("{count} agents · awaiting first refresh"),
        RegistryStatus::Refreshing => format!("{count} agents · refreshing…"),
        RegistryStatus::Loaded { at } => {
            format!("{count} agents · loaded {}", format_relative_time(at))
        }
        RegistryStatus::Failed { error, at } => {
            format!(
                "{count} agents · last refresh failed {} ({})",
                format_relative_time(at),
                error
            )
        }
    }
}

/// Render `Instant` as a coarse relative time ("just now", "5s ago",
/// "2m ago"). Avoids pulling in a heavy datetime library — registry
/// timestamps are best displayed in approximate buckets anyway.
fn format_relative_time(t: &Instant) -> String {
    let elapsed = t.elapsed().as_secs();
    if elapsed < 2 {
        "just now".to_string()
    } else if elapsed < 60 {
        format!("{elapsed}s ago")
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else {
        format!("{}h ago", elapsed / 3600)
    }
}

/// Render a single registry entry as a card.
///
/// Each card shows: title row (name + badges + Copy slug button),
/// slug + model, description, tools list, capabilities list. When
/// `is_expanded` is true, a details panel is appended below showing the
/// system prompt, instructions, parameters, and metadata.
///
/// Clicking anywhere on the card body toggles its expansion. The
/// "Copy slug" button stops short of the body's click handler and
/// only copies the slug.
///
/// Stays within ~6 visible lines per collapsed agent so the list
/// scrolls comfortably even with 30+ entries.
fn render_agent_card(
    entry: &RegistryEntry,
    appearance: &Appearance,
    mouse_states: &CardMouseStates,
    is_expanded: bool,
) -> Box<dyn Element> {
    let ui_builder = appearance.ui_builder();
    let theme = appearance.theme();

    // ── Title row: name + type badge + source badge + Copy slug ──
    let name_widget = ui_builder
        .paragraph(entry.agent.name.clone())
        .with_style(UiComponentStyles {
            font_size: Some(15.),
            ..Default::default()
        })
        .build()
        .finish();

    let type_badge = render_badge(
        appearance,
        format_agent_type(&entry.agent.agent_type),
        BadgeKind::Type,
    );

    let source_badge = render_badge(
        appearance,
        match entry.source {
            AgentSource::Hermon => "Hermon".to_string(),
            AgentSource::BuiltIn => "Built-in".to_string(),
        },
        match entry.source {
            AgentSource::Hermon => BadgeKind::Hermon,
            AgentSource::BuiltIn => BadgeKind::BuiltIn,
        },
    );

    // "Copy slug" button — kept small to fit visually on the same line
    // as the title row. Each `Button` has its own `MouseStateHandle`
    // and its own `on_click`, so even though it sits inside the
    // hover-clickable card, the button's click handler fires first and
    // the underlying card-body click does not bubble into a toggle.
    let slug_for_copy = entry.agent.slug.clone();
    let copy_slug_button = ui_builder
        .button(ButtonVariant::Text, mouse_states.copy_slug.clone())
        .with_text_label("Copy slug".to_string())
        .with_style(UiComponentStyles {
            font_size: Some(11.),
            ..Default::default()
        })
        .build()
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(WorkspaceAction::CopyAgentSlug {
                slug: slug_for_copy.clone(),
            });
        })
        .finish();

    let title_row = Flex::row()
        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_child(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Container::new(name_widget).with_padding_right(8.).finish())
                .with_child(Container::new(type_badge).with_padding_right(4.).finish())
                .with_child(source_badge)
                .finish(),
        )
        .with_child(copy_slug_button)
        .finish();

    // ── Slug + model line ────────────────────────────────────────
    let model_str = format!("{}/{}", entry.agent.model.provider_id, entry.agent.model.model_id);
    let slug_model_line = ui_builder
        .span(format!("{} · {}", entry.agent.slug, model_str))
        .with_style(UiComponentStyles {
            font_size: Some(12.),
            ..Default::default()
        })
        .build()
        .with_margin_top(2.)
        .finish();

    // ── Description (optional) ───────────────────────────────────
    // `Paragraph` (returned by `ui_builder.paragraph(...)`) already
    // soft-wraps long text by default — that's what differentiates it
    // from `Span`. So no explicit `.with_soft_wrap()` call is needed.
    let description_widget: Box<dyn Element> = match entry.agent.description.as_ref() {
        Some(desc) => ui_builder
            .paragraph(desc.clone())
            .with_style(UiComponentStyles {
                font_size: Some(13.),
                ..Default::default()
            })
            .build()
            .with_margin_top(8.)
            .finish(),
        None => Container::new(Flex::row().finish()).finish(),
    };

    // ── Tools summary ────────────────────────────────────────────
    let tools_line: Box<dyn Element> = if entry.agent.tools.is_empty() {
        Container::new(Flex::row().finish()).finish()
    } else {
        let preview: Vec<String> = entry
            .agent
            .tools
            .iter()
            .take(6)
            .map(|t| t.tool_id.clone())
            .collect();
        let suffix = if entry.agent.tools.len() > preview.len() {
            format!(" +{} more", entry.agent.tools.len() - preview.len())
        } else {
            String::new()
        };
        ui_builder
            .span(format!("Tools: {}{}", preview.join(" · "), suffix))
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            })
            .build()
            .with_margin_top(6.)
            .finish()
    };

    // ── Capabilities summary ─────────────────────────────────────
    let capabilities_line: Box<dyn Element> = if entry.agent.capabilities.is_empty() {
        Container::new(Flex::row().finish()).finish()
    } else {
        ui_builder
            .span(format!(
                "Capabilities: {}",
                entry.agent.capabilities.join(" · ")
            ))
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            })
            .build()
            .with_margin_top(2.)
            .finish()
    };

    // ── Optional details panel (when expanded) ──────────────────
    // Built lazily and only when the card is open, so collapsed cards
    // pay zero render cost for content the user isn't looking at.
    let details_panel: Box<dyn Element> = if is_expanded {
        render_details_panel(entry, appearance)
    } else {
        // Empty placeholder. We could `Option::None`-return and have
        // the caller skip adding it, but a zero-size element keeps
        // the column-builder code uniform.
        Container::new(Flex::row().finish()).finish()
    };

    // ── Card body (assembled column) ─────────────────────────────
    let card_body = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Start)
        .with_child(title_row)
        .with_child(slug_model_line)
        .with_child(description_widget)
        .with_child(tools_line)
        .with_child(capabilities_line)
        .with_child(details_panel)
        .finish();

    let card_container = Container::new(card_body)
        .with_padding_top(12.)
        .with_padding_bottom(12.)
        .with_padding_left(16.)
        .with_padding_right(16.)
        // `surface_2` is the elevated panel color in WarpTheme — it sits
        // visually above the page background, giving the card a subtle
        // separation without a hard border. Returns `Fill`, so use
        // `with_background` (the `Fill`-typed setter) rather than
        // `with_background_color` (which expects a raw `ColorU`).
        .with_background(theme.surface_2())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
        .finish();

    // ── Wrap in Hoverable so the entire card body is clickable ───
    //
    // Clicking the card toggles its expansion. The dispatched action
    // is handled by `Workspace::view`, which updates the
    // `BuiltInAgentsUiState` singleton; this view is subscribed to
    // that singleton and re-renders.
    //
    // The Hoverable's `FnOnce(&MouseState) -> Box<dyn Element>`
    // closure consumes the pre-built `card_container` and returns it
    // — we don't currently use hover state to vary the appearance, but
    // the parameter is wired through for future hover styling (e.g.,
    // a subtle background tint on hover).
    let slug_for_toggle = entry.agent.slug.clone();
    Container::new(
        Hoverable::new(mouse_states.body.clone(), move |_state| card_container)
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(WorkspaceAction::ToggleAgentDetails {
                    slug: slug_for_toggle.clone(),
                });
            })
            .finish(),
    )
    .with_margin_bottom(8.)
    .finish()
}

/// Render the details panel shown below the card summary when the card
/// is expanded.
///
/// Shows whatever rich agent metadata is set: system prompt,
/// instructions, parameters, metadata, and (for Hermon-sourced agents)
/// timestamps. Sections with no data are omitted entirely.
fn render_details_panel(entry: &RegistryEntry, appearance: &Appearance) -> Box<dyn Element> {
    let ui_builder = appearance.ui_builder();

    let mut sections = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);

    // Helper to add a labeled section. We use a closure-like inline
    // builder rather than a function because each call mutates the
    // outer `sections` builder.
    fn build_section(
        appearance: &Appearance,
        label: &str,
        body: String,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();
        let theme = appearance.theme();
        let label_widget = ui_builder
            .span(label.to_string())
            .with_style(UiComponentStyles {
                font_size: Some(11.),
                ..Default::default()
            })
            .build()
            .with_margin_top(8.)
            .with_margin_bottom(4.)
            .finish();
        let body_widget = ui_builder
            .paragraph(body)
            .with_style(UiComponentStyles {
                font_size: Some(12.),
                ..Default::default()
            })
            .build()
            .finish();
        let body_container = Container::new(body_widget)
            .with_padding_top(8.)
            .with_padding_bottom(8.)
            .with_padding_left(10.)
            .with_padding_right(10.)
            // `surface_1` reads as a "code block" — slightly different
            // from the card's `surface_2` so the indented-quote feel
            // is preserved without a hard border.
            .with_background(theme.surface_1())
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(6.)))
            .finish();
        Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(label_widget)
            .with_child(body_container)
            .finish()
    }

    if let Some(prompt) = entry.agent.system_prompt.as_ref() {
        sections =
            sections.with_child(build_section(appearance, "System prompt", prompt.clone()));
    }

    if let Some(instructions) = entry.agent.instructions.as_ref() {
        sections =
            sections.with_child(build_section(appearance, "Instructions", instructions.clone()));
    }

    if let Some(params) = entry.agent.parameters.as_ref() {
        // Pretty-print the parameters as JSON. `serde_json` is already
        // a dependency via hermon_client; this incurs no extra cost.
        let json = serde_json::to_string_pretty(params)
            .unwrap_or_else(|_| "(unable to format)".to_string());
        sections = sections.with_child(build_section(appearance, "Parameters", json));
    }

    if let Some(metadata) = entry.agent.metadata.as_ref() {
        if !metadata.is_empty() {
            let json = serde_json::to_string_pretty(metadata)
                .unwrap_or_else(|_| "(unable to format)".to_string());
            sections = sections.with_child(build_section(appearance, "Metadata", json));
        }
    }

    // Timestamps — only meaningful for Hermon-sourced agents.
    // Built-ins use the Unix epoch, so showing them would be
    // misleading. See `crate::ai::agent_registry::builtin`.
    if matches!(entry.source, AgentSource::Hermon) {
        let timestamps = format!(
            "Created {} · Updated {}",
            entry.agent.created_at, entry.agent.updated_at
        );
        sections = sections.with_child(
            ui_builder
                .span(timestamps)
                .with_style(UiComponentStyles {
                    font_size: Some(11.),
                    ..Default::default()
                })
                .build()
                .with_margin_top(8.)
                .finish(),
        );
    }

    Container::new(sections.finish())
        .with_margin_top(4.)
        .finish()
}

// ── Badges ────────────────────────────────────────────────────────────

/// Visual category for badge color/style.
#[derive(Debug, Clone, Copy)]
enum BadgeKind {
    Type,
    Hermon,
    BuiltIn,
}

/// Render a small inline badge (e.g., "SDLC", "Built-in").
///
/// Badges use the theme's foreground color with reduced opacity so they
/// remain legible across light/dark themes without hand-tuning.
fn render_badge(
    appearance: &Appearance,
    text: String,
    _kind: BadgeKind,
) -> Box<dyn Element> {
    // For now all badge kinds render the same — uniform pill style.
    // Future: per-kind color accents (e.g., Hermon = accent color).
    let ui_builder = appearance.ui_builder();
    let theme = appearance.theme();

    let label = ui_builder
        .span(text)
        .with_style(UiComponentStyles {
            font_size: Some(10.),
            ..Default::default()
        })
        .build()
        .finish();

    Container::new(label)
        .with_padding_left(6.)
        .with_padding_right(6.)
        .with_padding_top(2.)
        .with_padding_bottom(2.)
        // `surface_1` is the lower-elevation surface — used for the badge
        // pill so it sits *below* the card surface (`surface_2`) and
        // reads as a subtle inline marker, not a button.
        .with_background(theme.surface_1())
        .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
        .finish()
}

/// Convert [`AgentType`] to a short human-readable label.
fn format_agent_type(t: &hermon_client::types::agent::AgentType) -> String {
    use hermon_client::types::agent::AgentType;
    match t {
        AgentType::Chat => "Chat".to_string(),
        AgentType::Coding => "Coding".to_string(),
        AgentType::Orchestrator => "Orchestrator".to_string(),
        AgentType::Worker => "Worker".to_string(),
        AgentType::Sdlc => "SDLC".to_string(),
        AgentType::Custom => "Custom".to_string(),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hermon_client::types::agent::AgentType;

    #[test]
    fn format_status_idle() {
        let s = format_status(&RegistryStatus::Idle, 11);
        assert!(s.contains("11"));
        assert!(s.contains("awaiting"));
    }

    #[test]
    fn format_status_refreshing() {
        let s = format_status(&RegistryStatus::Refreshing, 0);
        assert!(s.contains("refreshing"));
    }

    #[test]
    fn format_status_loaded_includes_count() {
        let s = format_status(
            &RegistryStatus::Loaded { at: Instant::now() },
            42,
        );
        assert!(s.contains("42"));
        assert!(s.contains("loaded"));
    }

    #[test]
    fn format_status_failed_includes_error() {
        let s = format_status(
            &RegistryStatus::Failed {
                error: "connection refused".into(),
                at: Instant::now(),
            },
            5,
        );
        assert!(s.contains("connection refused"));
        assert!(s.contains("failed"));
    }

    #[test]
    fn format_relative_time_just_now() {
        let s = format_relative_time(&Instant::now());
        assert_eq!(s, "just now");
    }

    #[test]
    fn format_agent_type_all_variants() {
        // Sanity-check that every variant has a non-empty label.
        for t in [
            AgentType::Chat,
            AgentType::Coding,
            AgentType::Orchestrator,
            AgentType::Worker,
            AgentType::Sdlc,
            AgentType::Custom,
        ] {
            let label = format_agent_type(&t);
            assert!(!label.is_empty(), "agent type {t:?} should have a label");
        }
    }
}
