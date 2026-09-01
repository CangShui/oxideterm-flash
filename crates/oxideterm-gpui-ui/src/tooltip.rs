use gpui::{
    AnyElement, AnyView, App, AppContext, Context, ParentElement, Render, Styled, Window, div,
    prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

// Tauri TooltipContent wraps long labels at max-w-[360px]; native keeps the
// same ceiling so one-line tooltips cannot overflow the window.
const TOOLTIP_MAX_WIDTH: f32 = 360.0;

struct TooltipView {
    tokens: ThemeTokens,
    label: String,
    shortcut: Option<String>,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl gpui::IntoElement {
        tooltip_content(&self.tokens, self.label.clone(), self.shortcut.clone())
    }
}

pub fn tooltip_view(
    tokens: ThemeTokens,
    label: impl Into<String>,
    shortcut: Option<String>,
    cx: &mut App,
) -> AnyView {
    // GPUI mounts native tooltips at the window layer, keeping them outside
    // scroll masks and other clipped feature subtrees.
    cx.new(|_| TooltipView {
        tokens,
        label: label.into(),
        shortcut,
    })
    .into()
}

pub fn tooltip_content(
    tokens: &ThemeTokens,
    label: impl Into<String>,
    shortcut: Option<String>,
) -> AnyElement {
    let label = label.into();
    // Each tooltip renders from its own view (or a single overlay layer), so
    // the enter animation id must stay label-independent: deriving it from the
    // label made same-label tooltips share one animation timeline.
    let animation_id = "tooltip-enter";
    let tooltip = div()
        .max_w(px(TOOLTIP_MAX_WIDTH))
        .rounded(px(tokens.radii.xs))
        .border_1()
        .border_color(rgb(tokens.ui.border))
        .bg(rgba((tokens.ui.bg_elevated << 8) | 0xf2))
        .px(px(tokens.metrics.ui_tooltip_padding_x))
        .py(px(tokens.metrics.ui_tooltip_padding_y))
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(rgb(tokens.ui.text))
        .shadow_lg()
        .child(
            div()
                .flex()
                .min_w(px(0.0))
                .items_center()
                .gap(px(tokens.spacing.two))
                .child(label)
                .when_some(shortcut, |row, shortcut| {
                    row.child(
                        div()
                            .ml_auto()
                            .flex_none()
                            .rounded(px(tokens.radii.xs))
                            .border_1()
                            .border_color(rgb(tokens.ui.border))
                            .bg(rgb(tokens.ui.bg))
                            .px(px(tokens.spacing.one))
                            .py(px(tokens.spacing.one / 2.0))
                            .text_size(px(tokens.metrics.ui_tooltip_shortcut_font_size))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(shortcut),
                    )
                }),
        );
    crate::motion::fade_in(
        tokens,
        animation_id,
        tooltip,
        crate::motion::MotionDuration::Micro,
    )
}
