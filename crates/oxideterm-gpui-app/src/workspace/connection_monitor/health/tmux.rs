//! Owns the virtual terminal (Screen / tmux) Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{
    ResourceScreenSession, ResourceScreenSnapshot, ResourceScreenStatus, ResourceTmuxPane,
    ResourceTmuxSession, ResourceTmuxSnapshot, ResourceTmuxStatus, ResourceTmuxWindow,
    ScreenActionKind, ScreenCommandCapability, TmuxActionKind, TmuxCommandCapability,
    VirtualTerminalEngine, build_screen_action_command, build_screen_attach_command,
    build_screen_new_session_command, build_screen_rename_session_command,
    build_screen_send_command, build_tmux_action_command, build_tmux_attach_command,
    build_tmux_new_session_command, build_tmux_rename_session_command,
    build_tmux_rename_window_command, build_tmux_send_pane_command, build_tmux_snapshot_command,
    screen_capture_snapshot, screen_session_row_signature, tmux_capture_snapshot,
    tmux_session_row_signature, visible_screen_session_rows, visible_tmux_session_rows,
};

use oxideterm_gpui_ui::button::ButtonVariant;

impl WorkspaceApp {
    pub(super) fn render_host_tmux_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::TmuxSearch, cx)
            .expect("tmux search is a non-secret Host Tools input");
        // The context sidebar no longer tracks an AI chat width; health panels
        // render with the collapsed layout that width 0 selects.
        let sidebar_width = 0.0;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_tmux_panel(
                search_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_tmux_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::TmuxSearch)
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.host_tools.update(cx, |host_tools, _cx| {
                host_tools.ui.clear_input_focus();
            });
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(in crate::workspace) fn handle_host_tmux_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).tmux_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_tmux_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_tmux_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn handle_host_tmux_input_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).ui.host_tmux_input_dialog.is_none() {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_tmux_input_dialog(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            "enter" => {
                self.submit_host_tmux_input_dialog(cx);
                true
            }
            _ => false,
        }
    }

    pub(super) fn confirm_host_tmux_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_tmux_action_from_view(delay, cx);
        });
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_tmux_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_tmux_confirm_exit(delay, cx)
        })
    }

    pub(super) fn submit_host_tmux_input_dialog(&mut self, cx: &mut Context<Self>) {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.submit_tmux_input_from_view(cx);
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    pub(in crate::workspace) fn render_host_tmux_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (phase, view) = self
            .host_tools
            .read(cx)
            .render_host_tmux_confirm_view(&self.i18n)?;
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "host-tmux-confirm-motion",
                phase,
                view,
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.begin_host_tmux_confirm_exit(cx);
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_tmux_action(cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_tmux_input_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = WorkspaceImeTarget::HostTmuxDialogInput;
        let (kind, session_name, target_label, submit_disabled, input_control) = {
            let host_tools = self.host_tools.read(cx);
            let ui = &host_tools.ui;
            let dialog = ui.host_tmux_input_dialog.as_ref()?;
            let input_control = text_input(
                &self.tokens,
                TextInputView {
                    value: dialog.value.as_str(),
                    placeholder: self.i18n.t(host_tmux_input_placeholder_key(&dialog.kind)),
                    focused: ui.input_is_focused(HostToolsTextInput::TmuxDialog),
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam);
            (
                dialog.kind.clone(),
                dialog.session_name.clone(),
                dialog.target_label.clone(),
                dialog.value.trim().is_empty() || host_tools.tmux_action_running(),
                input_control,
            )
        };
        let submit_label = self.i18n.t(host_tmux_input_submit_key(&kind));
        let workspace = cx.entity();
        let input_control = text_input_anchor_probe(
            target.anchor_id(),
            input_control
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            host_tools.ui.focus_input(HostToolsTextInput::TmuxDialog);
                        });
                        this.ime_marked_text = None;
                        this.show_active_input_caret(cx);
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element();
        let cancel_action = self.workspace_confirm_footer_action_button(
            self.i18n.t("sidebar.host_tmux.confirm.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_tmux_input_dialog(cx);
                });
                this.ime_marked_text = None;
                this.clear_ime_selection();
                cx.notify();
            },
            cx,
        );
        let submit_action = self.workspace_confirm_footer_action_button(
            submit_label,
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            submit_disabled,
            None,
            |this, _event, _window, cx| {
                this.submit_host_tmux_input_dialog(cx);
            },
            cx,
        );
        let dialog = self
            .host_tools
            .read(cx)
            .render_host_tmux_input_dialog_shell(
                kind,
                session_name,
                target_label,
                input_control,
                cancel_action,
                submit_action,
                &self.tokens,
                &self.i18n,
            );

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_tmux_input_dialog(cx);
                        });
                        this.ime_marked_text = None;
                        this.clear_ime_selection();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(dialog))
                .into_any_element(),
        )
    }
}

impl HostToolsEntity {
    fn effective_virtual_terminal_engine(&self, connection_id: &str) -> VirtualTerminalEngine {
        if let Some(&engine) = self.ui.host_virtual_terminal_engine.get(connection_id) {
            return engine;
        }
        let tmux_snap = self.tmux_snapshot_for(connection_id);
        let screen_snap = self.screen_snapshot_for(connection_id);
        let screen_avail = screen_snap
            .as_ref()
            .map(|s| matches!(s.status, ResourceScreenStatus::Available { .. }))
            .unwrap_or(false);
        let tmux_avail = tmux_snap
            .as_ref()
            .map(|s| matches!(s.status, ResourceTmuxStatus::Available { .. }))
            .unwrap_or(false);

        if screen_avail && tmux_avail {
            VirtualTerminalEngine::Screen
        } else if screen_avail {
            VirtualTerminalEngine::Screen
        } else if tmux_avail {
            VirtualTerminalEngine::Tmux
        } else {
            VirtualTerminalEngine::Screen
        }
    }

    fn render_host_vt_engine_switcher(
        &self,
        connection_id: &str,
        active_engine: VirtualTerminalEngine,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cid = connection_id.to_string();
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.render_host_vt_engine_chip(
                VirtualTerminalEngine::Screen,
                active_engine == VirtualTerminalEngine::Screen,
                cid.clone(),
                tokens,
                i18n,
                cx,
            ))
            .child(self.render_host_vt_engine_chip(
                VirtualTerminalEngine::Tmux,
                active_engine == VirtualTerminalEngine::Tmux,
                cid,
                tokens,
                i18n,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_vt_engine_chip(
        &self,
        engine: VirtualTerminalEngine,
        active: bool,
        connection_id: String,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = match engine {
            VirtualTerminalEngine::Screen => {
                i18n.t("sidebar.host_virtual_terminal.engines.screen")
            }
            VirtualTerminalEngine::Tmux => i18n.t("sidebar.host_virtual_terminal.engines.tmux"),
        };
        host_vt_engine_chip(active, tokens)
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.ui
                        .host_virtual_terminal_engine
                        .insert(connection_id.clone(), engine);
                    this.ui
                        .host_virtual_terminal_user_selected
                        .insert(connection_id.clone(), true);
                    this.ui.host_tmux_search_query.clear();
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_tmux_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return host_tools_center_state(
                LucideIcon::WifiOff,
                tokens.ui.text_muted,
                i18n.t("profiler.panel.no_connection"),
                selectable_text,
                cx,
            );
        }

        let selected_connection_id = self.selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let tmux_snapshot = self.tmux_snapshot_for(selected_id);
        let screen_snapshot = self.screen_snapshot_for(selected_id);
        let tmux_rows = tmux_snapshot
            .as_ref()
            .map(|snapshot| visible_tmux_session_rows(snapshot, &self.ui.host_tmux_search_query))
            .unwrap_or_default();
        let tmux_status = tmux_snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        let screen_rows = screen_snapshot
            .as_ref()
            .map(|snapshot| visible_screen_session_rows(snapshot, &self.ui.host_tmux_search_query))
            .unwrap_or_default();
        let screen_status = screen_snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();

        self.sync_host_tmux_list_state(&tmux_rows, tmux_snapshot.as_ref(), selected_id);
        self.sync_host_screen_list_state(&screen_rows, screen_snapshot.as_ref(), selected_id);

        let engine = self.effective_virtual_terminal_engine(selected_id);
        let both_unavailable = matches!(tmux_status, ResourceTmuxStatus::Unavailable)
            && matches!(screen_status, ResourceScreenStatus::Unavailable);

        let visible_count = if engine == VirtualTerminalEngine::Screen {
            screen_rows.len()
        } else {
            tmux_rows.len()
        };

        div()
            .id("host-tmux-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        !self.tmux_snapshot_in_flight(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_vt_engine_switcher(
                        selected_id,
                        engine,
                        tokens,
                        i18n,
                        cx,
                    ))
                    .child(self.render_host_tmux_search(&search_ime, engine, tokens, i18n, cx))
                    .child(self.render_host_tmux_status_row(
                        visible_count,
                        selected_id,
                        engine,
                        &tmux_status,
                        &screen_status,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(if both_unavailable {
                host_tools_center_state(
                    LucideIcon::Terminal,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_virtual_terminal.both_unavailable"),
                    selectable_text,
                    cx,
                )
            } else if engine == VirtualTerminalEngine::Screen {
                self.render_host_screen_list(
                    screen_rows,
                    screen_snapshot,
                    self.tmux_snapshot_in_flight(),
                    screen_status,
                    selected_id,
                    tokens,
                    i18n,
                    mono_font_family,
                    selectable_text,
                    cx,
                )
            } else {
                self.render_host_tmux_list(
                    tmux_rows,
                    tmux_snapshot,
                    self.tmux_snapshot_in_flight(),
                    tmux_status,
                    selected_id,
                    sidebar_width,
                    tokens,
                    i18n,
                    mono_font_family,
                    selectable_text,
                    cx,
                )
            })
            .into_any_element()
    }

    fn render_host_tmux_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        engine: VirtualTerminalEngine,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        let placeholder = match engine {
            VirtualTerminalEngine::Screen => {
                i18n.t("sidebar.host_virtual_terminal.search_screen_placeholder")
            }
            VirtualTerminalEngine::Tmux => i18n.t("sidebar.host_tmux.search_placeholder"),
        };
        text_input_anchor_probe(
            ime.anchor_id(),
            text_input(
                tokens,
                TextInputView {
                    value: &self.ui.host_tmux_search_query,
                    placeholder,
                    focused: self.ui.input_is_focused(input),
                    caret_visible: ime.caret_visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: ime.selected_range(),
                    marked_text: ime.marked_text(),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, event: &MouseDownEvent, window, cx| {
                    host_tools.ui.focus_input(input);
                    // Only pointer metadata crosses to the workspace IME coordinator.
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::BeginPlainTextImeSelection {
                                input,
                                event: event.clone(),
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_tmux_status_row(
        &self,
        visible_count: usize,
        connection_id: &str,
        engine: VirtualTerminalEngine,
        tmux_status: &ResourceTmuxStatus,
        screen_status: &ResourceScreenStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match engine {
            VirtualTerminalEngine::Screen => match screen_status {
                ResourceScreenStatus::Available {
                    capability: ScreenCommandCapability::Full,
                    ..
                } => i18n.t("sidebar.host_tmux.capability.full"),
                ResourceScreenStatus::Available {
                    capability: ScreenCommandCapability::Partial,
                    ..
                } => i18n.t("sidebar.host_tmux.capability.partial"),
                _ => i18n.t("sidebar.host_tmux.capability.unknown"),
            },
            VirtualTerminalEngine::Tmux => match tmux_status {
                ResourceTmuxStatus::Available {
                    capability: TmuxCommandCapability::Full,
                    ..
                } => i18n.t("sidebar.host_tmux.capability.full"),
                ResourceTmuxStatus::Available {
                    capability: TmuxCommandCapability::Partial,
                    ..
                } => i18n.t("sidebar.host_tmux.capability.partial"),
                _ => i18n.t("sidebar.host_tmux.capability.unknown"),
            },
        };
        let (new_session_title, opened_notice) = match engine {
            VirtualTerminalEngine::Screen => {
                let title = i18n.t("sidebar.host_virtual_terminal.new_screen_session_title");
                let name = i18n.t("sidebar.host_virtual_terminal.new_screen_session_name");
                let notice = i18n
                    .t("sidebar.host_virtual_terminal.toast.new_screen_session_opened")
                    .replace("{{name}}", &name);
                (title, notice)
            }
            VirtualTerminalEngine::Tmux => {
                let title = i18n.t("sidebar.host_tmux.new_session_title");
                let name = i18n.t("sidebar.host_tmux.new_session_name");
                let notice = i18n
                    .t("sidebar.host_tmux.toast.new_session_opened")
                    .replace("{{name}}", &name);
                (title, notice)
            }
        };
        let refresh_label = match engine {
            VirtualTerminalEngine::Screen => {
                i18n.t("sidebar.host_virtual_terminal.actions.refresh_screen")
            }
            VirtualTerminalEngine::Tmux => i18n.t("sidebar.host_tmux.actions.refresh"),
        };
        let missing_notice = i18n.t("sidebar.host_tmux.toast.exec_terminal_missing");
        let connection_id_for_terminal = connection_id.to_string();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().min_w_0().flex_1().truncate().child(format!(
                "{} {} · {}",
                visible_count,
                i18n.t("sidebar.host_tmux.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::Plus,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_tmux.actions.new_session"),
                        "host-tmux-new-session",
                        true,
                        cx.listener(move |host_tools, _event, window, cx| {
                            if engine == VirtualTerminalEngine::Screen {
                                host_tools.dispatch_screen_new_session_terminal(
                                    connection_id_for_terminal.clone(),
                                    new_session_title.clone(),
                                    opened_notice.clone(),
                                    missing_notice.clone(),
                                    window,
                                    cx,
                                );
                            } else {
                                host_tools.dispatch_tmux_new_session_terminal(
                                    connection_id_for_terminal.clone(),
                                    new_session_title.clone(),
                                    opened_notice.clone(),
                                    missing_notice.clone(),
                                    window,
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.tmux_snapshot_in_flight(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        refresh_label,
                        "host-tmux-refresh",
                        true,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools
                                .request_active_tool_snapshot(HostSnapshotFeedback::Toast, cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_tmux_list(
        &self,
        rows: Vec<ResourceTmuxSession>,
        snapshot: Option<ResourceTmuxSnapshot>,
        loading: bool,
        status: ResourceTmuxStatus,
        connection_id: &str,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Terminal,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_tmux.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceTmuxStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::Terminal,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_tmux.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceTmuxStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n.t("sidebar.host_tmux.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceTmuxStatus::Unknown | ResourceTmuxStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Terminal,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_tmux.empty"),
                selectable_text,
                cx,
            );
        }

        let snapshot = Arc::new(snapshot.unwrap_or_default());
        let rows = Arc::new(rows);
        let connection_id = Arc::new(connection_id.to_string());
        let state = self.ui.host_tmux_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let tokens = *tokens;
        let i18n = i18n.clone();
        let show_context_columns = sidebar_width >= HOST_TMUX_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_tmux_table_header(show_context_columns, &tokens, &i18n))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let rows = Arc::clone(&rows);
                            let snapshot = Arc::clone(&snapshot);
                            let connection_id = Arc::clone(&connection_id);
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_tmux_row(
                                    connection_id.as_str(),
                                    snapshot.as_ref(),
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    &tokens,
                                    &i18n,
                                    mono_font_family.clone(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_screen_list(
        &self,
        rows: Vec<ResourceScreenSession>,
        snapshot: Option<ResourceScreenSnapshot>,
        loading: bool,
        status: ResourceScreenStatus,
        connection_id: &str,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Terminal,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_virtual_terminal.loading_screen"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceScreenStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::Terminal,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_virtual_terminal.screen_unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceScreenStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n
                        .t("sidebar.host_virtual_terminal.error_screen")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourceScreenStatus::Unknown | ResourceScreenStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Terminal,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_virtual_terminal.screen_empty"),
                selectable_text,
                cx,
            );
        }

        let snapshot = Arc::new(snapshot.unwrap_or_default());
        let rows = Arc::new(rows);
        let connection_id = Arc::new(connection_id.to_string());
        let state = self.ui.host_screen_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let tokens = *tokens;
        let i18n = i18n.clone();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_screen_table_header(&tokens, &i18n))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let rows = Arc::clone(&rows);
                            let snapshot = Arc::clone(&snapshot);
                            let connection_id = Arc::clone(&connection_id);
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_screen_row(
                                    connection_id.as_str(),
                                    snapshot.as_ref(),
                                    rows.get(index).cloned(),
                                    &tokens,
                                    &i18n,
                                    mono_font_family.clone(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_screen_table_header(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_TMUX_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(HOST_PROCESS_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(i18n.t("sidebar.host_tmux.columns.session")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_tmux.columns.attached")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(120.0))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_tmux.columns.created")),
            )
            .into_any_element()
    }

    fn render_host_screen_row(
        &self,
        connection_id: &str,
        _snapshot: &ResourceScreenSnapshot,
        session: Option<ResourceScreenSession>,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = session else {
            return div().into_any_element();
        };
        let theme = tokens.ui;
        let attached_label = if session.attached {
            i18n.t("sidebar.host_tmux.attached.yes")
        } else {
            i18n.t("sidebar.host_tmux.attached.no")
        };
        let screen_row_id = session.id.clone();

        div()
            .id(format!("host-screen-row-{screen_row_id}"))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_TMUX_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(session.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(tmux_attached_color(
                                session.attached,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(attached_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(120.0))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(tmux_time_label(&session.created)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font)
                            .child(session.id.clone()),
                    )
                    .child(self.render_host_screen_inline_actions(
                        connection_id,
                        &session,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_host_screen_inline_actions(
        &self,
        connection_id: &str,
        session: &ResourceScreenSession,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_attach = connection_id.to_string();
        let session_id_for_attach = session.id.clone();
        let connection_id_for_rename = connection_id.to_string();
        let session_id_for_rename = session.id.clone();
        let session_name_for_rename = session.name.clone();
        let connection_id_for_send = connection_id.to_string();
        let session_id_for_send = session.id.clone();
        let session_name_for_send = session.name.clone();
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let missing_notice = i18n.t("sidebar.host_tmux.toast.exec_terminal_missing");
        let attach_title = i18n
            .t("sidebar.host_virtual_terminal.screen_attach_title")
            .replace("{{name}}", &session.name);
        let opened_notice = i18n
            .t("sidebar.host_virtual_terminal.toast.screen_attach_opened")
            .replace("{{name}}", &session.name);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Terminal,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.attach"),
                "host-screen-attach",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.dispatch_screen_attach_terminal(
                        connection_id_for_attach.clone(),
                        session_id_for_attach.clone(),
                        attach_title.clone(),
                        opened_notice.clone(),
                        missing_notice.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Keyboard,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.send_command"),
                "host-screen-send",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_send.clone(),
                            session_id: session_id_for_send.clone(),
                            session_name: session_name_for_send.clone(),
                            target_label: session_name_for_send.clone(),
                            value: zeroize::Zeroizing::new(String::new()),
                            kind: HostTmuxInputDialogKind::SendScreenCommand {
                                target: session_id_for_send.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Pencil,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.rename_session"),
                "host-screen-rename",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_rename.clone(),
                            session_id: session_id_for_rename.clone(),
                            session_name: session_name_for_rename.clone(),
                            target_label: session_name_for_rename.clone(),
                            value: zeroize::Zeroizing::new(session_name_for_rename.clone()),
                            kind: HostTmuxInputDialogKind::RenameScreenSession {
                                target: session_id_for_rename.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Trash2,
                12.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_session"),
                "host-screen-kill",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: session_name_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillScreenSession {
                                target: session_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_confirm_view(
        &self,
        i18n: &I18n,
    ) -> Option<(oxideterm_gpui_ui::motion::ExitPhase, ConfirmDialogView)> {
        let (request, phase) = self.tmux_confirm_view()?;
        let title_key = match request.action {
            HostTmuxDestructiveAction::KillScreenSession { .. } => {
                "sidebar.host_virtual_terminal.confirm_screen_action_title"
            }
            _ => "sidebar.host_tmux.confirm.title",
        };
        let description = i18n
            .t(host_tmux_confirm_description_key(&request.action))
            .replace("{{name}}", &request.session_name)
            .replace("{{id}}", &request.session_id)
            .replace("{{target}}", &request.target_label);
        Some((
            phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(i18n.t(title_key)).into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(i18n.t("sidebar.host_tmux.confirm.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(i18n.t(host_tmux_confirm_label_key(&request.action)))
                    .into_any_element(),
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_tmux_input_dialog_shell(
        &self,
        kind: HostTmuxInputDialogKind,
        session_name: String,
        target_label: String,
        input_control: AnyElement,
        cancel_action: gpui::Div,
        submit_action: gpui::Div,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> gpui::Div {
        let theme = tokens.ui;
        let description = i18n
            .t(host_tmux_input_description_key(&kind))
            .replace("{{name}}", &session_name)
            .replace("{{target}}", &target_label);
        // The secret-bearing input stays materialized by WorkspaceApp; the Entity
        // receives only the element and safe labels, never a cloned input value.
        oxideterm_gpui_ui::modal::dialog_content(tokens)
            .w(px(HOST_TMUX_INPUT_DIALOG_WIDTH))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(i18n.t(host_tmux_input_title_key(&kind))),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(description),
                    ),
            )
            .child(div().px_4().py_4().child(input_control))
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(cancel_action)
                    .child(submit_action),
            )
    }

    fn render_host_tmux_table_header(
        &self,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_TMUX_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(HOST_PROCESS_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(i18n.t("sidebar.host_tmux.columns.session")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                    .child(i18n.t("sidebar.host_tmux.columns.attached")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_WINDOWS_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_tmux.columns.windows")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_TMUX_PANES_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_tmux.columns.panes")),
            )
            .when(show_context_columns, |header| {
                header.child(
                    div()
                        .flex_none()
                        .w(px(HOST_TMUX_ACTIVITY_COLUMN_WIDTH))
                        .truncate()
                        .child(i18n.t("sidebar.host_tmux.columns.activity")),
                )
            })
            .into_any_element()
    }

    fn render_host_tmux_row(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: Option<ResourceTmuxSession>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = session else {
            return div().into_any_element();
        };
        let expanded =
            self.ui.host_tmux_expanded_session_id.as_deref() == Some(session.id.as_str());
        let theme = tokens.ui;
        let pane_count = snapshot.pane_count_for_session(&session.id);
        let attached_label = if session.attached {
            i18n.t("sidebar.host_tmux.attached.yes")
        } else {
            i18n.t("sidebar.host_tmux.attached.no")
        };
        let session_id = session.id.clone();

        div()
            .id(format!("host-tmux-row-{session_id}"))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_TMUX_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep identity at the first flex level so narrow sidebars preserve names.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(session.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_ATTACHED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(tmux_attached_color(
                                session.attached,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(attached_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_WINDOWS_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(session.windows.to_string()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_TMUX_PANES_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(pane_count.to_string()),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_TMUX_ACTIVITY_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(tmux_time_label(&session.activity)),
                        )
                    }),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(format!(
                                "{} · {}",
                                session.id,
                                Self::active_tmux_window_label(snapshot, &session.id, i18n)
                            )),
                    )
                    .child(self.render_host_tmux_inline_actions(
                        connection_id,
                        &session,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(self.render_host_tmux_session_detail(
                    connection_id,
                    snapshot,
                    &session,
                    tokens,
                    i18n,
                    mono_font,
                    cx,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Expansion is page-owned view state and never re-enters WorkspaceApp.
                    if host_tools.ui.host_tmux_expanded_session_id.as_deref()
                        == Some(session_id.as_str())
                    {
                        host_tools.ui.host_tmux_expanded_session_id = None;
                    } else {
                        host_tools.ui.host_tmux_expanded_session_id = Some(session_id.clone());
                    }
                    host_tools.ui.host_tmux_expanded_window_id = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_tmux_inline_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_attach = connection_id.to_string();
        let session_id_for_attach = session.id.clone();
        let connection_id_for_rename = connection_id.to_string();
        let session_id_for_rename = session.id.clone();
        let session_name_for_rename = session.name.clone();
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let missing_notice = i18n.t("sidebar.host_tmux.toast.exec_terminal_missing");
        let attach_title = i18n
            .t("sidebar.host_tmux.attach_title")
            .replace("{{name}}", &session.name);
        let opened_notice = i18n
            .t("sidebar.host_tmux.toast.attach_opened")
            .replace("{{name}}", &session.name);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Terminal,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.attach"),
                "host-tmux-attach",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.dispatch_tmux_attach_terminal(
                        connection_id_for_attach.clone(),
                        session_id_for_attach.clone(),
                        attach_title.clone(),
                        opened_notice.clone(),
                        missing_notice.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Pencil,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.rename_session"),
                "host-tmux-rename-session",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_rename.clone(),
                            session_id: session_id_for_rename.clone(),
                            session_name: session_name_for_rename.clone(),
                            target_label: session_name_for_rename.clone(),
                            value: zeroize::Zeroizing::new(session_name_for_rename.clone()),
                            kind: HostTmuxInputDialogKind::RenameSession {
                                target: session_id_for_rename.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Trash2,
                13.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_session"),
                "host-tmux-kill-session",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: session_name_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillSession {
                                target: session_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_window_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        tmux_window: &ResourceTmuxWindow,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_rename = connection_id.to_string();
        let session_id_for_rename = session.id.clone();
        let session_name_for_rename = session.name.clone();
        let window_id_for_rename = tmux_window.id.clone();
        let window_label_for_rename = format!("#{} {}", tmux_window.index, tmux_window.name);
        let window_name_for_rename = tmux_window.name.clone();
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let window_id_for_kill = tmux_window.id.clone();
        let window_label_for_kill = format!("#{} {}", tmux_window.index, tmux_window.name);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Pencil,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.rename_window"),
                "host-tmux-rename-window",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_rename.clone(),
                            session_id: session_id_for_rename.clone(),
                            session_name: session_name_for_rename.clone(),
                            target_label: window_label_for_rename.clone(),
                            value: zeroize::Zeroizing::new(window_name_for_rename.clone()),
                            kind: HostTmuxInputDialogKind::RenameWindow {
                                target: window_id_for_rename.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Trash2,
                12.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_window"),
                "host-tmux-kill-window",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: window_label_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillWindow {
                                target: window_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_pane_actions(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        pane: &ResourceTmuxPane,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let is_running = self.tmux_action_running_for(&session.id);
        let connection_id_for_command = connection_id.to_string();
        let session_id_for_command = session.id.clone();
        let session_name_for_command = session.name.clone();
        let pane_id_for_command = pane.id.clone();
        let pane_label_for_command = format!("%{} {}", pane.index, pane.command);
        let connection_id_for_kill = connection_id.to_string();
        let session_id_for_kill = session.id.clone();
        let session_name_for_kill = session.name.clone();
        let pane_id_for_kill = pane.id.clone();
        let pane_label_for_kill = format!("%{} {}", pane.index, pane.command);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Keyboard,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.send_command"),
                "host-tmux-send-pane-command",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_input_from_view(
                        HostTmuxInputDialog {
                            connection_id: connection_id_for_command.clone(),
                            session_id: session_id_for_command.clone(),
                            session_name: session_name_for_command.clone(),
                            target_label: pane_label_for_command.clone(),
                            value: zeroize::Zeroizing::new(String::new()),
                            kind: HostTmuxInputDialogKind::SendPaneCommand {
                                target: pane_id_for_command.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::Trash2,
                12.0,
                rgb(MONITOR_RED),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 20.0,
                    disabled: is_running,
                    has_background: true,
                    background: Some(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)),
                    hover_background: Some(rgba((MONITOR_RED << 8) | 0x30)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(20.0)
                },
                i18n.t("sidebar.host_tmux.actions.kill_pane"),
                "host-tmux-kill-pane",
                true,
                cx.listener(move |host_tools, _event, window, cx| {
                    host_tools.open_tmux_confirm_from_view(
                        HostTmuxActionRequest {
                            connection_id: connection_id_for_kill.clone(),
                            session_id: session_id_for_kill.clone(),
                            session_name: session_name_for_kill.clone(),
                            target_label: pane_label_for_kill.clone(),
                            action: HostTmuxDestructiveAction::KillPane {
                                target: pane_id_for_kill.clone(),
                            },
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn render_host_tmux_session_detail(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: &ResourceTmuxSession,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let mut detail = div()
            .px_3()
            .pb_3()
            .pt_2()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(Self::render_host_tmux_detail_line(
                i18n.t("sidebar.host_tmux.columns.created"),
                tmux_time_label(&session.created),
            ))
            .child(Self::render_host_tmux_detail_line(
                i18n.t("sidebar.host_tmux.columns.activity"),
                tmux_time_label(&session.activity),
            ));
        // Child rows and their actions are materialized only after the session expands.
        for window in snapshot.windows_for_session(&session.id) {
            detail = detail.child(self.render_host_tmux_window_detail(
                connection_id,
                snapshot,
                session,
                window,
                tokens,
                i18n,
                mono_font.clone(),
                cx,
            ));
        }
        detail.into_any_element()
    }

    fn render_host_tmux_window_detail(
        &self,
        connection_id: &str,
        snapshot: &ResourceTmuxSnapshot,
        session: &ResourceTmuxSession,
        window: ResourceTmuxWindow,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let expanded = self.ui.host_tmux_expanded_window_id.as_deref() == Some(window.id.as_str());
        let window_id = window.id.clone();
        div()
            .mt_1()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .overflow_hidden()
            .child(
                div()
                    .id(format!("host-tmux-window-row-{}", window.id))
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.bg_hover)))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .font_family(mono_font.clone())
                            .text_color(rgb(if window.active {
                                theme.text
                            } else {
                                theme.text_muted
                            }))
                            .child(format!("#{} {}", window.index, window.name)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{} {}",
                                window.panes,
                                i18n.t("sidebar.host_tmux.columns.panes")
                            )),
                    )
                    .child(self.render_host_tmux_window_actions(
                        connection_id,
                        session,
                        &window,
                        tokens,
                        i18n,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            // Window disclosure state belongs to the Host Tools page entity.
                            if host_tools.ui.host_tmux_expanded_window_id.as_deref()
                                == Some(window_id.as_str())
                            {
                                host_tools.ui.host_tmux_expanded_window_id = None;
                            } else {
                                host_tools.ui.host_tmux_expanded_window_id =
                                    Some(window_id.clone());
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when(expanded, |card| {
                let mut body = div()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA));
                // Pane rows stay lazy so collapsed windows do not allocate action closures.
                for pane in snapshot.panes_for_window(&window.id) {
                    body = body.child(self.render_host_tmux_pane_detail(
                        connection_id,
                        session,
                        pane,
                        tokens,
                        i18n,
                        mono_font.clone(),
                        cx,
                    ));
                }
                card.child(body)
            })
            .into_any_element()
    }

    fn render_host_tmux_pane_detail(
        &self,
        connection_id: &str,
        session: &ResourceTmuxSession,
        pane: ResourceTmuxPane,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .font_family(mono_font)
            .child(
                div()
                    .flex_none()
                    .w(px(42.0))
                    .text_color(rgb(if pane.active {
                        MONITOR_EMERALD
                    } else {
                        theme.text_muted
                    }))
                    .child(format!("%{}", pane.index)),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(theme.text))
                    .child(format!("{} · {}", pane.command, pane.path)),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{} · {}", pane.pid, pane.size)),
            )
            .child(self.render_host_tmux_pane_actions(
                connection_id,
                session,
                &pane,
                tokens,
                i18n,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_tmux_detail_line(label: String, value: String) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_none().child(label))
            .child(div().min_w_0().flex_1().truncate().child(value))
            .into_any_element()
    }

    fn active_tmux_window_label(
        snapshot: &ResourceTmuxSnapshot,
        session_id: &str,
        i18n: &I18n,
    ) -> String {
        snapshot
            .windows
            .iter()
            .find(|window| window.session_id == session_id && window.active)
            .map(|window| {
                i18n.t("sidebar.host_tmux.active_window")
                    .replace("{{name}}", &window.name)
                    .replace("{{index}}", &window.index.to_string())
            })
            .unwrap_or_else(|| i18n.t("sidebar.host_tmux.no_active_window"))
    }

    fn sync_host_tmux_list_state(
        &self,
        rows: &[ResourceTmuxSession],
        snapshot: Option<&ResourceTmuxSnapshot>,
        selected_id: &str,
    ) {
        let ui = &self.ui;
        let signatures = rows
            .iter()
            .map(|session| {
                let expanded =
                    ui.host_tmux_expanded_session_id.as_deref() == Some(session.id.as_str());
                let child_count = if expanded {
                    let window_count = snapshot
                        .map(|snapshot| snapshot.windows_for_session(&session.id).len())
                        .unwrap_or_default();
                    let pane_count = ui
                        .host_tmux_expanded_window_id
                        .as_deref()
                        .and_then(|window_id| {
                            snapshot.map(|snapshot| snapshot.panes_for_window(window_id).len())
                        })
                        .unwrap_or_default();
                    window_count + pane_count
                } else {
                    0
                };
                tmux_session_row_signature(session, expanded, child_count)
            })
            .collect::<Vec<_>>();
        let identity = format!(
            "host-tmux:{selected_id}:{}:{}:{}",
            ui.host_tmux_search_query,
            ui.host_tmux_expanded_session_id
                .as_deref()
                .unwrap_or_default(),
            ui.host_tmux_expanded_window_id
                .as_deref()
                .unwrap_or_default()
        );
        sync_tauri_variable_list_state_by_signatures(
            &ui.host_tmux_list_state,
            &mut ui.host_tmux_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    fn sync_host_screen_list_state(
        &self,
        rows: &[ResourceScreenSession],
        _snapshot: Option<&ResourceScreenSnapshot>,
        selected_id: &str,
    ) {
        let ui = &self.ui;
        let signatures = rows
            .iter()
            .map(screen_session_row_signature)
            .collect::<Vec<_>>();
        let identity = format!("host-screen:{selected_id}:{}", ui.host_tmux_search_query);
        sync_tauri_variable_list_state_by_signatures(
            &ui.host_screen_list_state,
            &mut ui.host_screen_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_TMUX_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    fn dispatch_screen_attach_terminal(
        &self,
        connection_id: String,
        session_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.screen_attach_command(&connection_id, &session_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::TmuxActionFailed,
                ));
                return;
            }
        };
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    fn dispatch_screen_new_session_terminal(
        &self,
        connection_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.screen_new_session_command(&connection_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::TmuxActionFailed,
                ));
                return;
            }
        };
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    pub(super) fn screen_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceScreenSnapshot> {
        self.host_tmux.screen_snapshots.get(connection_id).cloned()
    }

    pub(super) fn screen_attach_command(
        &self,
        connection_id: &str,
        target: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_screen_attach_command(&os_type, target)
    }

    pub(super) fn screen_new_session_command(
        &self,
        connection_id: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_screen_new_session_command(&os_type, None)
    }

    fn dispatch_tmux_attach_terminal(
        &self,
        connection_id: String,
        session_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.tmux_attach_command(&connection_id, &session_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::TmuxActionFailed,
                ));
                return;
            }
        };
        // The builder is the only source of terminal commands for this action.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    fn dispatch_tmux_new_session_terminal(
        &self,
        connection_id: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.tmux_new_session_command(&connection_id) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::TmuxActionFailed,
                ));
                return;
            }
        };
        // The builder is the only source of terminal commands for this action.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }

    fn open_tmux_confirm_from_view(
        &mut self,
        request: HostTmuxActionRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(notice) = self.open_tmux_action_confirm(request, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
            return;
        }
        // The root owns only the shared focus adapter, not the confirm state.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::PrepareTmuxConfirm,
            )),
            cx,
        );
    }

    fn open_tmux_input_from_view(
        &mut self,
        dialog: HostTmuxInputDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The zeroizing dialog remains the sole owner of its secret-capable input.
        self.open_tmux_input_dialog(dialog, cx);
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::PrepareTmuxInputDialog,
            )),
            cx,
        );
    }

    pub(super) fn tmux_snapshot_for(&self, connection_id: &str) -> Option<ResourceTmuxSnapshot> {
        self.host_tmux.snapshots.get(connection_id).cloned()
    }

    pub(super) fn tmux_snapshot_in_flight(&self) -> bool {
        self.host_tmux.snapshot_in_flight
    }

    pub(super) fn tmux_action_running_for(&self, session_id: &str) -> bool {
        self.host_tmux
            .action_running
            .as_ref()
            .is_some_and(|request| request.session_id == session_id)
    }

    pub(super) fn tmux_action_running(&self) -> bool {
        self.host_tmux.action_running.is_some()
    }

    pub(super) fn tmux_attach_command(
        &self,
        connection_id: &str,
        target: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_attach_command(&os_type, target)
    }

    pub(super) fn tmux_new_session_command(&self, connection_id: &str) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_tmux_new_session_command(&os_type, None)
    }

    pub(in crate::workspace::connection_monitor) fn request_tmux_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        search_query: String,
        failure_fallback: String,
        unavailable_fallback: String,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.snapshot_in_flight {
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxSnapshotAlreadyRunning]
            } else {
                Vec::new()
            };
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxConnectionMissing]
            } else {
                Vec::new()
            };
        };
        let command = build_tmux_snapshot_command(&os_type);
        let request = HostTmuxSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            search_query,
            failure_fallback,
            unavailable_fallback,
        };
        self.host_tmux.snapshot_running = Some(request.clone());
        self.host_tmux.snapshot_in_flight = true;
        self.host_tmux.last_error = None;
        let spawned = self.spawn_tmux_snapshot_capture(
            command.command,
            request,
            HOST_TMUX_SNAPSHOT_TIMEOUT,
            HOST_TMUX_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_tmux.snapshot_running = None;
            self.host_tmux.snapshot_in_flight = false;
            return if feedback.should_toast() {
                vec![HostToolsNotice::TmuxConnectionMissing]
            } else {
                Vec::new()
            };
        }
        cx.notify();
        Vec::new()
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_tmux_snapshot(
        &mut self,
        delivery: HostTmuxSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.snapshot_running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        self.host_tmux.snapshot_in_flight = false;
        self.host_tmux.snapshot_running = None;
        let ((tmux_snapshot, screen_snapshot), notice) = match delivery.result {
            Ok(mut output) => {
                let mut tmux_snap =
                    tmux_capture_snapshot(&output.stdout, &output.stderr, output.exit_code);
                let mut screen_snap =
                    screen_capture_snapshot(&output.stdout, &output.stderr, output.exit_code);
                zeroize::Zeroize::zeroize(&mut output.stdout);
                zeroize::Zeroize::zeroize(&mut output.stderr);

                // Both capture parsers embed remote stdout/stderr into Error
                // messages. Replace every raw message with the localized
                // fallback regardless of which engine is displayed, so the
                // frozen snapshot for the inactive engine stays redacted too.
                if matches!(tmux_snap.status, ResourceTmuxStatus::Error { .. }) {
                    tmux_snap.status = ResourceTmuxStatus::Error {
                        message: delivery.request.failure_fallback.clone(),
                    };
                }
                if matches!(screen_snap.status, ResourceScreenStatus::Error { .. }) {
                    screen_snap.status = ResourceScreenStatus::Error {
                        message: delivery.request.failure_fallback.clone(),
                    };
                }

                let connection_id = delivery.request.connection_id.as_str();
                let user_selected = self
                    .ui
                    .host_virtual_terminal_user_selected
                    .get(connection_id)
                    .copied()
                    .unwrap_or(false);
                if !user_selected {
                    let screen_avail = matches!(screen_snap.status, ResourceScreenStatus::Available { .. });
                    let tmux_avail = matches!(tmux_snap.status, ResourceTmuxStatus::Available { .. });
                    if screen_avail && tmux_avail {
                        self.ui.host_virtual_terminal_engine.insert(
                            connection_id.to_string(),
                            VirtualTerminalEngine::Screen,
                        );
                    } else if screen_avail {
                        self.ui.host_virtual_terminal_engine.insert(
                            connection_id.to_string(),
                            VirtualTerminalEngine::Screen,
                        );
                    } else if tmux_avail {
                        self.ui.host_virtual_terminal_engine.insert(
                            connection_id.to_string(),
                            VirtualTerminalEngine::Tmux,
                        );
                    }
                }

                let engine = self.effective_virtual_terminal_engine(connection_id);
                let notice = match engine {
                    VirtualTerminalEngine::Screen => match screen_snap.status.clone() {
                        ResourceScreenStatus::Available { .. } => {
                            self.host_tmux.last_error = None;
                            Some(HostToolsNotice::TmuxSnapshotLoaded {
                                count: visible_screen_session_rows(
                                    &screen_snap,
                                    &delivery.request.search_query,
                                )
                                .len(),
                            })
                        }
                        ResourceScreenStatus::Unavailable => {
                            if matches!(tmux_snap.status, ResourceTmuxStatus::Unavailable) {
                                self.host_tmux.last_error =
                                    Some(delivery.request.unavailable_fallback.clone());
                                Some(HostToolsNotice::TmuxUnavailable)
                            } else {
                                None
                            }
                        }
                        ResourceScreenStatus::Error { .. } => {
                            self.host_tmux.last_error =
                                Some(delivery.request.failure_fallback.clone());
                            Some(HostToolsNotice::TmuxSnapshotFailed)
                        }
                        ResourceScreenStatus::Unknown => None,
                    },
                    VirtualTerminalEngine::Tmux => match tmux_snap.status.clone() {
                        ResourceTmuxStatus::Available { .. } => {
                            self.host_tmux.last_error = None;
                            Some(HostToolsNotice::TmuxSnapshotLoaded {
                                count: visible_tmux_session_rows(
                                    &tmux_snap,
                                    &delivery.request.search_query,
                                )
                                .len(),
                            })
                        }
                        ResourceTmuxStatus::Unavailable => {
                            if matches!(screen_snap.status, ResourceScreenStatus::Unavailable) {
                                self.host_tmux.last_error =
                                    Some(delivery.request.unavailable_fallback.clone());
                                Some(HostToolsNotice::TmuxUnavailable)
                            } else {
                                None
                            }
                        }
                        ResourceTmuxStatus::Error { .. } => {
                            self.host_tmux.last_error =
                                Some(delivery.request.failure_fallback.clone());
                            Some(HostToolsNotice::TmuxSnapshotFailed)
                        }
                        ResourceTmuxStatus::Unknown => None,
                    },
                };
                ((tmux_snap, screen_snap), notice)
            }
            Err(()) => {
                self.host_tmux.last_error = Some(delivery.request.failure_fallback.clone());
                (
                    (
                        ResourceTmuxSnapshot {
                            status: ResourceTmuxStatus::Error {
                                message: delivery.request.failure_fallback.clone(),
                            },
                            sessions: Vec::new(),
                            windows: Vec::new(),
                            panes: Vec::new(),
                        },
                        ResourceScreenSnapshot {
                            status: ResourceScreenStatus::Error {
                                message: delivery.request.failure_fallback.clone(),
                            },
                            sessions: Vec::new(),
                        },
                    ),
                    Some(HostToolsNotice::TmuxSnapshotFailed),
                )
            }
        };
        // Store per host: a capture finishing for an inactive host refreshes
        // only that host's frozen slot and never disturbs the visible one.
        self.host_tmux
            .snapshots
            .insert(delivery.request.connection_id.clone(), tmux_snapshot);
        self.host_tmux
            .screen_snapshots
            .insert(delivery.request.connection_id, screen_snapshot);
        if feedback.should_toast()
            && let Some(notice) = notice
        {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn open_tmux_action_confirm(
        &mut self,
        request: HostTmuxActionRequest,
        cx: &mut Context<Self>,
    ) -> Option<HostToolsNotice> {
        if self.host_tmux.action_running.is_some() {
            return Some(HostToolsNotice::TmuxActionAlreadyRunning);
        }
        HostToolConfirmState::open(&mut self.host_tmux.pending_confirm, request);
        cx.notify();
        None
    }

    pub(in crate::workspace::connection_monitor) fn tmux_confirm_view(
        &self,
    ) -> Option<(HostTmuxActionRequest, oxideterm_gpui_ui::motion::ExitPhase)> {
        self.host_tmux
            .pending_confirm
            .as_ref()
            .map(|state| (state.request.clone(), state.presence.phase()))
    }

    pub(in crate::workspace::connection_monitor) fn dismiss_tmux_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.pending_confirm.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn begin_tmux_confirm_exit(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self
            .host_tmux
            .pending_confirm
            .as_mut()
            .and_then(|state| state.presence.begin_exit())
        else {
            return false;
        };
        if delay.is_zero() {
            self.host_tmux.pending_confirm = None;
            cx.notify();
            return true;
        }
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |entity, cx| {
                if entity
                    .host_tmux
                    .pending_confirm
                    .as_ref()
                    .is_some_and(|state| state.presence.finish_exit(generation))
                {
                    entity.host_tmux.pending_confirm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
        true
    }

    pub(super) fn confirm_tmux_action(
        &mut self,
        delay: Duration,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let Some(request) = self
            .host_tmux
            .pending_confirm
            .as_ref()
            .map(|state| state.request.clone())
        else {
            return Vec::new();
        };
        if !self.begin_tmux_confirm_exit(delay, cx) {
            return Vec::new();
        }
        self.start_tmux_action(request, runtime, cx)
    }

    fn confirm_tmux_action_from_view(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::TmuxConnectionMissing,
            ));
            return;
        };
        for notice in self.confirm_tmux_action(delay, runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(in crate::workspace::connection_monitor) fn start_tmux_action(
        &mut self,
        request: HostTmuxActionRequest,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        let HostTmuxActionRequest {
            connection_id,
            session_id,
            session_name,
            target_label,
            action,
        } = request;
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return vec![HostToolsNotice::TmuxConnectionMissing];
        };
        let action_command = match action {
            HostTmuxDestructiveAction::KillSession { target } => {
                let action = TmuxActionKind::KillSession { target };
                build_tmux_action_command(&os_type, action)
                    .map(|c| zeroize::Zeroizing::new(c.command))
            }
            HostTmuxDestructiveAction::KillWindow { target } => {
                let action = TmuxActionKind::KillWindow { target };
                build_tmux_action_command(&os_type, action)
                    .map(|c| zeroize::Zeroizing::new(c.command))
            }
            HostTmuxDestructiveAction::KillPane { target } => {
                let action = TmuxActionKind::KillPane { target };
                build_tmux_action_command(&os_type, action)
                    .map(|c| zeroize::Zeroizing::new(c.command))
            }
            HostTmuxDestructiveAction::KillScreenSession { target } => {
                let action = ScreenActionKind::KillSession { target };
                build_screen_action_command(&os_type, action)
                    .map(|c| zeroize::Zeroizing::new(c.command))
            }
        };
        let command = match action_command {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::TmuxActionFailed],
        };
        let request = HostTmuxActionRun {
            connection_id,
            session_id,
            session_name,
            target_label,
        };
        self.start_tmux_action_command(command, request, runtime, cx)
    }

    pub(in crate::workspace::connection_monitor) fn start_tmux_action_command(
        &mut self,
        command: zeroize::Zeroizing<String>,
        request: HostTmuxActionRun,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        self.host_tmux.action_running = Some(request.clone());
        let spawned = self.spawn_tmux_action(
            command,
            request,
            HOST_TMUX_ACTION_TIMEOUT,
            HOST_TMUX_ACTION_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_tmux.action_running = None;
            return vec![HostToolsNotice::TmuxConnectionMissing];
        }
        cx.notify();
        Vec::new()
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_tmux_action(
        &mut self,
        delivery: HostTmuxActionDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_tmux.action_running.as_ref() != Some(&delivery.request) {
            return;
        }
        self.host_tmux.action_running = None;
        let HostTmuxActionRun {
            connection_id,
            target_label,
            ..
        } = delivery.request;
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::TmuxActionFinished {
                target_label,
                succeeded: delivery.result.unwrap_or(false),
            },
        ));
        self.refresh_tmux_snapshot_after_action(connection_id, cx);
        cx.notify();
    }

    fn refresh_tmux_snapshot_after_action(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.monitoring.tmux_enabled
            || !self.visibility.sidebar_is_visible()
            || self.active_tool() != ContextSidebarTool::Tmux
        {
            return;
        }
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        let search_query = self.ui.host_tmux_search_query.clone();
        let failure_fallback = messages.tmux_unknown_error.clone();
        let unavailable_fallback = messages.tmux_unavailable.clone();
        let notices = self.request_tmux_snapshot(
            connection_id,
            HostSnapshotFeedback::Silent,
            search_query,
            failure_fallback,
            unavailable_fallback,
            runtime,
            cx,
        );
        debug_assert!(notices.is_empty());
    }

    pub(in crate::workspace::connection_monitor) fn open_tmux_input_dialog(
        &mut self,
        dialog: HostTmuxInputDialog,
        cx: &mut Context<Self>,
    ) {
        self.ui.host_tmux_input_dialog = Some(dialog);
        self.ui.focus_input(HostToolsTextInput::TmuxDialog);
        cx.notify();
    }

    pub(in crate::workspace::connection_monitor) fn dismiss_tmux_input_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ui.host_tmux_input_dialog.take().is_some() {
            if self.ui.input_is_focused(HostToolsTextInput::TmuxDialog) {
                self.ui.clear_input_focus();
            }
            cx.notify();
        }
    }

    pub(in crate::workspace::connection_monitor) fn submit_tmux_input(
        &mut self,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if self.host_tmux.action_running.is_some() {
            return vec![HostToolsNotice::TmuxActionAlreadyRunning];
        }
        let Some(dialog) = self.ui.host_tmux_input_dialog.as_ref() else {
            return Vec::new();
        };
        if dialog.value.trim().is_empty() {
            return vec![HostToolsNotice::TmuxInputRequired];
        }
        let mut dialog = self
            .ui
            .host_tmux_input_dialog
            .take()
            .expect("tmux input dialog remains present after validation");
        self.ui.clear_input_focus();
        let trimmed_start = dialog.value.len() - dialog.value.trim_start().len();
        let trimmed_end = dialog.value.trim_end().len();
        dialog.value.truncate(trimmed_end);
        if trimmed_start > 0 {
            dialog.value.drain(..trimmed_start);
        }
        let Some(os_type) = self.connection_os_type(&dialog.connection_id) else {
            return vec![HostToolsNotice::TmuxConnectionMissing];
        };
        let command = match &dialog.kind {
            HostTmuxInputDialogKind::RenameSession { target } => {
                build_tmux_rename_session_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::RenameWindow { target } => {
                build_tmux_rename_window_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::SendPaneCommand { target } => {
                build_tmux_send_pane_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::RenameScreenSession { target } => {
                build_screen_rename_session_command(&os_type, target, dialog.value.as_str())
            }
            HostTmuxInputDialogKind::SendScreenCommand { target } => {
                build_screen_send_command(&os_type, target, dialog.value.as_str())
            }
        };
        // The original input clears here; the generated shell command has its
        // own zeroizing buffer until the SSH worker finishes.
        zeroize::Zeroize::zeroize(&mut dialog.value);
        let command = match command {
            Ok(command) => command,
            Err(_) => return vec![HostToolsNotice::TmuxActionFailed],
        };
        let request = HostTmuxActionRun {
            connection_id: dialog.connection_id,
            session_id: dialog.session_id,
            session_name: dialog.session_name,
            target_label: dialog.target_label,
        };
        self.start_tmux_action_command(command, request, runtime, cx)
    }

    fn submit_tmux_input_from_view(&mut self, cx: &mut Context<Self>) {
        if self.tmux_action_running() {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::TmuxActionAlreadyRunning,
            ));
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            cx.emit(HostToolsEvent::ShowNotice(
                HostToolsNotice::TmuxConnectionMissing,
            ));
            return;
        };
        for notice in self.submit_tmux_input(runtime, cx) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }
}

fn host_tmux_confirm_description_key(action: &HostTmuxDestructiveAction) -> &'static str {
    match action {
        HostTmuxDestructiveAction::KillSession { .. } => {
            "sidebar.host_tmux.confirm.kill_session_desc"
        }
        HostTmuxDestructiveAction::KillWindow { .. } => {
            "sidebar.host_tmux.confirm.kill_window_desc"
        }
        HostTmuxDestructiveAction::KillPane { .. } => "sidebar.host_tmux.confirm.kill_pane_desc",
        HostTmuxDestructiveAction::KillScreenSession { .. } => {
            "sidebar.host_virtual_terminal.confirm.kill_screen_session_desc"
        }
    }
}

fn host_tmux_confirm_label_key(action: &HostTmuxDestructiveAction) -> &'static str {
    match action {
        HostTmuxDestructiveAction::KillSession { .. } => "sidebar.host_tmux.actions.kill_session",
        HostTmuxDestructiveAction::KillWindow { .. } => "sidebar.host_tmux.actions.kill_window",
        HostTmuxDestructiveAction::KillPane { .. } => "sidebar.host_tmux.actions.kill_pane",
        HostTmuxDestructiveAction::KillScreenSession { .. } => {
            "sidebar.host_tmux.actions.kill_session"
        }
    }
}

fn host_tmux_input_title_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => {
            "sidebar.host_tmux.input.rename_session_title"
        }
        HostTmuxInputDialogKind::RenameWindow { .. } => {
            "sidebar.host_tmux.input.rename_window_title"
        }
        HostTmuxInputDialogKind::SendPaneCommand { .. } => {
            "sidebar.host_tmux.input.send_command_title"
        }
        HostTmuxInputDialogKind::RenameScreenSession { .. } => {
            "sidebar.host_virtual_terminal.input.rename_screen_session_title"
        }
        HostTmuxInputDialogKind::SendScreenCommand { .. } => {
            "sidebar.host_virtual_terminal.input.send_screen_command_title"
        }
    }
}

fn host_tmux_input_description_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => {
            "sidebar.host_tmux.input.rename_session_desc"
        }
        HostTmuxInputDialogKind::RenameWindow { .. } => {
            "sidebar.host_tmux.input.rename_window_desc"
        }
        HostTmuxInputDialogKind::SendPaneCommand { .. } => {
            "sidebar.host_tmux.input.send_command_desc"
        }
        HostTmuxInputDialogKind::RenameScreenSession { .. } => {
            "sidebar.host_virtual_terminal.input.rename_screen_session_desc"
        }
        HostTmuxInputDialogKind::SendScreenCommand { .. } => {
            "sidebar.host_virtual_terminal.input.send_screen_command_desc"
        }
    }
}

fn host_tmux_input_placeholder_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => {
            "sidebar.host_tmux.input.rename_session_placeholder"
        }
        HostTmuxInputDialogKind::RenameWindow { .. } => {
            "sidebar.host_tmux.input.rename_window_placeholder"
        }
        HostTmuxInputDialogKind::SendPaneCommand { .. } => {
            "sidebar.host_tmux.input.send_command_placeholder"
        }
        HostTmuxInputDialogKind::RenameScreenSession { .. } => {
            "sidebar.host_virtual_terminal.input.rename_screen_session_placeholder"
        }
        HostTmuxInputDialogKind::SendScreenCommand { .. } => {
            "sidebar.host_virtual_terminal.input.send_screen_command_placeholder"
        }
    }
}

fn host_tmux_input_submit_key(kind: &HostTmuxInputDialogKind) -> &'static str {
    match kind {
        HostTmuxInputDialogKind::RenameSession { .. } => "sidebar.host_tmux.actions.rename_session",
        HostTmuxInputDialogKind::RenameWindow { .. } => "sidebar.host_tmux.actions.rename_window",
        HostTmuxInputDialogKind::SendPaneCommand { .. } => "sidebar.host_tmux.actions.send_command",
        HostTmuxInputDialogKind::RenameScreenSession { .. } => {
            "sidebar.host_tmux.actions.rename_session"
        }
        HostTmuxInputDialogKind::SendScreenCommand { .. } => {
            "sidebar.host_tmux.actions.send_command"
        }
    }
}

fn host_vt_engine_chip(active: bool, tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .flex_none()
        .h(px(tokens.metrics.ui_button_sm_height * 0.75))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .cursor_pointer()
        .bg(if active {
            rgb(theme.bg_hover)
        } else {
            rgba(0x00000000)
        })
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(if active {
            rgb(theme.text)
        } else {
            rgb(theme.text_muted)
        })
        .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
}

fn tmux_attached_color(attached: bool, muted_color: u32) -> u32 {
    if attached {
        MONITOR_EMERALD
    } else {
        muted_color
    }
}

fn tmux_time_label(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    if trimmed.is_empty() {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}
