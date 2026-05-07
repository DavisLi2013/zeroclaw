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

- `crates/edge/zeroclaw-gateway/src/lib.rs` owns HTTP, REST chat, webhooks, auth,
  rate limiting, static web assets, SSE, session persistence, and channel
  handlers.
- `crates/edge/zeroclaw-gateway/src/ws.rs` owns WebSocket chat and currently calls
  `Agent::turn_streamed` directly.
- `crates/edge/zeroclaw-gateway/src/grpc.rs` already exposes
  `zeroclaw.v1.AgentService` with `CreateRun`, `StreamRun`, `CancelRun`, and
  `GetRun`.
- `src/main.rs` already wires a `grpc` command that starts only the gRPC server.

The main missing piece is an edge-side core client abstraction plus a gRPC
client implementation. Once the gateway code calls that client instead of
constructing `Agent` directly, the same HTTP/WebSocket/channel behavior can be
served by `zeroclaw-edge.exe` while all agent execution happens only in
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

It must not instantiate or call the local agent runtime in any production path.
Specifically, edge code must not call `Agent::from_config`,
`Agent::from_config_with_session_cwd`, `Agent::turn_streamed`, or
`zeroclaw_runtime::agent::process_message`. Edge receives user/channel input,
normalizes it, sends it to core over gRPC, receives gRPC events/responses, and
then translates those responses back to WebUI, REST, WebSocket, webhook, or
channel formats.

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
3. Edge creates an agent run through `CoreAgentClient`.
4. `GrpcCoreAgentClient` sends `CreateRun` to core.
5. Edge subscribes with `StreamRun`.
6. Core executes the agent turn and streams `RunEvent` values.
7. Edge maps `RunEvent` values back to existing WebUI/WebSocket/channel
   response formats.
8. If the user aborts, edge calls `CancelRun` on core.
9. Core emits `run.cancelled`; edge updates WebUI/session state.

## Configuration

Add gateway-side backend configuration:

```toml
[gateway.core]
endpoint = "http://127.0.0.1:42618"
bearer_token = "replace-with-paired-core-token"
timeout_ms = 600000
```

For `zeroclaw-edge.exe`, `gateway.core.endpoint` or the equivalent
`--core-grpc` CLI argument is required. Edge must fail fast if no core endpoint
is configured. There is no production fallback from edge to local agent
execution.

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
pub trait CoreAgentClient: Send + Sync {
    async fn run_chat_streamed(&self, request: AgentRunRequest)
        -> anyhow::Result<AgentRunStream>;

    async fn cancel_run(&self, run_id: &str, reason: &str)
        -> anyhow::Result<AgentCancelResult>;
}
```

Gateway handlers depend on `Arc<dyn CoreAgentClient>` instead of directly
creating `zeroclaw_runtime::agent::Agent`.

Initial implementations:

- `GrpcCoreAgentClient`: the only production client used by
  `zeroclaw-edge.exe`.
- `MockCoreAgentClient`: tests only; it must not call local agent runtime.

There is intentionally no local-agent implementation for edge. Compatibility for
old `zeroclaw.exe gateway start` can be handled by keeping the old binary path
outside `zeroclaw-edge.exe`, but the split edge binary itself must remain
gRPC-only.

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
- WebSocket chat calls `CoreAgentClient` instead of creating `Agent` directly.
- REST/webhook/channel chat calls `CoreAgentClient`.
- `GrpcCoreAgentClient` maps `RunEvent` to the gateway stream event model.
- WebSocket abort calls `CancelRun`.
- Core-unavailable failures are converted to stable gateway errors.
- Boundary tests or scans fail if edge modules reference
  `Agent::from_config`, `Agent::from_config_with_session_cwd`,
  `turn_streamed`, or `process_message`.

## Rollout

1. Add the `CoreAgentClient` abstraction.
2. Add the gRPC core client.
3. Add `zeroclaw-core.exe`.
4. Add `zeroclaw-edge.exe`.
5. Migrate WebSocket chat to `CoreAgentClient`.
6. Migrate REST/webhook/channel chat to `CoreAgentClient`.
7. Add boundary tests proving edge does not call local agent runtime.
8. Keep old `zeroclaw.exe` behavior outside the split edge binary until the
   split path is verified.

## Residual Risks

- Gateway currently has several direct local agent call sites. Missing one would
  violate the edge/core boundary and leave part of edge executing locally.
- The first split keeps core involved in agent/session memory behavior. A later
  phase can move more business session ownership to edge if needed.
- Core process-local run registry still requires sticky routing or a shared run
  store for multi-core deployments.
