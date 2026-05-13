//! Shimmering Wish loading text - renders the Wish/Hermon glyph with shimmering text.

use wish_core::ui::appearance::Appearance;
use wishui::elements::shimmering_text::{
    ShimmerConfig, ShimmeringTextElement, ShimmeringTextStateHandle,
};
use wishui::elements::Element;
use wishui::{AppContext, SingletonEntity};

/// Glyph rendered before the "Wishing..." shimmering text. Was originally the
/// upstream Warp brand glyph at PUA `U+E500` (only renders inside the bundled
/// Roboto font); now the universally-rendered sparkles emoji which carries the
/// "AI / wish / magic" semantic in any font, on any platform.
const WARP_GLYPH: &str = "\u{2728}";

/// Creates a shimmering text element with the Wish/Hermon glyph.
pub fn shimmering_warp_loading_text(
    text: impl Into<String>,
    font_size: f32,
    shimmer_handle: ShimmeringTextStateHandle,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();

    // Use same colors as common.rs for consistency
    let base_color = theme.disabled_text_color(theme.surface_1()).into_solid();
    let shimmer_color = theme.main_text_color(theme.surface_1()).into_solid();

    // Hardcoded shimmer config for consistent animation
    let config = ShimmerConfig::default();

    // Create a single shimmering element with glyph and text
    ShimmeringTextElement::new(
        format!("{} {}", WARP_GLYPH, text.into()),
        appearance.ui_font_family(),
        font_size,
        base_color,
        shimmer_color,
        config,
        shimmer_handle,
    )
    .finish()
}
