use super::*;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::modal::modal_backdrop;
use oxideterm_gpui_ui::scroll::ScrollableElement;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_onboarding_modal(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;

        modal_backdrop(rgba((theme.bg << 8) | 0x99))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri prevents outside dismiss until the disclaimer is accepted.
                    this.close_onboarding_if_allowed(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .w(px(ONBOARDING_WIDTH))
                    // Give the flex column a definite height so its scroll body cannot collapse.
                    .h_full()
                    .max_h(px(ONBOARDING_MAX_HEIGHT))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgba((theme.bg_panel << 8) | 0xf2))
                    .shadow_lg()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scrollbar()
                            // Welcome is the only onboarding step.
                            .child(match OnboardingStep::from_index(self.onboarding.step) {
                                OnboardingStep::Welcome => self.render_onboarding_welcome(cx),
                            }),
                    )
                    .child(self.onboarding_footer(cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(32.0))
            .py(px(16.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_card << 8) | ONBOARDING_CARD_ALPHA))
            // The onboarding footer is the rounded shell's bottom painted
            // child; keep its background clipped to the browser panel curve.
            .rounded_b(px(oxideterm_gpui_ui::modal::rounded_shell_child_radius(
                self.tokens.radii.lg,
            )))
            // Single-step flow: no back or next navigation, only completion.
            .child(div())
            .child(self.onboarding_button(
                self.i18n.t("onboarding.start_exploring"),
                Some(LucideIcon::ArrowRight),
                ButtonVariant::Default,
                false,
                |this, _window, cx| this.complete_onboarding(cx),
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_button(
        &self,
        label: String,
        icon: Option<LucideIcon>,
        variant: ButtonVariant,
        disabled: bool,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut button = button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .gap(px(6.0))
        .opacity(if disabled {
            ONBOARDING_DISABLED_OPACITY
        } else {
            1.0
        })
        .cursor(if disabled {
            CursorStyle::OperationNotAllowed
        } else {
            CursorStyle::PointingHand
        });
        if let Some(icon) = icon {
            button = button.child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text)));
        }
        button
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if !disabled {
                        action(this, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
