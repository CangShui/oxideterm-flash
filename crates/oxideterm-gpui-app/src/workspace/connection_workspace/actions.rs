use super::*;
use crate::workspace::new_connection::{
    NewConnectionTransport, terminal_serial_flow_from_profile, terminal_serial_parity_from_profile,
};
use oxideterm_gpui_terminal::TerminalNoticeVariant;
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopSecret,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn open_saved_serial_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .serial_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        let config = oxideterm_terminal::SerialSessionConfig {
            port_path: profile.port_path.clone(),
            baud_rate: profile.baud_rate,
            data_bits: profile.data_bits,
            stop_bits: profile.stop_bits,
            parity: terminal_serial_parity_from_profile(&profile.parity),
            flow_control: terminal_serial_flow_from_profile(&profile.flow_control),
        };
        match self.create_serial_terminal_tab(config, window, cx) {
            Ok(session_id) => {
                self.serial_terminal_profile_ids
                    .insert(session_id, profile.id.clone());
                self.register_terminal_trigger_saved_connection(
                    session_id,
                    oxideterm_terminal_triggers::SavedConnectionKind::Serial,
                    profile.id.clone(),
                    cx,
                );
                let _ = self.connection_store.mark_serial_profile_used(id);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.serial_profiles.open_failed")
                );
                self.push_command_palette_toast(status, None, TerminalNoticeVariant::Error, cx);
            }
        }
    }

    pub(in crate::workspace) fn open_saved_telnet_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self
            .connection_store
            .telnet_profiles()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
        else {
            return;
        };
        let config = oxideterm_terminal::TelnetSessionConfig {
            host: profile.host.clone(),
            port: profile.port,
        };
        match self.create_telnet_terminal_tab(config, profile.terminal, window, cx) {
            Ok(session_id) => {
                self.telnet_terminal_profile_ids
                    .insert(session_id, profile.id.clone());
                self.register_terminal_trigger_saved_connection(
                    session_id,
                    oxideterm_terminal_triggers::SavedConnectionKind::Telnet,
                    profile.id.clone(),
                    cx,
                );
                let _ = self.connection_store.mark_telnet_profile_used(id);
            }
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n.t("sessionManager.telnet_profiles.open_failed")
                );
                self.push_command_palette_toast(status, None, TerminalNoticeVariant::Error, cx);
            }
        }
    }

    pub(in crate::workspace) fn open_saved_remote_desktop_profile(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(saved) = self
            .connection_store
            .get_remote_desktop_profile(id)
            .cloned()
        else {
            return;
        };
        let password = match self.connection_store.get_remote_desktop_credential(id) {
            Ok(secret) => secret
                .map(SecretString::into_zeroizing)
                .map(RemoteDesktopSecret::from),
            Err(error) => {
                let status = format!(
                    "{}: {error}",
                    self.i18n
                        .t("sessionManager.remote_desktop_profiles.open_failed")
                );
                self.push_command_palette_toast(status, None, TerminalNoticeVariant::Error, cx);
                return;
            }
        };
        if password.is_none() {
            // Synced and imported assets intentionally omit device-local credentials.
            // Reopen the form bound to the SAME profile id: without the id the
            // submit would insert a duplicate asset on every password entry.
            // The SSH-only editing_saved_connection_id stays unset: setting it
            // flips the modal into EditProperties, which renders the SSH form
            // and routes the submit into the SSH store, dropping the update.
            let ungrouped_label = self.i18n.t("sessionManager.edit_properties.ungrouped");
            let auth_form =
                crate::workspace::new_connection::form_from_remote_desktop_profile(
                    &saved,
                    ungrouped_label,
                );
            self.open_new_connection_form(window, cx);
            let password_required = self
                .i18n
                .t("modals.new_connection.remote_desktop_password_required");
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    *form = auth_form;
                    form.error = Some(password_required);
                    form.focused_field = NewConnectionField::Password;
                }
            });
            return;
        }
        let profile = RemoteDesktopConnectionProfile {
            id: saved.id.clone(),
            label: saved.name,
            protocol: saved.protocol,
            endpoint: RemoteDesktopEndpoint::new(saved.host, saved.port),
            transport_endpoint: None,
            username: saved.username,
            domain: saved.domain,
            credential_ref: saved.credential_ref,
            read_only: saved.read_only,
            session_options: saved.session_options,
        };
        self.open_remote_desktop_connection_with_gateway(
            profile,
            password,
            saved.ssh_gateway_connection_id,
            window,
            cx,
        );
        let _ = self.connection_store.mark_remote_desktop_profile_used(id);
    }
}