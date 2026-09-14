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

pub(crate) const DEFAULT_CLOUD_SYNC_SERVER: &str = "";

const ENV_SYNC_JOIN: &str = "OXIDETERM_SYNC_JOIN";
const ENV_SYNC_URL: &str = "OXIDETERM_SYNC_URL";
const ENV_SYNC_ROOM: &str = "OXIDETERM_SYNC_ROOM";
const ENV_SYNC_MODE: &str = "OXIDETERM_SYNC_MODE";
const ENV_SYNC_DEVICE: &str = "OXIDETERM_SYNC_DEVICE";
const ENV_SYNC_PUSH: &str = "OXIDETERM_SYNC_PUSH";
const ENV_SYNC_PULL: &str = "OXIDETERM_SYNC_PULL";
const ENV_SYNC_APPLY: &str = "OXIDETERM_SYNC_APPLY";
const CLOUD_SYNC_AUTO_PUSH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CloudSyncFileState {
    modified_nanos: u128,
    length: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CloudSyncStoreState {
    settings: Option<CloudSyncFileState>,
    connections: Option<CloudSyncFileState>,
}

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
    pub trace_id: String,
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
        // Automatic convergence is the normal product behavior. Environment
        // overrides can still disable either edge for controlled diagnostics.
        push_on_peer_change: !env_disabled(ENV_SYNC_PUSH),
        pull_on_join: !env_disabled(ENV_SYNC_PULL),
        // Applying received snapshots is the point of syncing; keep it on by
        // default and allow opting out per instance for read-only senders.
        auto_apply: !env_disabled(ENV_SYNC_APPLY),
    })
}

impl WorkspaceApp {
    /// Keeps a short, human-readable trail for diagnosing whether the room
    /// connected, whether a snapshot was sent, and what changed locally.
    fn record_cloud_sync_log(&mut self, message: String) {
        const CLOUD_SYNC_LOG_LIMIT: usize = 8;
        self.cloud_sync_logs.push_back(message);
        while self.cloud_sync_logs.len() > CLOUD_SYNC_LOG_LIMIT {
            self.cloud_sync_logs.pop_front();
        }
    }

    fn set_cloud_sync_progress(&mut self, progress: f32) {
        self.cloud_sync_progress = Some(progress.clamp(0.0, 1.0));
    }

    /// Starts (or restarts) the sync engine when enabled through settings or
    /// environment. The generation token lets an old event pump die quietly
    /// after the user saves a new server or room.
    pub(crate) fn autostart_cloud_sync(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync_generation = self.cloud_sync_generation.wrapping_add(1);
        let generation = self.cloud_sync_generation;
        self.cloud_sync_auto_push_task = None;
        let Some(config) = resolve_config(self.settings_store.settings()) else {
            let trace_id = crate::logging::next_audit_trace_id();
            tracing::info!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.lifecycle.validation",
                result = "disabled",
                reason = "cloud sync is disabled or the room is empty",
                business_impact = "no automatic sync worker or change watcher is running",
                "cloud sync startup was rejected before transport initialization"
            );
            self.record_cloud_sync_log(
                self.i18n
                    .t("settings_view.general.cloudsync.status_disabled"),
            );
            self.cloud_sync = None;
            self.cloud_sync_config = None;
            self.cloud_sync_observed_store_state = None;
            self.set_cloud_sync_progress(0.0);
            self.cloud_sync_status = Some(
                self.i18n
                    .t("settings_view.general.cloudsync.status_disabled"),
            );
            cx.notify();
            return;
        };
        let snapshot_key = match derive_snapshot_key(&config.room) {
            Ok(key) => key,
            Err(error) => {
                let trace_id = crate::logging::next_audit_trace_id();
                tracing::warn!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "sync.lifecycle.key",
                    result = "failed",
                    error_kind = %error,
                    secret_detail_redacted = true,
                    business_impact = "the sync engine was not started",
                    "cloud sync snapshot-key derivation failed"
                );
                self.record_cloud_sync_log("failed to derive snapshot key".to_string());
                return;
            }
        };
        let runtime = self.forwarding_runtime.clone();
        let oxideterm_cloud_sync::SyncHandle { mut events, commands } =
            oxideterm_cloud_sync::spawn(
                oxideterm_cloud_sync::engine::SyncOptions {
                    server_url: config.server_url.clone(),
                    room_passphrase: config.room.clone(),
                    mode: config.transport,
                    device_id: config.device_id.clone(),
                },
                runtime.handle().clone(),
            );
        self.record_cloud_sync_log(format!(
            "{}",
            self.i18n_replace(
                "settings_view.general.cloudsync.log_started",
                &[("device", config.device_id.clone())]
            )
        ));
        self.cloud_sync_status = Some(format!("device {}", config.device_id));
        self.cloud_sync = Some(CloudSyncRuntime { commands, snapshot_key });
        self.cloud_sync_config = Some(config);
        self.cloud_sync_observed_store_state = Some(self.cloud_sync_store_state());
        self.set_cloud_sync_progress(0.05);
        self.start_cloud_sync_auto_push_watch(generation, cx);
        let trace_id = self.new_cloud_sync_trace_id();
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.lifecycle.response",
            generation,
            result = "started",
            auto_push_interval_ms = CLOUD_SYNC_AUTO_PUSH_INTERVAL.as_millis() as u64,
            push_on_peer_change = self.cloud_sync_push_on_peer_change(),
            pull_on_join = self.cloud_sync_pull_on_join(),
            auto_apply = self.cloud_sync_auto_apply(),
            secret_detail_redacted = true,
            business_impact = "the transport event pump and automatic local-change watcher are running",
            "cloud sync runtime startup completed"
        );
        cx.notify();

        // Pump engine events onto the GPUI thread so snapshots apply through
        // the same entity-owned stores the rest of the app uses.
        cx.spawn(async move |workspace, cx| {
            while let Some(event) = events.recv().await {
                let outcome = workspace.update(cx, |workspace, cx| {
                    if workspace.cloud_sync_generation != generation {
                        return;
                    }
                    workspace.handle_cloud_sync_event(event, cx);
                });
                if outcome.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn cloud_sync_store_state(&self) -> CloudSyncStoreState {
        CloudSyncStoreState {
            settings: cloud_sync_file_state(self.settings_store.path()),
            connections: cloud_sync_file_state(self.connection_store.path()),
        }
    }

    fn start_cloud_sync_auto_push_watch(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        // Workspace lifetime owns the watcher. Replacing the task on restart
        // is its cancellation path, so no sync observer survives its runtime.
        self.cloud_sync_auto_push_task = Some(cx.spawn(async move |workspace, cx| {
            loop {
                cx.background_executor()
                    .timer(CLOUD_SYNC_AUTO_PUSH_INTERVAL)
                    .await;
                let should_continue = workspace
                    .update(cx, |workspace, cx| {
                        if workspace.cloud_sync_generation != generation
                            || workspace.cloud_sync.is_none()
                        {
                            return false;
                        }
                        workspace.detect_and_push_cloud_sync_local_change(cx);
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    fn detect_and_push_cloud_sync_local_change(&mut self, cx: &mut Context<Self>) {
        let current = self.cloud_sync_store_state();
        let Some(previous) = self.cloud_sync_observed_store_state else {
            self.cloud_sync_observed_store_state = Some(current);
            return;
        };
        if current == previous {
            return;
        }
        let trace_id = self.new_cloud_sync_trace_id();
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.auto_push.request",
            settings_changed = current.settings != previous.settings,
            connections_changed = current.connections != previous.connections,
            debounce_ms = CLOUD_SYNC_AUTO_PUSH_INTERVAL.as_millis() as u64,
            result = "detected",
            business_impact = "a persisted local change will be broadcast automatically",
            "cloud sync automatic change watcher detected a local store update"
        );
        if !self.cloud_sync_broadcast_snapshot_with_trace(
            trace_id.clone(),
            "automatic-store-change",
            cx,
        ) {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.auto_push.response",
                result = "retry_pending",
                reason = "the snapshot could not be queued",
                business_impact = "the unchanged observation marker will make the watcher retry",
                "cloud sync automatic broadcast will be retried"
            );
        }
    }

    fn new_cloud_sync_trace_id(&self) -> String {
        let sequence = crate::logging::next_audit_trace_id();
        let device = self
            .cloud_sync_config
            .as_ref()
            .map(|config| config.device_id.as_str())
            .unwrap_or("unknown");
        format!("cloud-sync-{device}-{sequence}")
    }

    fn handle_cloud_sync_event(
        &mut self,
        event: oxideterm_cloud_sync::SyncEvent,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_cloud_sync::SyncEvent as Event;
        match event {
            Event::Joined { peers } => {
                let trace_id = self.new_cloud_sync_trace_id();
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "sync.room.joined",
                    peers,
                    push_on_peer_change = self.cloud_sync_push_on_peer_change(),
                    pull_on_join = self.cloud_sync_pull_on_join(),
                    result = "connected",
                    business_impact = "automatic snapshot exchange will start for the joined room",
                    "cloud sync joined the room"
                );
                self.set_cloud_sync_progress(0.15);
                self.record_cloud_sync_log(self.i18n_replace(
                    "settings_view.general.cloudsync.log_joined",
                    &[("peers", peers.to_string())],
                ));
                self.cloud_sync_status = Some(self.i18n_replace(
                    "settings_view.general.cloudsync.status_connected",
                    &[("peers", peers.to_string())],
                ));
                if self.cloud_sync_push_on_peer_change() {
                    self.cloud_sync_broadcast_snapshot(cx);
                }
                if self.cloud_sync_pull_on_join() {
                    self.cloud_sync_request_snapshot(cx);
                }
                cx.notify();
            }
            Event::PeerCount(peers) => {
                let trace_id = self.new_cloud_sync_trace_id();
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "sync.room.peer_count",
                    peers,
                    result = "updated",
                    business_impact = "the client reevaluated whether to publish local state",
                    "cloud sync peer count changed"
                );
                self.cloud_sync_status = Some(self.i18n_replace(
                    "settings_view.general.cloudsync.status_connected",
                    &[("peers", peers.to_string())],
                ));
                if self.cloud_sync_push_on_peer_change() {
                    self.cloud_sync_broadcast_snapshot(cx);
                }
                cx.notify();
            }
            Event::PeerDirect { device } => {
                self.set_cloud_sync_progress(0.15);
                self.cloud_sync_status = Some(self.i18n_replace(
                    "settings_view.general.cloudsync.status_direct",
                    &[("device", device)],
                ));
                cx.notify();
            }
            Event::EnvelopeReceived(envelope) => {
                let trace_id = envelope.trace_id.clone().unwrap_or_else(|| {
                    format!("cloud-sync-{}-{}", envelope.device, envelope.ts_ms)
                });
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "sync.receive.request",
                    remote_device = %envelope.device,
                    envelope_kind = ?envelope.kind,
                    envelope_timestamp_ms = envelope.ts_ms,
                    result = "received",
                    business_impact = "a remote snapshot entered validation and merge processing",
                    "cloud sync envelope reached the workspace"
                );
                self.record_cloud_sync_log(self.i18n_replace(
                    "settings_view.general.cloudsync.log_received",
                    &[("device", envelope.device.clone())],
                ));
                self.apply_cloud_sync_envelope(*envelope, cx);
            }
            Event::Notice(notice) => {
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id = self.new_cloud_sync_trace_id(),
                    stage = "sync.engine.notice",
                    notice_character_count = notice.chars().count(),
                    secret_detail_redacted = true,
                    result = "observed",
                    business_impact = "the settings status reflects the latest transport event",
                    "cloud sync engine notice reached the workspace"
                );
                self.cloud_sync_status = Some(notice);
                cx.notify();
            }
            Event::Closed(reason) => {
                tracing::warn!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id = self.new_cloud_sync_trace_id(),
                    stage = "sync.lifecycle.response",
                    result = "closed",
                    reason_character_count = reason.chars().count(),
                    secret_detail_redacted = true,
                    business_impact = "automatic synchronization is unavailable until reconnect succeeds",
                    "cloud sync transport session closed"
                );
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
    fn build_cloud_sync_snapshot(&self, trace_id: &str) -> Result<String, String> {
        let connections = self
            .connection_store
            .export_saved_connections_snapshot()
            .map_err(|error| error.to_string())?;
        let serial_profiles = serde_json::to_value(
            self.connection_store
                .export_serial_profiles_snapshot()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let telnet_profiles = serde_json::to_value(
            self.connection_store
                .export_telnet_profiles_snapshot()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let standalone_sftp_profiles = serde_json::to_value(
            self.connection_store
                .export_standalone_sftp_profiles_snapshot()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let remote_desktop_profiles = serde_json::to_value(
            self.connection_store
                .export_remote_desktop_profiles_snapshot()
                .map_err(|error| error.to_string())?,
        )
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
            "serialProfiles": serial_profiles,
            "telnetProfiles": telnet_profiles,
            "standaloneSftpProfiles": standalone_sftp_profiles,
            "remoteDesktopProfiles": remote_desktop_profiles,
        });
        super::new_connection::audit_sync_document(trace_id, "outbound", &document);
        if self.settings_store.settings().cloud_sync.sync_passwords {
            // Passwords are authoritative like any other connection field:
            // the sender includes every password slot, using `null` when no
            // secret is stored so receivers can clear a stale local value.
            document["passwords"] = serde_json::Value::Object(self.resolve_connection_passwords());
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
    fn resolve_connection_passwords(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut passwords = serde_json::Map::new();
        for connection in self.connection_store.connections() {
            let matches_password_slot = matches!(
                connection.auth,
                oxideterm_connections::SavedAuth::Password { .. }
            );
            if !matches_password_slot {
                continue;
            }
            let value = match self.connection_store.get_connection_password(&connection.id) {
                Ok(secret) => {
                    serde_json::Value::String(secret.expose_secret().to_string())
                }
                Err(_) => serde_json::Value::Null,
            };
            passwords.insert(connection.id.clone(), value);
        }
        passwords
    }

    pub(crate) fn cloud_sync_broadcast_snapshot(&mut self, cx: &mut Context<Self>) -> bool {
        let trace_id = self.new_cloud_sync_trace_id();
        if let Some(form) = self.connection_form_state(cx).form.as_ref() {
            tracing::debug!(
                target: "oxideterm::audit",
                trace_id = form.audit_trace_id,
                cloud_sync_trace_id = %trace_id,
                stage = "session.form.sync_trace_link",
                transport = ?form.transport,
                saved_connection_id = ?self.connection_form_state(cx).editing_saved_connection_id,
                result = "linked",
                secret_detail_redacted = true,
                business_impact = "the session edit trace can now be followed into encrypted snapshot construction and transport",
                "session form operation was linked to a cloud-sync trace"
            );
        }
        self.cloud_sync_broadcast_snapshot_with_trace(trace_id, "explicit-mutation", cx)
    }

    fn cloud_sync_broadcast_snapshot_with_trace(
        &mut self,
        trace_id: String,
        source: &'static str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(runtime) = self.cloud_sync.as_ref() else {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.broadcast.validation",
                source,
                result = "rejected",
                reason = "cloud sync runtime is not running",
                business_impact = "the local change was not queued for synchronization",
                "cloud sync broadcast was rejected before snapshot construction"
            );
            return false;
        };
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.broadcast.request",
            source,
            result = "accepted",
            business_impact = "the local stores are being converted into an encrypted snapshot",
            "cloud sync broadcast request entered snapshot construction"
        );
        match self.build_cloud_sync_snapshot(&trace_id) {
            Ok(snapshot) => {
                let encrypted_bytes = snapshot.len();
                match runtime.commands.try_send(
                    oxideterm_cloud_sync::SyncCommand::Broadcast {
                        trace_id: trace_id.clone(),
                        snapshot,
                    },
                ) {
                    Ok(()) => {
                        self.cloud_sync_observed_store_state =
                            Some(self.cloud_sync_store_state());
                        tracing::info!(
                            target: "oxideterm_gpui_app::cloud_sync",
                            trace_id,
                            stage = "sync.broadcast.response",
                            source,
                            encrypted_bytes,
                            result = "queued",
                            business_impact = "the encrypted snapshot is waiting for transport delivery",
                            "cloud sync snapshot was queued for broadcast"
                        );
                        self.record_cloud_sync_log(
                            self.i18n
                                .t("settings_view.general.cloudsync.log_push_queued"),
                        );
                        self.set_cloud_sync_progress(0.5);
                        self.cloud_sync_status = Some(
                            self.i18n
                                .t("settings_view.general.cloudsync.status_queued"),
                        );
                        cx.notify();
                        true
                    }
                    Err(error) => {
                        tracing::warn!(
                            target: "oxideterm_gpui_app::cloud_sync",
                            trace_id,
                            stage = "sync.broadcast.response",
                            source,
                            encrypted_bytes,
                            result = "rejected",
                            queue_error = %error,
                            secret_detail_redacted = true,
                            business_impact = "the local snapshot was not delivered and remains eligible for retry",
                            "cloud sync command queue rejected the snapshot"
                        );
                        false
                    }
                }
            }
            Err(error) => {
                tracing::warn!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "sync.broadcast.response",
                    source,
                    result = "failed",
                    error_kind = %error,
                    secret_detail_redacted = true,
                    business_impact = "the local stores were not broadcast and remain eligible for retry",
                    "cloud sync snapshot construction failed"
                );
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
                false
            }
        }
    }

    pub(crate) fn cloud_sync_request_snapshot(&mut self, cx: &mut Context<Self>) {
        let trace_id = self.new_cloud_sync_trace_id();
        let Some(runtime) = self.cloud_sync.as_ref() else {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.request.validation",
                result = "rejected",
                reason = "cloud sync runtime is not running",
                business_impact = "no remote snapshot was requested",
                "cloud sync request was rejected before transport delivery"
            );
            return;
        };
        let result = runtime.commands.try_send(
            oxideterm_cloud_sync::SyncCommand::Request {
                trace_id: trace_id.clone(),
            },
        );
        let queue_error = result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_else(|| "none".to_string());
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.request.response",
            result = if result.is_ok() { "queued" } else { "rejected" },
            queue_error,
            secret_detail_redacted = result.is_err(),
            business_impact = if result.is_ok() {
                "connected peers will be asked for their latest snapshot"
            } else {
                "the request did not enter the transport queue"
            },
            "cloud sync snapshot request reached the command queue"
        );
        if result.is_err() {
            return;
        }
        self.record_cloud_sync_log(
            self.i18n
                .t("settings_view.general.cloudsync.log_request_sent"),
        );
        self.set_cloud_sync_progress(0.25);
        self.cloud_sync_status = Some(
            self.i18n
                .t("settings_view.general.cloudsync.status_waiting"),
        );
        cx.notify();
    }

    /// Manual "force sync": broadcast the local snapshot and immediately ask
    /// the room for theirs. This wakes every connected client to converge in
    /// one round trip instead of waiting for the next automatic trigger.
    pub(crate) fn cloud_sync_force_sync(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.is_none() {
            return;
        }
        let trace_id = self.new_cloud_sync_trace_id();
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.force.request",
            result = "accepted",
            business_impact = "the client will request remote state and publish local state immediately",
            "manual force sync: push local snapshot and request remote snapshots"
        );
        self.cloud_sync_request_snapshot(cx);
        let queued =
            self.cloud_sync_broadcast_snapshot_with_trace(trace_id.clone(), "manual-force", cx);
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.force.response",
            result = if queued { "queued" } else { "partial" },
            business_impact = if queued {
                "the force-sync push entered transport delivery"
            } else {
                "the remote request was attempted but the local push was not queued"
            },
            "manual force sync reached its local terminal response"
        );
    }

    fn apply_cloud_sync_envelope(
        &mut self,
        envelope: oxideterm_cloud_sync::SyncEnvelope,
        cx: &mut Context<Self>,
    ) {
        let trace_id = envelope
            .trace_id
            .clone()
            .unwrap_or_else(|| format!("cloud-sync-{}-{}", envelope.device, envelope.ts_ms));
        if !self.cloud_sync_auto_apply() {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.receive.validation",
                result = "rejected",
                reason = "automatic apply is disabled",
                business_impact = "the received snapshot was not merged into local stores",
                "cloud sync snapshot was rejected before decryption"
            );
            return;
        }
        let Some(data) = envelope.data.as_ref() else {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.receive.validation",
                result = "rejected",
                reason = "the snapshot envelope did not contain data",
                business_impact = "the received snapshot was not merged",
                "cloud sync snapshot was rejected before decryption"
            );
            return;
        };
        let Some(payload) = data.get("payload").and_then(|payload| payload.as_str()) else {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.receive.validation",
                result = "rejected",
                reason = "the snapshot envelope did not contain a string payload",
                business_impact = "the received snapshot was not merged",
                "cloud sync snapshot was rejected before decryption"
            );
            return;
        };
        // The payload is the sealed envelope; without the shared key a frame
        // can never reach the merge layer.
        let key = match self.cloud_sync_snapshot_key() {
            Some(key) => key,
            None => {
                tracing::warn!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "sync.receive.validation",
                    result = "rejected",
                    reason = "the runtime snapshot key is unavailable",
                    business_impact = "the received snapshot could not be decrypted",
                    "cloud sync snapshot was rejected before decryption"
                );
                return;
            }
        };
        let sealed: oxideterm_cloud_sync::crypto::EncryptedSyncPayload =
            match serde_json::from_str(payload) {
                Ok(sealed) => sealed,
                Err(error) => {
                    tracing::warn!(
                        target: "oxideterm_gpui_app::cloud_sync",
                        trace_id,
                        stage = "envelope-decode-failed",
                        "cloud sync envelope decode failed: {error}"
                    );
                    return;
                }
            };
        let plaintext = match decrypt_snapshot(key, &sealed) {
            Ok(plaintext) => plaintext,
            Err(error) => {
                tracing::warn!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "envelope-auth-failed",
                    device = %envelope.device,
                    "cloud sync envelope failed authentication: {error}"
                );
                return;
            }
        };
        let Ok(document) = serde_json::from_str::<serde_json::Value>(plaintext.as_str()) else {
            tracing::warn!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.receive.decode",
                result = "failed",
                reason = "decrypted snapshot document is not valid JSON",
                secret_detail_redacted = true,
                business_impact = "the received snapshot was not merged",
                "cloud sync decrypted document could not be decoded"
            );
            return;
        };

        // Skip snapshots we have already applied so repeated broadcasts cannot
        // ping-pong between devices.
        let last_applied =
            read_last_applied_ts(self.settings_store.path(), &envelope.device).unwrap_or(0);
        if envelope.ts_ms <= last_applied {
            tracing::info!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.receive.validation",
                result = "skipped",
                envelope_timestamp_ms = envelope.ts_ms,
                last_applied_timestamp_ms = last_applied,
                reason = "the snapshot is not newer than the local apply marker",
                business_impact = "local data remained unchanged",
                "cloud sync duplicate or stale snapshot was skipped"
            );
            return;
        }

        // Remote deletions are destructive: surface them before applying.
        let deleted_count = match self.preview_cloud_sync_deletions(&document) {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id,
                    stage = "deletion-preview-failed",
                    "cloud sync deletion preview failed: {error:#}"
                );
                return;
            }
        };
        if deleted_count > 0 {
            tracing::info!(
                target: "oxideterm_gpui_app::cloud_sync",
                trace_id,
                stage = "sync.deletion.validation",
                deleted_count,
                result = "accepted",
                reason = "authenticated deletion tombstones are conflict-resolved by record identity",
                business_impact = "remote deletions will be applied without waiting for a hidden settings-only prompt",
                "cloud sync deletion tombstones entered automatic merge"
            );
        }

        self.apply_cloud_sync_document(
            document,
            envelope.ts_ms,
            &envelope.device,
            &trace_id,
            cx,
        );
    }

    /// Counts local sessions that an incoming snapshot would delete across
    /// saved connections and every profile type, without mutating anything.
    fn preview_cloud_sync_deletions(
        &self,
        document: &serde_json::Value,
    ) -> Result<usize, String> {
        let mut deleted_count = 0usize;
        if let Some(connections_value) = document.get("connections") {
            let snapshot: oxideterm_connections::SavedConnectionsSyncSnapshot =
                serde_json::from_value(connections_value.clone())
                    .map_err(|error| format!("connection snapshot decode failed: {error}"))?;
            // An all-deleted/empty connection list is the most destructive
            // snapshot shape. Treat it as a wipe attempt when this device has
            // any saved sessions and let the user decide instead of applying.
            if snapshot
                .records
                .iter()
                .all(|record| record.deleted)
                && !self.connection_store.connections().is_empty()
            {
                deleted_count += self.connection_store.connections().len();
            } else {
                deleted_count += self
                    .connection_store
                    .preview_saved_connections_snapshot_deletions(&snapshot)
                    .map_err(|error| error.to_string())?;
            }
        }
        if let Some(value) = document.get("serialProfiles") {
            let snapshot: oxideterm_connections::SerialProfilesSyncSnapshot =
                serde_json::from_value(value.clone())
                    .map_err(|error| format!("serial profiles snapshot decode failed: {error}"))?;
            deleted_count += self
                .connection_store
                .preview_serial_profiles_snapshot_deletions(&snapshot);
        }
        if let Some(value) = document.get("telnetProfiles") {
            let snapshot: oxideterm_connections::TelnetProfilesSyncSnapshot =
                serde_json::from_value(value.clone())
                    .map_err(|error| format!("telnet profiles snapshot decode failed: {error}"))?;
            deleted_count += self
                .connection_store
                .preview_telnet_profiles_snapshot_deletions(&snapshot);
        }
        if let Some(value) = document.get("standaloneSftpProfiles") {
            let snapshot: oxideterm_connections::StandaloneSftpProfilesSyncSnapshot =
                serde_json::from_value(value.clone()).map_err(|error| {
                    format!("standalone SFTP profiles snapshot decode failed: {error}")
                })?;
            deleted_count += self
                .connection_store
                .preview_standalone_sftp_profiles_snapshot_deletions(&snapshot);
        }
        if let Some(value) = document.get("remoteDesktopProfiles") {
            let snapshot: oxideterm_connections::RemoteDesktopProfilesSyncSnapshot =
                serde_json::from_value(value.clone()).map_err(|error| {
                    format!("remote desktop profiles snapshot decode failed: {error}")
                })?;
            deleted_count += self
                .connection_store
                .preview_remote_desktop_profiles_snapshot_deletions(&snapshot);
        }
        Ok(deleted_count)
    }

    /// Applies one validated snapshot document (connections + settings +
    /// optional passwords) and advances the sync marker.
    fn apply_cloud_sync_document(
        &mut self,
        document: serde_json::Value,
        ts_ms: u64,
        device: &str,
        trace_id: &str,
        cx: &mut Context<Self>,
    ) {
        super::new_connection::audit_sync_document(trace_id, "inbound", &document);
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.apply.request",
            remote_device = device,
            snapshot_timestamp_ms = ts_ms,
            result = "accepted",
            business_impact = "the remote document entered store-level merge processing",
            "cloud sync document entered the apply layer"
        );
        let mut applied_connections = 0usize;
        let mut applied_profiles = 0usize;
        let mut connection_updated_at: HashMap<String, u64> = HashMap::new();
        if let Some(connections_value) = document.get("connections") {
            let snapshot: oxideterm_connections::SavedConnectionsSyncSnapshot =
                match serde_json::from_value(connections_value.clone()) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        tracing::warn!(
                            target: "oxideterm_gpui_app::cloud_sync",
                            trace_id,
                            stage = "connection-snapshot-decode-failed",
                            "cloud sync connection snapshot decode failed: {error}"
                        );
                        return;
                    }
                };
            for record in &snapshot.records {
                if record.deleted {
                    continue;
                }
                let Ok(updated_at) = chrono::DateTime::parse_from_rfc3339(&record.updated_at) else {
                    continue;
                };
                connection_updated_at
                    .insert(record.id.clone(), updated_at.timestamp_millis() as u64);
            }
            match self.connection_store.apply_saved_connections_snapshot_with_trace(
                snapshot,
                oxideterm_connections::SavedConnectionsConflictStrategy::Merge,
                trace_id,
            ) {
                Ok(outcome) => applied_connections = outcome.result.applied,
                Err(error) => {
                    tracing::warn!(
                        target: "oxideterm_gpui_app::cloud_sync",
                        trace_id,
                        stage = "connection-snapshot-apply-failed",
                        "cloud sync connection snapshot apply failed: {error:#}"
                    );
                    return;
                }
            }
        }
        for (document_key, profile_kind) in [
            ("serialProfiles", "serial"),
            ("telnetProfiles", "telnet"),
            ("standaloneSftpProfiles", "sftp"),
            ("remoteDesktopProfiles", "remote desktop"),
        ] {
            let Some(profile_value) = document.get(document_key) else {
                continue;
            };
            let applied = match document_key {
                "serialProfiles" => serde_json::from_value::<
                    oxideterm_connections::SerialProfilesSyncSnapshot,
                >(profile_value.clone())
                .ok()
                .and_then(|snapshot| {
                    self.connection_store
                        .apply_serial_profiles_snapshot(snapshot)
                        .ok()
                }),
                "telnetProfiles" => serde_json::from_value::<
                    oxideterm_connections::TelnetProfilesSyncSnapshot,
                >(profile_value.clone())
                .ok()
                .and_then(|snapshot| {
                    self.connection_store
                        .apply_telnet_profiles_snapshot(snapshot)
                        .ok()
                }),
                "standaloneSftpProfiles" => serde_json::from_value::<
                    oxideterm_connections::StandaloneSftpProfilesSyncSnapshot,
                >(profile_value.clone())
                .ok()
                .and_then(|snapshot| {
                    self.connection_store
                        .apply_standalone_sftp_profiles_snapshot(snapshot)
                        .ok()
                }),
                _ => serde_json::from_value::<
                    oxideterm_connections::RemoteDesktopProfilesSyncSnapshot,
                >(profile_value.clone())
                .ok()
                .and_then(|snapshot| {
                    self.connection_store
                        .apply_remote_desktop_profiles_snapshot(snapshot)
                        .ok()
                }),
            };
            match applied {
                Some(count) => {
                    tracing::info!(target: "oxideterm::audit", trace_id,
                        stage = "session.sync.apply", protocol = profile_kind, count,
                        result = "completed", "远端协议会话快照已合并到本地存储");
                    applied_profiles += count;
                }
                None => {
                    tracing::warn!(target: "oxideterm::audit", trace_id,
                        stage = "session.sync.apply", protocol = profile_kind,
                        result = "failed", "远端协议会话快照解析或合并失败，不能视为已成功同步");
                    self.record_cloud_sync_log(format!(
                        "{}",
                        self.i18n_replace(
                            "settings_view.general.cloudsync.log_invalid_profile",
                            &[("kind", profile_kind.to_string())]
                        )
                    ));
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
            self.apply_synced_connection_passwords(passwords, &connection_updated_at);
        }

        write_last_applied_ts(self.settings_store.path(), device, ts_ms, trace_id);
        self.cloud_sync_observed_store_state = Some(self.cloud_sync_store_state());
        self.active_session_sidebar_rows_cache.borrow_mut().take();
        for connection_id in connection_updated_at.keys() {
            self.sync_saved_connection_node_title(connection_id);
            self.sync_saved_connection_x11_forwarding(connection_id);
        }
        tracing::info!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "sync.apply.response",
            applied_connections,
            applied_profiles,
            result = "completed",
            business_impact = "the local stores and active-session sidebar now reflect the remote snapshot",
            "cloud sync document apply completed"
        );
        self.record_cloud_sync_log(self.i18n_replace(
            "settings_view.general.cloudsync.log_applied",
            &[
                ("count", applied_connections.to_string()),
                ("profileCount", applied_profiles.to_string()),
            ],
        ));
        self.set_cloud_sync_progress(1.0);
        self.cloud_sync_status = Some(
            self.i18n
                .t("settings_view.general.cloudsync.status_applied"),
        );
        self.push_workspace_notice(
            TerminalNotice {
                title: self.i18n.t("settings_view.general.cloudsync.applied"),
                description: Some(format!(
                    "{device} · {applied_connections} · {applied_profiles}"
                )),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            },
            cx,
        );
        cx.notify();
    }

    /// Applies passwords that arrived inside an encrypted snapshot as an
    /// authoritative value: a non-null entry replaces the local credential and
    /// a `null` entry clears it, matching ordinary edit-sync semantics.
    fn apply_synced_connection_passwords(
        &mut self,
        passwords: &serde_json::Map<String, serde_json::Value>,
        connection_updated_at: &HashMap<String, u64>,
    ) {
        use oxideterm_connections::{ConnectionCredentialSlot, SavedAuth};
        for (id, value) in passwords {
            let Some(connection) = self.connection_store.get(id) else {
                continue;
            };
            let matches_password_slot = match &connection.auth {
                SavedAuth::Password { .. } => true,
                _ => false,
            };
            if !matches_password_slot {
                continue;
            }
            // Only a connection this snapshot actually accepted may drive its
            // password, mirroring ordinary per-field conflict resolution.
            let local_ts = connection
                .updated_at
                .map(|ts| ts.timestamp_millis() as u64)
                .unwrap_or_else(|| connection.created_at.timestamp_millis() as u64);
            let accepted = connection_updated_at
                .get(id)
                .is_some_and(|incoming_ts| local_ts <= *incoming_ts);
            if !accepted {
                continue;
            }
            let Some(plaintext) = value.as_str() else {
                let _ = self.connection_store.forget_connection_credential(
                    id,
                    ConnectionCredentialSlot::Primary,
                );
                continue;
            };
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
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id = %prompt.trace_id,
                    stage = "delete-prompt-skip",
                    deleted_count = prompt.deleted_count,
                    "user skipped remote deletion"
                );
            }
            CloudSyncDeleteAction::Sync => {
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id = %prompt.trace_id,
                    stage = "delete-prompt-sync",
                    deleted_count = prompt.deleted_count,
                    result = "accepted",
                    business_impact = "the remote deletions will be merged into the local stores",
                    "user applied remote deletions"
                );
                self.apply_cloud_sync_document(
                    prompt.document,
                    prompt.ts_ms,
                    &prompt.device,
                    &prompt.trace_id,
                    cx,
                );
            }
            CloudSyncDeleteAction::RestoreRemote => {
                tracing::info!(
                    target: "oxideterm_gpui_app::cloud_sync",
                    trace_id = %prompt.trace_id,
                    stage = "delete-prompt-restore",
                    deleted_count = prompt.deleted_count,
                    "user restored local copy over remote deletions"
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

fn read_last_applied_ts(settings_path: &std::path::Path, device: &str) -> Option<u64> {
    let text = std::fs::read_to_string(marker_path_for(settings_path)).ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?
        ["lastAppliedByDevice"]
        .get(device)?
        .as_u64()
}

fn write_last_applied_ts(
    settings_path: &std::path::Path,
    device: &str,
    ts_ms: u64,
    trace_id: &str,
) {
    let path = marker_path_for(settings_path);
    let mut state = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let Some(root) = state.as_object_mut() else {
        return;
    };
    let devices = root
        .entry("lastAppliedByDevice")
        .or_insert_with(|| serde_json::json!({}));
    let Some(devices) = devices.as_object_mut() else {
        return;
    };
    devices.insert(device.to_string(), serde_json::Value::from(ts_ms));
    let body = state.to_string();
    if let Err(error) = std::fs::write(&path, body) {
        tracing::warn!(
            target: "oxideterm_gpui_app::cloud_sync",
            trace_id,
            stage = "marker-persist-failed",
            secret_detail_redacted = true,
            business_impact = "the snapshot was applied but duplicate suppression may not survive restart",
            "failed to persist cloud sync marker: {error}"
        );
    }
}

fn cloud_sync_file_state(path: &std::path::Path) -> Option<CloudSyncFileState> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(CloudSyncFileState {
        modified_nanos,
        length: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applied_markers_are_independent_per_device() {
        let directory = tempfile::tempdir().expect("temporary settings directory");
        let settings_path = directory.path().join("settings.json");
        std::fs::write(&settings_path, "{}").expect("settings fixture");

        write_last_applied_ts(&settings_path, "desktop", 100, "trace-desktop");
        write_last_applied_ts(&settings_path, "laptop", 20, "trace-laptop");

        assert_eq!(read_last_applied_ts(&settings_path, "desktop"), Some(100));
        assert_eq!(read_last_applied_ts(&settings_path, "laptop"), Some(20));
        assert_eq!(read_last_applied_ts(&settings_path, "unknown"), None);
    }

    #[test]
    fn local_store_state_changes_when_persisted_content_changes() {
        let directory = tempfile::tempdir().expect("temporary store directory");
        let path = directory.path().join("connections.json");
        std::fs::write(&path, "{}").expect("initial store fixture");
        let initial = cloud_sync_file_state(&path).expect("initial state");

        std::fs::write(&path, "{\"connections\":[]}").expect("changed store fixture");
        let changed = cloud_sync_file_state(&path).expect("changed state");

        assert_ne!(initial, changed);
    }
}
