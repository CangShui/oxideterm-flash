// 真实 wrangler dev 本地部署验证
// 用 Node 内置 WebSocket 连到 http://127.0.0.1:8787 做端到端信令测试
import assert from "node:assert";

const BASE = "ws://127.0.0.1:8787";
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

function connect(room) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${BASE}/room/${room}`);
    ws.messages = [];
    ws.onmessage = (e) => {
      ws.messages.push(JSON.parse(e.data));
    };
    ws.onopen = () => resolve(ws);
    ws.onerror = (e) => reject(new Error(`ws error: ${e.message || "unknown"}`));
    setTimeout(() => reject(new Error("connect timeout")), 5000);
  });
}

function waitFor(ws, predicate, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const timer = setInterval(() => {
      const found = ws.messages.find(predicate);
      if (found) {
        clearInterval(timer);
        resolve(found);
      } else if (Date.now() - start > timeoutMs) {
        clearInterval(timer);
        reject(new Error(`timeout waiting; got: ${JSON.stringify(ws.messages)}`));
      }
    }, 20);
  });
}

const room = `room-${Date.now()}`;
const otherRoom = `other-${Date.now()}`;

// --- 1. 两个客户端加入同一房间 ---
const clientA = await connect(room);
const peersA = await waitFor(clientA, (m) => m.type === "peers" && m.count === 1);
check("客户端 A 收到 peers count=1 + self id", () => {
  assert.equal(peersA.count, 1);
  assert.ok(peersA.self);
});

const clientB = await connect(room);
const peersB = await waitFor(clientB, (m) => m.type === "peers" && m.count === 2);
check("客户端 B 收到 peers count=2 + self id", () => {
  assert.equal(peersB.count, 2);
  assert.ok(peersB.self);
  assert.notEqual(peersB.self, peersA.self);
});

await waitFor(clientA, (m) => m.type === "peers" && m.count === 2);
check("客户端 A 被通知人数变为 2", () => true);

// --- 2. offer / answer / ice 转发 ---
clientA.send(JSON.stringify({ type: "offer", sdp: "v=0 offer-test" }));
const offer = await waitFor(clientB, (m) => m.type === "offer");
check("offer 从 A 转发到 B（带 from=peerIdA）", () => {
  assert.equal(offer.sdp, "v=0 offer-test");
  assert.equal(offer.from, peersA.self);
});

clientB.send(JSON.stringify({ type: "answer", sdp: "v=0 answer-test" }));
const answer = await waitFor(clientA, (m) => m.type === "answer");
check("answer 从 B 转发到 A（带 from=peerIdB）", () => {
  assert.equal(answer.sdp, "v=0 answer-test");
  assert.equal(answer.from, peersB.self);
});

clientA.send(JSON.stringify({ type: "ice", candidate: { sdpMid: "0", candidate: "candidate:1 1 UDP" } }));
const ice = await waitFor(clientB, (m) => m.type === "ice");
check("ice candidate 从 A 转发到 B", () => {
  assert.deepEqual(ice.candidate, { sdpMid: "0", candidate: "candidate:1 1 UDP" });
});

// --- 3. hello（设备发现）转发 ---
clientA.send(JSON.stringify({ type: "hello", deviceId: "pc-A", port: 40981, addresses: ["192.168.1.5"] }));
const hello = await waitFor(clientB, (m) => m.type === "hello");
check("hello 设备发现信息 A → B", () => {
  assert.equal(hello.port, 40981);
  assert.deepEqual(hello.addresses, ["192.168.1.5"]);
});

// --- 4. clip（兜底中继）转发 ---
clientB.send(JSON.stringify({ type: "clip", cfFormat: 13, text: "hello from B", payload: "" }));
const clip = await waitFor(clientA, (m) => m.type === "clip");
check("clip 兜底中继 B → A", () => {
  assert.equal(clip.text, "hello from B");
});

// --- 5. 非法消息被忽略 ---
clientA.send("not-json{{{");
clientA.send(JSON.stringify({ type: "evil", payload: "x" }));
clientA.send(JSON.stringify({ noType: true }));
check("非法/未知类型消息不转发不崩溃", () => {
  const got = clientB.messages.some((m) => m.type === "evil");
  assert.equal(got, false);
});

// --- 6. 跨房间隔离 ---
const clientC = await connect(otherRoom);
const peersC = await waitFor(clientC, (m) => m.type === "peers");
check("不同房间的客户端互相隔离", () => {
  assert.equal(peersC.count, 1);
  assert.notEqual(peersC.self, peersA.self);
  assert.notEqual(peersC.self, peersB.self);
});
clientC.send(JSON.stringify({ type: "offer", sdp: "v=0 from C" }));
await new Promise((r) => setTimeout(r, 300));
check("房间 C 的 offer 不会泄漏到房间 A/B", () => {
  const leaked = clientA.messages.some((m) => m.sdp === "v=0 from C") ||
                 clientB.messages.some((m) => m.sdp === "v=0 from C");
  assert.equal(leaked, false);
});

// --- 7. 断线更新 ---
clientB.close();
await waitFor(clientA, (m) => m.type === "peers" && m.count === 1);
check("B 断线后 A 收到人数更新 count=1", () => true);

// --- 8. 相同房间再次加入（人数恢复） ---
const clientD = await connect(room);
const peersD = await waitFor(clientD, (m) => m.type === "peers");
check("新客户端 D 加入后 count=2（A + D）", () => {
  assert.equal(peersD.count, 2);
});

clientA.close();
clientC.close();
clientD.close();
await new Promise((r) => setTimeout(r, 200));

console.log(`\n${passed} passed, ${failed} failed`);
if (failed) {
  console.error("SOME CHECKS FAILED");
  process.exit(1);
} else {
  console.log("ALL REAL-WORLD LOCAL DEPLOY TESTS PASSED");
}
