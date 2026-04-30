# Frontend Web Gateway Protocol for ZeroClaw

本文档面向后续修改 `web/` 前端 UI 的开发者，说明浏览器如何连接本机或远端运行的 `zeroclaw.exe`，如何完成配对认证，如何通过 WebSocket 发送用户输入，并如何接收、组装和展示 ZeroClaw 的回复。

## 1. 结论

前端 Web UI 与 `zeroclaw.exe` 通信时，优先使用现有 HTTP/WebSocket Gateway：

- 默认 HTTP 地址：`http://127.0.0.1:42617`
- 默认 WebSocket 地址：`ws://127.0.0.1:42617/ws/chat`
- 默认绑定来源：`[gateway] host = "127.0.0.1"`，`port = 42617`
- 默认安全策略：`[gateway] require_pairing = true`
- 默认会话持久化：`[gateway] session_persistence = true`
- 浏览器聊天主协议：WebSocket `GET /ws/chat`
- 前端发送用户输入：`{"type":"message","content":"..."}`
- 前端接收回复：流式 `chunk`/`thinking`/`tool_call`/`tool_result`，最终以 `done` 收束

目前前端不应把“聊天请求”实现为普通 `POST /api/chat`，因为仓库中现有浏览器聊天入口是 WebSocket。REST API 主要用于状态、配置、配对、会话、记忆、工具、成本、诊断等管理能力。

## 2. 启动 zeroclaw.exe Gateway

在 Windows 上，推荐本机 Web UI 连接方式：

```powershell
zeroclaw.exe gateway start
```

等价于使用配置文件里的 `[gateway]` 地址：

```toml
[gateway]
host = "127.0.0.1"
port = 42617
require_pairing = true
allow_public_bind = false
session_persistence = true
```

也可以显式指定：

```powershell
zeroclaw.exe gateway start --host 127.0.0.1 --port 42617
```

如果需要让局域网或远端浏览器访问：

```powershell
zeroclaw.exe gateway start --host 0.0.0.0 --port 42617
```

此时需要在配置中显式允许公开绑定，或使用隧道：

```toml
[gateway]
allow_public_bind = true
```

安全建议：只有在受控网络、反向代理、TLS 或隧道环境下才绑定 `0.0.0.0`。默认 `127.0.0.1` 只允许本机访问。

## 3. 地址拼接规则

前端需要根据部署形态计算 Gateway 基础地址。

### 3.1 同源部署

如果 Web UI 由 Gateway 自身托管，前端直接使用相对路径：

```ts
const apiBase = "";
const wsBase =
  window.location.protocol === "https:"
    ? `wss://${window.location.host}`
    : `ws://${window.location.host}`;
```

请求示例：

- `GET /health`
- `POST /pair`
- `GET /api/status`
- `GET /ws/chat?session_id=<id>&token=<token>`

### 3.2 独立前端开发服务器

如果 Vite 前端运行在 `http://localhost:5173`，而 `zeroclaw.exe` Gateway 运行在默认端口：

```ts
const gatewayHttpOrigin = "http://127.0.0.1:42617";
const gatewayWsOrigin = "ws://127.0.0.1:42617";
```

请求示例：

- `GET http://127.0.0.1:42617/health`
- `POST http://127.0.0.1:42617/pair`
- `GET ws://127.0.0.1:42617/ws/chat?...`

### 3.3 配置了 path_prefix

如果配置：

```toml
[gateway]
path_prefix = "/zeroclaw"
```

所有 Gateway 路由都挂载在此前缀下：

- `GET /zeroclaw/health`
- `POST /zeroclaw/pair`
- `GET /zeroclaw/api/status`
- `GET /zeroclaw/ws/chat`

现有前端已通过 `window.__ZEROCLAW_BASE__` 和 `web/src/lib/basePath.ts` 支持该模式。

### 3.4 TLS

如果 Gateway 启用 TLS：

- HTTP origin 从 `http://...` 变为 `https://...`
- WebSocket origin 从 `ws://...` 变为 `wss://...`

前端不需要改变消息协议，只需要改变 URL scheme。

## 4. 配对与认证

默认情况下 Gateway 要求配对。前端必须先拿到 bearer token，之后 REST API 和 WebSocket 都要携带该 token。

### 4.1 查询公开健康状态

请求：

```http
GET /health
```

认证：不需要。

响应示例：

```json
{
  "status": "ok",
  "paired": false,
  "require_pairing": true,
  "runtime": {
    "pid": 12345,
    "updated_at": "2026-04-30T12:00:00Z",
    "uptime_seconds": 60,
    "components": {}
  }
}
```

前端行为：

- `require_pairing = false`：可跳过配对流程。
- `require_pairing = true` 且无本地 token：显示配对输入 UI。
- `paired = false`：可以尝试读取初始配对码。

### 4.2 查询初始配对码

请求：

```http
GET /pair/code
```

认证：不需要。

响应示例：

```json
{
  "success": true,
  "pairing_required": true,
  "pairing_code": "ABCD1234"
}
```

注意：

- 该接口只在初始未配对状态暴露配对码。
- 一旦已有设备配对，通常返回 `pairing_code: null`。
- 已配对后若要新增设备，可使用 CLI：`zeroclaw.exe gateway get-paircode --new`。

### 4.3 用配对码换取 token

请求：

```http
POST /pair
X-Pairing-Code: ABCD1234
```

认证：不需要。

成功响应：

```json
{
  "paired": true,
  "persisted": true,
  "token": "zc_...",
  "message": "Save this token - use it as Authorization: Bearer <token>"
}
```

失败响应：

```json
{
  "error": "Invalid pairing code"
}
```

限流响应：

```json
{
  "error": "Too many pairing requests. Please retry later.",
  "retry_after": 60
}
```

前端行为：

- 成功后保存 `token`，现有前端使用 `localStorage` key：`zeroclaw_token`。
- REST API 使用请求头：`Authorization: Bearer <token>`。
- WebSocket 使用 query 参数或 subprotocol 传 token，详见下一节。
- 收到 `401` 时应清除 token 并回到配对 UI。

## 5. WebSocket 聊天协议

这是 Web UI 输入信息并获取 ZeroClaw 回复的主协议。

### 5.1 连接 URL

基础格式：

```text
ws://127.0.0.1:42617/ws/chat?session_id=<session_id>&token=<token>&name=<session_name>
```

参数：

| 参数 | 必填 | 说明 |
|---|---:|---|
| `session_id` | 否 | 前端生成的稳定会话 ID。省略时服务端生成 UUID。建议前端持久保存。 |
| `token` | 配对开启时必填之一 | 浏览器无法设置 WebSocket `Authorization` header 时可用 query 参数传 token。 |
| `name` | 否 | 会话显示名。服务端会在会话持久化开启时保存。 |

推荐前端同时使用 subprotocol：

```ts
const protocols = ["zeroclaw.v1"];
if (token) protocols.push(`bearer.${token}`);

const ws = new WebSocket(
  `ws://127.0.0.1:42617/ws/chat?session_id=${encodeURIComponent(sessionId)}&token=${encodeURIComponent(token)}`,
  protocols,
);
```

认证优先级：

1. `Authorization: Bearer <token>` header
2. `Sec-WebSocket-Protocol: bearer.<token>`
3. `?token=<token>` query 参数

浏览器原生 `WebSocket` 通常不能自定义 `Authorization` header，因此 Web UI 应使用 query 参数和/或 `bearer.<token>` subprotocol。

### 5.2 建连后的第一帧

服务端连接成功后立即发送：

```json
{
  "type": "session_start",
  "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df",
  "resumed": true,
  "message_count": 12,
  "name": "My Session"
}
```

字段说明：

| 字段 | 类型 | 说明 |
|---|---|---|
| `type` | string | 固定为 `session_start`。 |
| `session_id` | string | 当前会话 ID。 |
| `resumed` | boolean | 是否从持久化历史恢复。 |
| `message_count` | number | 已恢复的消息数量。 |
| `name` | string | 可选，会话名。 |

前端行为：

- 如果本地没有会话 ID，应保存服务端返回的 `session_id`。
- 如果 `resumed = true`，可调用 `GET /api/sessions/{id}/messages` 补全历史记录。

### 5.3 可选 connect 帧

连接后客户端可以发送一次可选握手帧：

```json
{
  "type": "connect",
  "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df",
  "device_name": "Chrome on Windows",
  "capabilities": ["chat", "streaming"]
}
```

服务端响应：

```json
{
  "type": "connected",
  "message": "Connection established"
}
```

该帧不是必需的。现有前端可以直接发送 `message` 帧。

### 5.4 发送用户消息

请求帧：

```json
{
  "type": "message",
  "content": "你好，请帮我总结当前项目。"
}
```

约束：

- `type` 必须是 `message`。
- `content` 必须是非空字符串。
- 同一 `session_id` 的并发 turn 会被服务端串行化。如果已有响应运行中，可能收到 `SESSION_BUSY`。

### 5.5 接收流式回复

一次用户消息可能触发多种服务端帧。

#### thinking

模型思考/推理增量：

```json
{
  "type": "thinking",
  "content": "I need to inspect the workspace..."
}
```

前端建议：

- 单独显示在“思考中”区域。
- 不要拼到最终 assistant 正文，除非产品明确需要展示。

#### chunk

assistant 正文增量：

```json
{
  "type": "chunk",
  "content": "这是第一段回复"
}
```

前端行为：

- 将多个 `chunk.content` 追加到当前 assistant 草稿。
- 边接收边渲染，实现流式输出。

#### tool_call

工具调用开始：

```json
{
  "type": "tool_call",
  "id": "call_123",
  "name": "shell",
  "args": {
    "command": "cargo test"
  }
}
```

前端行为：

- 可展示工具卡片，状态为 running。
- `id` 可用于和后续 `tool_result` 关联。

#### tool_result

工具调用结果：

```json
{
  "type": "tool_result",
  "id": "call_123",
  "name": "shell",
  "output": "test result..."
}
```

前端行为：

- 找到同 `id` 的工具卡片，状态改为 completed。
- 对长输出做折叠，避免撑破聊天布局。

#### chunk_reset

服务端通知前端清空流式草稿，准备使用权威最终结果：

```json
{
  "type": "chunk_reset"
}
```

前端行为：

- 清空当前 assistant 草稿。
- 等待后续 `done.full_response` 作为最终 assistant 消息。

#### done

本轮最终回复：

```json
{
  "type": "done",
  "full_response": "这是完整、权威的最终回复。"
}
```

前端行为：

- 用 `full_response` 写入最终 assistant 消息。
- 将当前 turn 状态改为 idle。
- 停止 loading 指示。

#### aborted

用户取消当前响应后，服务端发送：

```json
{
  "type": "aborted"
}
```

前端行为：

- 停止 loading。
- 标记当前 assistant 消息为 interrupted。
- 可以保留已经显示的 partial chunks。

#### error

错误帧：

```json
{
  "type": "error",
  "message": "Message content cannot be empty",
  "code": "EMPTY_CONTENT"
}
```

常见 `code`：

| code | 场景 | 前端建议 |
|---|---|---|
| `INVALID_JSON` | 客户端发了非法 JSON | 记录前端 bug，提示重试。 |
| `UNKNOWN_MESSAGE_TYPE` | `type` 不是 `message` | 检查前端协议实现。 |
| `EMPTY_CONTENT` | `content` 为空 | 禁用空输入提交。 |
| `SESSION_BUSY` | 同会话已有响应运行中 | 禁用发送按钮或提示等待。 |
| `AUTH_ERROR` | provider/API key 认证失败 | 引导用户检查配置。 |
| `PROVIDER_ERROR` | provider/model 相关错误 | 显示模型配置错误。 |
| `AGENT_ERROR` | agent turn 通用失败 | 显示错误并允许重试。 |
| `AGENT_INIT_FAILED` | Agent 初始化失败 | 显示启动/配置错误。 |

### 5.6 推荐前端状态机

前端每个会话建议维护：

```ts
type ChatTurnState = "idle" | "connecting" | "streaming" | "tool_running" | "error";
```

状态转换：

1. WebSocket 初始化：`idle -> connecting`
2. 收到 `session_start`：`connecting -> idle`
3. 发送 `message`：`idle -> streaming`
4. 收到 `chunk`/`thinking`：保持 `streaming`
5. 收到 `tool_call`：`streaming -> tool_running`
6. 收到 `tool_result`：如果仍有文本流则回到 `streaming`
7. 收到 `done`：`streaming/tool_running -> idle`
8. 收到 `aborted`：`streaming/tool_running -> idle`
9. 收到 `error`：`* -> error`

## 6. 会话管理 REST API

REST 会话接口需要 bearer token。

请求头：

```http
Authorization: Bearer <token>
```

### 6.1 列出会话

```http
GET /api/sessions
```

响应：

```json
{
  "sessions": [
    {
      "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df",
      "created_at": "2026-04-30T12:00:00Z",
      "last_activity": "2026-04-30T12:05:00Z",
      "message_count": 4,
      "name": "My Session"
    }
  ]
}
```

### 6.2 加载会话消息

```http
GET /api/sessions/{id}/messages
```

响应：

```json
{
  "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df",
  "messages": [
    {
      "role": "user",
      "content": "你好"
    },
    {
      "role": "assistant",
      "content": "你好，有什么我可以帮你？"
    }
  ],
  "session_persistence": true
}
```

### 6.3 重命名会话

```http
PUT /api/sessions/{id}
Content-Type: application/json
Authorization: Bearer <token>

{
  "name": "Project Review"
}
```

响应：

```json
{
  "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df",
  "name": "Project Review"
}
```

### 6.4 删除会话

```http
DELETE /api/sessions/{id}
Authorization: Bearer <token>
```

响应：

```json
{
  "deleted": true,
  "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df"
}
```

### 6.5 查询会话运行状态

```http
GET /api/sessions/{id}/state
Authorization: Bearer <token>
```

响应：

```json
{
  "session_id": "7d77d27c-4c2d-4e9e-a35e-6ed6828335df",
  "state": "running",
  "turn_id": "turn-uuid",
  "turn_started_at": "2026-04-30T12:05:00Z"
}
```

### 6.6 取消当前回复

```http
POST /api/sessions/{id}/abort
Authorization: Bearer <token>
```

响应：

```json
{
  "status": "aborted"
}
```

如果没有正在运行的回复：

```json
{
  "status": "no_active_response"
}
```

前端行为：

- 点击停止按钮时调用该接口。
- 同时保持 WebSocket 连接，等待 `aborted` 帧。
- 该接口是幂等的，重复调用不会破坏会话。

## 7. 状态、事件与辅助 REST API

这些接口用于构建完整 Web UI，但不是发送聊天消息的主路径。

### 7.1 Gateway 状态

```http
GET /api/status
Authorization: Bearer <token>
```

响应字段包括：

- `provider`
- `model`
- `temperature`
- `uptime_seconds`
- `gateway_port`
- `locale`
- `memory_backend`
- `paired`
- `channels`
- `health`

### 7.2 健康快照

```http
GET /api/health
Authorization: Bearer <token>
```

返回运行时健康组件状态。

### 7.3 SSE 实时事件

```http
GET /api/events
Authorization: Bearer <token>
Accept: text/event-stream
```

服务端通过 SSE 推送观测事件，例如：

```json
{
  "type": "agent_start",
  "provider": "openrouter",
  "model": "anthropic/claude-sonnet-4",
  "timestamp": "2026-04-30T12:00:00Z"
}
```

```json
{
  "type": "tool_call",
  "tool": "shell",
  "duration_ms": 1200,
  "success": true,
  "timestamp": "2026-04-30T12:00:05Z"
}
```

历史事件：

```http
GET /api/events/history
Authorization: Bearer <token>
```

响应：

```json
{
  "events": []
}
```

前端建议：

- 聊天内容使用 WebSocket。
- 全局运行状态、工具调用摘要、成本/观测面板可订阅 SSE。

## 8. 浏览器端最小实现示例

下面是一个最小可用连接流程。

```ts
const gatewayHttp = "http://127.0.0.1:42617";
const gatewayWs = "ws://127.0.0.1:42617";

async function pairWithGateway(code: string): Promise<string> {
  const response = await fetch(`${gatewayHttp}/pair`, {
    method: "POST",
    headers: {
      "X-Pairing-Code": code,
    },
  });

  if (!response.ok) {
    throw new Error(await response.text());
  }

  const data = await response.json() as { token: string };
  localStorage.setItem("zeroclaw_token", data.token);
  return data.token;
}

function connectChat(sessionId: string, token: string) {
  const params = new URLSearchParams({
    session_id: sessionId,
    token,
  });

  const ws = new WebSocket(
    `${gatewayWs}/ws/chat?${params.toString()}`,
    ["zeroclaw.v1", `bearer.${token}`],
  );

  let assistantDraft = "";

  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    switch (msg.type) {
      case "session_start":
        console.log("session:", msg.session_id, "resumed:", msg.resumed);
        break;
      case "thinking":
        renderThinking(msg.content);
        break;
      case "chunk":
        assistantDraft += msg.content ?? "";
        renderAssistantDraft(assistantDraft);
        break;
      case "tool_call":
        renderToolCall(msg.id, msg.name, msg.args);
        break;
      case "tool_result":
        renderToolResult(msg.id, msg.name, msg.output);
        break;
      case "chunk_reset":
        assistantDraft = "";
        clearAssistantDraft();
        break;
      case "done":
        renderAssistantFinal(msg.full_response);
        assistantDraft = "";
        break;
      case "aborted":
        markCurrentTurnAborted();
        break;
      case "error":
        renderError(msg.code, msg.message);
        break;
    }
  };

  return {
    send(content: string) {
      ws.send(JSON.stringify({ type: "message", content }));
    },
    close() {
      ws.close();
    },
  };
}
```

## 9. 已存在协议的前端调用示例

本节只给出当前仓库已经存在的前端可调用协议示例。不存在的协议不提供示例，例如：

- 不存在 `POST /api/chat`，聊天请使用 `GET /ws/chat` WebSocket。
- 浏览器 Web UI 不直接调用原生 gRPC；当前文档不提供 gRPC-Web 示例。

### 9.1 网关地址与通用工具

适用于独立 Web UI 连接默认本机 Gateway：

```ts
const gatewayHttp = "http://127.0.0.1:42617";
const gatewayWs = "ws://127.0.0.1:42617";

const TOKEN_KEY = "zeroclaw_token";
const SESSION_KEY = "zeroclaw_session_id";

function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

function getOrCreateSessionId(): string {
  const existing = localStorage.getItem(SESSION_KEY);
  if (existing) return existing;

  const created = crypto.randomUUID();
  localStorage.setItem(SESSION_KEY, created);
  return created;
}

async function readJson<T>(response: Response): Promise<T> {
  if (response.status === 401) {
    clearToken();
    throw new Error("Unauthorized: pairing token is missing or expired");
  }

  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(`HTTP ${response.status}: ${text || response.statusText}`);
  }

  return response.json() as Promise<T>;
}
```

如果 Web UI 由 Gateway 同源托管，把 `gatewayHttp` 留空并从当前页面推导 WebSocket origin：

```ts
const gatewayHttp = "";
const gatewayWs =
  window.location.protocol === "https:"
    ? `wss://${window.location.host}`
    : `ws://${window.location.host}`;
```

如果配置了 `gateway.path_prefix = "/zeroclaw"`，给所有 path 加此前缀：

```ts
const basePath = "/zeroclaw";
```

### 9.2 查询 Gateway 是否在线

已存在协议：`GET /health`，不需要认证。

```ts
interface PublicHealth {
  status: "ok";
  paired: boolean;
  require_pairing: boolean;
  runtime: unknown;
}

async function getPublicHealth(basePath = ""): Promise<PublicHealth> {
  const response = await fetch(`${gatewayHttp}${basePath}/health`);
  return readJson<PublicHealth>(response);
}

async function ensureGatewayOnline(): Promise<void> {
  const health = await getPublicHealth();

  if (health.require_pairing && !getToken()) {
    showPairingScreen();
    return;
  }

  showChatScreen();
}
```

### 9.3 获取初始配对码

已存在协议：`GET /pair/code`，不需要认证。

```ts
interface PairCodeResponse {
  success: boolean;
  pairing_required: boolean;
  pairing_code: string | null;
}

async function getInitialPairCode(basePath = ""): Promise<string | null> {
  const response = await fetch(`${gatewayHttp}${basePath}/pair/code`);
  const data = await readJson<PairCodeResponse>(response);
  return data.pairing_code;
}

async function renderInitialPairCode(): Promise<void> {
  const code = await getInitialPairCode();

  if (code) {
    showPairCode(code);
  } else {
    showManualPairingInput();
  }
}
```

### 9.4 提交配对码换取 token

已存在协议：`POST /pair`，请求头使用 `X-Pairing-Code`。

```ts
interface PairResponse {
  paired: boolean;
  persisted: boolean;
  token: string;
  message: string;
}

async function pairWithCode(code: string, basePath = ""): Promise<string> {
  const response = await fetch(`${gatewayHttp}${basePath}/pair`, {
    method: "POST",
    headers: {
      "X-Pairing-Code": code.trim(),
    },
  });

  const data = await readJson<PairResponse>(response);
  setToken(data.token);
  return data.token;
}

async function onPairSubmit(code: string): Promise<void> {
  try {
    await pairWithCode(code);
    showChatScreen();
  } catch (error) {
    showError(error instanceof Error ? error.message : String(error));
  }
}
```

### 9.5 调用已认证 REST API

已存在协议：`GET /api/status`、`GET /api/sessions`、`GET /api/sessions/{id}/messages`、`POST /api/sessions/{id}/abort` 等。

```ts
async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
  basePath = "",
): Promise<T> {
  const headers = new Headers(options.headers);
  const token = getToken();

  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  if (
    options.body &&
    typeof options.body === "string" &&
    !headers.has("Content-Type")
  ) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(`${gatewayHttp}${basePath}${path}`, {
    ...options,
    headers,
  });

  return readJson<T>(response);
}

interface StatusResponse {
  provider: string | null;
  model: string;
  temperature: number;
  uptime_seconds: number;
  gateway_port: number;
  locale: string;
  memory_backend: string;
  paired: boolean;
  channels: Record<string, boolean>;
  health: unknown;
}

async function loadStatus(): Promise<StatusResponse> {
  return apiFetch<StatusResponse>("/api/status");
}
```

### 9.6 加载会话列表与历史消息

已存在协议：`GET /api/sessions`、`GET /api/sessions/{id}/messages`。

```ts
interface GatewaySession {
  session_id: string;
  created_at: string;
  last_activity: string;
  message_count: number;
  name?: string;
}

interface SessionListResponse {
  sessions: GatewaySession[];
}

interface SessionMessage {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
}

interface SessionMessagesResponse {
  session_id: string;
  messages: SessionMessage[];
  session_persistence: boolean;
}

async function listSessions(): Promise<GatewaySession[]> {
  const data = await apiFetch<SessionListResponse>("/api/sessions");
  return data.sessions;
}

async function loadSessionMessages(
  sessionId: string,
): Promise<SessionMessage[]> {
  const data = await apiFetch<SessionMessagesResponse>(
    `/api/sessions/${encodeURIComponent(sessionId)}/messages`,
  );
  return data.messages;
}
```

### 9.7 取消正在生成的回复

已存在协议：`POST /api/sessions/{id}/abort`。

```ts
interface AbortResponse {
  status: "aborted" | "no_active_response";
}

async function abortCurrentResponse(sessionId: string): Promise<AbortResponse> {
  return apiFetch<AbortResponse>(
    `/api/sessions/${encodeURIComponent(sessionId)}/abort`,
    { method: "POST" },
  );
}

async function onStopButtonClick(sessionId: string): Promise<void> {
  const result = await abortCurrentResponse(sessionId);

  if (result.status === "aborted") {
    markCurrentTurnStopping();
  }
}
```

### 9.8 WebSocket 聊天完整调用示例

已存在协议：`GET /ws/chat?session_id=...&token=...`。

```ts
type WsMessage =
  | { type: "session_start"; session_id: string; resumed: boolean; message_count: number; name?: string }
  | { type: "connected"; message: string }
  | { type: "thinking"; content: string }
  | { type: "chunk"; content: string }
  | { type: "tool_call"; id?: string; name: string; args: unknown }
  | { type: "tool_result"; id?: string; name: string; output: string }
  | { type: "chunk_reset" }
  | { type: "done"; full_response: string }
  | { type: "aborted" }
  | { type: "error"; message: string; code?: string };

class ZeroClawChatClient {
  private ws: WebSocket | null = null;
  private assistantDraft = "";

  constructor(
    private readonly onMessage: (message: WsMessage) => void,
  ) {}

  connect(): void {
    const token = getToken();
    const sessionId = getOrCreateSessionId();
    const params = new URLSearchParams({ session_id: sessionId });

    if (token) {
      params.set("token", token);
    }

    const protocols = ["zeroclaw.v1"];
    if (token) {
      protocols.push(`bearer.${token}`);
    }

    this.ws = new WebSocket(
      `${gatewayWs}/ws/chat?${params.toString()}`,
      protocols,
    );

    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data) as WsMessage;
      this.handleMessage(message);
      this.onMessage(message);
    };

    this.ws.onclose = () => {
      showDisconnectedState();
    };

    this.ws.onerror = () => {
      showError("WebSocket connection failed");
    };
  }

  send(content: string): void {
    const trimmed = content.trim();
    if (!trimmed) return;

    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("WebSocket is not connected");
    }

    this.ws.send(JSON.stringify({ type: "message", content: trimmed }));
    appendUserMessage(trimmed);
  }

  disconnect(): void {
    this.ws?.close();
    this.ws = null;
  }

  private handleMessage(message: WsMessage): void {
    switch (message.type) {
      case "session_start":
        localStorage.setItem(SESSION_KEY, message.session_id);
        setChatReady(message.resumed, message.message_count);
        break;
      case "thinking":
        renderThinking(message.content);
        break;
      case "chunk":
        this.assistantDraft += message.content;
        renderAssistantDraft(this.assistantDraft);
        break;
      case "tool_call":
        renderToolCall(message.id, message.name, message.args);
        break;
      case "tool_result":
        renderToolResult(message.id, message.name, message.output);
        break;
      case "chunk_reset":
        this.assistantDraft = "";
        clearAssistantDraft();
        break;
      case "done":
        this.assistantDraft = "";
        appendAssistantMessage(message.full_response);
        setChatIdle();
        break;
      case "aborted":
        this.assistantDraft = "";
        markCurrentTurnAborted();
        setChatIdle();
        break;
      case "error":
        showError(`${message.code ?? "ERROR"}: ${message.message}`);
        setChatIdle();
        break;
      case "connected":
        break;
    }
  }
}
```

### 9.9 可选 connect 握手帧

已存在协议：WebSocket 首帧可发送 `{"type":"connect" ...}`。这不是必需流程，普通聊天可以直接发送 `message`。

```ts
function sendOptionalConnectFrame(ws: WebSocket, sessionId: string): void {
  ws.send(JSON.stringify({
    type: "connect",
    session_id: sessionId,
    device_name: navigator.userAgent,
    capabilities: ["chat", "streaming"],
  }));
}
```

### 9.10 订阅 SSE 事件流

已存在协议：`GET /api/events`，需要 `Authorization: Bearer <token>`。由于浏览器原生 `EventSource` 不能设置自定义 Authorization header，前端应使用 `fetch` 读取 `text/event-stream`。

```ts
interface SseEvent {
  type: string;
  timestamp?: string;
  [key: string]: unknown;
}

async function subscribeEvents(
  onEvent: (event: SseEvent) => void,
): Promise<AbortController> {
  const controller = new AbortController();
  const headers = new Headers({ Accept: "text/event-stream" });
  const token = getToken();

  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  const response = await fetch(`${gatewayHttp}/api/events`, {
    headers,
    signal: controller.signal,
  });

  if (!response.ok || !response.body) {
    throw new Error(`SSE failed: ${response.status}`);
  }

  void readSseStream(response.body, onEvent);
  return controller;
}

async function readSseStream(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: SseEvent) => void,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const events = buffer.split("\n\n");
    buffer = events.pop() ?? "";

    for (const raw of events) {
      const dataLines = raw
        .split("\n")
        .filter((line) => line.startsWith("data:"))
        .map((line) => line.slice("data:".length).trim());

      if (dataLines.length === 0) continue;

      onEvent(JSON.parse(dataLines.join("\n")) as SseEvent);
    }
  }
}
```

### 9.11 获取 SSE 历史事件

已存在协议：`GET /api/events/history`。

```ts
interface EventHistoryResponse {
  events: SseEvent[];
}

async function loadEventHistory(): Promise<SseEvent[]> {
  const data = await apiFetch<EventHistoryResponse>("/api/events/history");
  return data.events;
}
```

### 9.12 同源前端中的最小组合流程

这个例子只组合已存在协议：`/health`、`/pair/code`、`/pair`、`/ws/chat`。

```ts
async function bootZeroClawUi(): Promise<ZeroClawChatClient | null> {
  const health = await getPublicHealth();

  if (health.require_pairing && !getToken()) {
    const code = await getInitialPairCode();
    if (code) showPairCode(code);
    showManualPairingInput();
    return null;
  }

  const client = new ZeroClawChatClient((message) => {
    console.debug("ZeroClaw message", message.type);
  });
  client.connect();
  return client;
}
```

## 10. CORS 与开发环境注意事项

当前仓库前端更偏向以下两种模式：

1. Gateway 托管编译后的 Web UI，同源调用 API。
2. Tauri 或本地客户端显式知道 Gateway URL。

如果纯浏览器 Vite dev server 直接跨源访问 `http://127.0.0.1:42617`，需要确认 Gateway 是否已允许对应跨源请求。若遇到浏览器 CORS 拦截，推荐开发期使用 Vite proxy，把 `/api`、`/pair`、`/health`、`/ws` 代理到 Gateway：

```ts
// web/vite.config.ts
server: {
  proxy: {
    "/api": "http://127.0.0.1:42617",
    "/pair": "http://127.0.0.1:42617",
    "/health": "http://127.0.0.1:42617",
    "/ws": {
      target: "ws://127.0.0.1:42617",
      ws: true,
    },
  },
}
```

如果 Web UI 由 Gateway 托管，通常不需要 CORS。

## 11. gRPC 说明

仓库中已有 gRPC 相关代码与设计，默认地址策略是：

- host 默认使用 `gateway.host`
- port 默认使用 `gateway.port + 1`
- 在默认配置下即 `127.0.0.1:42618`

但浏览器 Web UI 不应直接依赖原生 gRPC：

- 浏览器不能直接调用普通 gRPC HTTP/2 服务。
- 如果未来需要浏览器调用 gRPC，需要新增 gRPC-Web 或 Connect-Web 层，或通过后端代理转换。
- 当前“用户在 Web UI 输入信息并获取回复”的既有可用方案是 WebSocket `/ws/chat`。

## 12. 前端修改清单

后续修改 Web UI 时，建议按以下顺序实现：

1. 网关地址配置：允许用户输入或自动发现 `http://127.0.0.1:42617`。
2. 健康检查：调用 `GET /health`，展示 Gateway 是否在线、是否需要配对。
3. 配对流程：调用 `GET /pair/code` 和 `POST /pair`，保存 `token`。
4. WebSocket 连接：连接 `/ws/chat?session_id=...&token=...`。
5. 消息输入：发送 `{"type":"message","content": text}`。
6. 流式展示：追加 `chunk`，展示 `thinking`、`tool_call`、`tool_result`。
7. 最终落盘：收到 `done.full_response` 后替换草稿为最终 assistant 消息。
8. 会话恢复：启动时调用 `GET /api/sessions` 和 `GET /api/sessions/{id}/messages`。
9. 取消响应：停止按钮调用 `POST /api/sessions/{id}/abort`。
10. 错误处理：`401` 清 token；`SESSION_BUSY` 禁用重复发送；provider/agent 错误展示可操作提示。

## 13. 协议兼容性约定

前端应把 `type` 作为 WebSocket 帧的分发字段，并忽略未知字段。这样后端增加字段时不会破坏旧 UI。

建议前端保守处理未知 `type`：

```ts
default:
  console.debug("Unknown ZeroClaw WS message", msg);
  break;
```

必须稳定依赖的字段：

- 客户端发送：`type`, `content`
- 服务端 `session_start`：`type`, `session_id`, `resumed`, `message_count`
- 服务端 `chunk`：`type`, `content`
- 服务端 `thinking`：`type`, `content`
- 服务端 `tool_call`：`type`, `id`, `name`, `args`
- 服务端 `tool_result`：`type`, `id`, `name`, `output`
- 服务端 `done`：`type`, `full_response`
- 服务端 `error`：`type`, `message`, `code`

## 14. 安全要求

- 不要把 bearer token 写入日志、URL 展示区、错误上报系统或截图。
- 如果必须把 token 放在 WebSocket query 参数中，只在 HTTPS/WSS 或本机环回地址下使用。
- 公开绑定 `0.0.0.0` 时必须配合配对、TLS、反向代理访问控制或隧道。
- 不要在前端硬编码真实 API key；模型 provider API key 由 `zeroclaw.exe` 配置管理。
- `GET /pair/code` 只用于初始配对体验；已配对后新增设备应走 CLI 或受控管理流程。
