use super::*;

pub(in crate::workspace) const SIDEBAR_RESIZE_HOTZONE_INTERIOR_WIDTH: f32 = 8.0;
pub(in crate::workspace) const SIDEBAR_RESIZE_DIVIDER_WIDTH: f32 = 1.0;
pub(in crate::workspace) const SIDEBAR_RESIZE_HOTZONE_WIDTH: f32 =
    SIDEBAR_RESIZE_DIVIDER_WIDTH + SIDEBAR_RESIZE_HOTZONE_INTERIOR_WIDTH;
#[allow(dead_code)]
const ACTIVITY_TOOLBAR_BUTTON_SIZE: f32 = 28.0;
#[allow(dead_code)]
const ACTIVITY_TOOLBAR_ICON_SIZE: f32 = 15.0;
#[allow(dead_code)]
const ACTIVITY_TOOLBAR_GROUP_PADDING: f32 = 2.0;
#[allow(dead_code)]
const ACTIVITY_EMPTY_STATE_ICON_SIZE: f32 = 20.0;
#[allow(dead_code)]
const ACTIVITY_TOOLBAR_ACTIVE_BACKGROUND_ALPHA: u32 = 0x1f;
#[allow(dead_code)]
const ACTIVITY_TOOLBAR_ACTIVE_BORDER_ALPHA: u32 = 0x52;

#[derive(Clone, Copy)]
pub(in crate::workspace) enum SidebarResizeHotzonePlacement {
    BeforeSeam,
    AfterSeam,
}

impl SidebarResizeHotzonePlacement {
    fn origin(self, seam: f32) -> f32 {
        match self {
            Self::BeforeSeam => seam - SIDEBAR_RESIZE_HOTZONE_WIDTH,
            Self::AfterSeam => seam,
        }
    }

    fn divider_offset(self) -> f32 {
        match self {
            Self::BeforeSeam => SIDEBAR_RESIZE_HOTZONE_WIDTH - SIDEBAR_RESIZE_DIVIDER_WIDTH,
            Self::AfterSeam => 0.0,
        }
    }
}

pub(in crate::workspace) fn context_sidebar_frame_chrome(
    total_width: f32,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id("context-right-sidebar-frame")
        .relative()
        .flex_none()
        .w(px(total_width))
        .h_full()
        .min_w_0()
        .flex()
        .flex_row()
}

pub(in crate::workspace) fn context_sidebar_region_chrome() -> gpui::Div {
    div().relative().flex_1().min_w(px(0.0)).h_full().min_h_0()
}

pub(in crate::workspace) fn sidebar_resize_hotzone_chrome(
    element_id: &'static str,
    line_color: gpui::Rgba,
    placement: SidebarResizeHotzonePlacement,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(element_id)
        .absolute()
        .w(px(SIDEBAR_RESIZE_HOTZONE_WIDTH))
        .cursor_col_resize()
        // Keep the complete drag target inside the owning sidebar so the
        // adjacent terminal can select text from its first visible pixel.
        .occlude()
        .bg(rgba(0x00000000))
        .child(
            div()
                .absolute()
                .left(px(placement.divider_offset()))
                .top_0()
                .bottom_0()
                .w(px(SIDEBAR_RESIZE_DIVIDER_WIDTH))
                // Keep the resize cursor when the pointer lands on the painted line itself.
                .cursor_col_resize()
                .bg(line_color),
        )
}

pub(in crate::workspace) fn sidebar_resize_hotzone_origin(
    seam: f32,
    placement: SidebarResizeHotzonePlacement,
) -> f32 {
    placement.origin(seam)
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_animated_sidebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded_width = self.sidebar_panel_width();
        let expanded = !self.sidebar_collapsed;
        let content = div()
            .flex_none()
            .w(px(expanded_width))
            .h_full()
            .child(self.render_sidebar_region(window, cx));
        oxideterm_gpui_ui::motion::horizontal_reveal(
            &self.tokens,
            "workspace-left-sidebar-motion",
            content,
            expanded_width,
            expanded,
        )
    }

    pub(in crate::workspace) fn render_animated_context_sidebar_frame(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded_width = self.context_sidebar_width();
        let expanded = self.context_sidebar_visible();
        let content = div()
            .flex_none()
            .w(px(expanded_width))
            .h_full()
            .child(self.render_context_right_sidebar_frame(window, cx));
        oxideterm_gpui_ui::motion::horizontal_reveal(
            &self.tokens,
            "workspace-right-sidebar-motion",
            content,
            expanded_width,
            expanded,
        )
    }

    pub(in crate::workspace) fn render_sidebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(self.sidebar_panel_width()))
            .h_full()
            .child(self.render_sidebar(window, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_context_right_sidebar_frame(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        context_sidebar_frame_chrome(self.context_sidebar_width())
            .child(self.render_context_right_sidebar_region(window, cx))
            // Keeping the drag hotzone inside the frame keeps it attached to
            // the animated reveal instead of regressing hit-testing against
            // scroll-heavy Host Tools content.
            .child(self.render_context_sidebar_resize_hotzone(cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_context_right_sidebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (title_key, title_role, icon) = match self.active_context_sidebar_panel {
            ContextSidebarPanel::HostTools => (
                "sidebar.panels.host_tools",
                "host-tools",
                LucideIcon::Wrench,
            ),
        };
        context_sidebar_region_chrome()
            .child(
                div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex_none()
                            // The right sidebar header sits beside the main
                            // tabbar, so keep both chrome rows exactly aligned.
                            .h(px(self.tokens.metrics.tabbar_height))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .px_3()
                            // Match the center tab bar's chrome opacity instead
                            // of inheriting the more transparent sidebar body.
                            .bg(self.workspace_chrome_background(theme.bg))
                            // The context-sidebar titlebar is fixed chrome.
                            // Give it its own hitbox so wheel/drag events
                            // cannot fall through to a scrollable tool body.
                            .occlude()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            // Keep the title and collapse button in one real
                            // horizontal flex row. The region width is owned by
                            // the parent frame, so this row must never infer a
                            // smaller hand-derived width from the title text.
                            .child(self.render_context_sidebar_panel_title(
                                title_key, title_role, icon, cx,
                            ))
                            .child(
                                div()
                                    .id("context-sidebar-collapse")
                                    .flex_none()
                                    .size(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .cursor_pointer()
                                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::PanelRightClose,
                                        self.tokens.metrics.sidebar_collapse_icon_size,
                                        rgb(theme.text_muted),
                                    ))
                                    .on_mouse_move(cx.listener({
                                        let label = self.i18n.t("sidebar.tooltips.collapse");
                                        move |this, event: &MouseMoveEvent, _window, cx| {
                                            this.queue_workspace_tooltip(
                                                "context-sidebar-collapse",
                                                label.clone(),
                                                f32::from(event.position.x) + 12.0,
                                                f32::from(event.position.y) + 16.0,
                                                cx,
                                            );
                                        }
                                    }))
                                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                                        if !*hovered {
                                            this.clear_workspace_tooltip(
                                                "context-sidebar-collapse",
                                                cx,
                                            );
                                        }
                                    }))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.collapse_context_sidebar(cx);
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // Keep the sidebar tint below the titlebar so the
                            // translucent chrome is composited exactly once.
                            .bg(self.workspace_sidebar_background(theme.bg))
                            .child(match self.active_context_sidebar_panel {
                                ContextSidebarPanel::HostTools => {
                                    self.render_host_tools_context_panel(window, cx)
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_context_sidebar_resize_hotzone(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        sidebar_resize_hotzone_chrome(
            "workspace-context-sidebar-resize-hotzone",
            if self.context_sidebar_resizing {
                rgb(theme.accent)
            } else {
                rgba(0x00000000)
            },
            SidebarResizeHotzonePlacement::AfterSeam,
        )
        // The frame begins at the seam, so the handle occupies only sidebar
        // pixels and never covers the terminal content to its left.
        .left_0()
        .top_0()
        .bottom_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                // Double-click snaps the width back to the default, matching
                // the browser split-pane reset affordance.
                if event.click_count >= 2 {
                    this.reset_context_sidebar_width(cx);
                    return;
                }
                this.start_context_sidebar_resize(event, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }),
        )
        .on_hover(cx.listener(|this, hovered, _window, cx| {
            // Both resize handles share the hover flag that drives the
            // window-level column-resize cursor override.
            if this.sidebar_resize_hotzone_hovered != *hovered {
                this.sidebar_resize_hotzone_hovered = *hovered;
                cx.notify();
            }
        }))
        .into_any_element()
    }

    pub(in crate::workspace) fn reset_context_sidebar_width(&mut self, cx: &mut Context<Self>) {
        let trace_id = next_sidebar_resize_trace_id();
        let width_before = self.context_sidebar_width();
        self.context_sidebar_resizing = false;
        self.context_sidebar_resize_trace_id = None;
        // The persisted width is an i64 pixel count, so convert explicitly.
        self.set_context_sidebar_width(oxideterm_settings::AI_SIDEBAR_DEFAULT_WIDTH as f32, cx);
        self.persist_sidebar_settings(cx);
        tracing::info!(
            target: "oxideterm_gpui_app::sidebar_resize",
            trace_id,
            stage = "double-click-reset",
            sidebar = "context",
            width_before,
            width_after = self.context_sidebar_width(),
            result = "completed",
            business_impact = "the context sidebar returned to its default width without occupying adjacent terminal pixels",
            "sidebar resize reset completed"
        );
        cx.notify();
    }

    pub(in crate::workspace) fn reset_primary_sidebar_width(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let trace_id = next_sidebar_resize_trace_id();
        let width_before = self.sidebar_width;
        self.sidebar_resizing = false;
        self.sidebar_resize_trace_id = None;
        // The primary width includes the activity bar; the default metric is
        // the sidebar panel alone.
        let default_total_width =
            self.tokens.metrics.activity_bar_width + self.tokens.metrics.sidebar_default_width;
        self.set_sidebar_width(default_total_width, f32::from(window.viewport_size().width), cx);
        self.persist_sidebar_settings(cx);
        tracing::info!(
            target: "oxideterm_gpui_app::sidebar_resize",
            trace_id,
            stage = "double-click-reset",
            sidebar = "primary",
            width_before,
            width_after = self.sidebar_width,
            result = "completed",
            business_impact = "the primary sidebar returned to its default width while the terminal kept ownership of its first visible pixel",
            "sidebar resize reset completed"
        );
        cx.notify();
    }

    pub(in crate::workspace) fn start_context_sidebar_resize(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let trace_id = next_sidebar_resize_trace_id();
        let width_before = self.context_sidebar_width();
        self.context_sidebar_resize_trace_id = Some(trace_id);
        self.context_sidebar_resizing = true;
        // Snap the width to the pointer on press, mirroring the left sidebar
        // handle so the drag already feels attached at first movement.
        self.set_context_sidebar_width(
            f32::from(window.viewport_size().width) - f32::from(event.position.x),
            cx,
        );
        tracing::info!(
            target: "oxideterm_gpui_app::sidebar_resize",
            trace_id,
            stage = "mouse-press",
            sidebar = "context",
            pointer_x = f32::from(event.position.x),
            viewport_width = f32::from(window.viewport_size().width),
            width_before,
            width_after = self.context_sidebar_width(),
            validation = "pointer press landed inside the context sidebar-owned resize hotzone",
            result = "started",
            business_impact = "the context sidebar began resizing without taking pointer ownership from adjacent terminal content",
            "sidebar resize gesture started"
        );
        cx.notify();
    }

    pub(in crate::workspace) fn update_context_sidebar_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.context_sidebar_resizing {
            return;
        }
        if !event.dragging() {
            // The platform can report the button as released before a mouse-up
            // arrives; treat that as the end of the drag.
            self.finish_context_sidebar_resize(cx);
            return;
        }
        self.set_context_sidebar_width(
            f32::from(window.viewport_size().width) - f32::from(event.position.x),
            cx,
        );
    }

    pub(in crate::workspace) fn finish_context_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.context_sidebar_resizing {
            self.context_sidebar_resizing = false;
            let trace_id = self
                .context_sidebar_resize_trace_id
                .take()
                .unwrap_or_else(next_sidebar_resize_trace_id);
            // The shared persistence path saves the whole sidebar block,
            // including the width this drag just wrote into the store.
            self.persist_sidebar_settings(cx);
            tracing::info!(
                target: "oxideterm_gpui_app::sidebar_resize",
                trace_id,
                stage = "mouse-release",
                sidebar = "context",
                width_after = self.context_sidebar_width(),
                result = "completed",
                business_impact = "the context sidebar resize finished and its final width was submitted to the settings store",
                "sidebar resize gesture finished"
            );
            cx.notify();
        }
    }

    fn set_context_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        // Clamp with the same absolute bounds `context_sidebar_width` applies
        // on read so restored settings and live drags stay within one range.
        let next_width = width
            .clamp(AI_SIDEBAR_ABSOLUTE_MIN_WIDTH, AI_SIDEBAR_ABSOLUTE_MAX_WIDTH)
            .round() as i64;
        let settings = self.settings_store.settings_mut();
        if settings.sidebar_ui.ai_sidebar_width == next_width {
            return;
        }
        settings.sidebar_ui.ai_sidebar_width = next_width;
        cx.notify();
    }

    pub(in crate::workspace) fn render_left_sidebar_resize_hotzone(
        &mut self,
        top_offset: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let seam = self.tokens.metrics.activity_bar_width + self.sidebar_panel_width();
        sidebar_resize_hotzone_chrome(
            "workspace-left-sidebar-resize-hotzone",
            if self.sidebar_resizing {
                rgb(theme.accent)
            } else {
                rgba(0x00000000)
            },
            SidebarResizeHotzonePlacement::BeforeSeam,
        )
        .left(px(sidebar_resize_hotzone_origin(
            seam,
            SidebarResizeHotzonePlacement::BeforeSeam,
        )))
        .top(px(top_offset))
        .bottom_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                // Double-click snaps the primary sidebar back to its default
                // width instead of starting another drag.
                if event.click_count >= 2 {
                    this.reset_primary_sidebar_width(window, cx);
                    return;
                }
                this.start_sidebar_resize(event, window, cx);
                window.prevent_default();
                cx.stop_propagation();
            }),
        )
        .on_hover(cx.listener(|this, hovered, _window, cx| {
            if this.sidebar_resize_hotzone_hovered != *hovered {
                this.sidebar_resize_hotzone_hovered = *hovered;
                cx.notify();
            }
        }))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_context_sidebar_panel_title(
        &self,
        title_key: &'static str,
        title_role: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.render_window_drag_content_region(
            "context-sidebar-titlebar-title",
            div()
                .w_full()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(Self::render_lucide_icon(icon, 16.0, rgb(theme.accent)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "context-sidebar-title",
                            title_role,
                            self.i18n.t(title_key),
                            theme.text,
                            cx,
                        )),
                )
                .into_any_element(),
            cx,
        )
    }

    pub(in crate::workspace) fn render_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(theme.border))
            .child(self.render_sidebar_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    // Keep the body on the lighter sidebar tint while the
                    // fixed header independently matches workspace chrome.
                    .bg(self.workspace_sidebar_background(theme.bg_panel))
                    .child(self.render_sidebar_content(window, cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_sidebar_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let panel_section = self.effective_sidebar_panel_section();
        let title_key = match panel_section {
            _ => "sidebar.panels.sessions",
        };
        let title = self.i18n.t(title_key).to_uppercase();
        let mut header = div()
            // Align sidebar titles with the neighboring workspace tab bar.
            .h(px(self.tokens.metrics.tabbar_height))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            // Use the same image-background opacity as the adjacent tab bar
            // without stacking it over the sidebar body's translucent tint.
            .bg(self.workspace_chrome_background(theme.bg))
            .border_b_1()
            .border_color(rgb(theme.border))
            .px_2()
            .child(
                self.render_window_drag_content_region(
                    "sidebar-header-title-drag-region",
                    div()
                        .flex()
                        .items_center()
                        .truncate()
                        .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "sidebar-header-title",
                            title_key,
                            title,
                            theme.text_muted,
                            cx,
                        ))
                        .into_any_element(),
                    cx,
                ),
            );
        if panel_section == SidebarSection::Sessions {
            header = header
                .child(self.render_sidebar_action(
                    LucideIcon::Folder,
                    SidebarActionKind::NewFolder,
                    cx,
                ))
                .child(self.render_sidebar_action(
                    LucideIcon::Plus,
                    SidebarActionKind::NewConnection,
                    cx,
                ))
                // Session backup/restore and third-party migration live on the
                // active-session sidebar now that the standalone manager is gone.
                .child(self.render_sidebar_action(
                    LucideIcon::Download,
                    SidebarActionKind::ImportConnections,
                    cx,
                ))
                .child(self.render_sidebar_action(
                    LucideIcon::Upload,
                    SidebarActionKind::ExportConnections,
                    cx,
                ));
        }
        header.into_any_element()
    }

    pub(in crate::workspace) fn render_sidebar_action(
        &self,
        icon: LucideIcon,
        action: SidebarActionKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = match action {
            SidebarActionKind::NewFolder => self.i18n.t("sidebar.actions.new_folder"),
            SidebarActionKind::NewConnection => self.i18n.t("sidebar.tooltips.new_connection"),
            SidebarActionKind::ImportConnections => self.i18n.t("sidebar.actions.import_connections"),
            SidebarActionKind::ExportConnections => self.i18n.t("sidebar.actions.export_connections"),
        };

        div()
            .ml_1()
            .child(self.workspace_tooltip_icon_button(
                icon,
                self.tokens.metrics.sidebar_action_icon_size,
                rgb(theme.text),
                IconButtonOptions {
                    has_background: false,
                    background: None,
                    hover_background: Some(rgb(theme.bg_hover)),
                    ..IconButtonOptions::opaque_toolbar(
                        self.tokens.metrics.sidebar_action_size,
                        ButtonRadius::Md,
                    )
                },
                label,
                "sidebar-action",
                false,
                cx.listener(move |this, _event, window, cx| {
                    match action {
                        SidebarActionKind::NewFolder => {
                            this.open_new_session_folder_dialog(window, cx);
                        }
                        SidebarActionKind::NewConnection => {
                            this.open_new_connection_form(window, cx);
                        }
                        SidebarActionKind::ImportConnections => {
                            this.open_oxide_import_dialog(cx);
                        }
                        SidebarActionKind::ExportConnections => {
                            this.open_oxide_export_dialog(cx);
                        }
                    }
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_sidebar_content(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel_section = self.effective_sidebar_panel_section();
        if panel_section == SidebarSection::Sessions {
            // The remote file browser lives in Host Tools now; the sessions
            // sidebar renders as one uninterrupted navigator again.
            return self.render_active_sessions_sidebar_content(cx);
        }
        self.render_empty_sessions_sidebar_content(cx)
    }

    pub(in crate::workspace) fn render_empty_sessions_sidebar_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .px(px(self.tokens.metrics.empty_sidebar_padding_x))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w_full()
                    .h(px(self.tokens.metrics.empty_sidebar_height))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(div().mb_3().child(Self::render_lucide_icon(
                        LucideIcon::Server,
                        self.tokens.metrics.empty_sidebar_icon_size,
                        rgba((theme.text_muted << 8) | 0x4d),
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "sessions-sidebar-empty-title",
                                (),
                                self.i18n.t("sessions.tree.no_sessions"),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .mt_1()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "sessions-sidebar-empty-subtitle",
                                (),
                                self.i18n.t("sessions.tree.click_to_add"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SidebarActionKind {
    NewFolder,
    NewConnection,
    ImportConnections,
    ExportConnections,
}

#[cfg(test)]
mod sidebar_resize_region_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Context, CursorStyle, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render,
        Styled, TestAppContext, Window, canvas, div, px, size,
    };

    struct TestContextSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
        resize_moved: Rc<Cell<bool>>,
        resizing: bool,
    }

    struct TestLeftSidebarChrome {
        total_width: f32,
        resize_started: Rc<Cell<bool>>,
        hotzone_hovered: bool,
    }

    impl Render for TestLeftSidebarChrome {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let resize_started = self.resize_started.clone();
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .w(px(self.total_width))
                        .h_full()
                        .debug_selector(|| "left-frame".to_string())
                        // Simulate loaded sidebar content owning the full visible surface.
                        .child(div().absolute().size_full().occlude())
                        // Simulate a custom-painted child requesting a window-wide cursor.
                        .child(canvas(
                            |_, _, _| (),
                            |_, _, window, _| {
                                window.set_window_cursor_style(CursorStyle::Arrow);
                            },
                        )),
                )
                .child(
                    sidebar_resize_hotzone_chrome(
                        "left-hotzone-element",
                        rgba(0x000000ff),
                        SidebarResizeHotzonePlacement::BeforeSeam,
                    )
                        .left(px(sidebar_resize_hotzone_origin(
                            self.total_width,
                            SidebarResizeHotzonePlacement::BeforeSeam,
                        )))
                        .top_0()
                        .bottom_0()
                        .debug_selector(|| "left-hotzone".to_string())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _event, _window, _cx| {
                                resize_started.set(true);
                            }),
                        )
                        .on_hover(cx.listener(|this, hovered, _window, cx| {
                            if this.hotzone_hovered != *hovered {
                                this.hotzone_hovered = *hovered;
                                cx.notify();
                            }
                        })),
                )
                .when(self.hotzone_hovered, |root| {
                    root.child(canvas(
                        |_, _, _| (),
                        |_, _, window, _| {
                            // The resize handle must beat earlier window-wide cursor requests.
                            window.set_window_cursor_style(CursorStyle::ResizeColumn);
                        },
                    ))
                })
        }
    }

    impl Render for TestContextSidebarChrome {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let resize_started = self.resize_started.clone();
            let resize_moved = self.resize_moved.clone();
            let seam = f32::from(window.viewport_size().width) - self.total_width;
            div()
                .relative()
                .size_full()
                .flex()
                .justify_end()
                .child(
                    context_sidebar_frame_chrome(self.total_width)
                        .debug_selector(|| "context-frame".to_string())
                        .child(
                            context_sidebar_region_chrome()
                                .debug_selector(|| "context-region".to_string())
                                .child(
                                    div()
                                        .size_full()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        // Simulate loaded Host Tools content owning a blocking hitbox.
                                        .child(div().absolute().size_full().occlude())
                                        .child(
                                            div()
                                                .w_full()
                                                .min_w(px(0.0))
                                                .flex_none()
                                                .h(px(42.0))
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .justify_between()
                                                .gap(px(8.0))
                                                .px_3()
                                                .debug_selector(|| "context-titlebar".to_string())
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .flex_1()
                                                        .min_w(px(0.0))
                                                        .debug_selector(|| {
                                                            "context-title-drag".to_string()
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .size(px(28.0))
                                                        .debug_selector(|| {
                                                            "context-collapse".to_string()
                                                        }),
                                                ),
                                        ),
                                ),
                        ),
                )
                .child(
                    sidebar_resize_hotzone_chrome(
                        "context-hotzone-element",
                        rgba(0x000000ff),
                        SidebarResizeHotzonePlacement::AfterSeam,
                    )
                        .left(px(sidebar_resize_hotzone_origin(
                            seam,
                            SidebarResizeHotzonePlacement::AfterSeam,
                        )))
                        .top_0()
                        .bottom_0()
                        .debug_selector(|| "context-hotzone".to_string())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.resizing = true;
                                resize_started.set(true);
                                cx.notify();
                            }),
                        ),
                )
                .when(self.resizing, |root| {
                    root.child(
                        div()
                            .absolute()
                            .size_full()
                            .occlude()
                            .on_mouse_move(cx.listener(
                                move |this, event: &MouseMoveEvent, window, cx| {
                                    // Root capture owns movement after the pointer leaves the hotzone.
                                    this.total_width = (f32::from(window.viewport_size().width)
                                        - f32::from(event.position.x))
                                    .max(0.0);
                                    resize_moved.set(true);
                                    cx.notify();
                                },
                            )),
                    )
                })
        }
    }

    pub(in crate::workspace) fn right_edge(bounds: &gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.origin.x) + f32::from(bounds.size.width)
    }

    pub(in crate::workspace) fn assert_close(label: &str, actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.5,
            "{label}: expected {expected}, got {actual}"
        );
    }

    #[gpui::test]
    pub(in crate::workspace) fn left_sidebar_resize_hotzone_stays_out_of_terminal_content(
        cx: &mut TestAppContext,
    ) {
        let total_width = 280.0;
        let resize_started = Rc::new(Cell::new(false));
        let (_, cx) = cx.add_window_view(|_, _| TestLeftSidebarChrome {
            total_width,
            resize_started: resize_started.clone(),
            hotzone_hovered: false,
        });
        cx.simulate_resize(size(px(700.0), px(180.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let frame = cx.debug_bounds("left-frame").expect("left frame bounds");
        let hotzone = cx
            .debug_bounds("left-hotzone")
            .expect("left hotzone bounds");
        assert_close("left frame width", f32::from(frame.size.width), total_width);
        assert_close(
            "left hotzone width",
            f32::from(hotzone.size.width),
            SIDEBAR_RESIZE_HOTZONE_WIDTH,
        );
        assert_close(
            "left hotzone right edge",
            right_edge(&hotzone),
            right_edge(&frame),
        );

        cx.simulate_mouse_move(
            Point::new(
                frame.origin.x + frame.size.width + px(3.0),
                frame.origin.y + px(20.0),
            ),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::Arrow,
            "the terminal side of the seam should retain its normal cursor"
        );
        cx.simulate_mouse_down(
            Point::new(
                frame.origin.x + frame.size.width + px(3.0),
                frame.origin.y + px(20.0),
            ),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            !resize_started.get(),
            "the first terminal pixels must not start a sidebar resize"
        );

        cx.simulate_mouse_move(
            Point::new(
                frame.origin.x + frame.size.width - px(3.0),
                frame.origin.y + px(20.0),
            ),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::ResizeColumn,
            "hovering inside the sidebar-owned hotzone should apply the column-resize cursor"
        );
        cx.simulate_mouse_down(
            Point::new(
                frame.origin.x + frame.size.width - px(3.0),
                frame.origin.y + px(20.0),
            ),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            resize_started.get(),
            "the sidebar-owned resize hotzone should receive mouse down above loaded content"
        );
    }

    #[gpui::test]
    pub(in crate::workspace) fn context_sidebar_resize_hotzone_has_no_gap_after_content_load(
        cx: &mut TestAppContext,
    ) {
        let total_width = 620.0;
        let resize_started = Rc::new(Cell::new(false));
        let resize_moved = Rc::new(Cell::new(false));

        let (_, cx) = cx.add_window_view(|_, _| TestContextSidebarChrome {
            total_width,
            resize_started: resize_started.clone(),
            resize_moved: resize_moved.clone(),
            resizing: false,
        });
        cx.simulate_resize(size(px(700.0), px(180.0)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let frame = cx.debug_bounds("context-frame").expect("frame bounds");
        let region = cx.debug_bounds("context-region").expect("region bounds");
        let titlebar = cx
            .debug_bounds("context-titlebar")
            .expect("titlebar bounds");
        let collapse = cx
            .debug_bounds("context-collapse")
            .expect("collapse bounds");
        let hotzone = cx.debug_bounds("context-hotzone").expect("hotzone bounds");

        assert_close("frame width", f32::from(frame.size.width), total_width);
        assert_close(
            "region origin",
            f32::from(region.origin.x) - f32::from(frame.origin.x),
            0.0,
        );
        assert_close("region width", f32::from(region.size.width), total_width);
        assert_close(
            "titlebar width",
            f32::from(titlebar.size.width),
            f32::from(region.size.width),
        );

        // The collapse control should be at the right chrome edge, allowing for
        // the titlebar padding. This catches regressions where the titlebar row
        // shrinks to the intrinsic "OxideSens" title width.
        let right_padding = right_edge(&titlebar) - right_edge(&collapse);
        assert_close("collapse right padding", right_padding, 12.0);

        assert_close(
            "hotzone origin",
            f32::from(hotzone.origin.x) - f32::from(frame.origin.x),
            0.0,
        );
        assert_close(
            "hotzone width",
            f32::from(hotzone.size.width),
            SIDEBAR_RESIZE_HOTZONE_WIDTH,
        );

        cx.simulate_mouse_move(
            Point::new(frame.origin.x - px(3.0), frame.origin.y + px(20.0)),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::Arrow,
            "the main-content side of the seam should retain its normal cursor"
        );
        cx.simulate_mouse_down(
            Point::new(frame.origin.x - px(3.0), frame.origin.y + px(20.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            !resize_started.get(),
            "the last main-content pixels must not start a context-sidebar resize"
        );

        cx.simulate_mouse_move(
            Point::new(frame.origin.x + px(3.0), frame.origin.y + px(20.0)),
            None,
            Modifiers::default(),
        );
        assert_eq!(
            cx.update(|window, _cx| window.cursor_style_for_test()),
            CursorStyle::ResizeColumn,
            "hovering the context-sidebar hotzone should apply the column-resize cursor"
        );
        cx.simulate_mouse_down(
            Point::new(frame.origin.x + px(3.0), frame.origin.y + px(20.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        assert!(
            resize_started.get(),
            "frame-local resize hotzone should receive mouse down above loaded content"
        );
        cx.simulate_mouse_move(
            Point::new(frame.origin.x - px(40.0), frame.origin.y + px(20.0)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        assert!(
            resize_moved.get(),
            "root capture should continue the frame-local hotzone drag"
        );
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let resized_frame = cx
            .debug_bounds("context-frame")
            .expect("resized frame bounds");
        assert_close(
            "resized frame width",
            f32::from(resized_frame.size.width),
            660.0,
        );
    }
}
