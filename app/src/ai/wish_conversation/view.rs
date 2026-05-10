//! [`WishChatView`] — renders the active Wish conversation in the
//! terminal.
//!
//! Subscribes to [`ConversationManagerModel`] events and re-renders
//! when turns are appended or streaming chunks arrive. In guest mode
//! this is the primary surface for viewing AI responses.

use wish_core::ui::{appearance::Appearance, theme::WarpTheme};
use wishui::{
    elements::{Container, CrossAxisAlignment, Flex, MainAxisSize, ParentElement, Text},
    fonts::{Properties, Weight},
    Element, Entity, ModelHandle, SingletonEntity, View, ViewContext,
};

use super::model::{ConversationManagerEvent, ConversationManagerModel};
use super::types::{MessageBlock, Turn};

// Re-export so callers only need `wish_conversation::WishChatView`.
// The font family needed for Text elements comes from Appearance.

/// Default font size for chat text.
const CHAT_FONT_SIZE: f32 = 13.;
/// Smaller font size for system/meta labels.
const META_FONT_SIZE: f32 = 12.;

// ── View ────────────────────────────────────────────────────────────

/// The chat surface for Wish conversations. Shows completed turns
/// and the in-flight assistant response.
pub struct WishChatView {
    mgr: ModelHandle<ConversationManagerModel>,
}

impl Entity for WishChatView {
    type Event = ();
}

impl WishChatView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let mgr = ConversationManagerModel::handle(ctx);

        // Re-render on any conversation event.
        ctx.subscribe_to_model(&mgr, |_me, _model, event, ctx| match event {
            ConversationManagerEvent::TurnAppended { .. }
            | ConversationManagerEvent::Chunk { .. }
            | ConversationManagerEvent::ActiveChanged { .. }
            | ConversationManagerEvent::Created { .. }
            | ConversationManagerEvent::Removed { .. } => {
                ctx.notify();
            }
        });

        Self { mgr }
    }
}

impl View for WishChatView {
    fn ui_name() -> &'static str {
        "WishChatView"
    }

    fn render(&self, app: &wishui::AppContext) -> Box<dyn wishui::Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let font = appearance.ui_font_family();
        let mgr = self.mgr.as_ref(app);

        let mut column = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Start);

        // ── Header ──────────────────────────────────────────
        let header_label = mgr
            .adapter_label()
            .unwrap_or_else(|| "offline".to_string());
        let header_text = format!("Wish · {header_label}");
        let header = Container::new(
            Text::new(header_text, font, META_FONT_SIZE)
                .with_style(Properties::default().weight(Weight::Bold))
                .with_color(theme.accent().into_solid())
                .finish(),
        )
        .with_margin_bottom(6.)
        .finish();
        column.add_child(header);

        // ── Conversation body ───────────────────────────────
        if let Some(convo) = mgr.active_conversation() {
            for turn in &convo.turns {
                column.add_child(render_turn(turn, theme, font));
            }

            // In-flight assistant text (streaming)
            if let Some(in_flight) = mgr.in_flight(&convo.id) {
                let streaming_text = if !in_flight.text_buf.is_empty() {
                    in_flight.text_buf.clone()
                } else if in_flight.streaming {
                    "thinking...".to_string()
                } else {
                    String::new()
                };

                if !streaming_text.is_empty() {
                    let el = Container::new(
                        Text::new(streaming_text, font, CHAT_FONT_SIZE)
                            .with_color(theme.ansi_fg_cyan())
                            .finish(),
                    )
                    .with_margin_top(4.)
                    .with_margin_bottom(4.)
                    .finish();
                    column.add_child(el);
                }
            }
        } else {
            // No active conversation yet
            let hint = Text::new(
                "No conversation yet. Type a message to begin.",
                font,
                CHAT_FONT_SIZE,
            )
            .with_color(theme.foreground().into_solid())
            .finish();
            column.add_child(Container::new(hint).with_margin_top(4.).finish());
        }

        // Wrap entire view with some padding.
        Container::new(column.finish())
            .with_uniform_padding(8.)
            .finish()
    }
}

// ── Turn rendering helpers ──────────────────────────────────────────

use wishui::fonts::FamilyId;

fn render_turn(turn: &Turn, theme: &WarpTheme, font: FamilyId) -> Box<dyn wishui::Element> {
    match turn {
        Turn::User { content, .. } => {
            let label = Text::new("You", font, CHAT_FONT_SIZE)
                .with_style(Properties::default().weight(Weight::Bold))
                .with_color(theme.ansi_fg_green())
                .finish();
            let body = Text::new(content.clone(), font, CHAT_FONT_SIZE)
                .with_color(theme.foreground().into_solid())
                .finish();
            Container::new(
                Flex::column()
                    .with_cross_axis_alignment(CrossAxisAlignment::Start)
                    .with_child(label)
                    .with_child(body)
                    .finish(),
            )
            .with_margin_top(4.)
            .with_margin_bottom(4.)
            .finish()
        }

        Turn::Assistant { blocks, .. } => {
            let label = Text::new("Assistant", font, CHAT_FONT_SIZE)
                .with_style(Properties::default().weight(Weight::Bold))
                .with_color(theme.ansi_fg_cyan())
                .finish();

            let mut col = Flex::column()
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(label);

            for block in blocks {
                col.add_child(render_message_block(block, theme, font));
            }

            Container::new(col.finish())
                .with_margin_top(4.)
                .with_margin_bottom(4.)
                .finish()
        }

        Turn::System { note, .. } => {
            let text =
                Text::new(format!("[system] {note}"), font, META_FONT_SIZE)
                    .with_color(theme.ansi_fg_yellow())
                    .finish();
            Container::new(text)
                .with_margin_top(2.)
                .with_margin_bottom(2.)
                .finish()
        }
    }
}

fn render_message_block(
    block: &MessageBlock,
    theme: &WarpTheme,
    font: FamilyId,
) -> Box<dyn wishui::Element> {
    match block {
        MessageBlock::Text { text } => {
            Text::new(text.clone(), font, CHAT_FONT_SIZE)
                .with_color(theme.foreground().into_solid())
                .finish()
        }

        MessageBlock::Thinking { text } => {
            // Truncate long thinking blocks.
            let display = if text.len() > 200 {
                format!("[thinking] {}...", &text[..200])
            } else {
                format!("[thinking] {text}")
            };
            Text::new(display, font, META_FONT_SIZE)
                .with_color(theme.ansi_fg_magenta())
                .finish()
        }

        MessageBlock::ToolUse { tool_name, .. } => {
            let content = format!("[tool] {tool_name}");
            Text::new(content, font, META_FONT_SIZE)
                .with_color(theme.ansi_fg_yellow())
                .finish()
        }

        MessageBlock::ToolResult {
            output, is_error, ..
        } => {
            let color = if *is_error {
                theme.ansi_fg_red()
            } else {
                theme.ansi_fg_green()
            };
            // Truncate very long tool output for display.
            let display = if output.len() > 500 {
                format!("{}... (truncated)", &output[..500])
            } else {
                output.clone()
            };
            Text::new(display, font, META_FONT_SIZE)
                .with_color(color)
                .finish()
        }
    }
}
