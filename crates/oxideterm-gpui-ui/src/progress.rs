use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Div, IntoElement, ParentElement, Styled, div, prelude::*,
    px, relative, rgb,
};
use oxideterm_theme::ThemeTokens;

// Baseline sweep period at normal motion cadence. It is scaled through the
// motion profile so fast and reduced profiles keep the same relative flow.
const PROGRESS_INDETERMINATE_BASELINE_MS: u64 = 1200;

pub fn progress(tokens: &ThemeTokens, value: Option<f32>, indeterminate: bool) -> Div {
    let pct = value.unwrap_or(0.0).clamp(0.0, 100.0);
    div()
        .relative()
        .h(px(tokens.metrics.ui_progress_height))
        .w_full()
        .overflow_hidden()
        .rounded_full()
        .bg(rgb(tokens.ui.bg_panel))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .child(if indeterminate {
            progress_indeterminate_bar(tokens)
        } else {
            div()
                .h_full()
                .rounded_full()
                .bg(rgb(tokens.ui.accent))
                .w(relative(pct / 100.0))
                .into_any_element()
        })
}

fn progress_indeterminate_bar(tokens: &ThemeTokens) -> AnyElement {
    let bar = div()
        // `left` only offsets positioned elements, so the sweeping bar needs
        // relative positioning inside the clipped track.
        .relative()
        .h_full()
        .rounded_full()
        .bg(rgb(tokens.ui.accent))
        .w_1_3();
    if !tokens.motion.enabled {
        // Reduced motion keeps the static one-third bar; a repeating sweep
        // never settles and reads as flicker.
        return bar.into_any_element();
    }
    // Linear repeating sweep: the one-third bar travels the remaining
    // two-thirds of the track at a constant flow rate.
    let period = crate::motion::scaled_duration(tokens, PROGRESS_INDETERMINATE_BASELINE_MS);
    bar.with_animation(
        "progress-indeterminate",
        Animation::new(period)
            .repeat()
            .with_easing(|progress| progress),
        move |bar, progress| bar.left(relative(progress * 2.0 / 3.0)),
    )
    .into_any_element()
}
