use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn onboarding_section(
        &self,
        icon: LucideIcon,
        title_key: &str,
        hint_key: Option<&str>,
        body: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                icon,
                                ONBOARDING_ICON_SIZE,
                                rgb(self.tokens.ui.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t(title_key)),
                            ),
                    )
                    .when_some(hint_key, |row, key| {
                        row.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t(key)),
                        )
                    }),
            )
            .child(body)
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_feature_tile(
        &self,
        icon: LucideIcon,
        title_key: &'static str,
        description_key: &'static str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        // Full keys arrive as literals so the i18n audit can verify each one;
        // composing "onboarding.{fragment}" here hid deletions from the scan.
        self.onboarding_info_card(
            Some((icon, self.tokens.ui.accent)),
            title_key,
            Some(description_key),
            false,
            _cx,
        )
    }

    pub(in crate::workspace) fn onboarding_info_card(
        &self,
        icon: Option<(LucideIcon, u32)>,
        title_key: &str,
        detail_key: Option<&str>,
        accent: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.i18n.t(title_key);
        let detail = detail_key.map(|key| self.i18n.t(key)).unwrap_or_default();
        self.onboarding_info_card_with_text(icon, title, detail, accent)
    }

    pub(in crate::workspace) fn onboarding_info_card_with_text(
        &self,
        icon: Option<(LucideIcon, u32)>,
        title: String,
        detail: String,
        accent: bool,
    ) -> AnyElement {
        div()
            .flex()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if accent {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
            } else {
                rgb(self.tokens.ui.border)
            })
            .bg(if accent {
                rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
            } else {
                rgba((self.tokens.ui.bg_card << 8) | ONBOARDING_CARD_ALPHA)
            })
            .when_some(icon, |card, (icon, color)| {
                card.child(Self::render_lucide_icon(icon, 16.0, rgb(color)))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(title),
                    )
                    .when(!detail.is_empty(), |column| {
                        column.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(detail),
                        )
                    }),
            )
            .into_any_element()
    }
}
