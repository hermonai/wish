use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use itertools::Itertools as _;
use wish_core::context_flag::ContextFlag;
use wish_core::ui::appearance::Appearance;
use wish_core::ui::theme::color::internal_colors;
use wishui::assets::asset_cache::AssetSource;
use wishui::elements::{
    Align, CacheOption, ConstrainedBox, Container, CrossAxisAlignment, Flex, Image,
    MainAxisAlignment, MainAxisSize, MouseStateHandle, ParentElement,
};
use wishui::keymap::EditableBinding;
use wishui::platform::{Cursor, FilePickerConfiguration};
use wishui::ui_components::button::ButtonVariant;
use wishui::ui_components::components::{UiComponent, UiComponentStyles};
use wishui::ViewHandle;
use wishui::{
    AppContext, Element, Entity, ModelHandle, SingletonEntity as _, TypedActionView, View,
    ViewContext, WindowId,
};

use crate::code_review::diff_state::GitDeltaPreference;
use crate::code_review::telemetry_event::CodeReviewPaneEntrypoint;
use crate::pane_group::focus_state::PaneFocusHandle;
use crate::pane_group::{
    pane::view, BackingView, NewTerminalOptions, PaneConfiguration, PaneEvent, PanesLayout,
};
use crate::projects::ProjectManagementModel;
use crate::search::binding_source::BindingSource;
use crate::search::welcome_palette::{Event as WelcomePaletteEvent, WelcomePalette};
use crate::util::bindings::{keybinding_name_to_display_string, BindingGroup, CustomAction};
use crate::view_components::DismissibleToast;
use crate::workspace::{ToastStack, Workspace};

pub fn init(app: &mut AppContext) {
    use wishui::keymap::macros::*;

    app.register_editable_bindings([
        EditableBinding::new(
            "workspace:new_tab",
            "Terminal session",
            WelcomeViewAction::CreateTerminalSession,
        )
        .with_context_predicate(id!("WelcomeView"))
        .with_group(BindingGroup::Terminal.as_str())
        .with_custom_action(CustomAction::NewTab)
        .with_enabled(|| ContextFlag::CreateNewSession.is_enabled()),
        EditableBinding::new(
            "welcome_view:open_project",
            "Add repository",
            WelcomeViewAction::OpenProject,
        )
        .with_context_predicate(id!("WelcomeView"))
        .with_group(BindingGroup::Folders.as_str())
        .with_mac_key_binding("cmd-shift-N")
        .with_linux_or_windows_key_binding("alt-n"),
    ]);
}

#[derive(Debug, Clone, Copy)]
pub enum WelcomeViewAction {
    CreateTerminalSession,
    OpenProject,
    /// Dispatched by the "Already have an account? Log in" link in the
    /// welcome page footer. Routes through this view's
    /// [`TypedActionView::handle_action`], which lives under a
    /// `ViewContext` and so can access the [`AuthManager`] singleton
    /// and call [`AppContext::open_url`] — neither of which is
    /// available from the click closure's `EventContext`.
    LogIn,
}

pub struct WelcomeView {
    /// Configure which directory to open sessions into as per the "working directory for new
    /// sessions" setting.
    pub startup_directory: Option<PathBuf>,
    pane_configuration: ModelHandle<PaneConfiguration>,
    focus_handle: Option<PaneFocusHandle>,
    /// Search-style palette retained for keystroke routing (cmd+t etc.) but
    /// hidden from the rendered tree — the new welcome page uses an
    /// IntroSlide-style centered hero design instead. Removing the palette
    /// entirely would break the editable keybindings registered against
    /// `WelcomeViewAction`, which are scoped to its presence.
    palette: ViewHandle<WelcomePalette>,
    /// Mouse state for the primary "Get started" call-to-action. Held on the
    /// view (rather than constructed inline) so hover/click tracking persists
    /// across renders, per `WISH.md`'s guidance.
    get_started_mouse_state: MouseStateHandle,
    /// Mouse state for the small "Log in" link in the footer.
    login_mouse_state: MouseStateHandle,
}

impl WelcomeView {
    pub fn new(startup_directory: Option<PathBuf>, ctx: &mut ViewContext<Self>) -> Self {
        let pane_configuration = ctx.add_model(|_ctx| PaneConfiguration::new("New tab"));
        let window_id = ctx.window_id();
        let view_id = ctx.view_id();
        let palette = ctx.add_typed_action_view(|ctx| {
            let binding_source = BindingSource::View {
                window_id,
                view_id,
                binding_filter_fn: Some(Arc::new(|binding| {
                    binding.action.as_ref().is_some_and(|action| {
                        action
                            .as_any()
                            .downcast_ref::<WelcomeViewAction>()
                            .is_some()
                    }) || binding.name == "workspace:show_settings"
                })),
            };

            let open_project_keybinding =
                keybinding_name_to_display_string("welcome_view:open_project", ctx);

            let terminal_session_keybinding =
                keybinding_name_to_display_string("workspace:new_tab", ctx);

            WelcomePalette::new(
                startup_directory.clone(),
                binding_source,
                open_project_keybinding,
                terminal_session_keybinding,
                ctx,
            )
        });
        ctx.subscribe_to_view(&palette, |me, _, event, ctx| {
            me.handle_palette_event(event, ctx);
        });

        Self {
            startup_directory,
            pane_configuration,
            focus_handle: None,
            palette,
            get_started_mouse_state: MouseStateHandle::default(),
            login_mouse_state: MouseStateHandle::default(),
        }
    }

    pub fn pane_configuration(&self) -> ModelHandle<PaneConfiguration> {
        self.pane_configuration.clone()
    }

    fn handle_palette_event(&mut self, event: &WelcomePaletteEvent, ctx: &mut ViewContext<Self>) {
        match event {
            WelcomePaletteEvent::Close => self.close(ctx),
            WelcomePaletteEvent::ParentAction { action } => self.handle_action(action, ctx),
            WelcomePaletteEvent::NewConversationInProject { path } => {
                self.open_project_conversation(path, ctx);
                self.close(ctx);
            }
            _ => {
                // TODO
            }
        }
    }

    fn create_terminal_session(&mut self, ctx: &mut ViewContext<Self>) {
        update_workspace(ctx.window_id(), ctx, |workspace, ctx| {
            workspace.add_tab_with_pane_layout(
                PanesLayout::SingleTerminal(Box::new(
                    NewTerminalOptions::default()
                        .with_initial_directory_opt(self.startup_directory.clone()),
                )),
                Arc::new(HashMap::new()),
                None,
                ctx,
            );
        });
    }

    fn open_project(&mut self, ctx: &mut ViewContext<Self>) {
        let window_id = ctx.window_id();
        ctx.open_file_picker(
            move |result, ctx| match result {
                Ok(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        save_and_open_project(path, window_id, ctx);
                        ctx.emit(PaneEvent::Close);
                    }
                }
                Err(err) => {
                    ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
                        toast_stack.add_ephemeral_toast(
                            DismissibleToast::error(format!("{err}")),
                            window_id,
                            ctx,
                        );
                    });
                }
            },
            FilePickerConfiguration::new().folders_only(),
        );
    }

    fn open_project_conversation(&mut self, path: &String, ctx: &mut ViewContext<Self>) {
        let path_buf = PathBuf::from(path);
        // todo(jparker): What happens if the user deletes a project folder between when this list was generated and now?
        update_workspace(ctx.window_id(), ctx, |workspace, ctx| {
            // Create a new terminal tab with the project path as the initial directory
            workspace.add_tab_with_pane_layout(
                PanesLayout::SingleTerminal(Box::new(
                    NewTerminalOptions::default().with_initial_directory(&path_buf),
                )),
                Arc::new(HashMap::new()),
                None,
                ctx,
            );

            // Start AI mode in the new terminal
            workspace
                .active_tab_pane_group()
                .update(ctx, |pane_group, ctx| {
                    pane_group.start_agent_mode_in_new_pane(None, None, ctx);
                });

            // Open code review pane
            workspace.active_tab_pane_group().update(ctx, |tab, ctx| {
                if let Some(active_terminal) = tab.active_session_view(ctx) {
                    active_terminal.update(ctx, |terminal, ctx| {
                        terminal.toggle_code_review_pane(
                            GitDeltaPreference::OnlyDirty,
                            CodeReviewPaneEntrypoint::Other,
                            None,  // cli_agent
                            false, /* focus_new_pane */
                            ctx,
                        );
                    });
                }
            });

            // Update project accesstime
            ProjectManagementModel::handle(ctx).update(ctx, |projects, ctx| {
                projects.upsert_project(path_buf, ctx);
            });
        });
    }
}

fn update_workspace<F>(window_id: WindowId, ctx: &mut AppContext, update_fn: F)
where
    F: FnOnce(&mut Workspace, &mut ViewContext<Workspace>),
{
    if let Some(workspaces) = ctx.views_of_type::<Workspace>(window_id) {
        if let Ok(workspace) = workspaces.into_iter().exactly_one() {
            workspace.update(ctx, update_fn);
        }
    }
}

impl Entity for WelcomeView {
    type Event = PaneEvent;
}

impl View for WelcomeView {
    fn ui_name() -> &'static str {
        "WelcomeView"
    }

    /// Renders the Wish welcome page.
    ///
    /// This deliberately mirrors the onboarding `IntroSlide` look-and-feel
    /// (`crates/onboarding/src/slides/intro_slide.rs`) but lives outside
    /// the auth-gated onboarding flow, so users get the same hero surface
    /// on first launch and via the **Help → Show Welcome Page** menu item
    /// without needing to be logged in to Hermon.
    ///
    /// Layout (top to bottom, vertically centered on the pane):
    ///   - Hermon "H" logo (`bundled/svg/hermon-logo.svg`)
    ///   - "Welcome to Wish" — large foreground-color heading
    ///   - "A modern terminal with state of the art agents built in." —
    ///     dimmed subtitle
    ///   - "Get started" primary button → opens the user's first
    ///     terminal session and closes this welcome tab
    ///   - "Already have an account? Log in" — a small secondary link
    ///     that triggers the Hermon sign-in flow (no-op if the user
    ///     is in pure-local mode and dismisses the auth window)
    ///
    /// The legacy `WelcomePalette` field is held on the struct for
    /// keystroke routing but is intentionally NOT rendered: the
    /// search-style picker is preserved as command-palette
    /// functionality but the visible welcome surface is the simpler
    /// hero design.
    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let ui_builder = appearance.ui_builder();

        // ── Logo ───────────────────────────────────────────────────
        let logo = ConstrainedBox::new(
            Image::new(
                AssetSource::Bundled {
                    path: "bundled/svg/hermon-logo.svg",
                },
                CacheOption::BySize,
            )
            .finish(),
        )
        .with_height(64.)
        .with_width(64.)
        .finish();

        // ── Title ──────────────────────────────────────────────────
        let title = ui_builder
            .paragraph("Welcome to Wish")
            .with_style(UiComponentStyles {
                font_size: Some(32.),
                ..Default::default()
            })
            .build()
            .finish();

        // ── Subtitle ───────────────────────────────────────────────
        // Use the established `text_sub` color helper so the subtitle
        // reads as muted across light/dark themes without hand-tuning.
        let subtitle_color = internal_colors::text_sub(theme, theme.background().into_solid());
        let subtitle = ui_builder
            .paragraph("A modern terminal with state of the art agents built in.")
            .with_style(UiComponentStyles {
                font_size: Some(16.),
                font_color: Some(subtitle_color),
                ..Default::default()
            })
            .build()
            .finish();

        // ── Primary CTA: Get started ───────────────────────────────
        // Dispatches the existing `WelcomeViewAction::CreateTerminalSession`
        // which the action handler routes to `create_terminal_session` ->
        // `close(ctx)`, opening a new terminal tab and dismissing the
        // welcome view.
        let get_started_button = ui_builder
            .button(ButtonVariant::Accent, self.get_started_mouse_state.clone())
            .with_text_label("Get started".to_string())
            .with_style(UiComponentStyles {
                font_size: Some(15.),
                ..Default::default()
            })
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| {
                ctx.dispatch_typed_action(WelcomeViewAction::CreateTerminalSession);
            })
            .finish();

        // ── Footer: optional log-in link ───────────────────────────
        // Renders as a compact text row — visually low-priority compared
        // to the hero CTA above. The on_click opens the Hermon sign-in
        // URL via the standard auth flow. If Hermon isn't configured
        // (pure-local mode), the dispatched action is a no-op.
        let login_link_text = ui_builder
            .span("Already have an account? Log in")
            .with_style(UiComponentStyles {
                font_size: Some(13.),
                font_color: Some(subtitle_color),
                ..Default::default()
            })
            .build()
            .finish();
        let login_link = Container::new(login_link_text)
            .with_padding_top(6.)
            .with_padding_bottom(6.)
            .finish();
        // Wrap in a Hoverable-equivalent: small clickable surface around
        // the link. We use a button with Text variant for consistency
        // with other "looks like a link" surfaces in this codebase.
        let login_button = ui_builder
            .button(ButtonVariant::Text, self.login_mouse_state.clone())
            .with_text_label("Already have an account? Log in".to_string())
            .with_style(UiComponentStyles {
                font_size: Some(13.),
                font_color: Some(subtitle_color),
                ..Default::default()
            })
            .build()
            .with_cursor(Cursor::PointingHand)
            .on_click(|ctx, _, _| {
                // Routes through `WelcomeViewAction::LogIn` because the
                // sign-in URL lookup needs `ViewContext` (singleton
                // access + `open_url`), neither of which is available
                // on this click closure's `EventContext`.
                ctx.dispatch_typed_action(WelcomeViewAction::LogIn);
            })
            .finish();
        // Suppress the unused-binding lint: we built `login_link` for
        // illustrative readability above, but the actual rendered
        // element is `login_button` (which carries the click target).
        let _ = login_link;

        // ── Layout ─────────────────────────────────────────────────
        let centered_column = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(logo)
            .with_child(Container::new(title).with_margin_top(20.).finish())
            .with_child(Container::new(subtitle).with_margin_top(12.).finish())
            .with_child(
                Container::new(get_started_button)
                    .with_margin_top(28.)
                    .finish(),
            )
            .with_child(Container::new(login_button).with_margin_top(80.).finish())
            .finish();

        // Center the whole stack on the pane.
        Align::new(centered_column).finish()
    }
}

impl BackingView for WelcomeView {
    type PaneHeaderOverflowMenuAction = ();
    type CustomAction = ();
    type AssociatedData = ();

    fn handle_pane_header_overflow_menu_action(
        &mut self,
        _action: &Self::PaneHeaderOverflowMenuAction,
        _ctx: &mut ViewContext<Self>,
    ) {
        unimplemented!()
    }

    fn close(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(PaneEvent::Close);
    }

    fn focus_contents(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.focus(&self.palette)
    }

    fn render_header_content(
        &self,
        _ctx: &view::HeaderRenderContext<'_>,
        _app: &AppContext,
    ) -> view::HeaderContent {
        view::HeaderContent::simple("New tab")
    }

    fn set_focus_handle(&mut self, focus_handle: PaneFocusHandle, _ctx: &mut ViewContext<Self>) {
        self.focus_handle = Some(focus_handle);
    }
}

impl TypedActionView for WelcomeView {
    type Action = WelcomeViewAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            WelcomeViewAction::CreateTerminalSession => {
                self.create_terminal_session(ctx);
                self.close(ctx);
            }
            WelcomeViewAction::OpenProject => {
                self.open_project(ctx);
            }
            WelcomeViewAction::LogIn => {
                self.open_login_url(ctx);
            }
        }
    }
}

impl WelcomeView {
    /// Resolve the Hermon sign-in URL from the [`AuthManager`] singleton
    /// and open it in the system browser. Called from the welcome
    /// page's "Already have an account? Log in" link.
    ///
    /// Uses [`ModelHandle::update`] (rather than chained
    /// `AuthManager::handle(ctx).as_ref(ctx)`) because we need to call
    /// `ctx.open_url(...)` immediately afterwards on the same context;
    /// the `update` closure gives us a fresh borrow of the context
    /// that doesn't conflict with the singleton-handle borrow.
    fn open_login_url(&self, ctx: &mut ViewContext<Self>) {
        crate::auth::AuthManager::handle(ctx).update(ctx, |auth_manager, inner_ctx| {
            let url = auth_manager.sign_in_url(inner_ctx);
            inner_ctx.open_url(&url);
        });
    }
}

/// WARNING - Don't use. The [`crate::workspace::WorkspaceAction::OpenRepository`] is the
/// source-of-truth for this now.
fn save_and_open_project(path: String, window_id: WindowId, ctx: &mut AppContext) {
    ProjectManagementModel::handle(ctx).update(ctx, |projects, ctx| {
        let path_buf = PathBuf::from(&path);
        projects.upsert_project(path_buf.clone(), ctx);
        update_workspace(window_id, ctx, move |workspace, ctx| {
            workspace.add_tab_with_pane_layout(
                PanesLayout::SingleTerminal(Box::new(
                    NewTerminalOptions::default()
                        .with_initial_directory(path)
                        .with_homepage_hidden(),
                )),
                Arc::new(HashMap::new()),
                None,
                ctx,
            );
            workspace.active_tab_pane_group().update(ctx, |tab, ctx| {
                if let Some(active_terminal) = tab.active_session_view(ctx) {
                    active_terminal.update(ctx, |terminal, _ctx| {
                        terminal.maybe_set_pending_repo_init_path(path_buf);
                    });
                }
            });
        });
    });
}
