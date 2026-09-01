use std::time::Duration;

use gpui::{Div, ParentElement, Styled, div, px, rgb, rgba};
use oxideterm_theme::ThemeTokens;

// Normal-cadence baseline for the HUD dwell time. It feeds the motion token
// scaling so faster/slower profiles keep the same relative cadence.
const FONT_SIZE_HUD_BASELINE_MS: u64 = 1200;

pub fn font_size_hud_duration(tokens: &ThemeTokens) -> Duration {
    // The dwell time is informational rather than animated, so a disabled
    // motion profile keeps the baseline instead of scaled_duration_ms' zero,
    // which would cut the HUD before it can be read.
    if tokens.motion.enabled {
        Duration::from_millis(tokens.motion.scaled_duration_ms(FONT_SIZE_HUD_BASELINE_MS))
    } else {
        Duration::from_millis(FONT_SIZE_HUD_BASELINE_MS)
    }
}

pub fn font_size_hud(tokens: &ThemeTokens, size: f32, unit: &str) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .rounded(px(tokens.radii.xs))
                .border_1()
                .border_color(rgb(tokens.ui.border))
                .bg(rgba((tokens.ui.bg_elevated << 8) | 0xe6))
                .px(px(tokens.metrics.ui_font_hud_padding_x))
                .py(px(tokens.metrics.ui_font_hud_padding_y))
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_end()
                        // Inherit the configured terminal font from the HUD owner.
                        .text_size(px(tokens.metrics.ui_text_2xl))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(tokens.ui.text))
                        .child(format!("{size:.0}"))
                        .child(
                            div()
                                .ml(px(tokens.spacing.one / 2.0))
                                .text_size(px(tokens.metrics.ui_text_base))
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_color(rgb(tokens.ui.text_muted))
                                // The unit is locale-owned (e.g. terminal.font_size_unit)
                                // and must not be hardcoded here.
                                .child(unit.to_string()),
                        ),
                ),
        )
}
