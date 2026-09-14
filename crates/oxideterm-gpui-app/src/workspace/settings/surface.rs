use super::*;
use crate::workspace::root::init::terminal_highlight_rules;
use crate::workspace::root::init::terminal_preference_overrides;

const SETTINGS_SESSION_IMPORT_SECTION_INDEX: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SettingsNavSelectionMotion {
    duration: Duration,
    spatial: bool,
}

fn settings_nav_item_index(groups: &[Vec<SettingsTab>], tab: SettingsTab) -> Option<usize> {
    let mut item_index = 0;
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            item_index += 1;
        }
        for candidate in group {
            if *candidate == tab {
                return Some(item_index);
            }
            item_index += 1;
        }
    }
    None
}

fn settings_nav_selection_motion(tokens: &ThemeTokens) -> Option<SettingsNavSelectionMotion> {
    oxideterm_gpui_ui::segmented_control_motion(tokens).map(|motion| SettingsNavSelectionMotion {
        duration: motion.duration,
        spatial: motion.spatial,
    })
}

fn settings_nav_vertical_offset(
    scroll_handle: &ScrollHandle,
    source_index: usize,
    target_index: usize,
) -> Option<f32> {
    let source_bounds = scroll_handle.bounds_for_item(source_index)?;
    let target_bounds = scroll_handle.bounds_for_item(target_index)?;
    Some(f32::from(source_bounds.origin.y - target_bounds.origin.y))
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_tab(window, cx);
    }

    pub(in crate::workspace) fn close_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let close_active_settings_tab = self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        self.active_surface = ActiveSurface::Terminal;
        self.terminal_trigger_settings_pane = None;
        self.terminal_trigger_shell_confirmation_pending = false;
        self.clear_terminal_trigger_input_focus();
        self.terminal_triggers.cancel_edit();
        self.close_settings_select();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_navigation_editor(cx);
            settings.close_settings_search(true, cx);
        });
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        if close_active_settings_tab {
            self.close_active_tab(window, cx);
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_settings_background = self.settings_background_active();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(if has_settings_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_settings_nav(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .relative()
                    .child(self.render_settings_section_list_scroll(cx)),
            )
            .when_some(
                self.render_settings_select_overlay(cx),
                |surface, overlay| surface.child(overlay),
            )
            .when_some(
                self.render_cloud_sync_delete_prompt(cx),
                |surface, prompt| surface.child(prompt),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_settings_section_list_scroll(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_settings_section_list_state(cx);
        let state = self.settings_section_list_state.clone();
        let workspace = cx.entity();
        let spec = self.settings_section_list_spec(cx);
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;
        let transition_id = format!("settings-page-{active_tab:?}");
        // All settings pages now share the same variable-height section list.
        // This matches the browser/TanStack virtualizer direction and avoids
        // keeping a full flex tree mounted just because a tab is inside Settings.
        let list = div()
            .id("settings-content-scroll")
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                this.pause_settings_caret_blink_during_scroll(cx);
                // Tauri only closes an open select on page scroll. When no select is
                // visible, keep wheel scrolling free of state writes so large settings
                // pages do not rebuild just to maintain stale overlay anchors.
                if this.open_settings_select.is_some() {
                    this.close_settings_select();
                    this.clear_settings_select_anchors();
                    cx.notify();
                }
            }))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_settings_section_list_item(index, cx)
                    })
                },
            ));
        oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            SharedString::from(transition_id),
            list,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        )
    }

    pub(in crate::workspace) fn render_settings_section_list_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;
        let section_index = index.saturating_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT);
        let child = if index == 0 {
            self.render_settings_virtual_header(active_tab, cx)
        } else {
            self.render_settings_tab_section(active_tab, section_index, cx)
        };

        self.wrap_settings_section_list_item(index, child, cx)
    }

    pub(in crate::workspace) fn wrap_settings_section_list_item(
        &self,
        index: usize,
        child: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let outer_max_width = self.settings_content_outer_max_width();
        let mut inner = div()
            .w_full()
            .min_w(px(0.0))
            .max_w(px(outer_max_width))
            .px(px(padding))
            .pb(px(gap));
        if index == 0 {
            inner = inner.pt(px(padding));
        }
        if index + 1 == self.settings_section_list_item_count(cx) {
            inner = inner.pb(px(padding));
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .justify_center()
            .child(inner.child(child))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_content_outer_max_width(&self) -> f32 {
        // Native keeps Tauri's padded `mx-auto` settings shell, but uses a wider
        // semantic cap so large desktop windows do not leave every page pinned
        // to the original browser `max-w-4xl` column.
        self.tokens.metrics.settings_content_wide_max_width
            + self.tokens.metrics.settings_content_padding * 2.0
    }

    pub(in crate::workspace) fn render_settings_virtual_header(
        &self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(self.render_settings_page_header(tab, cx))
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .into_any_element()
    }

    pub(in crate::workspace) fn sync_settings_section_list_state(&mut self, cx: &App) {
        let spec = self.settings_section_list_spec(cx);
        let identity = self.settings_section_list_identity(cx);
        let signatures = self.settings_section_list_signatures(cx);
        sync_tauri_variable_list_state_by_signatures(
            &self.settings_section_list_state,
            &mut self.settings_section_list_cache.borrow_mut(),
            &identity,
            &signatures,
            spec,
        );
    }

    pub(in crate::workspace) fn settings_section_list_spec(
        &self,
        _cx: &App,
    ) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(SETTINGS_SECTION_LIST_ESTIMATED_HEIGHT),
            SETTINGS_SECTION_LIST_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn settings_section_list_identity(&self, cx: &App) -> String {
        let route = self.settings_workspace.read(cx).route_snapshot();
        settings_model_section_list_identity(route.active_tab, route.terminal_page)
    }

    pub(in crate::workspace) fn settings_section_list_signatures(&self, cx: &App) -> Vec<u64> {
        (0..self.settings_section_list_item_count(cx))
            .map(|index| self.settings_section_signature(index, cx))
            .collect()
    }

    pub(in crate::workspace) fn settings_section_signature(&self, index: usize, cx: &App) -> u64 {
        let mut hasher = DefaultHasher::new();
        // GPUI caches variable-row measurements. Hash only states that can
        // change section height so ListState remeasures affected rows without
        // serializing the entire settings file on every scroll render.
        let route = self.settings_workspace.read(cx).route_snapshot();
        format!("{:?}", route.active_tab).hash(&mut hasher);
        index.hash(&mut hasher);
        let settings = self.settings_store.settings();

        match route.active_tab {
            SettingsTab::General => {
                let launch_at_login = self.settings_workspace.read(cx).launch_at_login_snapshot();
                launch_at_login.enabled.hash(&mut hasher);
                launch_at_login.pending.hash(&mut hasher);
                launch_at_login.error.hash(&mut hasher);
                settings.general.minimize_to_tray_on_close.hash(&mut hasher);
                settings
                    .general
                    .external_connection_uris_enabled
                    .hash(&mut hasher);
            }
            SettingsTab::Terminal => {
                format!("{:?}", route.terminal_page).hash(&mut hasher);
                if settings_terminal_focus_handoff_list_item(route.terminal_page, index) {
                    // Selected chips can change width and wrap this card, but
                    // they must not invalidate measurements for every terminal row.
                    settings
                        .terminal
                        .command_bar
                        .focus_handoff_commands
                        .hash(&mut hasher);
                }
                if route.terminal_page == TerminalSettingsPage::Local {
                    settings.local_terminal.oh_my_posh_enabled.hash(&mut hasher);
                    settings.local_terminal.default_shell_id.hash(&mut hasher);
                    self.local_shells.borrow().len().hash(&mut hasher);
                }
            }
            SettingsTab::Sftp => {
                settings.sftp.speed_limit_enabled.hash(&mut hasher);
            }
            SettingsTab::CloudSync => {}
            SettingsTab::Appearance => {
                // App icon selection only changes paint state. Keeping it out
                // of the height signature prevents scroll anchoring from
                // jumping when the icon picker updates its selected badge.
            }
            SettingsTab::Network => {
                settings.network.upstream_proxy.is_some().hash(&mut hasher);
                settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| matches!(proxy.auth, SettingsUpstreamProxyAuth::Password { .. }))
                    .hash(&mut hasher);
                settings
                    .network
                    .upstream_proxy_disclaimer_accepted
                    .hash(&mut hasher);
                settings.network.application_proxy_mode.hash(&mut hasher);
                settings.general.update_proxy.mode.hash(&mut hasher);
                settings.general.update_proxy.protocol.hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .network_proxy_layout_flags()
                    .hash(&mut hasher);
            }
            SettingsTab::Help => {
                settings.general.update_channel.hash(&mut hasher);
            }
            SettingsTab::Connections => {
                self.connection_store.connections().len().hash(&mut hasher);
                self.connection_store
                    .managed_ssh_keys()
                    .len()
                    .hash(&mut hasher);
                self.settings_workspace
                    .read(cx)
                    .managed_key_status()
                    .is_some()
                    .hash(&mut hasher);
            }
            SettingsTab::SessionIO => {
                if settings_connection_importers_list_item(index) {
                    // Importer state only changes the first Session I/O card.
                    // Invalidating earlier measured rows makes GPUI move the
                    // current scroll anchor.
                    self.settings_workspace
                        .read(cx)
                        .connection_import_list_signature()
                        .hash(&mut hasher);
                }
                self.session_export_format(cx).tag().hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    pub(in crate::workspace) fn settings_section_list_item_count(&self, cx: &App) -> usize {
        let active_tab = self.settings_workspace.read(cx).route_snapshot().active_tab;
        settings_model_section_list_item_count(active_tab, self.settings_dynamic_section_counts(cx))
    }

    pub(in crate::workspace) fn settings_dynamic_section_counts(
        &self,
        cx: &App,
    ) -> SettingsDynamicSectionCounts {
        let route = self.settings_workspace.read(cx).route_snapshot();
        SettingsDynamicSectionCounts {
            terminal_page: route.terminal_page,
        }
    }

    pub(in crate::workspace) fn render_settings_tab_section(
        &mut self,
        tab: SettingsTab,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Section virtualization only pays off if item rendering is lazy.
        // Dispatch by section index instead of constructing the old full
        // settings Vec and discarding every non-visible card.
        match tab {
            SettingsTab::General => self.settings_general_section(section_index, cx),
            SettingsTab::Terminal => self.settings_terminal_section(section_index, cx),
            SettingsTab::Appearance => self.settings_appearance_section(section_index, cx),
            SettingsTab::Connections => self.settings_connections_section(section_index, cx),
            SettingsTab::Network => self.settings_network_section(section_index, cx),
            SettingsTab::Sftp => self.settings_sftp_section(section_index, cx),
            SettingsTab::CloudSync => self.settings_cloud_sync_section(cx),
            SettingsTab::SessionIO => self.settings_session_io_section(section_index, cx),
            SettingsTab::Help => self.settings_help_section(section_index, cx),
        }
    }

    pub(in crate::workspace) fn clear_settings_select_anchors(&mut self) {
        self.select_anchors
            .retain(|id, _| matches!(id, SelectAnchorId::NewConnectionGroup));
    }

    pub(in crate::workspace) fn pause_settings_caret_blink_during_scroll(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.focused_settings_input.is_none() {
            return;
        }
        // Browser caret blinking is compositor-local. Native blinking repaints
        // the workspace, so keep the caret visible while a settings scroll is
        // active and let blinking resume shortly after inertial scrolling stops.
        let pause_until = Instant::now() + Duration::from_millis(SETTINGS_SCROLL_CARET_PAUSE_MS);
        self.workspace_input.update(cx, |input, cx| {
            input.pause_settings_caret_until(pause_until, cx);
        });
    }

    pub(in crate::workspace) fn render_settings_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let settings_search_open = self.settings_workspace.read(cx).settings_search_open();
        let settings_nav_scroll = self.selectable_text_scroll_handle("settings-nav-scroll");
        let settings_nav_width = self.tokens.metrics.settings_nav_width;
        let navigation_layout = SettingsNavigationLayout::from_persisted_groups(
            &self.settings_store.settings().settings_navigation.groups,
        );
        let navigation_groups = navigation_layout.groups();
        let mut nav = div()
            .w(px(settings_nav_width))
            .min_w(px(settings_nav_width))
            .h_full()
            .flex_none()
            // Mirrors Tauri's `min-h-0` settings sidebar contract: the title
            // stays fixed and the tab list owns vertical overflow instead of
            // forcing the sidebar to grow with every added settings category.
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .pb_4()
            .bg(self.settings_panel_background(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border));

        nav = nav.child(
            div()
                .flex_none()
                .h(px(48.0))
                .px(px(20.0))
                .mb(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(theme.border))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text_heading))
                .child(self.i18n.t("settings_view.title"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            self.workspace_tooltip_icon_button(
                                LucideIcon::Search,
                                15.0,
                                rgb(if settings_search_open {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }),
                                IconButtonOptions {
                                    background: settings_search_open
                                        .then(|| self.settings_panel_background(theme.bg_active)),
                                    hover_background: Some(rgb(theme.bg_hover)),
                                    ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                },
                                self.i18n.t("settings_view.search.open"),
                                "settings-search",
                                true,
                                cx.listener(|this, _event, window, cx| {
                                    this.toggle_settings_search(window, cx);
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            ),
                        )
                        .child(self.workspace_tooltip_icon_button(
                            LucideIcon::ListTree,
                            15.0,
                            rgb(theme.text_muted),
                            IconButtonOptions {
                                hover_background: Some(rgb(theme.bg_hover)),
                                ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                            },
                            self.i18n.t("settings_view.navigation_editor.open"),
                            "settings-navigation-editor",
                            true,
                            cx.listener(|this, _event, _window, cx| {
                                this.open_settings_navigation_editor(cx);
                                cx.stop_propagation();
                            }),
                            cx.entity(),
                        )),
                ),
        );

        if settings_search_open {
            return nav
                .child(self.render_settings_search_panel(cx))
                .into_any_element();
        }

        let mut list = div()
            .id("settings-nav-scroll")
            .size_full()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(&settings_nav_scroll)
            .px_3()
            .flex()
            .flex_col();

        for (group_index, group) in navigation_groups.iter().enumerate() {
            if group_index > 0 {
                list = list.child(
                    div()
                        .flex_none()
                        .py_2()
                        .child(separator(&self.tokens, SeparatorOrientation::Horizontal)),
                );
            }
            for tab in group {
                list = list.child(self.render_settings_nav_item(
                    *tab,
                    navigation_groups,
                    &settings_nav_scroll,
                    cx,
                ));
            }
        }

        nav.child(div().flex_1().min_h(px(0.0)).relative().child(list).child(
            selectable_vertical_scrollbar_layer("settings-nav-scrollbar", &settings_nav_scroll),
        ))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_settings_nav_item(
        &self,
        tab: SettingsTab,
        navigation_groups: &[Vec<SettingsTab>],
        settings_nav_scroll: &ScrollHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.settings_workspace.read(cx).route_snapshot().active_tab == tab;
        let nav_item_index = settings_nav_item_index(navigation_groups, tab);
        let navigation_groups_for_click = navigation_groups.to_vec();
        let selection_transition = active.then_some(()).and_then(|()| {
            self.segmented_control_user_transition(
                selection_motion::SETTINGS_NAVIGATION_ID,
                nav_item_index?,
            )
        });
        let transition_scroll_handle = settings_nav_scroll.clone();
        let selection_surface = active.then(|| {
            let surface = div()
                .absolute()
                .inset_0()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(self.settings_panel_background(theme.bg_panel));
            let surface = oxideterm_gpui_ui::theme_card_surface_shadow(surface, &self.tokens);

            let Some((generation, vertical_offset_y)) = selection_transition else {
                return surface.into_any_element();
            };
            let Some(motion) = settings_nav_selection_motion(&self.tokens) else {
                return surface.into_any_element();
            };

            let animation_id = (
                gpui::ElementId::from(selection_motion::SETTINGS_NAVIGATION_ID),
                format!("selection-{generation}"),
            );
            if motion.spatial
                && let Some(vertical_offset_y) = vertical_offset_y
            {
                // The measured item bounds include real group separators and
                // flex growth, so the indicator crosses groups continuously.
                return surface
                    .with_animation(
                        animation_id,
                        Animation::new(motion.duration)
                            .with_easing(oxideterm_gpui_ui::motion::ease_in_out_cubic),
                        move |surface, progress| {
                            let offset =
                                oxideterm_gpui_ui::motion::lerp(vertical_offset_y, 0.0, progress);
                            // Moving both edges preserves the absolute
                            // indicator's height throughout the transition.
                            surface.top(px(offset)).bottom(px(-offset))
                        },
                    )
                    .into_any_element();
            }

            surface
                .with_animation(
                    animation_id,
                    Animation::new(motion.duration)
                        .with_easing(oxideterm_gpui_ui::motion::ease_out_cubic),
                    |surface, progress| surface.opacity(progress),
                )
                .into_any_element()
        });
        div()
            // Keep selection and hover surfaces stable when the settings
            // window has spare vertical space.
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .w_full()
            .flex_none()
            .mb(px(4.0))
            .px_3()
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(self.tokens.radii.md))
            .bg(rgba(0x00000000))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(rgb(if active {
                theme.text_heading
            } else {
                theme.text
            }))
            .cursor_pointer()
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(rgba((theme.bg_hover << 8) | 0x80))
                }
            })
            .when_some(selection_surface, |item, surface| item.child(surface))
            .child(div().flex_none().child(Self::render_lucide_icon(
                settings_tab_lucide(tab.icon()),
                18.0,
                rgb(theme.accent),
            )))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t(tab.label_key())),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let active_tab = this.settings_workspace.read(cx).route_snapshot().active_tab;
                    if active_tab != tab
                        && let Some(source_index) =
                            settings_nav_item_index(&navigation_groups_for_click, active_tab)
                        && let Some(target_index) =
                            settings_nav_item_index(&navigation_groups_for_click, tab)
                    {
                        let vertical_offset_y = settings_nav_vertical_offset(
                            &transition_scroll_handle,
                            source_index,
                            target_index,
                        );
                        this.begin_user_segmented_control_transition_with_vertical_offset(
                            selection_motion::SETTINGS_NAVIGATION_ID,
                            target_index,
                            vertical_offset_y,
                            cx,
                        );
                    }
                    this.settings_workspace
                        .update(cx, |settings, cx| settings.set_active_tab(tab, cx));
                    this.close_settings_select();
                    this.focused_settings_input = None;
                    this.settings_slider_drag = None;
                    this.clear_ime_selection();
                    if tab == SettingsTab::General {
                        #[cfg(not(target_os = "macos"))]
                        this.refresh_launch_at_login_status(cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_settings_page_header(
        &self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.i18n.t(tab.title_key());
        let description = self.i18n.t(tab.description_key());
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(24.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    // Settings headers mirror Tauri block text. Keep the
                    // wrapper full-width so CJK descriptions are never measured
                    // as a one-glyph column by nested flex layout.
                    .line_height(px(30.0))
                    .child(self.render_selectable_text_scoped(
                        "settings-page-title",
                        tab.title_key(),
                        title,
                        self.tokens.ui.text_heading,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px((self.tokens.metrics.ui_text_base + 6.0).max(20.0)))
                    .child(self.render_selectable_text_scoped(
                        "settings-page-description",
                        tab.description_key(),
                        description,
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn edit_settings(
        &mut self,
        edit: impl FnOnce(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) {
        let previous_settings = self.settings_store.settings().clone();
        edit(self.settings_store.settings_mut());
        let settings = self.settings_store.settings().clone();
        crate::logging::audit_settings_change(
            self.settings_store.path(),
            &previous_settings,
            &settings,
            true,
        );
        self.apply_loaded_settings_to_runtime(&settings, cx);
        let _ = self.settings_store.save();
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
        self.sync_tab_titles(cx);
        cx.notify();
    }

    /// Live-editing variant for pointer-driven controls such as slider drags.
    /// Runtime state updates every step, while the disk write coalesces into
    /// one save when the interaction finishes (see finish_settings_slider_drag).
    pub(in crate::workspace) fn edit_settings_deferring_save(
        &mut self,
        edit: impl FnOnce(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) {
        let previous_settings = self.settings_store.settings().clone();
        edit(self.settings_store.settings_mut());
        let settings = self.settings_store.settings().clone();
        crate::logging::audit_settings_change(
            self.settings_store.path(),
            &previous_settings,
            &settings,
            false,
        );
        self.apply_loaded_settings_to_runtime(&settings, cx);
        self.settings_save_pending = true;
        self.sync_tab_titles(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn reload_after_external_sync(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let previous_settings = self.settings_store.settings().clone();
        let settings_path = self.settings_store.path().to_path_buf();
        let connection_path = self.connection_store.path().to_path_buf();
        let next_settings = SettingsStore::load_from_path(settings_path)
            .map_err(|error| format!("Failed to reload settings after external sync: {error}"))?;
        let next_connections = ConnectionStore::load(connection_path).map_err(|error| {
            format!("Failed to reload connections after external sync: {error}")
        })?;
        let settings = next_settings.settings().clone();
        crate::logging::audit_settings_change(
            self.settings_store.path(),
            &previous_settings,
            &settings,
            false,
        );
        self.settings_store = next_settings;
        self.connection_store = next_connections;
        self.sync_ssh_config_sync_service();
        self.settings_workspace.update(cx, |settings, _cx| {
            settings.acknowledge_external_store_state()
        });
        // External sync mutates persisted stores outside the GPUI controls.
        // Re-apply the same runtime side effects used by edit_settings instead
        // of relying on stale in-memory settings or browser-style stores.
        self.apply_loaded_settings_to_runtime(&settings, cx);
        self.sync_tab_titles(cx);
        if previous_settings.appearance.window_opacity != settings.appearance.window_opacity {
            // Detached native windows are separate render roots and need an
            // explicit refresh when CLI or cloud sync changes shared opacity.
            cx.refresh_windows();
        }
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn apply_loaded_settings_to_runtime(
        &mut self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        install_application_proxy_policy_from_settings(settings, &self.connection_store);
        // The debug-logging toggle owns a live log writer plus CPU sampler pair,
        // so it must follow every settings path (checkbox, cloud sync, startup),
        // not only the diagnostics checkbox handler.
        let _ = crate::logging::set_debug_logging(settings.diagnostics.debug_logging);
        crate::app_icon::install_runtime_app_icon(settings.appearance.app_icon);
        let previous_locale = self.i18n.locale();
        self.i18n
            .set_locale(locale_from_settings(settings.general.language));
        if self.i18n.locale() != previous_locale {
            // The native menu bar renders outside GPUI surfaces, so it keeps
            // the previous language until menus are rebuilt on locale change.
            cx.set_menus(crate::platform::app_menus(&self.i18n));
        }
        oxideterm_desktop_presence::set_keep_running_on_close(
            settings.general.minimize_to_tray_on_close,
        );
        let render_policy = compute_render_policy(
            self.render_profile_override
                .unwrap_or(settings.appearance.render_profile),
            &self.detected_graphics,
        );
        // Settings changes can flip the render profile while a modal is open;
        // update the shared backdrop gate before the next top-layer render.
        set_tauri_backdrop_blur_allowed(render_policy.allow_background_blur);
        // Tokens resolve motion through the render policy, so the policy must
        // be recomputed before the theme refresh.
        self.tokens = tokens_from_settings(&settings, &render_policy);
        crate::workspace::detached_tab_window::set_detached_window_bootstrap_background(
            self.tokens.ui.bg,
        );
        self.render_policy = render_policy;
        self.sftp_transfer_manager
            .apply_settings(sftp_runtime_settings_from_settings(&settings));
        if !settings.terminal.command_bar.enabled || !settings.terminal.command_bar.project_tasks {
            // Close stale project task UI when the owning awareness feature is disabled.
            self.close_terminal_project_panel(cx);
        }
        // Working-directory awareness was removed; never leave a picker open.
        self.close_terminal_cwd_picker(cx);
        self.ssh_registry.set_idle_timeout(Some(Duration::from_secs(
            settings.connection_pool.idle_timeout_secs as u64,
        )));

        self.tab_host.update(cx, |tab_host, _cx| {
            tab_host
                .configure_terminal_output_highlight(settings.terminal.highlight_tab_on_new_output);
        });
        // Monitoring settings own recurring remote shells and page-scoped GPU work.
        self.apply_host_tool_monitoring_settings(cx);
        self.sidebar_collapsed = settings.sidebar_ui.collapsed;
        self.sidebar_motion_generation = self.sidebar_motion_generation.wrapping_add(1);
        self.context_sidebar_motion_generation =
            self.context_sidebar_motion_generation.wrapping_add(1);
        self.sidebar_rendered = !settings.sidebar_ui.collapsed;
        self.context_sidebar_rendered = crate::workspace::sidebar::context_sidebar_panel_visible(
            settings.sidebar_ui.ai_sidebar_collapsed,
            settings.sidebar_ui.zen_mode,
        );
        let viewport_width = self.tokens.metrics.window_min_width;
        // External settings reloads use the same responsive limits as pointer
        // resizing, so persisted pixel widths cannot bypass the live viewport.
        self.sidebar_width = crate::workspace::sidebar::clamp_responsive_sidebar_width(
            settings.sidebar_ui.width as f32,
            viewport_width,
            self.tokens.metrics.sidebar_min_width,
            self.tokens.metrics.sidebar_max_width,
        );
        let panes = self
            .tab_host
            .read(cx)
            .panes()
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.clone()))
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            let preferences = self.terminal_preferences_for_pane(pane_id, cx);
            let retained_overrides = pane.read(cx).preference_overrides_snapshot();
            let session_highlight_rule_set_id = pane
                .read(cx)
                .session_highlight_rule_set_id()
                .map(str::to_string);
            let refreshed_session_highlight_override =
                session_highlight_rule_set_id.and_then(|id| {
                    let terminal = &self.settings_store.settings().terminal;
                    let rules = if id == GLOBAL_HIGHLIGHT_RULE_SET_ID {
                        terminal.effective_highlight_rules()
                    } else {
                        &terminal.highlight_rule_set(&id)?.rules
                    };
                    Some(TerminalHighlightRuleSetOverride {
                        id,
                        rules: terminal_highlight_rules(rules),
                    })
                });
            let local_shell_id = retained_overrides.local_shell_id.clone();
            let session_id = self.tabs(cx).iter().find_map(|tab| {
                tab.root_pane
                    .as_ref()
                    .and_then(|root| root.session_id_for_pane(pane_id))
            });
            let ssh_node_id = session_id.and_then(|session_id| {
                self.workspace_runtime
                    .read(cx)
                    .ssh_terminal_node_id(session_id)
            });
            let refreshed_overrides = local_shell_id
                .as_deref()
                .map(|shell_id| {
                    self.terminal_preference_overrides_for_local_shell(Some(&ShellInfo::new(
                        shell_id, shell_id, shell_id,
                    )))
                })
                .or_else(|| {
                    ssh_node_id
                        .as_ref()
                        .map(|node_id| self.terminal_preference_overrides_for_ssh_node(node_id))
                })
                .or_else(|| {
                    let semantic_scheme_id = retained_overrides.semantic_scheme_id.clone();
                    let highlight_rule_set_id = retained_overrides.highlight_rule_set_id.clone();
                    if semantic_scheme_id.is_none() && highlight_rule_set_id.is_none() {
                        return None;
                    }
                    let mut overrides = retained_overrides.clone();
                    let refreshed = terminal_preference_overrides(
                        ConnectionTerminalOptions {
                            semantic_scheme: semantic_scheme_id,
                            highlight_rule_set: highlight_rule_set_id,
                            ..ConnectionTerminalOptions::default()
                        },
                        &self.settings_store.settings().terminal,
                    );
                    overrides.semantic_scheme = refreshed.semantic_scheme;
                    overrides.highlight_rules = refreshed.highlight_rules;
                    Some(overrides)
                });
            let _ = pane.update(cx, |pane, cx| {
                if let Some(overrides) = refreshed_overrides {
                    pane.set_preference_overrides(overrides, preferences.clone(), cx);
                } else {
                    pane.set_preferences(preferences.clone(), cx);
                }
                if pane.session_highlight_rule_set_id().is_some() {
                    pane.set_session_highlight_override(
                        refreshed_session_highlight_override,
                        preferences,
                        cx,
                    );
                }
            });
        }
        self.sync_terminal_command_sender_appearance(cx);
        self.sync_active_terminal_metadata_context(cx);
    }
}

fn settings_connection_importers_list_item(list_index: usize) -> bool {
    list_index
        .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
        .is_some_and(|section_index| section_index == SETTINGS_SESSION_IMPORT_SECTION_INDEX)
}

const SETTINGS_TERMINAL_FOCUS_HANDOFF_SECTION_INDEX: usize = 1;

fn settings_terminal_focus_handoff_list_item(
    terminal_page: TerminalSettingsPage,
    list_index: usize,
) -> bool {
    terminal_page == TerminalSettingsPage::CommandBar
        && list_index
            .checked_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT)
            .is_some_and(|section_index| {
                section_index == SETTINGS_TERMINAL_FOCUS_HANDOFF_SECTION_INDEX
            })
}
