use std::sync::Arc;

use parking_lot::FairMutex;
use wishui::prelude::Empty;
use wishui::{
    elements::{
        ChildView, Container, CrossAxisAlignment, Expanded, Flex, MainAxisSize, ParentElement,
    },
    AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle,
};

use crate::{
    terminal::view::{TerminalModel, PADDING_LEFT},
    ui_components::icons::Icon,
    view_components::action_button::{ActionButton, ButtonSize, KeystrokeSource, TooltipAlignment},
};

use super::{AgentFooterButtonTheme, USE_AGENT_KEYSTROKE};
use crate::terminal::view::block_banner::WishificationMode;

/// Footer view rendered for detected subshell/SSH commands, offering both
/// "Wishify" and "Use agent" buttons in a horizontal row.
pub(super) struct WishifyFooterView {
    terminal_model: Arc<FairMutex<TerminalModel>>,
    warpify_button: ViewHandle<ActionButton>,
    use_agent_button: ViewHandle<ActionButton>,
    dismiss_button: ViewHandle<ActionButton>,
    mode: Option<WishificationMode>,
}

impl WishifyFooterView {
    pub fn new(terminal_model: Arc<FairMutex<TerminalModel>>, ctx: &mut ViewContext<Self>) -> Self {
        let button_size = ButtonSize::XSmall;

        let warpify_button = ctx.add_typed_action_view(|_ctx| {
            ActionButton::new("Wishify subshell", AgentFooterButtonTheme::new(None))
                .with_icon(Icon::Warp)
                .with_size(button_size)
                .with_tooltip("Enable Wish shell integration in this session")
                .with_tooltip_alignment(TooltipAlignment::Left)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(WishifyFooterViewAction::Wishify);
                })
        });

        let use_agent_button = ctx.add_typed_action_view(|ctx| {
            ActionButton::new("Use agent", AgentFooterButtonTheme::new(None))
                .with_icon(Icon::Oz)
                .with_keybinding(KeystrokeSource::Fixed(USE_AGENT_KEYSTROKE.clone()), ctx)
                .with_size(button_size)
                .with_tooltip("Ask the Wish agent to assist")
                .with_tooltip_alignment(TooltipAlignment::Left)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(WishifyFooterViewAction::UseAgent);
                })
        });

        let dismiss_button = ctx.add_typed_action_view(|_ctx| {
            ActionButton::new("Dismiss", AgentFooterButtonTheme::new(None))
                .with_size(button_size)
                .on_click(|ctx| {
                    ctx.dispatch_typed_action(WishifyFooterViewAction::Dismiss);
                })
        });

        Self {
            terminal_model,
            warpify_button,
            use_agent_button,
            dismiss_button,
            mode: None,
        }
    }

    /// Updates the warpify button label, keybinding, and stores the current warpification mode.
    pub fn set_mode(&mut self, mode: WishificationMode, ctx: &mut ViewContext<Self>) {
        let (label, binding_name) = match mode {
            WishificationMode::Ssh { .. } => {
                ("Wishify SSH session", "terminal:warpify_ssh_session")
            }
            WishificationMode::Subshell { .. } => ("Wishify subshell", "terminal:warpify_subshell"),
        };
        self.warpify_button.update(ctx, |button, ctx| {
            button.set_label(label, ctx);
            button.set_keybinding(Some(KeystrokeSource::Binding(binding_name)), ctx);
        });
        self.mode = Some(mode);
        ctx.notify();
    }

    /// Returns the current warpification mode, if set.
    pub fn mode(&self) -> Option<&WishificationMode> {
        self.mode.as_ref()
    }

    /// Clears the warpification mode.
    pub fn clear_mode(&mut self, ctx: &mut ViewContext<Self>) {
        self.mode = None;
        self.warpify_button.update(ctx, |button, ctx| {
            button.set_keybinding(None, ctx);
        });
        ctx.notify();
    }
}

#[derive(Debug, Clone)]
pub enum WishifyFooterViewAction {
    Wishify,
    UseAgent,
    Dismiss,
}

pub enum WishifyFooterViewEvent {
    Wishify { mode: WishificationMode },
    UseAgent,
    Dismiss,
}

impl Entity for WishifyFooterView {
    type Event = WishifyFooterViewEvent;
}

impl View for WishifyFooterView {
    fn ui_name() -> &'static str {
        "WishifyFooterView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        let terminal_model = self.terminal_model.lock();

        let button_row = Flex::row()
            .with_spacing(4.)
            .with_main_axis_size(MainAxisSize::Max)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(ChildView::new(&self.warpify_button).finish())
            .with_child(ChildView::new(&self.use_agent_button).finish())
            .with_child(Expanded::new(1., Empty::new().finish()).finish())
            .with_child(ChildView::new(&self.dismiss_button).finish());

        let mut container = Container::new(button_row.finish())
            .with_horizontal_padding(*PADDING_LEFT)
            .with_vertical_padding(4.);

        if terminal_model.is_alt_screen_active() {
            if let Some(bg_color) = terminal_model.alt_screen().inferred_bg_color() {
                container = container.with_background(bg_color);
            }
        }

        container.finish()
    }
}

impl TypedActionView for WishifyFooterView {
    type Action = WishifyFooterViewAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            WishifyFooterViewAction::Wishify => {
                if let Some(mode) = self.mode.clone() {
                    self.clear_mode(ctx);
                    ctx.emit(WishifyFooterViewEvent::Wishify { mode });
                }
            }
            WishifyFooterViewAction::UseAgent => {
                self.clear_mode(ctx);
                ctx.emit(WishifyFooterViewEvent::UseAgent);
            }
            WishifyFooterViewAction::Dismiss => {
                self.clear_mode(ctx);
                ctx.emit(WishifyFooterViewEvent::Dismiss);
            }
        }
    }
}
