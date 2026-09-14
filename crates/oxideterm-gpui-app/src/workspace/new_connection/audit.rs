// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Safe projections are built before formatting. No raw text or equality token leaves this module.
use super::form_state::*;
use oxideterm_connections::{
    ConnectionTerminalOptions, ConnectionX11ForwardingOptions, SavedUpstreamProxyProtocol,
    SshAlgorithmPreferences, StandaloneSftpTransferMode,
};
use oxideterm_remote_desktop::RemoteDesktopSessionOptions;
use std::{
    collections::{BTreeMap, hash_map::RandomState},
    hash::BuildHasher,
    sync::OnceLock,
};

#[derive(Clone, Eq, PartialEq)]
struct Value {
    summary: String,
    equality: u64,
}
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.summary)
    }
}
type Fields = BTreeMap<String, Value>;
trait Project {
    fn project(&self, path: &str, fields: &mut Fields);
}
impl Project for String {
    fn project(&self, path: &str, fields: &mut Fields) {
        // Process-random equality tokens detect same-length replacements without
        // copying secrets or exposing reusable password digests in diagnostics.
        static KEY: OnceLock<RandomState> = OnceLock::new();
        fields.insert(
            path.into(),
            Value {
                summary: format!("[文本已隐藏; 字符数={}]", self.chars().count()),
                equality: KEY.get_or_init(RandomState::new).hash_one(self),
            },
        );
    }
}
impl<T: Project> Project for Option<T> {
    fn project(&self, path: &str, fields: &mut Fields) {
        self.is_some().project(&format!("{path}.present"), fields);
        if let Some(value) = self {
            value.project(path, fields);
        }
    }
}
impl<T: Project> Project for Vec<T> {
    fn project(&self, path: &str, fields: &mut Fields) {
        self.len().project(&format!("{path}.count"), fields);
        for (index, value) in self.iter().enumerate() {
            value.project(&format!("{path}[{index}]"), fields);
        }
    }
}
macro_rules! scalar {
    ($($ty:ty),* $(,)?) => { $(impl Project for $ty {
        fn project(&self, path: &str, fields: &mut Fields) {
            fields.insert(path.into(), Value { summary: format!("{self:?}"), equality: 0 });
        }
    })* };
}
scalar!(
    bool,
    u8,
    u64,
    usize,
    ConnectionRouteTarget,
    NewConnectionField,
    NewConnectionTransport,
    NewConnectionUpstreamProxyAuth,
    NewConnectionUpstreamProxyPolicy,
    SshAuthTab,
    SavedUpstreamProxyProtocol,
    StandaloneSftpTransferMode,
    oxideterm_ssh::SshAlgorithmCategory,
    oxideterm_terminal::SerialFlowControl,
    oxideterm_terminal::SerialParity
);

// These metadata-only options implement Serialize; redact every string before logging.
fn metadata(value: &serde_json::Value, path: &str, fields: &mut Fields) {
    match value {
        serde_json::Value::String(text) => text.project(path, fields),
        serde_json::Value::Object(items) => {
            for (key, value) in items {
                metadata(value, &format!("{path}.{key}"), fields);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                metadata(value, &format!("{path}[{index}]"), fields);
            }
        }
        _ => {
            fields.insert(
                path.into(),
                Value {
                    summary: value.to_string(),
                    equality: 0,
                },
            );
        }
    }
}
macro_rules! metadata_options {
    ($($ty:ty),*) => { $(impl Project for $ty {
        fn project(&self, path: &str, fields: &mut Fields) {
            match serde_json::to_value(self) {
                Ok(value) => metadata(&value, path, fields),
                Err(_) => { fields.insert(path.into(), Value { summary: "[审计投影失败]".into(), equality: 0 }); }
            }
        }
    })* };
}
metadata_options!(
    ConnectionTerminalOptions,
    ConnectionX11ForwardingOptions,
    SshAlgorithmPreferences,
    RemoteDesktopSessionOptions
);
impl Project for oxideterm_terminal::SerialPortInfo {
    fn project(&self, path: &str, fields: &mut Fields) {
        self.port_path.project(&format!("{path}.port_path"), fields);
        self.display_name
            .project(&format!("{path}.display_name"), fields);
    }
}

// Exhaustive destructuring forces newly added form controls to choose an audit policy.
macro_rules! form_fields {
    ($ty:ident { $($field:ident),* $(,)? }) => {
        impl Project for $ty {
            fn project(&self, path: &str, fields: &mut Fields) {
                let Self { $($field),* } = self;
                $($field.project(&format!("{path}.{}", stringify!($field)), fields);)*
            }
        }
    };
}
form_fields!(NewConnectionForm {
    audit_trace_id,
    transport,
    local_shell_id,
    name,
    host,
    port,
    username,
    auth_tab,
    gssapi_enabled,
    gssapi_server_identity,
    gssapi_delegate_credentials,
    gssapi_credentials_available,
    gssapi_credentials_check_pending,
    password,
    remote_desktop_session_options,
    remote_desktop_profile_id,
    remote_desktop_ssh_gateway_connection_id,
    serial_profile_id,
    telnet_profile_id,
    standalone_sftp_profile_id,
    standalone_sftp_transfer_mode,
    standalone_sftp_secondary,
    saved_password_keychain_id,
    password_loaded,
    password_visible,
    key_path,
    managed_key_id,
    cert_path,
    passphrase,
    passphrase_visible,
    save_password,
    group,
    notes,
    sftp_initial_remote_path,
    post_connect_command,
    proxy_command_enabled,
    proxy_command,
    proxy_command_keychain_id,
    color,
    icon_background_color,
    icon,
    icon_picker_expanded,
    basic_section_expanded,
    authentication_section_expanded,
    route_section_expanded,
    standalone_sftp_secondary_route_section_expanded,
    ssh_options_section_expanded,
    terminal_section_expanded,
    appearance_section_expanded,
    remote_gateway_section_expanded,
    vnc_preferences_section_expanded,
    remote_features_section_expanded,
    serial_parameters_section_expanded,
    sftp_options_section_expanded,
    local_shell_section_expanded,
    advanced_connections_expanded,
    tags,
    proxy_hops,
    proxy_chain_expanded,
    jump_server_form,
    jump_server_edit_index,
    jump_server_target,
    upstream_proxy_policy,
    upstream_proxy_protocol,
    upstream_proxy_host,
    upstream_proxy_port,
    upstream_proxy_auth,
    upstream_proxy_username,
    upstream_proxy_password,
    upstream_proxy_password_keychain_id,
    upstream_proxy_remote_dns,
    upstream_proxy_no_proxy,
    agent_forwarding,
    identity_agent,
    agent_forwarding_socket,
    legacy_ssh_compatibility,
    ssh_algorithms,
    ssh_algorithm_editor_open,
    ssh_algorithm_editor_category,
    connect_timeout_seconds,
    connect_timeout_seconds_text,
    dedicated_new_terminal_connection,
    x11_forwarding,
    terminal,
    agent_available,
    save_connection,
    field_focused,
    focused_field,
    selected_field,
    error,
    success_feedback_message,
    pending,
    serial_ports,
    serial_ports_loading,
    serial_port_path,
    serial_baud_rate,
    serial_data_bits,
    serial_stop_bits,
    serial_parity,
    serial_flow_control,
    serial_profile_name,
    telnet_profile_name,
});
form_fields!(NewConnectionProxyHop {
    saved_connection_id,
    persisted_proxy_hop_index,
    host,
    port,
    username,
    auth_tab,
    password,
    key_path,
    managed_key_id,
    cert_path,
    passphrase,
    gssapi_enabled,
    gssapi_server_identity,
    gssapi_delegate_credentials,
    agent_forwarding,
    identity_agent,
    agent_forwarding_socket,
    legacy_ssh_compatibility,
    ssh_algorithms,
});
form_fields!(StandaloneSftpSecondaryForm {
    host,
    port,
    username,
    auth_tab,
    password,
    password_keychain_id,
    password_visible,
    key_path,
    managed_key_id,
    cert_path,
    passphrase,
    gssapi_enabled,
    gssapi_server_identity,
    gssapi_delegate_credentials,
    passphrase_visible,
    save_password,
    identity_agent,
    agent_available,
    legacy_ssh_compatibility,
    ssh_algorithms,
    connect_timeout_seconds,
    connect_timeout_seconds_text,
    initial_remote_path,
    proxy_hops,
    proxy_chain_expanded,
    proxy_command_enabled,
    proxy_command,
    proxy_command_keychain_id,
    upstream_proxy_policy,
    upstream_proxy_protocol,
    upstream_proxy_host,
    upstream_proxy_port,
    upstream_proxy_auth,
    upstream_proxy_username,
    upstream_proxy_password,
    upstream_proxy_password_keychain_id,
    upstream_proxy_remote_dns,
    upstream_proxy_no_proxy,
});

#[derive(Debug)]
pub(in crate::workspace) struct FormAuditSnapshot {
    trace_id: u64,
    fields: Fields,
}
pub(in crate::workspace) fn connection_form_audit_snapshot(
    form: &NewConnectionForm,
) -> FormAuditSnapshot {
    let mut fields = Fields::new();
    form.project("form", &mut fields);
    FormAuditSnapshot {
        trace_id: form.audit_trace_id,
        fields,
    }
}
pub(in crate::workspace) fn audit_connection_form_transition(
    before: Option<&FormAuditSnapshot>,
    after: Option<&FormAuditSnapshot>,
) {
    let trace_id = after
        .or(before)
        .map(|snapshot| snapshot.trace_id)
        .unwrap_or(0);
    if before.is_none()
        || after.is_none()
        || before
            .zip(after)
            .is_some_and(|(a, b)| a.trace_id != b.trace_id)
    {
        tracing::debug!(target: "oxideterm::audit", trace_id, stage = "session.form.lifecycle",
            previous_trace_id = before.map(|s| s.trace_id), open = after.is_some(),
            "会话表单打开、替换或关闭，敏感文本不写入日志");
    }
    let empty = Fields::new();
    let a = before.map(|s| &s.fields).unwrap_or(&empty);
    let b = after.map(|s| &s.fields).unwrap_or(&empty);
    for key in a.keys().chain(b.keys().filter(|key| !a.contains_key(*key))) {
        if a.get(key) != b.get(key) {
            tracing::debug!(target: "oxideterm::audit", trace_id, stage = "session.form.control_change",
                field = key, before = ?a.get(key), after = ?b.get(key),
                "会话控件状态已变化，包含非焦点字段、嵌套端点、校验反馈和异步结果");
        }
    }
}

pub(super) fn persistence<T>(
    trace_id: u64,
    protocol: &str,
    operation: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let started = std::time::Instant::now();
    tracing::debug!(target: "oxideterm::audit", trace_id, stage = "session.persistence.request", protocol,
        "会话保存请求进入存储层，开始校验并写入配置及受保护凭据");
    let result = operation();
    tracing::info!(target: "oxideterm::audit", trace_id, stage = "session.persistence.response", protocol,
        succeeded = result.is_ok(), elapsed_ms = started.elapsed().as_millis() as u64,
        "会话存储操作已返回，错误正文可能包含敏感输入，因此仅记录成功或失败");
    result
}

pub(in crate::workspace) fn audit_sync_document(
    trace_id: &str,
    direction: &str,
    document: &serde_json::Value,
) {
    // Only session metadata is projected. Never traverse the decrypted password map.
    for section in [
        "connections",
        "serialProfiles",
        "telnetProfiles",
        "standaloneSftpProfiles",
        "remoteDesktopProfiles",
    ] {
        let Some(snapshot) = document.get(section) else {
            continue;
        };
        let mut fields = Fields::new();
        metadata(snapshot, section, &mut fields);
        tracing::debug!(target: "oxideterm::audit", trace_id, stage = "session.sync.document",
            direction, section, fields = ?fields,
            "同步会话快照字段明细：数字与开关按值记录，所有文本隐藏，未读取密码映射");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_saved_protocols_report_control_changes_in_emitted_audit() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};
        #[derive(Clone)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl Write for Capture {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Capture(buffer.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            for transport in [NewConnectionTransport::Ssh, NewConnectionTransport::Serial,
                NewConnectionTransport::Telnet, NewConnectionTransport::StandaloneSftp,
                NewConnectionTransport::Rdp, NewConnectionTransport::Vnc] {
                let mut form = NewConnectionForm::default();
                form.transport = transport;
                form.password = "sample-secret-alpha".into();
                let before = connection_form_audit_snapshot(&form);
                form.password = "sample-secret-bravo".into();
                form.save_password = !form.save_password;
                let after = connection_form_audit_snapshot(&form);
                buffer.lock().unwrap().clear();
                audit_connection_form_transition(Some(&before), Some(&after));
                let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
                assert!(output.contains("form.password"), "{transport:?}");
                assert!(output.contains("form.save_password"), "{transport:?}");
                assert!(output.contains(&format!("trace_id={}", form.audit_trace_id)));
                assert!(!output.contains("sample-secret"));
            }
        });
    }

    #[test]
    fn audit_preserves_persistence_success_and_failure() {
        let mut writes = 0;
        let value = persistence(1, "serial", || {
            writes += 1;
            Ok(7)
        }).unwrap();
        assert_eq!((value, writes), (7, 1));
        let failure = persistence::<()>(1, "sftp", || {
            Err(anyhow::anyhow!("representative-private-error"))
        }).unwrap_err();
        assert_eq!(failure.to_string(), "representative-private-error");
    }

    #[test]
    fn audit_catches_unfocused_and_nested_controls_without_disclosing_text() {
        let mut form = NewConnectionForm::default();
        let before = connection_form_audit_snapshot(&form);
        form.save_password = !form.save_password;
        form.standalone_sftp_secondary.password = "secret-alpha".into();
        let after = connection_form_audit_snapshot(&form);
        assert_ne!(
            before.fields["form.save_password"],
            after.fields["form.save_password"]
        );
        assert_ne!(
            before.fields["form.standalone_sftp_secondary.password"],
            after.fields["form.standalone_sftp_secondary.password"]
        );
        assert!(!format!("{after:?}").contains("secret-alpha"));
        form.standalone_sftp_secondary.password = "secret-bravo".into();
        let replaced = connection_form_audit_snapshot(&form);
        assert_ne!(
            after.fields["form.standalone_sftp_secondary.password"],
            replaced.fields["form.standalone_sftp_secondary.password"]
        );
        assert_eq!(
            format!(
                "{:?}",
                after.fields["form.standalone_sftp_secondary.password"]
            ),
            format!(
                "{:?}",
                replaced.fields["form.standalone_sftp_secondary.password"]
            )
        );
    }
    #[test]
    fn audit_catches_removal_of_jump_credentials_and_profile_options() {
        let mut form = NewConnectionForm::default();
        form.standalone_sftp_secondary.host = "private-endpoint".into();
        form.notes = "token-in-notes".into();
        form.proxy_command = "credential-in-command".into();
        let mut hop = NewConnectionProxyHop::new();
        hop.password = "jump-credential".into();
        form.proxy_hops.push(hop);
        let before = connection_form_audit_snapshot(&form);
        form.serial_data_bits = 7;
        form.proxy_hops.clear();
        let after = connection_form_audit_snapshot(&form);
        assert!(before.fields.contains_key("form.proxy_hops[0].password"));
        assert!(!after.fields.contains_key("form.proxy_hops[0].password"));
        assert_ne!(
            before.fields["form.serial_data_bits"],
            after.fields["form.serial_data_bits"]
        );
        let output = format!("{before:?}");
        for secret in [
            "private-endpoint",
            "token-in-notes",
            "credential-in-command",
            "jump-credential",
        ] {
            assert!(!output.contains(secret));
        }
    }
}
