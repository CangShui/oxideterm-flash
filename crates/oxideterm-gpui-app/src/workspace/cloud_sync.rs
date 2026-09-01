// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud-sync runtime glue: config resolution, engine lifecycle, snapshot
//! build/apply against the live connection and settings stores.
//!
//! Snapshot documents are sealed end-to-end with a key derived from the room
//! passphrase (see `oxideterm_cloud_sync::crypto`), so the signaling worker
//! and unrelated peers never see connection metadata. A remote deletion of
//! saved sessions is surfaced through a three-way prompt (skip, apply, or
//! restore the local copy to the room) instead of being applied silently.

use super::*;

use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::modal::{dismissible_dialog_backdrop, overlay_content_boundary};
use oxideterm_settings::CloudSyncMode;
use zeroize::Zeroizing;

use oxideterm_cloud_sync::crypto::{decrypt_snapshot, derive_snapshot_key, encrypt_snapshot};

/// Default endpoint matches the repository's deployed Cloudflare Worker.
pub(crate) const DEFAULT_CLOUD_SYNC_SERVER: &str =
    "https://ditto-cloud-sync.cangshui.workers.dev";

const ENV_SYNC_JOIN: &str = "OXIDETERM_SYNC_JOIN";
const ENV_SYNC_URL: &str = "OXIDETERM_SYNC_URL";
const ENV_SYNC_ROOM: &str = "OXIDETERM_SYNC_ROOM";
const ENV_SYNC_MODE: &str = "OXIDETERM_SYNC_MODE";
const ENV_SYNC_DEVICE: &str = "OXIDETERM_SYNC_DEVICE";
const ENV_SYNC_PUSH: &str = "OXIDETERM_SYNC_PUSH";
const ENV_SYNC_PULL: &str = "OXIDETERM_SYNC_PULL";
const ENV_SYNC_APPLY: &str = "OXIDETERM_SYNC_APPLY";

#[derive(Clone, Debug)]
pub(crate) struct ResolvedCloudSyncConfig {
    pub server_url: String,
    pub room: String,
    pub device_id: String,
    pub transport: oxideterm_cloud_sync::TransportMode,
    /// Broadcast the local snapshot whenever the peer count changes.
    pub push_on_peer_change: bool,
    /// Send a snapshot request right after joining the room.
    pub pull_on_join: bool,
    /// Apply received snapshots newer than the local marker.
    pub auto_apply: bool,
}

pub(crate) struct CloudSyncRuntime {
    pub commands: tokio::sync::mpsc::Sender<oxideterm_cloud_sync::SyncCommand>,
    /// Shared key derived from the room passphrase. Owned by the runtime and
    /// zeroized on drop so secrets never outlive the sync session.
    snapshot_key: Zeroizing<[u8; 32]>,
}

/// Pending remote-deletion prompt. The envelope stays parked until the user
/// picks skip, apply, or restore-local-to-room.
#[derive(Clone, Debug)]
pub(crate) struct CloudSyncDeletePrompt {
    pub device: String,
    pub ts_ms: u64,
    pub deleted_count: usize,
    pub document: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloudSyncDeleteAction {
    /// Leave the local store untouched; discard the pending snapshot.
    Skip,
    /// Apply the remote snapshot, including the deletions.
    Sync,
    /// Discard the pending snapshot and broadcast the local copy so the room
    /// restores the sessions the remote deleted.
    RestoreRemote,
}

fn env_flag(key: &str) -> bool {
    std::env::var(key).is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

fn env_disabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "0")
        .unwrap_or(false)
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

/// Resolves the effective cloud-sync configuration from persisted settings
/// plus environment overrides. Returns `None` when syncing stays disabled.
pub(crate) fn resolve_config(settings: &PersistedSettings) -> Option<ResolvedCloudSyncConfig> {
    let force_join = env_flag(ENV_SYNC_JOIN);
    if !settings.cloud_sync.enabled && !force_join {
        return None;
    }
    let server_url = env_value(ENV_SYNC_URL)
        .or_else(|| non_empty(settings.cloud_sync.server_url.clone()))
        .unwrap_or_else(|| DEFAULT_CLOUD_SYNC_SERVER.to_string());
    let room = env_value(ENV_SYNC_ROOM).or_else(|| non_empty(settings.cloud_sync.room.clone()))?;
    let transport = env_value(ENV_SYNC_MODE)
        .and_then(|value| oxideterm_cloud_sync::TransportMode::parse(&value))
        .unwrap_or(match settings.cloud_sync.mode {
            CloudSyncMode::Auto => oxideterm_cloud_sync::TransportMode::Auto,
            CloudSyncMode::Relay => oxideterm_cloud_sync::TransportMode::Relay,
            CloudSyncMode::PeerToPeer => oxideterm_cloud_sync::TransportMode::PeerToPeer,
        });
    let device_id =
        env_value(ENV_SYNC_DEVICE).unwrap_or_else(|| oxideterm_cloud_sync::new_device_id());

    Some(ResolvedCloudSyncConfig {
        server_url,
        room,
        device_id,
        transport,
        push_on_peer_change: env_flag(ENV_SYNC_PUSH),
        pull_on_join: env_flag(ENV_SYNC_PULL),
        // Applying received snapshots is the point of syncing; keep it on by
        // default and allow opting out per instance for read-only senders.
        auto_apply: !env_disabled(ENV_SYNC_APPLY),
    })
}

impl WorkspaceApp {
    /// Starts the sync engine when enabled through settings or environment.
    pub(crate) fn autostart_cloud_sync(&mut self, cx: &mut Context<Self>) {
        eprintln!(
            "[cloud-sync] autostart: enabled={} room='{}' join_env={}",
            self.settings_store.settings().cloud_sync.enabled,
            self.settings_store.settings().cloud_sync.room,
            env_flag(ENV_SYNC_JOIN)
        );
        let Some(config) = resolve_config(self.settings_store.settings()) else {
            eprintln!("[cloud-sync] autostart: disabled by config");
            return;
        };
        let snapshot_key = match derive_snapshot_key(&config.room) {
            Ok(key) => key,
            Err(error) => {
                eprintln!("[cloud-sync] failed to derive snapshot key: {error}");
                return;
            }
        };
        eprintln!(
            "[cloud-sync] autostart: starting device={} server={} mode={:?}",
            config.device_id, config.server_url, config.transport
        );
        let runtime = self.forwarding_runtime.clone();
        let oxideterm_cloud_sync::SyncHandle { events: mut events, commands } =
            oxideterm_cloud_sync::spawn(
                oxideterm_cloud_sync::engine::SyncOptions {
                    server_url: config.server_url.clone(),
                    room_passphrase: config.room.clone(),
                    mode: config.transport,
                    device_id: config.device_id.clone(),
                },
                runtime.handle().clone(),
            );
        self.cloud_sync_status = Some(format!("device {}", config.device_id));
        self.cloud_sync = Some(CloudSyncRuntime { commands, snapshot_key });
        self.cloud_sync_config = Some(config);

        // Pump engine events onto the GPUI thread so snapshots apply through
        // the same entity-owned stores the rest of the app uses.
        cx.spawn(async move |workspace, cx| {
            while let Some(event) = events.recv().await {
                let outcome = workspace.update(cx, |workspace, cx| {
                    workspace.handle_cloud_sync_event(event, cx);
                });
                if outcome.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_cloud_sync_event(
        &mut self,
        event: oxideterm_cloud_sync::SyncEvent,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_cloud_sync::SyncEvent as Event;
        match event {
            Event::Joined { peers } => {
                eprintln!("[cloud-sync] joined room, peers={peers}");
                self.cloud_sync_status = Some(format!("joined ({peers} peers)"));
                if self.cloud_sync_push_on_peer_change() {
                    self.cloud_sync_broadcast_snapshot(cx);
                }
                if self.cloud_sync_pull_on_join() {
                    self.cloud_sync_request_snapshot();
                }
                cx.notify();
            }
            Event::PeerCount(peers) => {
                self.cloud_sync_status = Some(format!("{peers} peers"));
                if self.cloud_sync_push_on_peer_change() {
                    self.cloud_sync_broadcast_snapshot(cx);
                }
                cx.notify();
            }
            Event::PeerDirect { device } => {
                eprintln!("[cloud-sync] direct datachannel open: {device}");
                self.cloud_sync_status = Some(format!("direct link: {device}"));
                cx.notify();
            }
            Event::EnvelopeReceived(envelope) => {
                eprintln!(
                    "[cloud-sync] snapshot received from {} ts={}",
                    envelope.device, envelope.ts_ms
                );
                self.apply_cloud_sync_envelope(*envelope, cx);
            }
            Event::Notice(notice) => {
                self.cloud_sync_status = Some(notice);
                cx.notify();
            }
            Event::Closed(reason) => {
                self.cloud_sync_status = Some(format!("disconnected: {reason}"));
                cx.notify();
            }
        }
    }

    fn cloud_sync_push_on_peer_change(&self) -> bool {
        self.cloud_sync_config
            .as_ref()
            .is_some_and(|config| config.push_on_peer_change)
    }

    fn cloud_sync_pull_on_join(&self) -> bool {
        self.cloud_sync_config
            .as_ref()
            .is_some_and(|config| config.pull_on_join)
    }

    fn cloud_sync_auto_apply(&self) -> bool {
        self.cloud_sync_config
            .as_ref()
            .is_some_and(|config| config.auto_apply)
    }

    fn cloud_sync_snapshot_key(&self) -> Option<&Zeroizing<[u8; 32]>> {
        self.cloud_sync.as_ref().map(|runtime| &runtime.snapshot_key)
    }

    /// Builds an encrypted configuration snapshot document from the live
    /// stores. Connection passwords are embedded only when the user enabled
    /// `sync_passwords`; the sealed envelope keeps them private in transit.
    fn build_cloud_sync_snapshot(&self) -> Result<String, String> {
        let connections = self
            .connection_store
            .export_saved_connections_snapshot()
            .map_err(|error| error.to_string())?;
        let settings_snapshot = oxideterm_settings::export_oxide_settings_snapshot_json(
            self.settings_store.settings(),
            None,
            false,
        )
        .map_err(|error| error.to_string())?;
        let settings_value: serde_json::Value =
            serde_json::from_str(&settings_snapshot).map_err(|error| error.to_string())?;
        let mut document = serde_json::json!({
            "connections": connections,
            "settings": settings_value,
        });
        if self.settings_store.settings().cloud_sync.sync_passwords {
            if let Some(passwords) = self.resolve_connection_passwords() {
                document["passwords"] = serde_json::Value::Object(passwords);
            }
        }
        let plaintext = serde_json::to_string(&document).map_err(|error| error.to_string())?;
        let key = self
            .cloud_sync_snapshot_key()
            .ok_or_else(|| "cloud sync is not running".to_string())?;
        let sealed = encrypt_snapshot(key, &plaintext).map_err(|error| error.to_string())?;
        serde_json::to_string(&sealed).map_err(|error| error.to_string())
    }

    /// Resolves plaintext passwords for password-authenticated saved
    /// connections. Returns `None` when nothing is resolvable so callers can
    /// skip the field entirely.
    fn resolve_connection_passwords(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut passwords = serde_json::Map::new();
        for connection in self.connection_store.connections() {
            let matches_password_slot = matches!(
                connection.auth,
                oxideterm_connections::SavedAuth::Password { .. }
            );
            if !matches_password_slot {
                continue;
            }
            if let Ok(secret) = self.connection_store.get_connection_password(&connection.id) {
                passwords.insert(
                    connection.id.clone(),
                    serde_json::Value::String(secret.expose_secret().to_string()),
                );
            }
        }
        (!passwords.is_empty()).then_some(passwords)
    }

    pub(crate) fn cloud_sync_broadcast_snapshot(&mut self, cx: &mut Context<Self>) {
        let Some(runtime) = self.cloud_sync.as_ref() else {
            return;
        };
        match self.build_cloud_sync_snapshot() {
            Ok(snapshot) => {
                let _ = runtime
                    .commands
                    .try_send(oxideterm_cloud_sync::SyncCommand::Broadcast(snapshot));
            }
            Err(error) => {
                self.push_workspace_notice(
                    TerminalNotice {
                        title: self
                            .i18n
                            .t("settings_view.general.cloudsync.snapshot_build_failed"),
                        description: Some(error),
                        status_text: None,
                        progress: None,
                        variant: TerminalNoticeVariant::Error,
                    },
                    cx,
                );
            }
        }
    }

    pub(crate) fn cloud_sync_request_snapshot(&mut self) {
        let Some(runtime) = self.cloud_sync.as_ref() else {
            return;
        };
        let _ = runtime
            .commands
            .try_send(oxideterm_cloud_sync::SyncCommand::Request);
    }

    fn apply_cloud_sync_envelope(
        &mut self,
        envelope: oxideterm_cloud_sync::SyncEnvelope,
        cx: &mut Context<Self>,
    ) {
        if !self.cloud_sync_auto_apply() {
            return;
        }
        let Some(data) = envelope.data.as_ref() else { return };
        let Some(payload) = data.get("payload").and_then(|payload| payload.as_str()) else {
            return;
        };
        // The payload is the sealed envelope; without the shared key a frame
        // can never reach the merge layer.
        let key = match self.cloud_sync_snapshot_key() {
            Some(key) => key,
            None => return,
        };
        let sealed: oxideterm_cloud_sync::crypto::EncryptedSyncPayload =
            match serde_json::from_str(payload) {
                Ok(sealed) => sealed,
                Err(error) => {
                    eprintln!("[cloud-sync] envelope decode failed: {error}");
                    return;
                }
            };
        let plaintext = match decrypt_snapshot(key, &sealed) {
            Ok(plaintext) => plaintext,
            Err(error) => {
                eprintln!(
                    "[cloud-sync] envelope from {} failed authentication: {error}",
                    envelope.device
                );
                return;
            }
        };
        let Ok(document) = serde_json::from_str::<serde_json::Value>(plaintext.as_str()) else {
            return;
        };

        // Skip snapshots we have already applied so repeated broadcasts cannot
        // ping-pong between devices.
        let marker_path = marker_path_for(self.settings_store.path());
        let last_applied = read_last_applied_ts(&marker_path).unwrap_or(0);
        if envelope.ts_ms <= last_applied {
            return;
        }

        // Remote deletions are destructive: surface them before applying.
        if let Some(connections_value) = document.get("connections") {
            let snapshot: oxideterm_connections::SavedConnectionsSyncSnapshot =
                match serde_json::from_value(connections_value.clone()) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        eprintln!("[cloud-sync] connection snapshot decode failed: {error}");
                        return;
                    }
                };
            match self
                .connection_store
                .preview_saved_connections_snapshot_deletions(&snapshot)
            {
                Ok(deleted_count) if deleted_count > 0 => {
                    self.cloud_sync_delete_prompt = Some(CloudSyncDeletePrompt {
                        device: envelope.device,
                        ts_ms: envelope.ts_ms,
                        deleted_count,
                        document,
                    });
                    cx.notify();
                    return;
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("[cloud-sync] deletion preview failed: {error:#}");
                    return;
                }
            }
        }

        self.apply_cloud_sync_document(document, envelope.ts_ms, &envelope.device, cx);
    }

    /// Applies one validated snapshot document (connections + settings +
    /// optional passwords) and advances the sync marker.
    fn apply_cloud_sync_document(
        &mut self,
        document: serde_json::Value,
        ts_ms: u64,
        device: &str,
        cx: &mut Context<Self>,
    ) {
        let mut applied_connections = 0usize;
        if let Some(connections_value) = document.get("connections") {
            let snapshot: oxideterm_connections::SavedConnectionsSyncSnapshot =
                match serde_json::from_value(connections_value.clone()) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        eprintln!("[cloud-sync] connection snapshot decode failed: {error}");
                        return;
                    }
                };
            match self.connection_store.apply_saved_connections_snapshot(
                snapshot,
                oxideterm_connections::SavedConnectionsConflictStrategy::Merge,
            ) {
                Ok(outcome) => applied_connections = outcome.result.applied,
                Err(error) => {
                    eprintln!("[cloud-sync] connection snapshot apply failed: {error:#}");
                    return;
                }
            }
        }
        if let Some(settings_value) = document.get("settings") {
            if let Ok(settings_text) = serde_json::to_string(settings_value) {
                if let Ok(merged) = oxideterm_settings::merge_oxide_settings_snapshot(
                    self.settings_store.settings(),
                    &settings_text,
                    None,
                ) {
                    *self.settings_store.settings_mut() = merged;
                    let _ = self.settings_store.save();
                }
            }
        }
        if let Some(passwords) = document.get("passwords").and_then(|value| value.as_object()) {
            self.apply_synced_connection_passwords(passwords);
        }

        let marker_path = marker_path_for(self.settings_store.path());
        write_last_applied_ts(&marker_path, ts_ms);
        eprintln!(
            "[cloud-sync] snapshot applied: device={} connections={applied_connections}",
            device
        );
        self.push_workspace_notice(
            TerminalNotice {
                title: self.i18n.t("settings_view.general.cloudsync.applied"),
                description: Some(format!("{device} · {applied_connections}")),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            },
            cx,
        );
        cx.notify();
    }

    /// Stores passwords that arrived inside an encrypted snapshot. Only
    /// password slots that currently have no keychain reference are written;
    /// existing local credentials are never overwritten.
    fn apply_synced_connection_passwords(
        &mut self,
        passwords: &serde_json::Map<String, serde_json::Value>,
    ) {
        use oxideterm_connections::{ConnectionCredentialSlot, SavedAuth};
        for (id, value) in passwords {
            let Some(plaintext) = value.as_str() else {
                continue;
            };
            let Some(connection) = self.connection_store.get(id) else {
                continue;
            };
            let matches_password_slot = match &connection.auth {
                SavedAuth::Password { keychain_id, .. } => keychain_id.is_none(),
                _ => false,
            };
            if !matches_password_slot {
                continue;
            }
            let secret =
                oxideterm_connections::SecretString::from(Zeroizing::new(plaintext.to_string()));
            let _ = self
                .connection_store
                .store_connection_credential(id, ConnectionCredentialSlot::Primary, &secret);
        }
    }

    /// User decision for a pending remote-deletion prompt.
    pub(crate) fn resolve_cloud_sync_delete_prompt(
        &mut self,
        action: CloudSyncDeleteAction,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.cloud_sync_delete_prompt.take() else {
            return;
        };
        match action {
            CloudSyncDeleteAction::Skip => {
                eprintln!(
                    "[cloud-sync] user skipped remote deletion of {} sessions",
                    prompt.deleted_count
                );
            }
            CloudSyncDeleteAction::Sync => {
                self.apply_cloud_sync_document(prompt.document, prompt.ts_ms, &prompt.device, cx);
            }
            CloudSyncDeleteAction::RestoreRemote => {
                eprintln!(
                    "[cloud-sync] user restored local copy over {} remote deletions",
                    prompt.deleted_count
                );
                self.cloud_sync_broadcast_snapshot(cx);
            }
        }
        cx.notify();
    }

    /// Renders the three-way remote-deletion prompt inside the settings panel.
    /// The backdrop and the skip action both park the snapshot without
    /// applying it, matching the "not now" semantics of the request.
    pub(crate) fn render_cloud_sync_delete_prompt(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = self.cloud_sync_delete_prompt.as_ref()?;
        let theme = self.tokens.ui;
        let title = self.i18n_with(
            "settings_view.general.cloudsync.delete_prompt_title",
            &[("count", prompt.deleted_count.to_string())],
        );
        let description = self.i18n.t("settings_view.general.cloudsync.delete_prompt_desc");

        let actions = vec![
            (
                CloudSyncDeleteAction::Skip,
                self.i18n.t("settings_view.general.cloudsync.delete_prompt_skip"),
                ButtonVariant::Default,
            ),
            (
                CloudSyncDeleteAction::Sync,
                self.i18n.t("settings_view.general.cloudsync.delete_prompt_sync"),
                ButtonVariant::Default,
            ),
            (
                CloudSyncDeleteAction::RestoreRemote,
                self.i18n.t("settings_view.general.cloudsync.delete_prompt_restore"),
                ButtonVariant::Destructive,
            ),
        ];

        let dialog = div()
            .w(px(400.0))
            .rounded(px(self.tokens.radii.lg))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_elevated))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::Hsla::from(rgba(0x00000080)),
                offset: gpui::point(px(0.0), px(16.0)),
                blur_radius: px(32.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(10.0))
                    .px(px(24.0))
                    .pt(px(24.0))
                    .pb(px(16.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(20.0))
                            .text_color(rgb(theme.text_muted))
                            .child(description),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .children(actions.into_iter().map(|(action, label, variant)| {
                        let label = label.clone();
                        div()
                            .w_full()
                            .h(px(40.0))
                            .flex()
                            .items_center()
                            .justify_start()
                            .px(px(24.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(match variant {
                                ButtonVariant::Destructive => theme.error,
                                _ => theme.text,
                            }))
                            .hover(|element| element.bg(rgb(theme.bg_hover)))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.resolve_cloud_sync_delete_prompt(action, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(label)
                            .into_any_element()
                    })),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.resolve_cloud_sync_delete_prompt(CloudSyncDeleteAction::Skip, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(overlay_content_boundary(dialog))
                .into_any_element(),
        )
    }
}

fn marker_path_for(settings_path: &std::path::Path) -> std::path::PathBuf {
    settings_path
        .parent()
        .map(|parent| parent.join("cloud_sync_state.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("cloud_sync_state.json"))
}

fn read_last_applied_ts(settings_path: &std::path::Path) -> Option<u64> {
    let text = std::fs::read_to_string(marker_path_for(settings_path)).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("lastAppliedTsMs")?
        .as_u64()
}

fn write_last_applied_ts(settings_path: &std::path::Path, ts_ms: u64) {
    let path = marker_path_for(settings_path);
    let body = serde_json::json!({ "lastAppliedTsMs": ts_ms }).to_string();
    if let Err(error) = std::fs::write(&path, body) {
        eprintln!("[cloud-sync] failed to persist sync marker: {error}");
    }
}
