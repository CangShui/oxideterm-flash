use super::*;

/// Contains only values needed to build one modal frame after releasing the Entity borrow.
struct ConnectionFormModalSnapshot {
    transport: NewConnectionTransport,
    name: String,
    host: String,
    port: String,
    username: String,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
    gssapi_server_identity: String,
    gssapi_delegate_credentials: bool,
    gssapi_credentials_available: Option<bool>,
    password_present: bool,
    remote_desktop_profile_id: Option<String>,
    standalone_sftp_profile_id: Option<String>,
    serial_profile_id: Option<String>,
    telnet_profile_id: Option<String>,
    saved_password_keychain_id: Option<String>,
    password_loaded: bool,
    key_path: String,
    managed_key_id: String,
    cert_path: String,
    save_password: bool,
    group: String,
    notes: String,
    post_connect_command: String,
    proxy_command_enabled: bool,
    proxy_command: zeroize::Zeroizing<String>,
    proxy_command_configured: bool,
    color: String,
    icon_background_color: String,
    icon: String,
    icon_picker_expanded: bool,
    legacy_ssh_compatibility: bool,
    ssh_algorithm_editor_open: bool,
    agent_forwarding: bool,
    identity_agent: String,
    agent_available: Option<bool>,
    error: Option<String>,
    feedback_success: bool,
    pending: bool,
    serial_port_path: String,
    serial_baud_rate: String,
    sftp_initial_remote_path: String,
    connect_timeout_seconds_text: String,
}

impl ConnectionFormModalSnapshot {
    fn from_form(form: &NewConnectionForm) -> Self {
        Self {
            transport: form.transport,
            name: form.name.clone(),
            host: form.host.clone(),
            port: form.port.clone(),
            username: form.username.clone(),
            auth_tab: form.auth_tab,
            gssapi_enabled: form.gssapi_enabled,
            gssapi_server_identity: form.gssapi_server_identity.clone(),
            gssapi_delegate_credentials: form.gssapi_delegate_credentials,
            gssapi_credentials_available: form.gssapi_credentials_available,
            password_present: !form.password.is_empty(),
            remote_desktop_profile_id: form.remote_desktop_profile_id.clone(),
            standalone_sftp_profile_id: form.standalone_sftp_profile_id.clone(),
            serial_profile_id: form.serial_profile_id.clone(),
            telnet_profile_id: form.telnet_profile_id.clone(),
            saved_password_keychain_id: form.saved_password_keychain_id.clone(),
            password_loaded: form.password_loaded,
            key_path: form.key_path.clone(),
            managed_key_id: form.managed_key_id.clone(),
            cert_path: form.cert_path.clone(),
            save_password: form.save_password,
            group: form.group.clone(),
            notes: form.notes.clone(),
            post_connect_command: form.post_connect_command.clone(),
            proxy_command_enabled: form.proxy_command_enabled,
            // Rendering owns one bounded zeroizing copy; persisted forms retain only a keychain id.
            proxy_command: zeroize::Zeroizing::new(form.proxy_command.clone()),
            proxy_command_configured: form.proxy_command_keychain_id.is_some(),
            color: form.color.clone(),
            icon_background_color: form.icon_background_color.clone(),
            icon: form.icon.clone(),
            icon_picker_expanded: form.icon_picker_expanded,
            legacy_ssh_compatibility: form.legacy_ssh_compatibility,
            ssh_algorithm_editor_open: form.ssh_algorithm_editor_open,
            agent_forwarding: form.agent_forwarding,
            identity_agent: form.identity_agent.clone(),
            agent_available: form.agent_available,
            error: form.error.clone(),
            feedback_success: form.feedback_is_success(),
            pending: form.pending,
            serial_port_path: form.serial_port_path.clone(),
            serial_baud_rate: form.serial_baud_rate.clone(),
            sftp_initial_remote_path: form.sftp_initial_remote_path.clone(),
            connect_timeout_seconds_text: form.connect_timeout_seconds_text.clone(),
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_new_connection_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(ConnectionFormModalSnapshot::from_form)
        else {
            return div().into_any_element();
        };
        if form.gssapi_enabled && form.gssapi_credentials_available.is_none() {
            self.ensure_kerberos_credentials_availability(cx);
        }
        let theme = self.tokens.ui;
        let mode = new_connection_form_mode(
            self.connection_form_state(cx)
                .editing_saved_connection_id
                .as_deref(),
            self.connection_form_state(cx)
                .duplicating_saved_connection_id
                .as_deref(),
            self.connection_form_state(cx)
                .saved_connection_prompt_action,
        );
        let prompt_mode = mode == NewConnectionFormMode::SavedConnectionPrompt;
        let duplicate_mode = mode == NewConnectionFormMode::DuplicateTemplate;
        let edit_properties_mode = mode.submits_saved_connection_properties();
        let remote_desktop_edit_mode = form.remote_desktop_profile_id.is_some();
        let serial_edit_mode = form.serial_profile_id.is_some();
        let telnet_edit_mode = form.telnet_profile_id.is_some();
        let standalone_sftp_edit_mode = form.standalone_sftp_profile_id.is_some();
        // Saved non-SSH profiles edit persisted assets without acquiring a runtime owner.
        let saved_profile_edit_mode = remote_desktop_edit_mode
            || serial_edit_mode
            || telnet_edit_mode
            || standalone_sftp_edit_mode;
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let local_terminal_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && form.transport == NewConnectionTransport::LocalTerminal;
        let serial_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && form.transport == NewConnectionTransport::Serial;
        let telnet_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && form.transport == NewConnectionTransport::Telnet;
        let standalone_sftp_mode = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && form.transport == NewConnectionTransport::StandaloneSftp;
        if standalone_sftp_mode {
            return self.render_standalone_sftp_connection_modal(
                window,
                !saved_profile_edit_mode,
                cx,
            );
        }
        let remote_desktop_protocol = if !prompt_mode && !duplicate_mode && !edit_properties_mode {
            match form.transport {
                NewConnectionTransport::Rdp => {
                    Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
                }
                NewConnectionTransport::Vnc => {
                    Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc)
                }
                _ => None,
            }
        } else {
            None
        };
        let local_transport_mode = serial_mode || telnet_mode;
        let remote_desktop_mode = remote_desktop_protocol.is_some();
        let ssh_submission_mode = !local_terminal_mode
            && !local_transport_mode
            && !remote_desktop_mode
            && !standalone_sftp_mode;
        let shows_transport_selector = !prompt_mode
            && !duplicate_mode
            && !edit_properties_mode
            && !saved_profile_edit_mode;
        let shows_icon_field = connection_icon_field_visible(mode, form.transport);
        let title = if local_terminal_mode {
            self.i18n
                .t("modals.new_connection.transport_local_terminal")
        } else if prompt_mode {
            self.i18n
                .t("sessionManager.connect_prompt.title")
                .replace("{{name}}", &form.name)
        } else if duplicate_mode {
            self.i18n
                .t("sessionManager.edit_properties.duplicate_title")
        } else if edit_properties_mode || saved_profile_edit_mode {
            self.i18n.t("sessionManager.edit_properties.title")
        } else if standalone_sftp_mode {
            self.i18n.t("sftp.standalone.form_title")
        } else {
            self.i18n.t("ssh.form.title")
        };
        let description = if local_terminal_mode {
            self.i18n
                .t("modals.new_connection.local_terminal_description")
        } else if prompt_mode {
            format!("{}@{}:{}", form.username, form.host, form.port)
        } else if duplicate_mode {
            self.i18n
                .t("sessionManager.edit_properties.duplicate_description")
        } else if edit_properties_mode || saved_profile_edit_mode {
            self.i18n.t("sessionManager.edit_properties.description")
        } else if telnet_mode {
            self.i18n.t("modals.new_connection.telnet_description")
        } else if standalone_sftp_mode {
            self.i18n.t("sftp.standalone.form_description")
        } else if serial_mode {
            self.i18n.t("modals.new_connection.serial_description")
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
        {
            self.i18n.t("modals.new_connection.rdp_description")
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc)
        {
            self.i18n.t("modals.new_connection.vnc_description")
        } else {
            self.i18n.t("ssh.form.subtitle")
        };
        let connect_timeout_valid = form
            .connect_timeout_seconds_text
            .trim()
            .parse::<u64>()
            .is_ok_and(|seconds| seconds > 0);
        let has_required_fields = if local_terminal_mode {
            true
        } else if serial_mode {
            !form.serial_port_path.trim().is_empty()
                && form
                    .serial_baud_rate
                    .trim()
                    .parse::<u32>()
                    .is_ok_and(|baud| baud > 0)
        } else if telnet_mode {
            !form.host.trim().is_empty() && form.port.trim().parse::<u16>().is_ok()
        } else if remote_desktop_protocol
            == Some(oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp)
        {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && (remote_desktop_edit_mode
                    || form.password_present
                    || form.saved_password_keychain_id.is_some())
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else if remote_desktop_mode {
            !form.host.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok_and(|port| port > 0)
        } else {
            !form.host.trim().is_empty()
                && !form.username.trim().is_empty()
                && form.port.trim().parse::<u16>().is_ok()
                && connect_timeout_valid
        };
        let primary_disabled = form.pending || !has_required_fields;
        let form_visible = self.connection_form_state(cx).presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let base_modal_width = if prompt_mode || edit_properties_mode || saved_profile_edit_mode {
            TAURI_EDIT_MODAL_WIDTH
        } else if shows_transport_selector {
            self.tokens.metrics.modal_width
                + NEW_CONNECTION_TYPE_SIDEBAR_WIDTH
                + self.tokens.metrics.modal_section_gap
        } else {
            self.tokens.metrics.modal_width
        };
        let algorithm_editor_visible =
            form.ssh_algorithm_editor_open && (ssh_submission_mode || standalone_sftp_mode);
        let requested_modal_width = if algorithm_editor_visible {
            base_modal_width
                + SSH_ALGORITHM_CATEGORY_COLUMN_WIDTH
                + SSH_ALGORITHM_DETAIL_COLUMN_WIDTH
                + self.tokens.metrics.modal_section_gap * 2.0
        } else {
            base_modal_width
        };
        let available_modal_width = (f32::from(window.viewport_size().width)
            - NEW_CONNECTION_MODAL_VIEWPORT_MARGIN * 2.0)
            .max(TAURI_EDIT_MODAL_WIDTH);
        let modal_width = requested_modal_width.min(available_modal_width);
        // This is a long, continuously scrolling surface. A full-window
        // backdrop filter would run GPU blur and composite passes for every
        // scroll frame, so retain the dialog tint without the live blur.
        modal_backdrop(dialog_backdrop_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    // Tauri NewConnectionModal is a Radix Dialog; overlay
                    // pointer-down calls onOpenChange(false), which closes and
                    // restores focus to the active pane in native.
                    this.close_new_connection_form(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                oxideterm_gpui_ui::motion::form_transition(
                    &self.tokens,
                    "new-connection-form-enter",
                    modal_container(&self.tokens)
                .w(px(modal_width))
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(modal_header(&self.tokens, title, description))
                .child(
                    modal_body(&self.tokens)
                        .flex_1()
                        .min_h(px(0.0))
                        .child(
                            div()
                                .size_full()
                                .min_h(px(0.0))
                                .flex()
                                .gap(px(self.tokens.metrics.modal_section_gap))
                                .when(shows_transport_selector, |content| {
                                    content.child(self.render_transport_selector(cx))
                                })
                                .child(
                                    div()
                                        .id("new-connection-modal-form-scroll")
                                        .flex_1()
                                        .min_h(px(0.0))
                                        .min_w(px(0.0))
                                        .selectable_overflow_y_scroll(
                                            &self.selectable_text_scroll_handle(
                                                "new-connection-modal-form-scroll",
                                            ),
                                        )
                                        .on_scroll_wheel(cx.listener(
                                            |this, _event, _window, cx| {
                                                // Tauri/Radix closes select content when the modal
                                                // scroll body moves its trigger. Native caches the
                                                // trigger anchor explicitly, so clear both popup
                                                // ownership and the stale group-select bounds here.
                                                if this.connection_form_state(cx).open_select.is_some() {
                                                    this.close_new_connection_select(cx);
                                                    this.clear_new_connection_select_anchor(cx);
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(
                                            div()
                                        .flex()
                                        .flex_col()
                                        .min_w(px(0.0))
                                        .gap(px(self.tokens.metrics.modal_section_gap))
                                        .when(serial_mode, |content| {
                                            content.child(self.render_serial_form_branch(cx))
                                        })
                                        .when(telnet_mode, |content| {
                                            content.child(self.render_telnet_form_branch(cx))
                                        })
                                        .when_some(remote_desktop_protocol, |content, protocol| {
                                            content
                                                .child(self.render_remote_desktop_form_branch(protocol, cx))
                                        })
                                        .when(
                                            !serial_mode
                                                && !local_terminal_mode
                                                && !telnet_mode
                                                && !remote_desktop_mode,
                                            |content| {
                                                content
                                .when(standalone_sftp_mode, |content| {
                                    content.child(
                                        div()
                                            .p_3()
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgba(
                                                (theme.warning << 8)
                                                    | TAURI_PROMPT_FEEDBACK_BORDER_ALPHA,
                                            ))
                                            .bg(rgba(
                                                (theme.warning << 8)
                                                    | TAURI_PROMPT_FEEDBACK_ALPHA,
                                            ))
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .text_color(rgb(theme.text))
                                            .child(self.i18n.t("sftp.standalone.usage_notice")),
                                    )
                                })
                                .when(
                                    (ssh_submission_mode || standalone_sftp_mode) && !prompt_mode,
                                    |content| {
                                        let basic = div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(self.tokens.metrics.modal_section_gap))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.name"),
                                            &form.name,
                                            self.i18n.t("ssh.form.name_placeholder"),
                                            NewConnectionField::Name,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_group_select(
                                            if edit_properties_mode
                                                || standalone_sftp_edit_mode
                                            {
                                                self.i18n.t("sessionManager.edit_properties.group")
                                            } else {
                                                self.i18n.t("ssh.form.group")
                                            },
                                            &form.group,
                                            cx,
                                        ))
                                        .child(self.render_connection_notes_fields(&form.notes, cx))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .gap(px(self.tokens.metrics.form_host_port_gap))
                                                .child(div().flex_1().child(
                                                    self.render_connection_field(
                                                        if standalone_sftp_mode {
                                                            self.i18n.t("sftp.standalone.host")
                                                        } else {
                                                            self.i18n.t("ssh.form.host")
                                                        },
                                                        &form.host,
                                                        if standalone_sftp_mode {
                                                            self.i18n
                                                                .t("sftp.standalone.host_placeholder")
                                                        } else {
                                                            self.i18n.t("ssh.form.host_placeholder")
                                                        },
                                                        NewConnectionField::Host,
                                                        false,
                                                        cx,
                                                    ),
                                                ))
                                                .child(
                                                    div()
                                                        .w(px(self.tokens.metrics.form_port_width))
                                                        .child(self.render_connection_field(
                                                            if standalone_sftp_mode {
                                                                self.i18n.t("sftp.standalone.port")
                                                            } else {
                                                                self.i18n.t("ssh.form.port")
                                                            },
                                                            &form.port,
                                                            SSH_DEFAULT_PORT_TEXT.to_string(),
                                                            NewConnectionField::Port,
                                                            false,
                                                            cx,
                                                        )),
                                                ),
                                        )
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.username"),
                                            &form.username,
                                            "root".to_string(),
                                            NewConnectionField::Username,
                                            false,
                                            cx,
                                        ))
                                        .into_any_element();
                                        content.child(self.render_connection_form_section(
                                            ConnectionFormSection::Basic,
                                            basic,
                                            cx,
                                        ))
                                    },
                                )
                                .when(prompt_mode, |content| {
                                    content.child(self.render_connection_group_select(
                                        if edit_properties_mode {
                                            self.i18n.t("sessionManager.edit_properties.group")
                                        } else {
                                            self.i18n.t("ssh.form.group")
                                        },
                                        &form.group,
                                        cx,
                                    ))
                                })
                                .when_some(
                                    if prompt_mode {
                                        form.error.clone()
                                    } else {
                                        None
                                    },
                                    |content, error| {
                                        content.child(self.render_prompt_feedback_box(
                                            error,
                                            form.feedback_success,
                                        ))
                                    },
                                )
                                .child({
                                    let selector = self.render_auth_selector(
                                        form.auth_tab,
                                        if prompt_mode {
                                            AuthSelectorContext::Prompt
                                        } else if mode == NewConnectionFormMode::EditProperties
                                            || standalone_sftp_edit_mode
                                        {
                                            AuthSelectorContext::EditProperties
                                        } else {
                                            AuthSelectorContext::Standard
                                        },
                                        false,
                                        cx,
                                    );
                                    let authentication = div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(self.tokens.metrics.modal_section_gap))
                                        .child(self.render_connection_checkbox(
                                            self.i18n.t("ssh.form.kerberos_preferred"),
                                            form.gssapi_enabled,
                                            |form| form.gssapi_enabled = !form.gssapi_enabled,
                                            cx,
                                        ))
                                        .when(form.gssapi_enabled, |content| {
                                            content
                                                .child(self.render_connection_hint(
                                                    self.i18n.t("ssh.form.gssapi_desc"),
                                                ))
                                                .child(self.render_connection_field(
                                                    self.i18n.t(
                                                        "ssh.form.gssapi_server_identity",
                                                    ),
                                                    &form.gssapi_server_identity,
                                                    self.i18n.t(
                                                        "ssh.form.gssapi_server_identity_placeholder",
                                                    ),
                                                    NewConnectionField::GssapiServerIdentity,
                                                    false,
                                                    cx,
                                                ))
                                                .child(self.render_connection_hint(
                                                    self.i18n.t(
                                                        "ssh.form.gssapi_server_identity_hint",
                                                    ),
                                                ))
                                                .child(self.render_kerberos_credentials_status(
                                                    form.gssapi_credentials_available,
                                                ))
                                                .child(
                                                    self.render_connection_checkbox_with_warning(
                                                        "kerberos-delegation-help",
                                                        "kerberos-delegation-tooltip",
                                                        "ssh.form.gssapi_delegate_credentials",
                                                        "ssh.form.gssapi_delegation_warning",
                                                        form.gssapi_delegate_credentials,
                                                        |form| {
                                                            form.gssapi_delegate_credentials =
                                                                !form.gssapi_delegate_credentials;
                                                        },
                                                        cx,
                                                    ),
                                                )
                                        })
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(self.i18n.t(
                                                    "ssh.form.fallback_authentication",
                                                )),
                                        )
                                        .child(selector)
                                .when(form.auth_tab == SshAuthTab::Password, |content| {
                                    if edit_properties_mode
                                        && form.saved_password_keychain_id.is_some()
                                    {
                                        let content = if form.password_loaded {
                                            content.child(self.render_connection_secret_field(
                                                self.i18n.t(
                                                    "sessionManager.edit_properties.saved_password",
                                                ),
                                                String::new(),
                                                NewConnectionField::Password,
                                                cx,
                                            ))
                                        } else {
                                            content.child(
                                                self.render_edit_saved_password_field(cx),
                                            )
                                        };
                                        content.child(self.render_connection_hint(
                                            self.i18n.t(
                                                "sessionManager.edit_properties.password_hint",
                                            ),
                                        ))
                                    } else if edit_properties_mode {
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    } else if prompt_mode {
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    } else if standalone_sftp_edit_mode {
                                        content
                                            .child(self.render_connection_secret_field(
                                                self.i18n.t("ssh.form.password"),
                                                String::new(),
                                                NewConnectionField::Password,
                                                cx,
                                            ))
                                            .when(
                                                form.saved_password_keychain_id.is_some(),
                                                |content| {
                                                    content.child(self.render_connection_hint(
                                                        self.i18n.t(
                                                            "sessionManager.edit_properties.password_hint",
                                                        ),
                                                    ))
                                                },
                                            )
                                    } else {
                                        // Main SSH passwords use the same always-save keychain
                                        // contract as RDP/VNC; omit the divergent opt-out checkbox.
                                        content.child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.password"),
                                            String::new(),
                                            NewConnectionField::Password,
                                            cx,
                                        ))
                                    }
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::DefaultKey
                                        && !prompt_mode
                                        && !edit_properties_mode
                                        && !standalone_sftp_edit_mode,
                                    |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.default_key_desc"),
                                            ))
                                            .child(self.render_connection_secret_field(
                                                self.i18n.t("ssh.form.passphrase"),
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                cx,
                                            ))
                                    },
                                )
                                .when(
                                    form.auth_tab == SshAuthTab::SshKey
                                        || ((prompt_mode
                                            || edit_properties_mode
                                            || standalone_sftp_edit_mode)
                                            && form.auth_tab == SshAuthTab::DefaultKey),
                                    |content| {
                                        let key_label = if edit_properties_mode
                                            || standalone_sftp_edit_mode
                                        {
                                            self.i18n.t("sessionManager.edit_properties.key_path")
                                        } else {
                                            self.i18n.t("ssh.form.key_file")
                                        };
                                        let key_placeholder: String =
                                            "~/.ssh/id_ed25519".to_string();
                                        let key_field = if prompt_mode {
                                            self.render_connection_field(
                                                key_label,
                                                &form.key_path,
                                                key_placeholder,
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                key_label,
                                                &form.key_path,
                                                key_placeholder,
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        };
                                        content
                                            .child(key_field)
                                            .child(self.render_connection_secret_field(
                                                self.i18n.t("ssh.form.passphrase"),
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                cx,
                                            ))
                                            .when(
                                                edit_properties_mode
                                                    || standalone_sftp_edit_mode,
                                                |content| {
                                                    content.child(self.render_connection_hint(
                                                        self.i18n.t(
                                                            "sessionManager.edit_properties.passphrase_hint",
                                                        ),
                                                    ))
                                                },
                                            )
                                    },
                                )
                                .when(form.auth_tab == SshAuthTab::ManagedKey, |content| {
                                    content
                                        .child(self.render_managed_key_select(
                                            self.i18n.t("ssh.form.managed_key"),
                                            &form.managed_key_id,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            cx,
                                        ))
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.managed_key_hint"),
                                        ))
                                })
                                .when(form.auth_tab == SshAuthTab::Certificate, |content| {
                                    let content = if prompt_mode {
                                        content
                                    } else {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.certificate_note"),
                                        ))
                                    };
                                    content
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        })
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                cx,
                                            )
                                        })
                                        .child(self.render_connection_secret_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            cx,
                                        ))
                                        .when(
                                            edit_properties_mode
                                                || standalone_sftp_edit_mode,
                                            |content| {
                                                content.child(self.render_connection_hint(
                                                    self.i18n.t(
                                                        "sessionManager.edit_properties.passphrase_hint",
                                                    ),
                                                ))
                                            },
                                        )
                                })
                                .when(form.auth_tab == SshAuthTab::Agent, |content| {
                                    let content = content
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.agent_desc"),
                                        ))
                                        .when(!prompt_mode, |content| {
                                            content
                                                .child(self.render_connection_field(
                                                    self.i18n.t("ssh.form.agent_endpoint"),
                                                    &form.identity_agent,
                                                    self.i18n
                                                        .t("ssh.form.agent_endpoint_placeholder"),
                                                    NewConnectionField::IdentityAgent,
                                                    false,
                                                    cx,
                                                ))
                                                .child(self.render_connection_hint(
                                                    self.i18n.t("ssh.form.agent_endpoint_hint"),
                                                ))
                                                .child(self.render_agent_status(
                                                    form.agent_available,
                                                ))
                                        });
                                    if !prompt_mode {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.agent_hint"),
                                        ))
                                    } else {
                                        content
                                    }
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::TwoFactor
                                        && !prompt_mode
                                        && !edit_properties_mode,
                                    |content| {
                                        content
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.two_factor_desc"),
                                            ))
                                            .child(self.render_connection_hint(
                                                self.i18n.t("ssh.form.two_factor_hint"),
                                            ))
                                            .child(self.render_connection_hint_with_color(
                                                self.i18n.t("ssh.form.two_factor_warning"),
                                                self.tokens.ui.warning,
                                            ))
                                    },
                                )
                                .into_any_element();
                                    if (ssh_submission_mode || standalone_sftp_mode)
                                        && !prompt_mode
                                    {
                                        self.render_connection_form_section(
                                            ConnectionFormSection::Authentication,
                                            authentication,
                                            cx,
                                        )
                                    } else {
                                        authentication
                                    }
                                })
                                .when(
                                    !prompt_mode && !standalone_sftp_mode,
                                    |content| {
                                    let route_body = div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(self.tokens.metrics.modal_section_gap))
                                        .child(self.render_proxy_command_section(
                                            form.proxy_command_enabled,
                                            &form.proxy_command,
                                            form.proxy_command_configured,
                                            false,
                                            cx,
                                        ))
                                        // ProxyCommand replaces the regular route without erasing
                                        // its saved upstream-proxy or jump-server configuration.
                                        .when(!form.proxy_command_enabled, |route| {
                                            route
                                                .child(self.render_upstream_proxy_policy_section(false, cx))
                                                .child(self.render_proxy_chain_section(false, cx))
                                        })
                                        .into_any_element();
                                    let ssh_options_body = self.render_connection_ssh_options(
                                        form.agent_forwarding,
                                        form.legacy_ssh_compatibility,
                                        cx,
                                    );
                                    let terminal_body = div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(self.tokens.metrics.modal_section_gap))
                                        .child(self.render_connection_terminal_options(cx))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.post_connect_command"),
                                            &form.post_connect_command,
                                            self.i18n
                                                .t("ssh.form.post_connect_command_placeholder"),
                                            NewConnectionField::PostConnectCommand,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.post_connect_command_hint"),
                                        ))
                                        .into_any_element();
                                    content
                                        .child(self.render_connection_form_section(
                                            ConnectionFormSection::Route,
                                            route_body,
                                            cx,
                                        ))
                                        .child(self.render_connection_form_section(
                                            ConnectionFormSection::SshOptions,
                                            ssh_options_body,
                                            cx,
                                        ))
                                        .child(self.render_connection_form_section(
                                            ConnectionFormSection::Terminal,
                                            terminal_body,
                                            cx,
                                        ))
                                    },
                                )
                                .when(standalone_sftp_mode, |content| {
                                    let route_body = div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(self.tokens.metrics.modal_section_gap))
                                        .child(self.render_proxy_command_section(
                                            form.proxy_command_enabled,
                                            &form.proxy_command,
                                            form.proxy_command_configured,
                                            false,
                                            cx,
                                        ))
                                        .when(!form.proxy_command_enabled, |route| {
                                            route
                                                .child(self.render_upstream_proxy_policy_section(false, cx))
                                                .child(self.render_proxy_chain_section(false, cx))
                                        })
                                        .into_any_element();
                                    let sftp_options = self.render_standalone_sftp_options(
                                        &form.sftp_initial_remote_path,
                                        &form.connect_timeout_seconds_text,
                                        cx,
                                    );
                                    content
                                        .child(self.render_connection_form_section(
                                            ConnectionFormSection::Route,
                                            route_body,
                                            cx,
                                        ))
                                        .child(self.render_connection_form_section(
                                            ConnectionFormSection::SftpOptions,
                                            sftp_options,
                                            cx,
                                        ))
                                })
                                    })
                                    .when(shows_icon_field, |content| {
                                        let appearance_body = div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(self.tokens.metrics.modal_section_gap))
                                            .child(self.render_edit_icon_field(
                                                &form.icon,
                                                &form.color,
                                                &form.icon_background_color,
                                                form.icon_picker_expanded,
                                                cx,
                                            ))
                                            .child(
                                            // The two asset colors form one responsive pair:
                                            // share a row when the modal has room and wrap together
                                            // without compressing either input below a usable width.
                                            div()
                                                .flex()
                                                .flex_row()
                                                .flex_wrap()
                                                .gap(px(self.tokens.spacing.three))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w(px(
                                                            CONNECTION_ICON_COLOR_CONTROL_MIN_WIDTH,
                                                        ))
                                                        .child(self.render_edit_color_field(
                                                            self.i18n.t(
                                                                "sessionManager.edit_properties.icon_color",
                                                            ),
                                                            &form.color,
                                                            NewConnectionField::Color,
                                                            cx,
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w(px(
                                                            CONNECTION_ICON_COLOR_CONTROL_MIN_WIDTH,
                                                        ))
                                                        .child(self.render_edit_color_field(
                                                            self.i18n.t(
                                                                "sessionManager.edit_properties.icon_background_color",
                                                            ),
                                                            &form.icon_background_color,
                                                            NewConnectionField::IconBackgroundColor,
                                                            cx,
                                                        )),
                                                ),
                                            )
                                            .into_any_element();
                                        content.child(self.render_connection_form_section(
                                            ConnectionFormSection::Appearance,
                                            appearance_body,
                                            cx,
                                        ))
                                    })
                                ),
                        )
                        .when(algorithm_editor_visible, |content| {
                            content
                                .child(self.render_ssh_algorithm_category_column(cx))
                                .child(self.render_ssh_algorithm_detail_column(cx))
                        })
                        )
                        .when_some(
                            if prompt_mode {
                                None
                            } else {
                                form.error.clone()
                            },
                            |content, error| {
                                let feedback_color = if form.feedback_success {
                                    theme.success
                                } else {
                                    theme.error
                                };
                                content.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(feedback_color))
                                        .child(error),
                                )
                            },
                        ),
                )
                .child(
                    modal_footer(&self.tokens)
                        .flex_none()
                        .child(self.render_connection_button(
                            self.i18n.t("ssh.form.cancel"),
                            false,
                            ConnectionButtonAction::Cancel,
                            false,
                            cx,
                        ))
                        .when(
                            local_terminal_mode,
                            |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("modals.new_connection.local_terminal_open"),
                                    true,
                                    ConnectionButtonAction::Connect,
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            !edit_properties_mode
                                && self.connection_form_state(cx).saved_connection_prompt_action.is_none()
                                && (ssh_submission_mode || standalone_sftp_mode),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("ssh.form.test"),
                                    false,
                                    ConnectionButtonAction::Test,
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            !edit_properties_mode
                                && !saved_profile_edit_mode
                                && self.connection_form_state(cx).saved_connection_prompt_action.is_none()
                                && !remote_desktop_mode
                                && !local_terminal_mode,
                            |footer| {
                                footer
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save"),
                                        false,
                                        ConnectionButtonAction::Save,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_connection_button(
                                        if local_transport_mode {
                                            self.i18n.t("modals.new_connection.local_open")
                                        } else if standalone_sftp_mode {
                                            self.i18n.t("sftp.standalone.open")
                                        } else {
                                            self.i18n.t("ssh.form.connect")
                                        },
                                        false,
                                        ConnectionButtonAction::Connect,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_connection_button(
                                        if local_transport_mode {
                                            self.i18n.t("modals.new_connection.local_save_and_open")
                                        } else if standalone_sftp_mode {
                                            self.i18n.t("sftp.standalone.save_and_open")
                                        } else {
                                            self.i18n.t("ssh.form.save_and_connect")
                                        },
                                        true,
                                        ConnectionButtonAction::SaveAndConnect,
                                        primary_disabled,
                                        cx,
                                    ))
                            },
                        )
                        .when(
                            edit_properties_mode
                                || saved_profile_edit_mode
                                || self.connection_form_state(cx).saved_connection_prompt_action.is_some(),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    if self.connection_form_state(cx).saved_connection_prompt_action
                                        == Some(SavedConnectionPromptAction::Test)
                                    {
                                        self.i18n.t("ssh.form.test")
                                    } else if self.connection_form_state(cx).saved_connection_prompt_action
                                        == Some(SavedConnectionPromptAction::Connect)
                                    {
                                        self.i18n.t("ssh.form.connect")
                                    } else if edit_properties_mode
                                        && self
                                            .connection_form_state(cx).editing_saved_connection_connect_after_save_node_id
                                            .is_some()
                                    {
                                        self.i18n
                                            .t("sessionManager.edit_properties.save_and_reconnect")
                                    } else if edit_properties_mode || saved_profile_edit_mode {
                                        self.i18n.t("sessionManager.edit_properties.save")
                                    } else {
                                        self.i18n.t("modals.new_connection.local_open")
                                    },
                                    true,
                                    if (edit_properties_mode || saved_profile_edit_mode)
                                        && self.connection_form_state(cx).saved_connection_prompt_action.is_none()
                                    {
                                        ConnectionButtonAction::Save
                                    } else {
                                        ConnectionButtonAction::Connect
                                    },
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .when(
                            remote_desktop_mode
                                && !edit_properties_mode
                                && !remote_desktop_edit_mode
                                && self.connection_form_state(cx).saved_connection_prompt_action.is_none(),
                            |footer| {
                                footer
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save"),
                                        false,
                                        ConnectionButtonAction::Save,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.connect"),
                                        false,
                                        ConnectionButtonAction::Connect,
                                        primary_disabled,
                                        cx,
                                    ))
                                    .child(self.render_connection_button(
                                        self.i18n.t("ssh.form.save_and_connect"),
                                        true,
                                        ConnectionButtonAction::SaveAndConnect,
                                        primary_disabled,
                                        cx,
                                    ))
                            },
                        ),
                    ),
                    form_visible,
                ),
        )
        .when(!form_visible, |backdrop| {
            // The exiting payload remains mounted only for painting; this top
            // event shield prevents stale form actions during delayed cleanup.
            backdrop.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
            )
        })
        .into_any_element()
    }
}
