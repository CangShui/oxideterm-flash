// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Direct WebRTC DataChannel transport between sync peers.
//!
//! One `P2pPeer` wraps a single RTCPeerConnection bound to one remote device.
//! SDP negotiation flows over the signaling room using non-trickle ICE: local
//! description is applied only after gathering completes, so every candidate
//! travels inside the offer or answer and no side channel is needed.

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::mpsc;

use webrtc::api::media_engine::MediaEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

fn api() -> Arc<API> {
    static API: std::sync::OnceLock<Arc<API>> = std::sync::OnceLock::new();
    Arc::clone(API.get_or_init(|| {
        let media = MediaEngine::default();
        Arc::new(APIBuilder::new().with_media_engine(media).build())
    }))
}

/// Events surfaced from one peer connection back to the engine loop.
#[derive(Debug)]
pub enum P2pEvent {
    ChannelOpen,
    Message(Vec<u8>),
    Closed,
}

pub struct P2pPeer {
    events_tx: mpsc::Sender<P2pEvent>,
    channel: Option<Arc<RTCDataChannel>>,
    connection: Arc<RTCPeerConnection>,
}

impl P2pPeer {
    pub async fn new(events_tx: mpsc::Sender<P2pEvent>) -> Result<Self> {
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let connection = Arc::new(
            api()
                .new_peer_connection(config)
                .await
                .context("failed to create sync peer connection")?,
        );

        let state_tx = events_tx.clone();
        connection.on_peer_connection_state_change(Box::new(move |state| {
            if matches!(
                state,
                RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
            ) {
                let _ = state_tx.try_send(P2pEvent::Closed);
            }
            Box::pin(async {})
        }));

        // The responder side receives its channel from the remote offer.
        let incoming_tx = events_tx.clone();
        connection.on_data_channel(Box::new(move |channel: Arc<RTCDataChannel>| {
            let tx = incoming_tx.clone();
            Box::pin(async move {
                wire_channel(&channel, tx).await;
            })
        }));

        Ok(Self { events_tx, channel: None, connection })
    }

    /// Initiator path: create the reliable sync channel plus the SDP offer.
    pub async fn create_offer(&mut self) -> Result<RTCSessionDescription> {
        let channel = self
            .connection
            .create_data_channel("oxideterm-sync", None)
            .await
            .context("failed to create sync data channel")?;
        wire_channel(&channel, self.events_tx.clone()).await;
        self.channel = Some(channel);

        let offer = self
            .connection
            .create_offer(None)
            .await
            .context("failed to create sync offer")?;
        self.connection
            .set_local_description(offer.clone())
            .await
            .context("failed to apply local offer description")?;
        wait_for_gathering(&self.connection).await;
        Ok(offer)
    }

    pub async fn accept_answer(&self, answer: RTCSessionDescription) -> Result<()> {
        self.connection
            .set_remote_description(answer)
            .await
            .context("failed to apply remote answer")
    }

    pub async fn accept_offer(
        &mut self,
        offer: RTCSessionDescription,
    ) -> Result<RTCSessionDescription> {
        self.connection
            .set_remote_description(offer)
            .await
            .context("failed to apply remote offer")?;
        let answer = self
            .connection
            .create_answer(None)
            .await
            .context("failed to create sync answer")?;
        self.connection
            .set_local_description(answer.clone())
            .await
            .context("failed to apply local answer description")?;
        wait_for_gathering(&self.connection).await;
        Ok(answer)
    }

    /// Sends one application frame over the open DataChannel.
    pub async fn send(&self, frame: &[u8]) -> Result<()> {
        let channel = self.channel.as_ref().with_context(|| {
            "sync data channel is not open yet; relay transport should be used instead"
        })?;
        channel.send(&bytes::Bytes::copy_from_slice(frame)).await?;
        Ok(())
    }

    pub fn has_channel(&self) -> bool {
        self.channel.is_some()
    }
}

async fn wait_for_gathering(connection: &Arc<RTCPeerConnection>) {
    let mut promise = connection.gathering_complete_promise().await;
    // The promise resolves once gathering finishes; a missed pre-completed
    // state still returns an already-closed receiver here.
    let _ = promise.recv().await;
}

async fn wire_channel(channel: &Arc<RTCDataChannel>, events_tx: mpsc::Sender<P2pEvent>) {
    let open_tx = events_tx.clone();
    channel.on_open(Box::new(move || {
        let _ = open_tx.try_send(P2pEvent::ChannelOpen);
        Box::pin(async {})
    }));
    let message_tx = events_tx.clone();
    channel.on_message(Box::new(move |message: DataChannelMessage| {
        let _ = message_tx.try_send(P2pEvent::Message(message.data.to_vec()));
        Box::pin(async {})
    }));
}
