// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Self-hosted cloud sync transport for OxideTerm configuration snapshots.
//!
//! Peers join a room on a Cloudflare Worker signaling endpoint. The room always
//! provides a relay path (JSON frames wrapped in `clip` signaling messages); a
//! direct WebRTC DataChannel is negotiated between peers that both advertise
//! peer-to-peer support, and is preferred for delivery once open. Payload
//! bytes are opaque to this crate: callers pass serialized snapshot documents
//! in and deliver received documents to their own merge/apply layer.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod crypto;
pub mod engine;
pub mod p2p;
pub mod signaling;

pub use engine::{spawn, SyncCommand, SyncEvent, SyncHandle};

/// Transport selection for one sync participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportMode {
    /// Deliver every frame through the worker relay.
    Relay,
    /// Prefer the WebRTC DataChannel; fall back to the relay when no direct
    /// channel is open to the destination peer.
    PeerToPeer,
    /// Resolve `Auto` from the environment at configuration time.
    Auto,
}

impl TransportMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "relay" => Some(Self::Relay),
            "p2p" | "peer" | "peer2peer" => Some(Self::PeerToPeer),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// One application-level frame exchanged between sync peers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncEnvelope {
    pub v: u32,
    pub kind: SyncEnvelopeKind,
    pub device: String,
    #[serde(default, rename = "traceId", skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(rename = "tsMs")]
    pub ts_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncEnvelopeKind {
    /// Presence announcement carrying device identity and capability flags.
    Hello,
    /// A full configuration snapshot document.
    Snapshot,
    /// Ask any peer holding a snapshot to re-broadcast it.
    Request,
}

/// Capability flags exchanged inside hello envelopes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HelloCaps {
    /// Peer can participate in WebRTC DataChannel transport.
    pub p2p: bool,
}

pub const SYNC_SCHEMA_VERSION: u32 = 1;

/// Derives the worker room identifier from the user-facing room passphrase so
/// the server never learns or stores the original value.
pub fn derive_room_id(room_passphrase: &str) -> String {
    let digest = Sha256::digest(room_passphrase.trim().as_bytes());
    hex(&digest[..])[..32].to_string()
}

/// Builds the WebSocket room URL for the configured worker endpoint.
pub fn room_url(server_url: &str, room_passphrase: &str) -> String {
    let base = server_url.trim().trim_end_matches('/');
    let ws_base = base
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| base.strip_prefix("http://").map(|rest| format!("ws://{rest}")))
        .unwrap_or_else(|| base.to_string());
    format!("{ws_base}/room/{}", derive_room_id(room_passphrase))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn new_device_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_envelope_roundtrip_preserves_trace_id() {
        let envelope = SyncEnvelope {
            v: SYNC_SCHEMA_VERSION,
            kind: SyncEnvelopeKind::Snapshot,
            device: "desktop".to_string(),
            trace_id: Some("cloud-sync-desktop-42".to_string()),
            ts_ms: 42,
            data: Some(serde_json::json!({ "payload": "sealed" })),
        };

        let encoded = serde_json::to_string(&envelope).expect("serialize envelope");
        let decoded: SyncEnvelope =
            serde_json::from_str(&encoded).expect("deserialize envelope");

        assert_eq!(decoded.trace_id.as_deref(), Some("cloud-sync-desktop-42"));
    }

    #[test]
    fn legacy_sync_envelope_without_trace_id_remains_readable() {
        let decoded: SyncEnvelope = serde_json::from_str(
            r#"{"v":1,"kind":"request","device":"laptop","tsMs":7}"#,
        )
        .expect("deserialize legacy envelope");

        assert_eq!(decoded.trace_id, None);
    }
}
