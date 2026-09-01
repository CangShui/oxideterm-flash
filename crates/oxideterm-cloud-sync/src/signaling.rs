// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! WebSocket client for the Cloudflare Worker signaling room.
//!
//! The worker whitelists a fixed set of message types and forwards them to
//! every other peer in the room. Sync frames ride inside `clip` messages so
//! no worker changes are required; payloads are base64-encoded so the server
//! never observes sync content in plaintext logs.

use anyhow::{Context, Result};
use base64::Engine as _;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};

pub type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
pub type WsMessage = Message;

/// Connects to the signaling room and returns the established stream.
pub async fn connect_room(server_url: &str, room_passphrase: &str) -> Result<WsStream> {
    let url = crate::room_url(server_url, room_passphrase);
    let result = connect_async(url.as_str()).await;
    match &result {
        Ok(_) => eprintln!("[cloud-sync] connected to {url}"),
        Err(error) => eprintln!("[cloud-sync] FAILED to connect to {url}: {error}"),
    }
    let (stream, _response) = result
        .with_context(|| format!("failed to connect signaling room {url}"))?;
    Ok(stream)
}

/// Wraps an application frame into the worker's relayed `clip` message.
pub fn wrap_relay_frame(envelope_json: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(envelope_json.as_bytes());
    serde_json::json!({ "type": "clip", "text": encoded }).to_string()
}

/// Unwraps a relayed `clip` message back into the application frame. Returns
/// `None` for foreign traffic or malformed envelopes so callers can ignore it.
pub fn unwrap_relay_frame(message: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    if value.get("type")?.as_str()? != "clip" {
        return None;
    }
    let text = value.get("text")?.as_str()?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(text).ok()?;
    String::from_utf8(decoded).ok()
}
