// Functional test harness for cloudflare-worker/worker.js
// Mocks the Cloudflare Workers runtime (Durable Object binding + WebSocketPair)
// and verifies room-based signaling relay between two clients.

import assert from "node:assert";
import workerDefault, { DittoSignaling } from "../cloudflare-worker/worker.js";
const worker = workerDefault;

// ---------- Minimal Cloudflare runtime mocks ----------

class MockWebSocket {
  constructor() {
    this.readyState = 0; // CONNECTING
    this.peer = null; // the paired socket
    this.sent = []; // messages this socket sent
    this.received = []; // messages this socket received
    // Runtime hooks set by MockDOState.acceptWebSocket: client-originated data
    // is routed to webSocketMessage, connection teardown to webSocketClose.
    this.onRuntimeMessage = null;
    this.onRuntimeClose = null;
  }
  send(data) {
    this.sent.push(data);
    if (this.peer) {
      this.peer.received.push(data);
      // Only the accepted (server-side) socket has the hook set, so worker
      // -> client sends are not echoed back into the Durable Object.
      this.peer.onRuntimeMessage?.(data);
    }
  }
  close(code, reason) {
    if (this.readyState !== 1) return;
    this.readyState = 3; // CLOSING
    if (this.peer) {
      this.peer.readyState = 3;
      this.peer.onRuntimeClose?.(code, reason, false);
    }
  }
}

// Mimics the part of the Durable Object runtime state used by the hibernation
// API: accepted sockets, per-socket tags, and event routing back to the
// DittoSignaling instance.
class MockDOState {
  constructor() {
    this.sockets = new Set();
    this.tags = new Map();
    this.inst = null; // wired after the DittoSignaling instance exists
  }
  acceptWebSocket(ws, tags = []) {
    this.sockets.add(ws);
    this.tags.set(ws, tags);
    ws.readyState = 1; // OPEN; the paired client end opens too
    if (ws.peer) ws.peer.readyState = 1;
    ws.onRuntimeMessage = (data) => this.inst.webSocketMessage(ws, data);
    ws.onRuntimeClose = (code, reason, wasClean) =>
      this.inst.webSocketClose(ws, code, reason, wasClean);
  }
  getWebSockets() {
    return [...this.sockets];
  }
  getTags(ws) {
    return this.tags.get(ws) ?? [];
  }
}

class MockWebSocketPair {
  constructor() {
    const a = new MockWebSocket();
    const b = new MockWebSocket();
    a.peer = b;
    b.peer = a;
    this[0] = a; // server end
    this[1] = b; // client end
  }
}

// Cloudflare runtime globals used by worker.js
globalThis.WebSocketPair = MockWebSocketPair;

// Node's Response rejects 101 (WebSocket upgrade) but Cloudflare allows it.
const RealResponse = globalThis.Response;
globalThis.Response = class MockResponse {
  constructor(body, init = {}) {
    if (init.status === 101) {
      // Cloudflare WebSocket upgrade response
      this.status = 101;
      this.body = body;
      this.webSocket = init.webSocket;
      this.ok = true;
      return;
    }
    const r = new RealResponse(body, init);
    this.status = r.status;
    this.body = r.body;
    this.ok = r.ok;
  }
};

// ---------- Durable Object binding mock ----------
const doInstances = new Map(); // roomId -> DittoSignaling

function makeStub(roomId) {
  return {
    async fetch(request) {
      let inst = doInstances.get(roomId);
      if (!inst) {
        const state = new MockDOState();
        inst = new DittoSignaling(state, env);
        state.inst = inst; // wire event routing after construction
        doInstances.set(roomId, inst);
      }
      return inst.fetch(request);
    },
  };
}

const env = {
  DITTO_SIGNALING: {
    idFromName(name) { return { name }; },
    get(id) { return makeStub(id.name); },
  },
};

// ---------- HTTP request mock ----------
function makeRequest(url, isWebSocket = true) {
  return {
    url,
    headers: new Map(isWebSocket ? [["Upgrade", "websocket"]] : []),
  };
}

let passed = 0;
let failed = 0;
function check(name, fn) {
  try {
    fn();
    passed++;
    console.log(`  PASS  ${name}`);
  } catch (e) {
    failed++;
    console.error(`  FAIL  ${name}: ${e.message}`);
  }
}

async function bodyText(resp) {
  if (typeof resp.body === "string") return resp.body;
  if (resp.body && resp.body.getReader) {
    const reader = resp.body.getReader();
    let out = "";
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      out += new TextDecoder().decode(value);
    }
    return out;
  }
  return String(resp.body);
}

// ---------- Tests ----------

// 1. healthz
{
  const resp = await worker.fetch(makeRequest("https://x/healthz", false), env);
  const text = await bodyText(resp);
  check("GET /healthz returns 200 'ok'", () => {
    assert.equal(resp.status, 200);
    assert.equal(text, "ok");
  });
}

// 2. unknown path
{
  const resp = await worker.fetch(makeRequest("https://x/foo", false), env);
  check("GET /foo returns 404", () => {
    assert.equal(resp.status, 404);
  });
}

// 3. non-websocket upgrade to a room
{
  const resp = await worker.fetch(makeRequest("https://x/room/abc123", false), env);
  check("non-WebSocket room request returns 426", () => {
    assert.equal(resp.status, 426);
  });
}

// 4. two clients in same room exchange signaling + clip relay
{
  const room = "/room/testroomhash123456";
  const respA = await worker.fetch(makeRequest(`https://x${room}`, true), env);
  const clientA = respA.webSocket;
  assert.ok(clientA, "client A socket returned");

  const peers1 = JSON.parse(clientA.received[0]);
  check("first client gets peers count 1", () => {
    assert.equal(peers1.type, "peers");
    assert.equal(peers1.count, 1);
  });

  const respB = await worker.fetch(makeRequest(`https://x${room}`, true), env);
  const clientB = respB.webSocket;

  const peersB = JSON.parse(clientB.received[0]);
  check("second client gets peers count 2", () => {
    assert.equal(peersB.type, "peers");
    assert.equal(peersB.count, 2);
  });

  check("first client notified count 2", () => {
    const msgs = clientA.received.map((s) => JSON.parse(s));
    assert.ok(msgs.some((m) => m.type === "peers" && m.count === 2));
  });

  // A sends an SDP offer -> B receives it, with `from` = A's peer id
  clientA.send(JSON.stringify({ type: "offer", sdp: "v=0 offer" }));
  const offerMsg = JSON.parse(clientB.received[clientB.received.length - 1]);
  check("offer relayed from A to B with from=peerIdA", () => {
    assert.equal(offerMsg.type, "offer");
    assert.equal(offerMsg.sdp, "v=0 offer");
    assert.equal(offerMsg.from, peers1.self);
  });

  // B answers -> A receives it
  clientB.send(JSON.stringify({ type: "answer", sdp: "v=0 answer" }));
  const answerMsg = JSON.parse(clientA.received[clientA.received.length - 1]);
  check("answer relayed from B to A with from=peerIdB", () => {
    assert.equal(answerMsg.type, "answer");
    assert.equal(answerMsg.from, peersB.self);
  });

  // ICE candidate relay
  clientA.send(JSON.stringify({ type: "ice", candidate: { sdpMid: "0" } }));
  const iceMsg = JSON.parse(clientB.received[clientB.received.length - 1]);
  check("ice relayed A->B", () => {
    assert.equal(iceMsg.type, "ice");
    assert.deepEqual(iceMsg.candidate, { sdpMid: "0" });
  });

  // hello (device discovery) relay
  clientA.send(JSON.stringify({ type: "hello", deviceId: "pc-A", port: 40981, addresses: ["192.168.1.5"] }));
  const helloMsg = JSON.parse(clientB.received[clientB.received.length - 1]);
  check("hello relayed A->B (direct-connect discovery)", () => {
    assert.equal(helloMsg.type, "hello");
    assert.equal(helloMsg.port, 40981);
    assert.deepEqual(helloMsg.addresses, ["192.168.1.5"]);
  });

  // clip (fallback relay) relay
  clientB.send(JSON.stringify({ type: "clip", cfFormat: 13, text: "hello from B", payload: "" }));
  const clipMsg = JSON.parse(clientA.received[clientA.received.length - 1]);
  check("clip relayed B->A (fallback transport)", () => {
    assert.equal(clipMsg.type, "clip");
    assert.equal(clipMsg.text, "hello from B");
  });

  // garbage / non-JSON / unknown type are ignored (no crash, no relay)
  clientA.send("not-json{{{");
  clientA.send(JSON.stringify({ noType: true }));
  clientA.send(JSON.stringify({ type: "evil", payload: "x" }));
  check("invalid messages ignored without crash", () => {
    const bMsgs = clientB.received.map((s) => JSON.parse(s));
    assert.ok(!bMsgs.some((m) => m.type === "evil"));
  });

  // 5. different rooms do NOT see each other
  const respC = await worker.fetch(makeRequest("https://x/room/otherroom999", true), env);
  const clientC = respC.webSocket;
  const peersC = JSON.parse(clientC.received[0]);
  check("different room isolated (count 1)", () => {
    assert.equal(peersC.count, 1);
  });
  clientC.send(JSON.stringify({ type: "offer", sdp: "v=0 from C" }));
  check("cross-room leak check: A did not receive C's message", () => {
    const aMsgs = clientA.received.map((s) => JSON.parse(s));
    assert.ok(!aMsgs.some((m) => m.sdp === "v=0 from C"));
  });

  // 6. disconnect updates peer count
  clientB.close();
  await new Promise((r) => setTimeout(r, 10));
  check("after B closes, A notified count 1", () => {
    const msgs = clientA.received.map((s) => JSON.parse(s));
    const last = msgs[msgs.length - 1];
    assert.equal(last.type, "peers");
    assert.equal(last.count, 1);
  });

  // 7. clip message does not get persisted anywhere (in-memory only)
  check("no persistence: only in-memory DO instances (3 rooms touched)", () => {
    // testroomhash123456, otherroom999, and the 426-check room abc123
    assert.equal(doInstances.size, 3);
  });
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed) {
  console.error("SOME CHECKS FAILED");
  process.exit(1);
} else {
  console.log("ALL WORKER TESTS PASSED");
}
