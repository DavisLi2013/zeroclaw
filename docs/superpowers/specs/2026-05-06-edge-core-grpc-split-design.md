# ZeroClaw Edge/Core gRPC Split Design

## Goal

Split the current single-process deployment into two explicit executables:

```text
webui / REST clients / WebSocket clients / webhook channels
        |
        v
zeroclaw-edge.exe
        |
      gRPC
        |
        v
zeroclaw-core.exe
```

`zeroclaw-edge.exe` owns every user-facing and business-facing gateway surface.
`zeroclaw-core.exe` owns only the gRPC agent execution service. The web UI never
connects to core directly.

## Feasibility

The split is feasible because the repository already contains both sides of the
future boundary:

- `crates/zeroclaw-gateway/src/lib.rs` owns HTTP, REST chat, webhooks, auth,
  rate limiting, static web assets, SSE, session persistence, and channel
  handlers.
- `crates/zeroclaw-gateway/src/ws.rs` owns WebSocket chat and currently calls
  `Agent::turn_streamed` directly.
- `crates/zeroclaw-gateway/src/grpc.rs` already exposes
  `zeroclaw.v1.AgentService` with `CreateRun`, `StreamRun`, `CancelRun`, and
  `GetRun`.
- `src/main.rs` already wires a `grpc` command that starts only the gRPC server.

The main missing piece is an edge-side agent backend abstraction plus a gRPC
client implementation. Once the gateway code calls that abstraction instead of
constructing `Agent` directly, the same HTTP/WebSocket/channel behavior can be
served by `zeroclaw-edge.exe` while actual agent execution happens in
`zeroclaw-core.exe`.

## Executable Responsibilities

### zeroclaw-edge.exe

`zeroclaw-edge.exe` is the only process exposed to WebUI, REST clients,
WebSocket clients, webhook senders, and channel integrations.

It owns:

- WebUI static asset serving.
- REST chat and management APIs.
- WebSocket chat.
- SSE/event fanout for WebUI.
- Webhook/channel adapters.
- Pairing token checks for external clients.
- Gateway rate limiting and idempotency.
- User-facing session state and WebUI message persistence.
- gRPC client connection to `zeroclaw-core.exe`.

It must not instantiate the local agent runtime for user requests in the
split-mode path.

### zeroclaw-core.exe

`zeroclaw-core.exe` is an internal service. It exposes only
`zeroclaw.v1.AgentService` over gRPC.

It owns:

- Agent runtime construction.
- Provider calls.
- Tool execution.
- Core cancellation token handling.
- Run status and retained run events for gRPC streaming.

It must not expose WebUI, REST APIs, WebSocket, webhooks, or channel handlers.

## Runtime Flow

1. WebUI connects to `zeroclaw-edge.exe` over HTTP/WebSocket.
2. Edge validates pairing/auth/rate-limit/session rules.
3. Edge creates an agent run through the configured `AgentBackend`.
4. In split mode, `GrpcAgentBackend` sends `CreateRun` to core.
5. Edge subscribes with `StreamRun`.
6. Core executes the agent turn and streams `RunEvent` values.
7. Edge maps `RunEvent` values back to existing WebUI/WebSocket/channel
   response formats.
8. If the user aborts, edge calls `CancelRun` on core.
9. Core emits `run.cancelled`; edge updates WebUI/session state.

## Configuration

Add gateway-side backend configuration:

```toml
[gateway.agent_backend]
kind = "grpc"
endpoint = "http://127.0.0.1:42618"
bearer_token = "replace-with-paired-core-token"
timeout_ms = 600000
```

For compatibility, the default during migration remains local backend for the
legacy `zeroclaw.exe gateway start` command. `zeroclaw-edge.exe` defaults to
`kind = "grpc"` and fails fast if no endpoint is configured.

## CLI Shape

Add two binary entrypoints:

```text
zeroclaw-core.exe --project-root <PATH> --host 127.0.0.1 --port 42618
zeroclaw-edge.exe --project-root <PATH> --host 0.0.0.0 --port 42617 --core-grpc http://127.0.0.1:42618
```

The existing `zeroclaw.exe` can remain as a compatibility binary while the split
stabilizes.

## Internal Boundary

Introduce an internal gateway abstraction:

```rust
#[async_trait::async_trait]
pub trait AgentBackend: Send + Sync {
    async fn run_chat_streamed(&self, request: AgentRunRequest)
        -> anyhow::Result<AgentRunStream>;

    async fn cancel_run(&self, run_id: &str, reason: &str)
        -> anyhow::Result<AgentCancelResult>;
}
```

Gateway handlers depend on `Arc<dyn AgentBackend>` instead of directly creating
`zeroclaw_runtime::agent::Agent`.

Initial implementations:

- `LocalAgentBackend`: preserves existing behavior for compatibility and tests.
- `GrpcAgentBackend`: production backend for `zeroclaw-edge.exe`.

## Security

- Core defaults to loopback bind.
- Public core bind keeps the existing public-bind warning and should require
  explicit operator opt-in.
- Edge authenticates to core using gRPC `authorization: Bearer <token>`.
- WebUI authenticates only with edge.
- mTLS and service identity policy remain follow-up production hardening.

## Testing Strategy

Tests should prove the boundary, not only the protocol:

- `zeroclaw-core.exe` starts the gRPC service and does not start HTTP gateway.
- `zeroclaw-edge.exe` requires a core gRPC endpoint.
- WebSocket chat calls `AgentBackend` instead of creating `Agent` directly.
- REST/webhook/channel chat calls `AgentBackend`.
- `GrpcAgentBackend` maps `RunEvent` to the gateway stream event model.
- WebSocket abort calls `CancelRun`.
- Core-unavailable failures are converted to stable gateway errors.

## Rollout

1. Add the abstraction with local backend as the default.
2. Add gRPC backend behind configuration.
3. Add `zeroclaw-core.exe`.
4. Add `zeroclaw-edge.exe`.
5. Migrate WebSocket chat to `AgentBackend`.
6. Migrate REST/webhook/channel chat to `AgentBackend`.
7. Change `zeroclaw-edge.exe` to require gRPC backend.
8. Keep old `zeroclaw.exe` behavior until the split path is verified.

## Residual Risks

- Gateway currently has several direct local agent call sites. Missing one would
  leave part of edge executing locally.
- The first split keeps core involved in agent/session memory behavior. A later
  phase can move more business session ownership to edge if needed.
- Core process-local run registry still requires sticky routing or a shared run
  store for multi-core deployments.
