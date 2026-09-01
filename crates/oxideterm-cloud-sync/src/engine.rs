// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Sync room orchestration: presence, transport selection, frame routing.
//!
//! Every participant keeps the signaling WebSocket open for the whole session
//! so the worker relay remains available as a universal fallback. Peers that
//! both advertise DataChannel support additionally negotiate a direct WebRTC
//! connection; frames then bypass the worker while that channel stays open.
//! This yields pure-relay, pure-P2P, and mixed topologies without special
//! configuration.

use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::future::Future;
use tokio::sync::mpsc;

use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::p2p::{P2pEvent, P2pPeer};
use crate::signaling::{self, WsMessage, WsStream};
use crate::{HelloCaps, SyncEnvelope, SyncEnvelopeKind, TransportMode, SYNC_SCHEMA_VERSION};

#[derive(Clone, Debug)]
pub struct SyncOptions {
    pub server_url: String,
    pub room_passphrase: String,
    pub mode: TransportMode,
    pub device_id: String,
}

pub enum SyncCommand {
    /// Broadcast a serialized snapshot document to every reachable peer.
    Broadcast(String),
    /// Ask the room for the newest snapshot; peers holding one respond.
    Request,
    Shutdown,
}

pub enum SyncEvent {
    Joined { peers: usize },
    PeerCount(usize),
    PeerDirect { device: String },
    EnvelopeReceived(Box<SyncEnvelope>),
    Notice(String),
    Closed(String),
}

pub struct SyncHandle {
    pub events: mpsc::Receiver<SyncEvent>,
    pub commands: mpsc::Sender<SyncCommand>,
}

/// Spawns the engine on the supplied Tokio runtime handle.
pub fn spawn(options: SyncOptions, runtime: tokio::runtime::Handle) -> SyncHandle {
    let (event_tx, event_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(16);
    runtime.spawn(async move {
        run_engine(options, event_tx, command_rx).await;
    });
    SyncHandle { events: event_rx, commands: command_tx }
}

async fn run_engine(
    options: SyncOptions,
    events: mpsc::Sender<SyncEvent>,
    mut commands: mpsc::Receiver<SyncCommand>,
) {
    let mut backoff = std::time::Duration::from_secs(1);
    loop {
        let reason = run_session(&options, &events, &mut commands).await;
        let _ = events.send(SyncEvent::Closed(reason)).await;
        // Self-heal transient worker/network failures; shutdown is observed
        // again through the command channel after reconnect.
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(std::time::Duration::from_secs(30));
    }
}

struct SessionState {
    peers: HashMap<String, PeerEntry>,
    last_snapshot: Option<String>,
    local_tx: mpsc::Sender<(String, P2pEvent)>,
}

#[derive(Default)]
struct PeerEntry {
    supports_p2p: bool,
    hello_seen: bool,
    p2p: Option<P2pPeer>,
    direct_open: bool,
}

enum ParsedSignaling {
    Envelope(Box<SyncEnvelope>),
    Sdp { from: String, is_offer: bool, sdp: String },
    Peers { count: usize },
    Ignored,
}

async fn run_session(
    options: &SyncOptions,
    events: &mpsc::Sender<SyncEvent>,
    commands: &mut mpsc::Receiver<SyncCommand>,
) -> String {
    let ws = match signaling::connect_room(&options.server_url, &options.room_passphrase).await {
        Ok(stream) => stream,
        Err(error) => return format!("connect failed: {error:#}"),
    };
    let (mut sink, mut stream) = ws.split();

    let _ = events.send(SyncEvent::Notice("room joined".into())).await;

    let (local_tx, mut local_rx) = mpsc::channel::<(String, P2pEvent)>(64);
    let mut state = SessionState {
        peers: HashMap::new(),
        last_snapshot: None,
        local_tx,
    };

    send_hello(&mut sink, options).await;

    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(SyncCommand::Broadcast(snapshot)) => {
                        state.last_snapshot = Some(snapshot.clone());
                        broadcast_snapshot(&state, options, &mut sink, &snapshot).await;
                        let _ = events.send(SyncEvent::Notice("snapshot broadcast".into())).await;
                    }
                    Some(SyncCommand::Request) => {
                        let envelope = SyncEnvelope {
                            v: SYNC_SCHEMA_VERSION,
                            kind: SyncEnvelopeKind::Request,
                            device: options.device_id.clone(),
                            ts_ms: now_ms(),
                            data: None,
                        };
                        if let Ok(json) = serde_json::to_string(&envelope) {
                            deliver_to_all(&state, options, &mut sink, &json).await;
                        }
                    }
                    Some(SyncCommand::Shutdown) | None => {
                        let _ = sink.close().await;
                        return "shutdown".into();
                    }
                }
            }
            incoming = stream.next() => {
                let Some(message_result) = incoming else {
                    return "worker closed the connection".into();
                };
                let message = match message_result {
                    Ok(message) => message,
                    Err(error) => return format!("signaling read failed: {error}"),
                };
                let WsMessage::Text(text) = message else { continue };
                match parse_signaling_message(&text, &options.device_id) {
                    ParsedSignaling::Envelope(envelope) => {
                        handle_envelope(&mut state, options, &mut sink, events, *envelope).await;
                    }
                    ParsedSignaling::Sdp { from, is_offer, sdp } => {
                        handle_sdp(&mut state, options, &mut sink, events, &from, is_offer, sdp).await;
                    }
                    ParsedSignaling::Peers { count } => {
                        let joined = state.peers.is_empty() && count > 0;
                        if joined {
                            let _ = events.send(SyncEvent::Joined { peers: count }).await;
                        } else {
                            let _ = events.send(SyncEvent::PeerCount(count)).await;
                        }
                    }
                    ParsedSignaling::Ignored => {}
                }
            }
            Some((device, event)) = local_rx.recv() => {
                match event {
                    P2pEvent::ChannelOpen => {
                        if let Some(entry) = state.peers.get_mut(&device) {
                            entry.direct_open = true;
                        }
                        let _ = events.send(SyncEvent::PeerDirect { device }).await;
                    }
                    P2pEvent::Message(bytes) => {
                        match serde_json::from_slice::<SyncEnvelope>(&bytes) {
                            Ok(envelope) => {
                                handle_envelope(&mut state, options, &mut sink, events, envelope).await;
                            }
                            Err(error) => {
                                let _ = events.send(SyncEvent::Notice(format!(
                                    "direct frame decode failed from {device}: {error}"
                                ))).await;
                            }
                        }
                    }
                    P2pEvent::Closed => {
                        if let Some(entry) = state.peers.get_mut(&device) {
                            entry.direct_open = false;
                            entry.p2p = None;
                        }
                    }
                }
            }
        }
    }
}

fn send_hello(
    sink: &mut futures_util::stream::SplitSink<WsStream, WsMessage>,
    options: &SyncOptions,
) -> impl Future<Output = ()> + Send {
    let hello = SyncEnvelope {
        v: SYNC_SCHEMA_VERSION,
        kind: SyncEnvelopeKind::Hello,
        device: options.device_id.clone(),
        ts_ms: now_ms(),
        data: Some(
            serde_json::to_value(HelloCaps {
                p2p: options.mode != TransportMode::Relay,
            })
            .expect("hello caps serialize"),
        ),
    };
    async move {
        if let Ok(json) = serde_json::to_string(&hello) {
            let _ = sink
                .send(WsMessage::Text(signaling::wrap_relay_frame(&json).into()))
                .await;
        }
    }
}

fn parse_signaling_message(text: &str, self_device: &str) -> ParsedSignaling {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return ParsedSignaling::Ignored,
    };
    let kind = value.get("type").and_then(|kind| kind.as_str()).unwrap_or("");

    match kind {
        // Presence updates come straight from the worker.
        "peers" => {
            let count = value.get("count").and_then(|count| count.as_u64()).unwrap_or(0);
            ParsedSignaling::Peers { count: count as usize }
        }
        // Targeted WebRTC negotiation messages carry a `to` device id; sync
        // envelopes ride inside clip frames and broadcast to everyone instead.
        "offer" | "answer" => {
            let addressed_to = value.get("to").and_then(|to| to.as_str());
            if addressed_to != Some(self_device) {
                return ParsedSignaling::Ignored;
            }
            let from = value
                .get("from")
                .and_then(|from| from.as_str())
                .unwrap_or_default()
                .to_string();
            let sdp = value
                .get("sdp")
                .and_then(|sdp| sdp.as_str())
                .unwrap_or_default()
                .to_string();
            ParsedSignaling::Sdp { from, is_offer: kind == "offer", sdp }
        }
        "ice" => ParsedSignaling::Ignored,
        "clip" => {
            let Some(frame) = signaling::unwrap_relay_frame(text) else {
                return ParsedSignaling::Ignored;
            };
            let Ok(envelope) = serde_json::from_str::<SyncEnvelope>(&frame) else {
                return ParsedSignaling::Ignored;
            };
            if envelope.device == self_device {
                return ParsedSignaling::Ignored;
            }
            ParsedSignaling::Envelope(Box::new(envelope))
        }
        _ => ParsedSignaling::Ignored,
    }
}

fn build_session_description(
    is_offer: bool,
    sdp: String,
) -> Result<RTCSessionDescription, String> {
    if is_offer {
        RTCSessionDescription::offer(sdp).map_err(|error| error.to_string())
    } else {
        RTCSessionDescription::answer(sdp).map_err(|error| error.to_string())
    }
}

async fn handle_envelope(
    state: &mut SessionState,
    options: &SyncOptions,
    sink: &mut futures_util::stream::SplitSink<WsStream, WsMessage>,
    events: &mpsc::Sender<SyncEvent>,
    envelope: SyncEnvelope,
) {
    match envelope.kind {
        SyncEnvelopeKind::Hello => {
            let caps = envelope
                .data
                .as_ref()
                .and_then(|data| serde_json::from_value::<HelloCaps>(data.clone()).ok());
            let entry = state.peers.entry(envelope.device.clone()).or_default();
            entry.supports_p2p = caps.map(|caps| caps.p2p).unwrap_or(false);
            let first_hello = !entry.hello_seen;
            entry.hello_seen = true;

            // Deterministic initiator: the greater device id offers so both
            // sides never create simultaneous offers for the same pair. The
            // responder only reacts to a targeted offer afterwards.
            let initiate = first_hello
                && options.mode != TransportMode::Relay
                && entry.supports_p2p
                && options.device_id.as_str() > envelope.device.as_str();

            // A freshly registered peer must learn about our current snapshot
            // even if the peer-count push fired before its hello arrived.
            let greet_with_snapshot = first_hello && state.last_snapshot.is_some();

            if initiate {
                start_offer(state, sink, events, &envelope.device).await;
            }
            if greet_with_snapshot {
                if let Some(snapshot) = state.last_snapshot.clone() {
                    let response = SyncEnvelope {
                        v: SYNC_SCHEMA_VERSION,
                        kind: SyncEnvelopeKind::Snapshot,
                        device: options.device_id.clone(),
                        ts_ms: now_ms(),
                        data: Some(serde_json::json!({ "payload": snapshot })),
                    };
                    if let Ok(json) = serde_json::to_string(&response) {
                        deliver_to_all(state, options, sink, &json).await;
                    }
                }
            }
        }
        SyncEnvelopeKind::Request => {
            if let Some(snapshot) = state.last_snapshot.clone() {
                let response = SyncEnvelope {
                    v: SYNC_SCHEMA_VERSION,
                    kind: SyncEnvelopeKind::Snapshot,
                    device: options.device_id.clone(),
                    ts_ms: now_ms(),
                    data: Some(serde_json::json!({ "payload": snapshot })),
                };
                if let Ok(json) = serde_json::to_string(&response) {
                    deliver_to_all(state, options, sink, &json).await;
                }
            }
        }
        SyncEnvelopeKind::Snapshot => {
            let _ = events.send(SyncEvent::EnvelopeReceived(Box::new(envelope))).await;
        }
    }
}

async fn start_offer(
    state: &mut SessionState,
    sink: &mut futures_util::stream::SplitSink<WsStream, WsMessage>,
    events: &mpsc::Sender<SyncEvent>,
    device: &str,
) {
    let offer = {
        let entry = state.peers.entry(device.to_string()).or_default();
        let mut peer = match P2pPeer::new(tagged_sender(device, &state.local_tx)).await {
            Ok(peer) => peer,
            Err(error) => {
                let _ = events.send(SyncEvent::Notice(format!("p2p init failed: {error:#}"))).await;
                return;
            }
        };
        match peer.create_offer().await {
            Ok(offer) => {
                entry.p2p = Some(peer);
                offer
            }
            Err(error) => {
                let _ = events.send(SyncEvent::Notice(format!("offer failed for {device}: {error:#}"))).await;
                return;
            }
        }
    };
    let payload = serde_json::json!({
        "type": "offer",
        "to": device,
        "sdp": offer.sdp,
    })
    .to_string();
    if let Err(error) = sink.send(WsMessage::Text(payload.into())).await {
        let _ = events.send(SyncEvent::Notice(format!("offer send failed: {error}"))).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_sdp(
    state: &mut SessionState,
    options: &SyncOptions,
    sink: &mut futures_util::stream::SplitSink<WsStream, WsMessage>,
    events: &mpsc::Sender<SyncEvent>,
    from: &str,
    is_offer: bool,
    sdp_text: String,
) {
    let description = match build_session_description(is_offer, sdp_text) {
        Ok(description) => description,
        Err(error) => {
            let _ = events.send(SyncEvent::Notice(format!("bad SDP from {from}: {error}"))).await;
            return;
        }
    };

    if !is_offer {
        let accepted = match state.peers.get_mut(from).and_then(|entry| entry.p2p.as_ref()) {
            Some(peer) => peer.accept_answer(description).await.is_ok(),
            None => false,
        };
        if !accepted {
            let _ = events
                .send(SyncEvent::Notice(format!("answer from {from} had no pending offer")))
                .await;
        }
        return;
    }

    if options.mode == TransportMode::Relay {
        return;
    }
    let answer = {
        let entry = state.peers.entry(from.to_string()).or_default();
        if entry.p2p.is_none() {
            match P2pPeer::new(tagged_sender(from, &state.local_tx)).await {
                Ok(peer) => entry.p2p = Some(peer),
                Err(error) => {
                    let _ = events.send(SyncEvent::Notice(format!("p2p init failed: {error:#}"))).await;
                    return;
                }
            }
        }
        let mut peer = entry.p2p.take().expect("peer just inserted above");
        let result = peer.accept_offer(description).await;
        entry.p2p = Some(peer);
        match result {
            Ok(answer) => answer,
            Err(error) => {
                let _ = events.send(SyncEvent::Notice(format!("offer rejected by {from}: {error:#}"))).await;
                return;
            }
        }
    };
    let payload = serde_json::json!({
        "type": "answer",
        "to": from,
        "sdp": answer.sdp,
    })
    .to_string();
    if let Err(error) = sink.send(WsMessage::Text(payload.into())).await {
        let _ = events.send(SyncEvent::Notice(format!("answer send failed: {error}"))).await;
    }
}

async fn broadcast_snapshot(
    state: &SessionState,
    options: &SyncOptions,
    sink: &mut futures_util::stream::SplitSink<WsStream, WsMessage>,
    snapshot: &str,
) {
    let envelope = SyncEnvelope {
        v: SYNC_SCHEMA_VERSION,
        kind: SyncEnvelopeKind::Snapshot,
        device: options.device_id.clone(),
        ts_ms: now_ms(),
        data: Some(serde_json::json!({ "payload": snapshot })),
    };
    if let Ok(json) = serde_json::to_string(&envelope) {
        deliver_to_all(state, options, sink, &json).await;
    }
}

async fn deliver_to_all(
    state: &SessionState,
    options: &SyncOptions,
    sink: &mut futures_util::stream::SplitSink<WsStream, WsMessage>,
    json: &str,
) {
    // Direct channels first; any peer without an open channel gets the relay copy.
    for (device, entry) in state.peers.iter() {
        if options.mode != TransportMode::Relay
            && entry.direct_open
            && let Some(peer) = entry.p2p.as_ref()
            && peer.has_channel()
        {
            match peer.send(json.as_bytes()).await {
                Ok(()) => continue,
                Err(error) => {
                    eprintln!("[cloud-sync] direct send to {device} failed: {error}");
                }
            }
        }
        let frame = signaling::wrap_relay_frame(json);
        if let Err(error) = sink.send(WsMessage::Text(frame.into())).await {
            eprintln!("[cloud-sync] relay to {device} failed: {error}");
        }
    }
}

/// Tags per-peer WebRTC callbacks with the owning device id so the engine's
/// single local channel can route them without dynamic select trees.
fn tagged_sender(device: &str, local_tx: &mpsc::Sender<(String, P2pEvent)>) -> mpsc::Sender<P2pEvent> {
    let device = device.to_string();
    let local_tx = local_tx.clone();
    let (tx, mut rx) = mpsc::channel::<P2pEvent>(32);
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if local_tx.send((device.clone(), event)).await.is_err() {
                break;
            }
        }
    });
    tx
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
