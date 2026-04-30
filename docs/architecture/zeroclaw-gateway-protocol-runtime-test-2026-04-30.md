# ZeroClaw Gateway Protocol Runtime Test - 2026-04-30

本文档记录一次在本机实际启动 `zeroclaw.exe` 并调用现有 Gateway 协议的测试结果。

## 运行实例

- 可执行文件：`D:\workspace\axi-research\zeroclaw\target\release\zeroclaw.exe`
- 版本：`zeroclaw 0.7.3`
- 监听地址：`http://127.0.0.1:43397`
- WebSocket 地址：`ws://127.0.0.1:43397/ws/chat`
- Gateway 配置目录：`C:\Users\ADMINI~1\AppData\Local\Temp\zeroclaw-protocol-test-43397`
- 宿主进程：`cmd.exe`
- 宿主 PID：`51884`
- ZeroClaw PID：`85776`
- 状态：测试完成后保持运行，未关闭 `zeroclaw.exe`

启动命令：

```cmd
cmd.exe /k cd /d "D:\workspace\axi-research\zeroclaw" && set ZEROCLAW_GATEWAY_HOST=127.0.0.1 && set ZEROCLAW_GATEWAY_PORT=43397 && set ZEROCLAW_GATEWAY_TIMEOUT_SECS=120 && "D:\workspace\axi-research\zeroclaw\target\release\zeroclaw.exe" gateway start --config-dir "C:\Users\ADMINI~1\AppData\Local\Temp\zeroclaw-protocol-test-43397" --host 127.0.0.1 --port 43397
```

说明：

- 本次使用临时配置目录，避免改写用户默认 `~/.zeroclaw/config.toml`。
- Gateway 默认要求 pairing，本次按真实 pairing 流程换取 bearer token。
- 文档不记录 bearer token。

## 测试结果摘要

| 协议 | 端点 | 结果 |
|---|---|---|
| HTTP public health | `GET /health` | 成功，HTTP 200 |
| HTTP pairing code | `GET /pair/code` | 成功，HTTP 200，返回一次性配对码 |
| HTTP pairing exchange | `POST /pair` | 成功，HTTP 200，返回 bearer token |
| HTTP auth enforcement | `GET /api/status` without token | 成功验证，返回 HTTP 401 |
| HTTP authenticated status | `GET /api/status` with token | 成功，HTTP 200 |
| HTTP sessions | `GET /api/sessions` with token | 成功，HTTP 200 |
| HTTP session messages | `GET /api/sessions/{id}/messages` with token | 成功，HTTP 200 |
| SSE stream | `GET /api/events` with token | 成功，HTTP 200，`Content-Type: text/event-stream` |
| SSE history | `GET /api/events/history` with token | 成功，HTTP 200 |
| Metrics | `GET /metrics` | 成功，HTTP 200 |
| WebSocket chat | `GET /ws/chat?session_id=...&token=...` | 成功连接并收到 JSON 帧；Agent 因临时配置缺少模型返回 `AGENT_INIT_FAILED` |

## 实际返回

### `GET /health`

请求：

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:43397/health
```

返回摘要：

```json
{
  "status": "ok",
  "paired": true,
  "require_pairing": true,
  "runtime": {
    "pid": 85776,
    "uptime_seconds": 146,
    "components": {
      "gateway": {
        "status": "ok",
        "last_error": null,
        "restart_count": 0
      }
    }
  }
}
```

### `GET /pair/code`

配对前返回摘要：

```json
{
  "success": true,
  "pairing_required": true,
  "pairing_code": "942215"
}
```

配对后再次调用返回：

```json
{
  "success": true,
  "pairing_required": true,
  "pairing_code": null
}
```

这符合现有实现：初始未配对时公开一次性配对码，首次配对成功后不再公开新码。

### `POST /pair`

请求：

```powershell
Invoke-RestMethod `
  -Method Post `
  -Uri http://127.0.0.1:43397/pair `
  -Headers @{ "X-Pairing-Code" = "<one-time-code>" }
```

返回摘要：

```json
{
  "paired": true,
  "persisted": true,
  "token_prefix": "zc_42f..."
}
```

完整 token 未写入本文档。

### `GET /api/status`

未携带 token：

```json
{
  "status": 401
}
```

携带 token：

```json
{
  "paired": true,
  "model": "anthropic/claude-sonnet-4",
  "gateway_port": 42617,
  "memory_backend": "sqlite"
}
```

注意：`gateway_port` 来自配置默认值；本次进程实际命令行覆盖监听端口为 `43397`。

### `GET /api/sessions`

请求：

```powershell
Invoke-RestMethod `
  -Uri http://127.0.0.1:43397/api/sessions `
  -Headers @{ Authorization = "Bearer <token>" }
```

返回摘要：

```json
{
  "sessions_count": 0
}
```

### `GET /api/sessions/{id}/messages`

请求：

```powershell
Invoke-RestMethod `
  -Uri http://127.0.0.1:43397/api/sessions/9617418f-c145-4cb9-bfe0-3f9e0a06ec1b/messages `
  -Headers @{ Authorization = "Bearer <token>" }
```

返回：

```json
{
  "session_id": "9617418f-c145-4cb9-bfe0-3f9e0a06ec1b",
  "messages": [],
  "session_persistence": true
}
```

### `GET /api/events`

请求使用 `ResponseHeadersRead` 打开 SSE 连接并读取响应头。

返回摘要：

```json
{
  "status": 200,
  "content_type": "text/event-stream"
}
```

### `GET /api/events/history`

返回摘要：

```json
{
  "events_count": 0
}
```

### `GET /metrics`

返回摘要：

```json
{
  "status": 200,
  "content_type": "text/plain; version=0.0.4; charset=utf-8",
  "body_prefix": "# Prometheus backend not enabled. Set [observability] backend = \"prometheus\" in "
}
```

### `GET /ws/chat`

请求：

```text
ws://127.0.0.1:43397/ws/chat?session_id=9617418f-c145-4cb9-bfe0-3f9e0a06ec1b&token=<token>
```

WebSocket subprotocol：

```text
zeroclaw.v1
bearer.<token>
```

实际收到服务端 JSON 帧：

```json
{
  "type": "error",
  "code": "AGENT_INIT_FAILED",
  "message": "Failed to initialise agent: no model configured: providers.fallback = None resolves with no model, and no [[providers.models.*]] entry has a `model` field set. Configure at least one [providers.models.<name>] model = \"...\" or define a [[model_routes]] hint."
}
```

结论：

- WebSocket 协议、token 认证、JSON 帧返回链路均已实际打通。
- 本次未得到自然语言 assistant 回复，原因是临时配置目录没有 provider/model 配置。
- 若要让 `done.full_response` 返回真实 assistant 文本，需要在该配置目录中配置至少一个 `[providers.models.<name>] model = "..."`，并提供对应 provider API key 或可访问的本地模型服务。

## 当前保留的运行状态

测试结束后未关闭 `zeroclaw.exe`。

可用检查命令：

```powershell
Invoke-RestMethod -Uri http://127.0.0.1:43397/health
```

进程检查命令：

```powershell
Get-Process -Id 85776
```

停止命令，仅在需要手动清理时使用：

```powershell
Stop-Process -Id 85776
Stop-Process -Id 51884
```
