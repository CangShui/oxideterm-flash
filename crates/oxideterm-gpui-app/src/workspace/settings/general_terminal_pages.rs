use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalCommandSpecsAction {
    Format,
    Example,
    Save,
}

pub(in crate::workspace) const SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH: f32 = 300.0; // Tauri TerminalTab custom font input w-[300px].
// The command-spec document needs enough room for structured editing while
// remaining bounded by the workspace viewport.
const TERMINAL_COMMAND_SPECS_MODAL_WIDTH: f32 = 880.0;
const TERMINAL_COMMAND_SPECS_MODAL_HEIGHT: f32 = 720.0;
const TERMINAL_COMMAND_SPECS_EDITOR_MIN_HEIGHT: f32 = 520.0;
const TERMINAL_COMMAND_SPECS_ACTION_ICON_SIZE: f32 = 12.0;

impl WorkspaceApp {
    /// Opens a native file picker and applies the chosen executable to the
    /// external-editor setting, so operators do not have to type paths by hand.
    pub(in crate::workspace) fn browse_external_editor_path(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.general.external_editor_prompt"),
            )),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.first().and_then(|path| path.to_str()) else {
                return;
            };
            let picked = path.to_string();
            let _ = this.update(cx, |this, cx| {
                this.settings_input_draft = picked;
                this.apply_settings_input_draft(SettingsInput::ExternalEditorPath, cx);
            });
        })
        .detach();
    }

    fn render_external_editor_browse_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .id("settings-external-editor-browse")
            .flex_none()
            .w(px(self.tokens.metrics.ui_button_sm_height))
            .h(px(self.tokens.metrics.ui_button_sm_height))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_hover))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_panel)))
            .child(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                14.0,
                rgb(theme.text),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.browse_external_editor_path(cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_general_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.settings_card(
                "settings_view.general.language",
                "settings_view.general.language_hint",
                vec![
                    self.language_select_row(settings.general.language, cx),
                    self.card_separator(),
                    self.setting_row(
                        "settings_view.general.external_editor",
                        "settings_view.general.external_editor_hint",
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(self.settings_text_input_control(
                                SettingsInput::ExternalEditorPath,
                                settings.general.external_editor.clone(),
                                "notepad++.exe".to_string(),
                                300.0,
                                cx,
                            ))
                            .child(self.render_external_editor_browse_button(cx))
                            .into_any_element(),
                        cx,
                    ),
                ],
            ),
            1 => {
                let data_dir_info = self.settings_data_directory_info();
                let data_dir = data_dir_info.path.display().to_string();
                self.plain_settings_card(vec![
                    self.card_title("settings_view.general.data_directory"),
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("settings_view.general.data_directory")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.general.data_directory_hint")),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(px(self.tokens.metrics.ui_text_base))
                                .text_color(rgb(self.tokens.ui.text))
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .truncate()
                                .child(data_dir),
                        )
                        .when(data_dir_info.can_change, |row| {
                            row.child(self.settings_data_directory_change_button(cx))
                        })
                        .when(data_dir_info.can_change && data_dir_info.is_custom, |row| {
                            row.child(self.settings_data_directory_reset_button(cx))
                        })
                        .into_any_element(),
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(
                            self.i18n
                                .t("settings_view.general.data_directory_restart_notice"),
                        )
                        .into_any_element(),
                ])
            }
            2 => self.launch_at_login_settings_card(cx),
            3 => self.settings_card(
                "settings_view.general.connection_uri_integration",
                "settings_view.general.connection_uri_integration_hint",
                vec![self.general_checkbox_row(
                    "settings_view.general.external_connection_uris",
                    "settings_view.general.external_connection_uris_hint",
                    settings.general.external_connection_uris_enabled,
                    |settings, enabled| settings.general.external_connection_uris_enabled = enabled,
                    cx,
                )],
            ),
            4 if cfg!(any(target_os = "windows", target_os = "macos")) => {
                let (label_key, hint_key) = close_to_background_label_keys();
                self.settings_card(
                    "settings_view.general.window_behavior",
                    "settings_view.general.window_behavior_hint",
                    vec![self.general_checkbox_row(
                        label_key,
                        hint_key,
                        settings.general.minimize_to_tray_on_close,
                        |settings, enabled| settings.general.minimize_to_tray_on_close = enabled,
                        cx,
                    )],
                )
            }
            _ => div().into_any_element(),
        }
    }

    /// Self-hosted cloud sync: endpoint, room, transport mode, and a single
    /// manual "force sync" action that wakes every connected client. Engine
    /// lifecycle is handled by autostart_cloud_sync.
    pub(in crate::workspace) fn settings_cloud_sync_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let status = self
            .cloud_sync_status
            .clone()
            .unwrap_or_else(|| self.i18n.t("settings_view.general.cloudsync.status_idle"));
        let mode_label = oxideterm_gpui_settings_view::cloud_sync_mode_label(
            settings.cloud_sync.mode,
            &self.i18n,
        );
        self.plain_settings_card(vec![
            self.card_title("settings_view.general.cloudsync.title"),
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.general.cloudsync.description"))
                .into_any_element(),
            self.setting_row(
                "settings_view.general.cloudsync.enabled",
                "settings_view.general.cloudsync.enabled_hint",
                checkbox(&self.tokens, String::new(), settings.cloud_sync.enabled)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| {
                                    settings.cloud_sync.enabled = !settings.cloud_sync.enabled
                                },
                                cx,
                            );
                            this.autostart_cloud_sync(cx);
                        }),
                    )
                    .into_any_element(),
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.general.cloudsync.server",
                "settings_view.general.cloudsync.server_hint",
                self.settings_text_input_control(
                    SettingsInput::CloudSyncServerUrl,
                    settings.cloud_sync.server_url.clone(),
                    crate::workspace::cloud_sync::DEFAULT_CLOUD_SYNC_SERVER.to_string(),
                    320.0,
                    cx,
                ),
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.general.cloudsync.room",
                "settings_view.general.cloudsync.room_hint",
                self.settings_text_input_control(
                    SettingsInput::CloudSyncRoom,
                    settings.cloud_sync.room.clone(),
                    String::new(),
                    240.0,
                    cx,
                ),
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.general.cloudsync.sync_passwords",
                "settings_view.general.cloudsync.sync_passwords_hint",
                checkbox(
                    &self.tokens,
                    String::new(),
                    settings.cloud_sync.sync_passwords,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| {
                                settings.cloud_sync.sync_passwords =
                                    !settings.cloud_sync.sync_passwords
                            },
                            cx,
                        );
                    }),
                )
                .into_any_element(),
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.general.cloudsync.mode",
                "settings_view.general.cloudsync.mode_hint",
                self.settings_select_control(
                    SettingsSelect::CloudSyncMode,
                    mode_label,
                    false,
                    Some(160.0),
                    cx,
                ),
                cx,
            ),
            self.card_separator(),
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(12.0))
                .child(self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.general.cloudsync.force_sync"),
                    None,
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Default,
                            radius: ButtonRadius::Md,
                            disabled: self.cloud_sync.is_none(),
                        },
                        ..ToolbarButtonOptions::default()
                    },
                    cx.listener(|this, _event, _window, cx| {
                        this.cloud_sync_force_sync(cx);
                        cx.stop_propagation();
                    }),
                ))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .truncate()
                        .child(status),
                )
                .into_any_element(),
            self.card_separator(),
            {
                let progress = self.cloud_sync_progress.unwrap_or(0.0);
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w_full()
                            .h(px(4.0))
                            .rounded(px(self.tokens.radii.sm))
                            .bg(rgb(self.tokens.ui.bg_hover))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(rgb(self.tokens.ui.accent)),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .children(
                                self.cloud_sync_logs
                                    .iter()
                                    .rev()
                                    .take(3)
                                    .rev()
                                    .cloned()
                                    .map(|entry| {
                                        div()
                                            .w_full()
                                            .min_w(px(0.0))
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .truncate()
                                            .child(entry)
                                    }),
                            ),
                    )
                    .into_any_element()
            },
        ])
    }

    /// Session import/export: import from third-party clients (SecureCRT,
    /// Xshell, Termius, MobaXterm, WindTerm, Electerm, FinalShell) or export
    /// saved sessions to the same client formats. Passwords are never included
    /// in either direction.
    pub(in crate::workspace) fn settings_session_io_section(
        &mut self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match section_index {
            0 => {
                // Dedicated .oxide bundle import card
                self.session_io_oxide_import_section(cx)
            }
            1 => {
                // Full importer surface moved from the Connections page: source
                // picker, path selection, preview, and apply stay one surface.
                let mut importer = self
                    .settings_workspace
                    .read(cx)
                    .connection_import_snapshot();
                let mut rows =
                    vec![self.connection_import_input_row(importer.source, &importer.paths, cx)];

                if let Some(preview) = importer.preview.take() {
                    rows.push(self.connection_import_preview_toolbar(
                        &preview,
                        &importer.selected_draft_ids,
                        importer.duplicate_strategy,
                        cx,
                    ));
                    rows.push(self.connection_import_preview_list(
                        preview,
                        &importer.selected_draft_ids,
                        cx,
                    ));
                }
                if let Some(status) = importer.status {
                    rows.push(self.connection_status_row(status.to_string()));
                }

                self.connection_section(
                    "settings_view.connections.importers.title",
                    "settings_view.connections.importers.description",
                    rows,
                )
            }
            2 => {
                // Export card: pick a client format then write sessions.
                let connections = self.connection_store.connections();
                let count = connections.len();
                let format = self.session_export_format(cx);
                let format_label = session_export_format_label(format, &self.i18n);
                self.plain_settings_card(vec![
                    self.card_title("settings_view.sessionio.export_title"),
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.sessionio.export_desc"))
                        .into_any_element(),
                    self.setting_row(
                        "settings_view.sessionio.export_format",
                        "settings_view.sessionio.export_format_hint",
                        self.settings_select_control(
                            SettingsSelect::SessionExportFormat,
                            format_label,
                            false,
                            Some(220.0),
                            cx,
                        ),
                        cx,
                    ),
                    self.card_separator(),
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(self.workspace_toolbar_action_button(
                            format!(
                                "{} ({count})",
                                self.i18n.t("settings_view.sessionio.export_button")
                            ),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Default,
                                    radius: ButtonRadius::Md,
                                    disabled: count == 0,
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(|this, _event, _window, cx| {
                                this.export_sessions_to_file(cx);
                                cx.stop_propagation();
                            }),
                        ))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.sessionio.no_passwords")),
                        )
                        .into_any_element(),
                ])
            }
            _ => div().into_any_element(),
        }
    }

    fn session_io_oxide_import_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.connection_section(
            "settings_view.sessionio.import_oxide",
            "settings_view.sessionio.import_oxide_hint",
            vec![
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.sessionio.import_oxide_button"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Default,
                                size: ButtonSize::Default,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        cx.listener(|this, _event, _window, cx| {
                            crate::logging::audit_button_click("sessionio-import-oxide", "settings");
                            tracing::debug!(
                                target: "oxideterm::audit",
                                stage = "sessionio.import.oxide",
                                result = "opened",
                                business_impact = "the encrypted .oxide import wizard was opened from Settings",
                                "设置页已打开 .oxide 导入对话框"
                            );
                            this.open_oxide_import_dialog(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .into_any_element(),
            ],
        )
    }

    fn export_sessions_to_file(&mut self, cx: &mut Context<Self>) {
        use oxideterm_connections::{
            SessionExportContent, SessionExportFormat, export_sessions,
            export_sessions_to_finalshell_directory, suggested_file_name,
        };
        let format = self.session_export_format(cx);
        let trace_id = crate::logging::next_audit_trace_id();
        tracing::debug!(
            target: "oxideterm::audit",
            trace_id,
            stage = "sessionio.export.request",
            format = ?format,
            result = "received",
            "用户触发设置页会话导出"
        );
        if format == SessionExportFormat::OxideEncrypted {
            crate::logging::audit_button_click("sessionio-export-oxide", "settings");
            tracing::debug!(
                target: "oxideterm::audit",
                stage = "sessionio.export.oxide",
                result = "opened",
                business_impact = "the encrypted .oxide export wizard was opened from Settings",
                "设置页已打开 .oxide 导出对话框"
            );
            self.open_oxide_export_dialog(cx);
            return;
        }
        let connections = self.connection_store.connections().to_vec();
        let directory = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Downloads"))
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        if format.is_directory() {
            // FinalShell import expects a folder tree; pick the destination
            // directory instead of a file path.
            let prompt_key = "settings_view.sessionio.export_choose_directory";
            let receiver = cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(SharedString::from(self.i18n.t(prompt_key))),
            });
            let settings_path = self.settings_store.path().to_path_buf();
            cx.spawn(async move |workspace, cx| {
                if let Ok(Ok(Some(paths))) = receiver.await {
                    let Some(target) = paths.into_iter().next() else {
                        return;
                    };
                    let result = export_sessions_to_finalshell_directory(&connections, &target)
                        .map(|count| format!("{} ({count} files)", target.display()));
                    let _ = workspace.update(cx, |workspace, cx| {
                        match result {
                            Ok(description) => {
                                tracing::info!(
                                    target: "oxideterm::audit",
                                    trace_id,
                                    stage = "sessionio.export.response",
                                    format = ?format,
                                    result = "completed",
                                    "会话目录导出完成"
                                );
                                workspace.push_workspace_notice(
                                    TerminalNotice {
                                        title: workspace
                                            .i18n
                                            .t("settings_view.sessionio.export_done"),
                                        description: Some(description),
                                        status_text: None,
                                        progress: None,
                                        variant: TerminalNoticeVariant::Success,
                                    },
                                    cx,
                                );
                            }
                            Err(error) => {
                                tracing::warn!(
                                    target: "oxideterm::audit",
                                    trace_id,
                                    stage = "sessionio.export.response",
                                    format = ?format,
                                    result = "failed",
                                    error_kind = format!("{error:#}").chars().take(160).collect::<String>(),
                                    "会话目录导出失败"
                                );
                                workspace.push_workspace_notice(
                                    TerminalNotice {
                                        title: workspace
                                            .i18n
                                            .t("settings_view.sessionio.export_failed"),
                                        description: Some(format!("{error:#}")),
                                        status_text: None,
                                        progress: None,
                                        variant: TerminalNoticeVariant::Error,
                                    },
                                    cx,
                                );
                            }
                        }
                        let _ = settings_path;
                    });
                }
            })
            .detach();
            return;
        }

        // File-based formats render once and reuse the same write flow.
        let content = match export_sessions(&connections, format) {
            Ok(content) => {
                tracing::debug!(
                    target: "oxideterm::audit",
                    trace_id,
                    stage = "sessionio.export.content",
                    format = ?format,
                    result = "rendered",
                    "会话导出内容生成完成，等待选择保存路径"
                );
                content
            }
            Err(error) => {
                tracing::warn!(
                    target: "oxideterm::audit",
                    trace_id,
                    stage = "sessionio.export.content",
                    format = ?format,
                    result = "failed",
                    error_kind = format!("{error:#}").chars().take(160).collect::<String>(),
                    "会话导出内容生成失败"
                );
                self.push_workspace_notice(
                    TerminalNotice {
                        title: self.i18n.t("settings_view.sessionio.export_failed"),
                        description: Some(format!("{error:#}")),
                        status_text: None,
                        progress: None,
                        variant: TerminalNoticeVariant::Error,
                    },
                    cx,
                );
                return;
            }
        };
        let suggested = suggested_file_name(format);
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        let settings_path = self.settings_store.path().to_path_buf();
        cx.spawn(async move |workspace, cx| {
            if let Ok(Ok(Some(path))) = receiver.await {
                let result = match content {
                    SessionExportContent::Text(text) => {
                        std::fs::write(&path, text.as_bytes()).map_err(|error| error.to_string())
                    }
                    SessionExportContent::Binary(bytes) => {
                        std::fs::write(&path, bytes).map_err(|error| error.to_string())
                    }
                };
                let _ = workspace.update(cx, |workspace, cx| {
                    match result {
                        Ok(()) => {
                            tracing::info!(
                                target: "oxideterm::audit",
                                trace_id,
                                stage = "sessionio.export.response",
                                format = ?format,
                                result = "completed",
                                "会话导出文件已写入磁盘"
                            );
                            workspace.push_workspace_notice(
                                TerminalNotice {
                                    title: workspace.i18n.t("settings_view.sessionio.export_done"),
                                    description: Some(path.display().to_string()),
                                    status_text: None,
                                    progress: None,
                                    variant: TerminalNoticeVariant::Success,
                                },
                                cx,
                            );
                        }
                        Err(error) => {
                            tracing::warn!(
                                target: "oxideterm::audit",
                                trace_id,
                                stage = "sessionio.export.response",
                                format = ?format,
                                result = "failed",
                                error_kind = error.chars().take(160).collect::<String>(),
                                "会话导出文件写入失败"
                            );
                            workspace.push_workspace_notice(
                                TerminalNotice {
                                    title: workspace
                                        .i18n
                                        .t("settings_view.sessionio.export_failed"),
                                    description: Some(error),
                                    status_text: None,
                                    progress: None,
                                    variant: TerminalNoticeVariant::Error,
                                },
                                cx,
                            );
                        }
                    }
                    let _ = settings_path;
                });
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    pub(in crate::workspace) fn launch_at_login_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let launch_at_login = self.settings_workspace.read(cx).launch_at_login_snapshot();
        // TODO(signing): Once every macOS artifact is Developer ID signed and
        // notarized, replace this system-settings handoff with an in-app
        // SMAppService register/unregister toggle and remove the manual copy.
        let mut description = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.general.launch_at_login")),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_macos_hint"),
                    ),
            );
        if let Some(error) = launch_at_login.error {
            let error = match error {
                LaunchAtLoginError::OperationFailed(error) => error,
            };
            description = description.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_failed")
                            .replace("{{error}}", error.as_ref()),
                    ),
            );
        }
        let manage_button = self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.manage_login_items"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                let result = oxideterm_gpui_platform::autostart::open_login_items_settings()
                    .map_err(|error| error.to_string());
                this.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_launch_at_login_settings_handoff(result, cx);
                });
                cx.stop_propagation();
            }),
        );

        self.settings_card(
            "settings_view.general.startup",
            "settings_view.general.startup_hint",
            vec![
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(description)
                    .child(manage_button)
                    .into_any_element(),
            ],
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn launch_at_login_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let launch_at_login = self.settings_workspace.read(cx).launch_at_login_snapshot();
        let loading = launch_at_login.pending;
        let enabled = launch_at_login.enabled;
        let error = launch_at_login.error;
        let control = div()
            .flex_none()
            .opacity(if loading { 0.55 } else { 1.0 })
            .child(
                checkbox(&self.tokens, String::new(), enabled).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if !this
                            .settings_workspace
                            .read(cx)
                            .launch_at_login_snapshot()
                            .pending
                        {
                            this.set_launch_at_login_enabled(!enabled, cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        let mut label = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.general.launch_at_login")),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.general.launch_at_login_hint")),
            );
        if loading {
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_updating"),
                    ),
            );
        }
        if let Some(error) = error {
            let error = match error {
                LaunchAtLoginError::ApprovalRequired => self
                    .i18n
                    .t("settings_view.general.launch_at_login_approval_required"),
                LaunchAtLoginError::OperationFailed(error)
                | LaunchAtLoginError::TaskFailed(error) => error.to_string(),
            };
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_failed")
                            .replace("{{error}}", error.as_ref()),
                    ),
            );
        }

        self.settings_card(
            "settings_view.general.startup",
            "settings_view.general.startup_hint",
            vec![
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(label)
                    .child(control)
                    .into_any_element(),
            ],
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn refresh_launch_at_login_status(&mut self, cx: &mut Context<Self>) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_launch_at_login_operation(
                async move {
                    let result = runtime
                        .spawn_blocking(oxideterm_gpui_platform::autostart::is_enabled)
                        .await
                        .map_err(|error| {
                            LaunchAtLoginError::TaskFailed(error.to_string().into())
                        })?;
                    result.map_err(|error| {
                        LaunchAtLoginError::OperationFailed(error.to_string().into())
                    })
                },
                cx,
            );
        });
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn set_launch_at_login_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_launch_at_login_operation(
                async move {
                    let result = runtime
                        .spawn_blocking(move || {
                            oxideterm_gpui_platform::autostart::set_enabled(enabled)?;
                            let actual = oxideterm_gpui_platform::autostart::is_enabled()?;
                            if actual != enabled {
                                return Err(std::io::Error::other(
                                    "the operating system did not retain the startup setting",
                                ));
                            }
                            Ok(actual)
                        })
                        .await
                        .map_err(|error| {
                            LaunchAtLoginError::TaskFailed(error.to_string().into())
                        })?;
                    result.map_err(|error| {
                        if error.kind() == std::io::ErrorKind::PermissionDenied {
                            LaunchAtLoginError::ApprovalRequired
                        } else {
                            LaunchAtLoginError::OperationFailed(error.to_string().into())
                        }
                    })
                },
                cx,
            );
        });
    }

    pub(in crate::workspace) fn general_checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(div().flex_none().child(
                checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                        cx.stop_propagation();
                    }),
                ),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_data_directory_info(
        &self,
    ) -> oxideterm_settings::DataDirectoryInfo {
        oxideterm_settings::data_directory_info().unwrap_or_else(|_| {
            let path = self
                .settings_store
                .path()
                .parent()
                .unwrap_or_else(|| self.settings_store.path())
                .to_path_buf();
            oxideterm_settings::DataDirectoryInfo {
                default_path: path.clone(),
                path,
                is_custom: false,
                can_change: false,
            }
        })
    }

    pub(in crate::workspace) fn settings_data_directory_change_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.change"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.pick_settings_data_directory(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn settings_data_directory_reset_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.reset_to_default"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.settings_workspace.update(cx, |settings, cx| {
                    settings.open_data_directory_reset_confirm(cx);
                });
                this.reset_standard_confirm_focus();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn pick_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.general.select_data_directory"),
            )),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_data_directory_picker(selection, cx);
        });
    }

    pub(in crate::workspace) fn cancel_settings_data_directory_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_data_directory_confirm_exit(false, delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_data_directory_confirm_exit(true, delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_data_directory_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings_workspace = self.settings_workspace.read(cx);
        let confirm = settings_workspace.data_directory_confirm()?;
        let (title_key, description) = match confirm {
            DataDirectoryConfirm::Conflict { files_found, .. } => (
                "settings_view.general.data_directory_conflict",
                self.i18n
                    .t("settings_view.general.data_directory_conflict_detail")
                    .replace("{{files}}", &files_found.join(", ")),
            ),
            DataDirectoryConfirm::Reset => (
                "settings_view.general.reset_data_directory",
                self.i18n
                    .t("settings_view.general.reset_data_directory_confirm"),
            ),
        };
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "settings-data-directory-confirm-motion",
                settings_workspace.data_directory_confirm_phase(),
                ConfirmDialogView {
                    variant: ConfirmDialogVariant::Default,
                    title: div().child(self.i18n.t(title_key)).into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("common.actions.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t("common.actions.confirm"))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.cancel_settings_data_directory_confirm(cx);
                    cx.stop_propagation();
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_settings_data_directory(cx);
                    cx.stop_propagation();
                }),
            ),
        )
    }

    pub(in crate::workspace) fn settings_terminal_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        if section_index == 0 {
            return self.terminal_page_switcher(cx);
        }

        let terminal_page = self
            .settings_workspace
            .read(cx)
            .route_snapshot()
            .terminal_page;
        match (terminal_page, section_index - 1) {
            (TerminalSettingsPage::Display, 0) => {
                let mut rows = vec![self.select_setting_row(
                    "settings_view.terminal.font_family",
                    "settings_view.terminal.font_family_hint",
                    SettingsSelect::TerminalFontFamily,
                    font_family_label(settings.terminal.font_family),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )];
                if settings.terminal.font_family == oxideterm_settings::FontFamily::Custom {
                    rows.push(self.setting_row(
                        "settings_view.terminal.custom_font_stack",
                        "settings_view.terminal.custom_font_stack_hint",
                        self.settings_text_input_control(
                            SettingsInput::TerminalCustomFontFamily,
                            settings.terminal.custom_font_family.clone(),
                            "'Sarasa Fixed SC', 'Fira Code', monospace".to_string(),
                            SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH,
                            cx,
                        ),
                        cx,
                    ));
                }
                rows.push(self.card_separator());
                rows.push(self.select_setting_row(
                    "settings_view.terminal.cjk_font_family",
                    "settings_view.terminal.cjk_font_family_hint",
                    SettingsSelect::TerminalCjkFontFamily,
                    terminal_cjk_font_label(&settings.terminal.cjk_font_family, &self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ));
                rows.extend([
                    self.terminal_preview(settings),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.font_ligatures",
                        "settings_view.terminal.font_ligatures_hint",
                        settings.terminal.font_ligatures,
                        set_font_ligatures,
                        cx,
                    ),
                    self.card_separator(),
                    self.font_size_row(settings, cx),
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.line_height",
                        "settings_view.terminal.line_height_hint",
                        SettingsInput::TerminalLineHeight,
                        compact_decimal(settings.terminal.line_height),
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.smooth_scroll",
                        "settings_view.terminal.smooth_scroll_hint",
                        settings.terminal.smooth_scroll,
                        set_terminal_smooth_scroll,
                        cx,
                    ),
                    self.card_separator(),
                    self.select_setting_row(
                        "settings_view.terminal.encoding",
                        "settings_view.terminal.encoding_hint",
                        SettingsSelect::TerminalEncoding,
                        terminal_encoding_label(settings.terminal.terminal_encoding),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.show_performance_overlay",
                        "settings_view.terminal.show_performance_overlay_hint",
                        settings.terminal.show_fps_overlay,
                        set_show_terminal_performance_overlay,
                        cx,
                    ),
                ]);
                self.settings_card(
                    "settings_view.terminal.font",
                    "settings_view.terminal.font_family_hint",
                    rows,
                )
            }
            (TerminalSettingsPage::Display, 1) => self.settings_card(
                "settings_view.terminal.cursor",
                "settings_view.terminal.cursor_style_hint",
                vec![
                    self.select_setting_row(
                        "settings_view.terminal.cursor_style",
                        "settings_view.terminal.cursor_style_hint",
                        SettingsSelect::TerminalCursorStyle,
                        cursor_style_label(settings.terminal.cursor_style, &self.i18n),
                        self.tokens.metrics.settings_select_narrow_width,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.cursor_blink",
                        "settings_view.terminal.cursor_blink_hint",
                        settings.terminal.cursor_blink,
                        set_terminal_cursor_blink,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Display, 2) => self.settings_card(
                "settings_view.terminal.command_marks",
                "settings_view.terminal.command_marks_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.command_marks",
                        "settings_view.terminal.command_marks_hint",
                        settings.terminal.command_marks.enabled,
                        set_command_marks_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_marks_hover_actions",
                        "settings_view.terminal.command_marks_hover_actions_hint",
                        settings.terminal.command_marks.show_hover_actions,
                        set_command_marks_hover_actions,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Display, 3) => self.settings_card(
                "settings_view.terminal.buffer",
                "settings_view.terminal.scrollback_hint",
                vec![
                    self.number_row(
                        "settings_view.terminal.scrollback",
                        "settings_view.terminal.scrollback_hint",
                        SettingsInput::TerminalScrollback,
                        settings.terminal.scrollback,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.highlight_tab_on_new_output",
                        "settings_view.terminal.highlight_tab_on_new_output_hint",
                        settings.terminal.highlight_tab_on_new_output,
                        set_highlight_tab_on_new_output,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Input, 0) => self.terminal_input_settings_card(settings, cx),
            (TerminalSettingsPage::CommandBar, 0) => self.settings_card(
                "settings_view.terminal.command_bar",
                "settings_view.terminal.command_bar_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.command_bar",
                        "settings_view.terminal.command_bar_hint",
                        settings.terminal.command_bar.enabled,
                        set_command_bar_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_git_status",
                        "settings_view.terminal.command_bar_git_status_hint",
                        settings.terminal.command_bar.git_status,
                        set_command_bar_git_status,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_project_tasks",
                        "settings_view.terminal.command_bar_project_tasks_hint",
                        settings.terminal.command_bar.project_tasks,
                        set_command_bar_project_tasks,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::CommandBar, 1) => self.settings_card(
                "settings_view.terminal.command_bar_focus_handoff",
                "settings_view.terminal.command_bar_focus_handoff_hint",
                vec![self.focus_handoff_commands_row(settings, cx)],
            ),
            (TerminalSettingsPage::CommandBar, 2) => self.settings_card(
                "settings_view.terminal.quick_commands",
                "settings_view.terminal.quick_commands_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        settings.terminal.command_bar.quick_commands_enabled,
                        set_quick_commands_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_bar",
                        "settings_view.terminal.quick_bar_hint",
                        settings.terminal.command_bar.quick_bar_enabled,
                        set_quick_bar_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_confirm",
                        "settings_view.terminal.quick_commands_confirm_hint",
                        settings
                            .terminal
                            .command_bar
                            .quick_commands_confirm_before_run,
                        set_quick_commands_confirm,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_toast",
                        "settings_view.terminal.quick_commands_toast_hint",
                        settings.terminal.command_bar.quick_commands_show_toast,
                        set_quick_commands_toast,
                        cx,
                    ),
                    self.card_separator(),
                    self.terminal_command_specs_editor_row(cx),
                ],
            ),
            (TerminalSettingsPage::Transfer, 0) => self.settings_card(
                "settings_view.terminal.in_band_transfer.title",
                "settings_view.terminal.in_band_transfer.runtime_note",
                vec![
                    self.bool_row(
                        "settings_view.terminal.in_band_transfer.enabled",
                        "settings_view.terminal.in_band_transfer.enabled_hint",
                        settings.terminal.in_band_transfer.enabled,
                        set_in_band_transfer_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.in_band_transfer.allow_directory",
                        "settings_view.terminal.in_band_transfer.allow_directory_hint",
                        settings.terminal.in_band_transfer.allow_directory,
                        set_in_band_transfer_allow_directory,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_chunk_bytes",
                        "settings_view.terminal.in_band_transfer.max_chunk_bytes_hint",
                        SettingsInput::InBandTransferMaxChunkBytes,
                        settings.terminal.in_band_transfer.max_chunk_bytes,
                        128.0,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_file_count",
                        "settings_view.terminal.in_band_transfer.max_file_count_hint",
                        SettingsInput::InBandTransferMaxFileCount,
                        settings.terminal.in_band_transfer.max_file_count,
                        128.0,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_total_bytes",
                        "settings_view.terminal.in_band_transfer.max_total_bytes_hint",
                        SettingsInput::InBandTransferMaxTotalBytes,
                        settings.terminal.in_band_transfer.max_total_bytes,
                        160.0,
                        cx,
                    ),
                    self.in_band_transfer_runtime_note(),
                ],
            ),
            (TerminalSettingsPage::Highlight, 0) => self.highlight_rules_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn focus_handoff_commands_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::TerminalCommandBarFocusHandoff;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input, cx)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut command_editor = div()
            .w_full()
            .h(px(44.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        if value.is_empty() {
            command_editor = self.render_settings_multiline_textarea_lines(
                command_editor,
                target,
                "aider, custom-tui",
                true,
                line_height,
                cx,
            );
        } else {
            command_editor = self.render_settings_multiline_textarea_lines(
                command_editor,
                target,
                &value,
                false,
                line_height,
                cx,
            );
        }

        if let Some(marked) = self.marked_text_for_target(target, cx) {
            command_editor = command_editor.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }

        let control = text_input_anchor_probe(
            target.anchor_id(),
            command_editor,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );

        let mut presets = div().w_full().flex().flex_row().flex_wrap().gap(px(8.0));
        for command in RECOMMENDED_FOCUS_HANDOFF_COMMANDS {
            let enabled = settings
                .terminal
                .command_bar
                .focus_handoff_commands
                .iter()
                .any(|candidate| candidate == command);
            let command_name = (*command).to_string();
            let theme = self.tokens.ui;
            let chip = div()
                .h(px(32.0))
                .px(px(12.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if enabled {
                    rgba((theme.accent << 8) | 0x99)
                } else {
                    rgba((theme.border << 8) | 0x99)
                })
                .bg(if enabled {
                    rgba((theme.accent << 8) | 0x20)
                } else {
                    rgba(0x00000000)
                })
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_color(if enabled {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                })
                .hover(move |style| {
                    style
                        .border_color(rgba((theme.accent << 8) | 0x88))
                        .bg(rgb(theme.bg_hover))
                        .text_color(rgb(theme.text))
                })
                .when(enabled, |chip| chip.child("✓"))
                .child(command_name.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(input) = this.focused_settings_input.take() {
                            this.clear_settings_input_draft(input);
                        }
                        let command_name = command_name.clone();
                        this.edit_settings(
                            move |settings| {
                                let commands =
                                    &mut settings.terminal.command_bar.focus_handoff_commands;
                                if let Some(index) = commands
                                    .iter()
                                    .position(|candidate| candidate == &command_name)
                                {
                                    commands.remove(index);
                                } else {
                                    commands.push(command_name);
                                }
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                );
            presets = presets.child(chip);
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.command_bar_focus_handoff_presets"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(
                                "settings_view.terminal.command_bar_focus_handoff_presets_hint",
                            )),
                    ),
            )
            .child(presets)
            .child(self.card_separator())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.command_bar_focus_handoff_advanced"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(
                                "settings_view.terminal.command_bar_focus_handoff_advanced_hint",
                            )),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn terminal_command_specs_editor_row(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let theme = self.tokens.ui;

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.terminal.command_specs")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.terminal.command_specs_hint")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .truncate()
                                    .child(path.display().to_string()),
                            )
                            .child(self.terminal_command_specs_summary()),
                    ),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.terminal.command_specs_edit"),
                Some(Self::render_lucide_icon(
                    LucideIcon::Pencil,
                    TERMINAL_COMMAND_SPECS_ACTION_ICON_SIZE,
                    rgb(theme.text),
                )),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(|this, _event, window, cx| {
                    this.open_terminal_command_specs_editor(window, cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_command_specs_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let theme = self.tokens.ui;
        let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(TERMINAL_COMMAND_SPECS_MODAL_WIDTH))
            .max_w_full()
            .h(px(TERMINAL_COMMAND_SPECS_MODAL_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.command_specs"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.command_specs_hint"),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(rgb(theme.text_muted))
                            .truncate()
                            .child(path.display().to_string()),
                    ),
            )
            .child(
                // The modal body is the sole scroll owner. The editor grows with
                // its JSON document instead of introducing a nested scroll view.
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .id("terminal-command-specs-editor-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("terminal-command-specs-editor-scroll"),
                    )
                    .bg(rgb(theme.bg_elevated))
                    .child(self.terminal_command_specs_editor_control(cx)),
            )
            .child(
                dialog_footer(&self.tokens)
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.terminal_command_specs_summary()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_format",
                                TerminalCommandSpecsAction::Format,
                                cx,
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_example",
                                TerminalCommandSpecsAction::Example,
                                cx,
                            ))
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.terminal.command_specs_close"),
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.close_terminal_command_specs_editor(cx);
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_save",
                                TerminalCommandSpecsAction::Save,
                                cx,
                            )),
                    ),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_terminal_command_specs_editor(cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(dialog))
            .into_any_element()
    }

    fn terminal_command_specs_editor_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let input = SettingsInput::TerminalCommandSpecsJson;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.terminal_command_specs_editor_initial_value()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(TERMINAL_COMMAND_SPECS_EDITOR_MIN_HEIGHT))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        textarea = self.render_settings_multiline_textarea_lines(
            textarea,
            target,
            &value,
            false,
            line_height,
            cx,
        );
        if let Some(marked) = self.marked_text_for_target(target, cx) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let control =
            text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });

        control.into_any_element()
    }

    pub(in crate::workspace) fn open_terminal_command_specs_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.terminal_command_specs_editor_initial_value();
        self.prepare_modal_interaction_boundary(cx);
        self.terminal_command_specs_editor_open = true;
        self.focus_settings_input(SettingsInput::TerminalCommandSpecsJson, value, cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_command_specs_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_specs_editor_open = false;
        if self.focused_settings_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(SettingsInput::TerminalCommandSpecsJson);
            self.clear_ime_selection();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_multiline_textarea_lines(
        &self,
        mut textarea: Div,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: bool,
        line_height: f32,
        cx: &App,
    ) -> Div {
        let selection = self.ime_selected_range_for_target(target, cx);
        let theme = self.tokens.ui;
        for (line_range, line_text) in settings_multiline_line_ranges(value) {
            let (selection_range, caret_offset) =
                settings_multiline_line_selection(selection.as_ref(), &line_range);
            // Browser textareas hit-test contiguous line boxes. Keep the
            // manually rendered GPUI lines at the same height used by IME
            // y-to-line mapping so pointer selection cannot drift vertically.
            let mut line = div().h(px(line_height)).min_h(px(line_height));
            if placeholder {
                // Browser placeholder text is not part of the editable value;
                // keep it muted and do not feed it through selection segments.
                line = line
                    .text_color(rgb(theme.text_muted))
                    .child(line_text.as_str().to_string());
            } else {
                // Tauri uses a real textarea, so caret/selection sit inside the
                // current visual line. Native renders line elements manually and
                // must split the shared UTF-16 IME selection per line.
                line = line.child(text_input_value_segments(
                    &self.tokens,
                    &line_text,
                    false,
                    selection_range,
                    caret_offset,
                    self.input_caret.visible(),
                ));
            }
            textarea = textarea.child(line);
        }
        textarea
    }

    pub(in crate::workspace) fn terminal_command_specs_button(
        &self,
        label_key: &'static str,
        action: TerminalCommandSpecsAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Command-spec editor actions behave like Tauri Button onClick handlers:
        // disabled/loading guards live at the shared workspace boundary, not in
        // each feature listener.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if action == TerminalCommandSpecsAction::Save {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.handle_terminal_command_specs_action(action, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn load_terminal_command_specs_editor_value(&self) -> String {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub(in crate::workspace) fn terminal_command_specs_editor_initial_value(&self) -> String {
        super::terminal_command_bar::completion::terminal_command_specs_editor_initial_json(
            &self.load_terminal_command_specs_editor_value(),
        )
    }

    pub(in crate::workspace) fn terminal_command_specs_summary(&self) -> String {
        let built_in_count =
            super::terminal_command_bar::completion::built_in_terminal_fig_specs().len();
        let custom_count = super::terminal_command_bar::completion::user_terminal_fig_specs_count(
            self.settings_store.path(),
        );
        self.i18n
            .t("settings_view.terminal.command_specs_summary")
            .replace("{builtIn}", &built_in_count.to_string())
            .replace("{custom}", &custom_count.to_string())
    }

    pub(in crate::workspace) fn current_terminal_command_specs_editor_value(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.settings_input_draft.clone()
        } else {
            self.terminal_command_specs_editor_initial_value()
        }
    }

    pub(in crate::workspace) fn handle_terminal_command_specs_action(
        &mut self,
        action: TerminalCommandSpecsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = SettingsInput::TerminalCommandSpecsJson;
        match action {
            TerminalCommandSpecsAction::Example => {
                let example =
                    super::terminal_command_bar::completion::terminal_command_specs_example_json();
                self.focus_settings_input(input, example, cx);
                window.focus(&self.focus_handle, cx);
            }
            TerminalCommandSpecsAction::Format => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(
                    &value,
                ) {
                    Ok(pretty) => {
                        self.focus_settings_input(input, pretty, cx);
                        window.focus(&self.focus_handle, cx);
                    }
                    Err(error) => self.push_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n.t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
            TerminalCommandSpecsAction::Save => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(
                    &value,
                ) {
                    Ok(pretty) => {
                        let path =
                            super::terminal_command_bar::completion::terminal_command_specs_path(
                                self.settings_store.path(),
                            );
                        let result = path
                            .parent()
                            .map(std::fs::create_dir_all)
                            .transpose()
                            .and_then(|_| std::fs::write(&path, pretty.as_bytes()));
                        match result {
                            Ok(()) => {
                                if self.focused_settings_input == Some(input) {
                                    self.settings_input_draft = pretty;
                                }
                                self.push_settings_toast(
                                    self.i18n.t("settings_view.terminal.command_specs_saved"),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                                cx.notify();
                            }
                            Err(error) => self.push_settings_toast(
                                error.to_string(),
                                TerminalNoticeVariant::Error,
                                cx,
                            ),
                        }
                    }
                    Err(error) => self.push_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n.t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
        }
    }

    pub(in crate::workspace) fn in_band_transfer_number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.number_input(input, value.to_string(), width, cx),
            cx,
        )
    }

    pub(in crate::workspace) fn in_band_transfer_runtime_note(&self) -> AnyElement {
        const TAURI_RUNTIME_NOTE_BORDER_ALPHA: f32 = 0.30;
        const TAURI_RUNTIME_NOTE_BACKGROUND_ALPHA: f32 = 0.10;

        // Tauri renders this as `border-amber-500/30 bg-amber-500/10 p-3 text-xs`;
        // keep the amber opacity mapping explicit instead of folding it into
        // the generic settings card row style.
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(TAURI_RUNTIME_NOTE_BORDER_ALPHA),
            ))
            .bg(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(TAURI_RUNTIME_NOTE_BACKGROUND_ALPHA),
            ))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t("settings_view.terminal.in_band_transfer.runtime_note"),
            )
            .into_any_element()
    }
}

pub(in crate::workspace) fn close_to_background_label_keys() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        (
            "settings_view.general.keep_running_on_close",
            "settings_view.general.keep_running_on_close_hint",
        )
    } else {
        (
            "settings_view.general.minimize_to_tray_on_close",
            "settings_view.general.minimize_to_tray_on_close_hint",
        )
    }
}
