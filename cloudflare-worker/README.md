# Cloudflare Worker 信令后端

## 作用

这个 Worker 只用于 WebRTC 的 P2P 连接建立：

- 两个 Ditto 客户端都连到同一个房间 URL
- 客户端通过 Worker 交换 SDP / ICE 信令
- 连接建立后，剪贴板文本/图片直接通过 WebRTC DataChannel 在设备之间 P2P 传输
- Worker 不接触、不存储剪贴板内容，也不落任何持久化存储
- Worker 使用 Durable Object 的 WebSocket Hibernation API（`state.acceptWebSocket` + `webSocketMessage`/`webSocketClose`）：空闲房间在事件之间休眠，不计时长费用。若改用普通 `server.accept()`，一条 24 小时在线的 WebSocket 就会烧掉免费版每日时长配额（13,000 GB-s ≈ 28.2 DO 小时/天）的 85%

## 已部署实例

- 线上地址：`https://ditto-cloud-sync.cangshui.workers.dev`
- 部署账号：沧水（s8s@live.com）
- 部署验证：HTTP 端点 + WebSocket 端到端信令测试（8/8 通过）
- 测试脚本：`worker.test.mjs`（mock 运行时）、`deploy.test.mjs`（真实本地 wrangler dev）

## 部署

```bash
npm install -g wrangler
wrangler login
cd cloudflare-worker
wrangler deploy
```

注意：免费套餐的 Durable Object 迁移必须使用 `new_sqlite_classes`（已在 `wrangler.toml` 配置）。

部署后你会得到一个类似 `https://ditto-cloud-sync.<你的子域>.workers.dev` 的地址。

## 本地测试

```bash
wrangler dev
# 健康检查
curl http://localhost:8787/healthz
```

## 客户端填入的服务器地址

客户端“云同步”设置中的服务器地址填：

```
https://ditto-cloud-sync.<你的子域>.workers.dev
```

## 安全说明

- 房间 ID 是客户端根据“同步房间名/配对码”哈希后拼到 URL 里的，例如：
  `https://worker/room/ab12cd34...`
- 生产环境建议在 Worker 前面加 Cloudflare Access 或自定义鉴权；如需加密房间名，可在客户端对房间名再做 HMAC。
- 该实现只负责信令，不提供 TURN。绝大多数家用 NAT 通过 STUN 即可直连；如果网络环境必须 TURN，请在客户端 WebRTC 配置中自行加入 TURN 服务器。
