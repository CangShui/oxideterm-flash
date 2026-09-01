use super::*;
use gpui::StatefulInteractiveElement;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_activity_bar(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // The new local terminal leads the activity bar because it is the most
        // frequent startup action in every workspace.
        let mut top_items_before_plugins = vec![(SidebarSection::Workspace, LucideIcon::Square)];
        top_items_before_plugins.extend([(SidebarSection::Sessions, LucideIcon::Link2)]);
        let top_items_after_plugins = [(SidebarSection::HostTools, LucideIcon::Wrench)];
        let mut bottom_items = vec![(SidebarSection::Settings, LucideIcon::Settings)];
        if !cfg!(target_os = "windows") {
            // Keep the monitor tool directly above Settings on non-Windows
            // builds.
            bottom_items.insert(0, (SidebarSection::Monitor, LucideIcon::Monitor));
        }
        let mut bar = div()
            .w(px(self.tokens.metrics.activity_bar_width))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .border_r_1()
            .border_color(rgb(theme.border));

        bar = bar.child(
            div()
                .relative()
                .flex_none()
                .w_full()
                .h(px(self.tokens.metrics.tabbar_height))
                .flex()
                .items_center()
                .justify_center()
                // Paint top chrome directly over the window background so it
                // matches the adjacent sidebar and workspace tab bars exactly.
                .bg(self.workspace_chrome_background(theme.bg))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .id("activity-sidebar-toggle")
                        .size(px(self.tokens.metrics.activity_icon_size))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .cursor_pointer()
                        // Keep the primary-sidebar toggle feedback aligned with its context-sidebar counterpart.
                        .hover(move |button| button.bg(rgb(theme.bg_hover)))
                        .child(Self::render_lucide_icon(
                            if self.sidebar_collapsed {
                                LucideIcon::PanelLeft
                            } else {
                                LucideIcon::PanelLeftClose
                            },
                            self.tokens.metrics.sidebar_collapse_icon_size,
                            rgb(theme.text_muted),
                        ))
                        .on_mouse_move(cx.listener({
                            let label = self.i18n.t(if self.sidebar_collapsed {
                                "sidebar.actions.expand"
                            } else {
                                "sidebar.actions.collapse"
                            });
                            move |this, event: &MouseMoveEvent, _window, cx| {
                                this.queue_workspace_tooltip(
                                    "activity-sidebar-toggle",
                                    label.clone(),
                                    f32::from(event.position.x) + 12.0,
                                    f32::from(event.position.y) + 16.0,
                                    cx,
                                );
                            }
                        }))
                        .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                            if !*hovered {
                                this.clear_workspace_tooltip("activity-sidebar-toggle", cx);
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.toggle_sidebar(cx);
                            }),
                        ),
                ),
        );

        let mut primary_items = div()
            .id("activity-primary-items")
            .min_h_0()
            .flex_1()
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center();
        for (section, icon) in top_items_before_plugins {
            primary_items = primary_items.child(self.render_activity_icon(section, icon, cx));
        }
        for (section, icon) in top_items_after_plugins {
            primary_items = primary_items.child(self.render_activity_icon(section, icon, cx));
        }

        let mut bottom = div().relative().flex().flex_col().items_center().child(
            div()
                .w(px(self.tokens.metrics.divider_width))
                .h(px(self.tokens.metrics.divider_height))
                .bg(rgb(theme.divider)),
        );
        bottom = bottom.children(
            bottom_items
                .into_iter()
                .map(|(section, icon)| self.render_activity_icon(section, icon, cx)),
        );

        bar.child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .pb_2()
                .flex()
                .flex_col()
                .items_center()
                // The content column keeps the lighter sidebar tint without
                // placing another translucent layer beneath the top chrome.
                .bg(self.workspace_sidebar_background(theme.bg))
                .child(primary_items)
                .child(bottom),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_activity_icon(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = match section {
            SidebarSection::Monitor if cfg!(target_os = "macos") => self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Launcher),
            SidebarSection::Settings => self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Settings),
            SidebarSection::HostTools => {
                self.context_sidebar_visible()
                    && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
            }
            _ => self.active_sidebar_section == section,
        };
        let tooltip = self.activity_icon_tooltip(section);
        let tooltip_id = format!("activity-icon-{}", section.as_settings_key());
        let tooltip_id_for_move = tooltip_id.clone();
        let badge_count = 0u32;
        let badge_is_error = false;
        let badge_color = theme.accent;
        let badge_text_color = theme.accent_text;

        // Activity entries use the same static selected-card treatment as the
        // settings navigation while retaining the shared icon-button states.
        let button = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.activity_icon_glyph_size,
                rgb(if active { theme.accent } else { theme.text }),
            ),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: self.tokens.metrics.activity_icon_size,
                radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                has_background: active,
                background: active.then(|| self.settings_panel_background(theme.bg_panel)),
                border: active.then(|| rgb(theme.border)),
                hover_background: Some(rgb(theme.bg_hover)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                    self.tokens.metrics.activity_icon_size,
                )
            },
        );
        let button = if active {
            oxideterm_gpui_ui::theme_card_surface_shadow(button, &self.tokens)
        } else {
            button
        };

        button
            .id(("activity-icon", section as u64))
            .relative()
            .mb(px(self.tokens.metrics.activity_icon_gap))
            .when(badge_count > 0, |icon_el| {
                icon_el.child(
                    div()
                        .absolute()
                        // Tauri badges sit outside the `h-9 w-9` Button
                        // (`-top-1 -right-1`) so the active button chrome and
                        // numeric pill do not visually collide.
                        .right(px(-4.0))
                        .top(px(-4.0))
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px(px(3.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgb(badge_color))
                        .text_color(rgb(badge_text_color))
                        .text_size(px(9.0))
                        .child(if badge_count > 99 {
                            "99+".to_string()
                        } else {
                            badge_count.to_string()
                        }),
                )
            })
            .on_mouse_move(cx.listener({
                let tooltip = tooltip;
                move |this, event: &MouseMoveEvent, _window, cx| {
                    this.queue_workspace_tooltip(
                        tooltip_id_for_move.clone(),
                        tooltip.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if !*hovered {
                    this.clear_workspace_tooltip(&tooltip_id, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if section == SidebarSection::Settings {
                        this.open_settings(window, cx);
                    } else if section == SidebarSection::Workspace {
                        this.open_new_connection_form(window, cx);
                    } else if section == SidebarSection::Monitor && cfg!(target_os = "macos") {
                        this.open_launcher_tab(window, cx);
                    } else if section == SidebarSection::HostTools {
                        let _ =
                            this.toggle_context_sidebar_panel(ContextSidebarPanel::HostTools, cx);
                    } else {
                        this.active_surface = ActiveSurface::Terminal;
                        this.toggle_sidebar_section(section, cx);
                    }
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn activity_icon_tooltip(&self, section: SidebarSection) -> String {
        match section {
            SidebarSection::Sessions => self.i18n.t("sidebar.panels.sessions"),
            SidebarSection::Connections => self.i18n.t("sidebar.panels.open_session_manager"),
            SidebarSection::HostTools => self.i18n.t("sidebar.panels.host_tools"),
            SidebarSection::Automation => self.i18n.t("sidebar.panels.activity"),
            SidebarSection::Workspace => self.i18n.t("sidebar.actions.new_local_terminal"),
            SidebarSection::Monitor if cfg!(target_os = "macos") => {
                self.i18n.t("launcher.tabTitle")
            }
            SidebarSection::Monitor => self.i18n.t("sidebar.panels.connection_monitor"),
            SidebarSection::Settings => self.i18n.t("sidebar.tooltips.settings"),
        }
    }

    pub(in crate::workspace) fn render_lucide_icon(
        icon: LucideIcon,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_animated_chevron(
        &self,
        id: impl Into<gpui::ElementId>,
        expanded: bool,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        // A single right-facing asset keeps state changes continuous instead
        // of swapping two unrelated SVG trees between renders.
        oxideterm_gpui_ui::motion::animated_chevron(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::ChevronRight.path())
                .size(px(size))
                .text_color(color),
            expanded,
        )
    }

    pub(in crate::workspace) fn render_loading_icon(
        &self,
        id: impl Into<gpui::ElementId>,
        size: f32,
        color: Rgba,
    ) -> AnyElement {
        // Spinner frames are requested only while callers render a genuine
        // loading state; static status icons continue using render_lucide_icon.
        oxideterm_gpui_ui::motion::animated_spinner(
            &self.tokens,
            id,
            svg()
                .path(LucideIcon::LoaderCircle.path())
                .size(px(size))
                .text_color(color),
        )
    }
}
