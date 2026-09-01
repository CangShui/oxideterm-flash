/**
 * Ditto Cloud Sync — Cloudflare Worker signaling server.
 *
 * This worker is intentionally stateless with respect to clipboard data:
 * - It never sees, stores, or logs clipboard content.
 * - It only relays WebRTC signaling messages (SDP offer/answer and ICE candidates)
 *   between Ditto clients that join the same room.
 * - All room/connection state is kept in memory in a Durable Object. No KV/R2/D1
 *   storage is used, and no data survives a Worker restart.
 *
 * Client flow:
 *   1. Client connects WebSocket to  https://<worker>/room/<roomId>
 *   2. Client sends JSON signaling messages:
 *        { "type": "offer", "sdp": "..." }
 *        { "type": "answer", "sdp": "..." }
 *        { "type": "ice", "candidate": { ... } }
 *   3. Worker forwards those messages to every other client in the same room.
 *   4. Clients establish a direct WebRTC DataChannel and exchange clipboard data P2P.
 *
 * WebSocket handling uses the Durable Object WebSocket Hibernation API
 * (state.acceptWebSocket + webSocketMessage/webSocketClose) instead of
 * server.accept() + event listeners. This matters on the Workers Free plan:
 * duration is billed for the whole lifetime of a plain accept()ed WebSocket,
 * so an idle always-connected room would burn the entire daily quota.
 * With hibernation, the object goes to sleep between events and only bills
 * duration while a handler is actually running.
 */

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/healthz") {
      return new Response("ok", { status: 200 });
    }

    // Room id is derived from the user's sync room name/passphrase on the client,
    // e.g. sha256("my-room").substring(0, 32). This keeps pairing simple and avoids
    // storing room metadata on the server.
    const match = url.pathname.match(/^\/room\/([A-Za-z0-9_-]{1,128})$/);
    if (!match) {
      return new Response("Not found", { status: 404 });
    }

    const roomId = match[1];
    const id = env.DITTO_SIGNALING.idFromName(roomId);
    const stub = env.DITTO_SIGNALING.get(id);
    return stub.fetch(request);
  },
};

export class DittoSignaling {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    // server WebSocket -> peer id. Peer ids are cosmetic (used only for the
    // `from` echo and `self` field); application routing keys on device ids in
    // envelopes, so regenerating an id after the object wakes is harmless.
    this.peerIds = new Map();
  }

  async fetch(request) {
    if ((request.headers.get("Upgrade") || "").toLowerCase() !== "websocket") {
      return new Response("Expected WebSocket connection", { status: 426 });
    }

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    const roomId = this.roomIdFromUrl(request.url);

    // Hibernation API: never call server.accept() here. The room id travels as
    // a socket tag so handlers can recover it after the object wakes from
    // hibernation or is recreated from scratch.
    this.state.acceptWebSocket(server, [roomId]);

    // Notify the joining client of its own peer id and current room count, then
    // notify already-connected clients of the updated room count.
    const peerId = this.ensurePeerId(server);
    server.send(JSON.stringify({ type: "peers", count: this.peerCount(), self: peerId }));
    this.broadcast({ type: "peers", count: this.peerCount() }, server);

    return new Response(null, { status: 101, webSocket: client });
  }

  webSocketMessage(server, message) {
    let msg;
    try {
      msg = JSON.parse(message);
    } catch {
      return;
    }

    if (!msg || typeof msg.type !== "string") {
      return;
    }

    // Only signaling payloads are expected. If a non-signaling payload is sent,
    // it is ignored; clipboard data must never go through this server.
    // Signaling and ephemeral clipboard relay messages. Nothing is persisted.
    if (!["offer", "answer", "ice", "bye", "hello", "peer", "clip"].includes(msg.type)) {
      return;
    }

    msg.from = this.ensurePeerId(server);
    console.log(
      `[relay] room=${this.roomId(server)} type=${msg.type} from=${msg.from} peers=${this.peerCount()} text=${(msg.text || "").substring(0, 40)}`
    );
    this.broadcast(msg, server);
  }

  webSocketClose(server) {
    this.peerIds.delete(server);
    console.log(`[close] room=${this.roomId(server)} remaining=${this.peerCount()}`);
    this.broadcast({ type: "peers", count: this.peerCount() }, null);
  }

  ensurePeerId(server) {
    let id = this.peerIds.get(server);
    if (!id) {
      id = crypto.randomUUID();
      this.peerIds.set(server, id);
    }
    return id;
  }

  // Counts only open sockets so the number stays correct whether the runtime
  // has already removed a closed socket from getWebSockets() or not.
  peerCount() {
    return this.state.getWebSockets().filter((ws) => ws.readyState === 1).length;
  }

  roomId(server) {
    const tags = this.state.getTags(server);
    return tags.length > 0 ? tags[0] : "";
  }

  roomIdFromUrl(url) {
    try {
      const u = new URL(url);
      return u.pathname.split("/").filter(Boolean).pop() || "";
    } catch {
      return "";
    }
  }

  broadcast(msg, except) {
    const text = JSON.stringify(msg);
    for (const ws of this.state.getWebSockets()) {
      if (ws === except || ws.readyState !== 1) continue; // WebSocket.OPEN === 1
      try {
        ws.send(text);
      } catch {
        // ignore per-client send failures; webSocketClose handler will clean up
      }
    }
  }
}
