use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_onboarding_welcome(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(32.0))
            .pt(px(32.0))
            .pb(px(24.0))
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(30.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("onboarding.welcome")),
                            )
                            .child(
                                div()
                                    .w(px(3.0))
                                    .h(px(21.0))
                                    .rounded(px(2.0))
                                    .bg(rgba((self.tokens.ui.text << 8) | 0x66)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("onboarding.subtitle")),
                    ),
            )
            .child(self.onboarding_info_card(None, "onboarding.project_intro", None, false, cx))
            .child(div().grid().grid_cols(2).gap(px(8.0)).children([
                self.onboarding_feature_tile(LucideIcon::Zap, "highlight_performance", cx),
                self.onboarding_feature_tile(LucideIcon::Lock, "highlight_security_arch", cx),
                self.onboarding_feature_tile(LucideIcon::Cpu, "highlight_crossplatform", cx),
                self.onboarding_feature_tile(LucideIcon::Puzzle, "highlight_extensible", cx),
            ]))
            .child(self.onboarding_language_picker(cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_language_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.settings_store.settings().general.language;
        let mut grid = div().grid().grid_cols(4).gap(px(6.0));
        for (language, label) in ONBOARDING_LANGUAGES {
            let is_selected = language == selected;
            grid = grid.child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(if is_selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.border)
                    })
                    .bg(if is_selected {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgb(self.tokens.ui.bg_card)
                    })
                    .text_color(if is_selected {
                        rgb(self.tokens.ui.accent_text)
                    } else {
                        rgb(self.tokens.ui.text)
                    })
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .cursor(CursorStyle::PointingHand)
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(|settings| settings.general.language = language, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        self.onboarding_section(
            LucideIcon::Home,
            "onboarding.select_language",
            None,
            grid.into_any_element(),
        )
    }
}
