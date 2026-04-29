# gRPC S2S MVP Design

## Goal

Add a first internal gRPC binding for the Server1 -> Server2 deployment model:

```text
Millions of C-side users -> Server 1 -> gRPC -> Server 2 running zeroclaw.exe
```

The MVP exposes a typed, server-to-server agent invocation API while reusing the
existing ZeroClaw runtime, gateway session handling, tool security, cancellation,
and streaming agent events.

## Scope

In scope:

- A `zeroclaw.v1.AgentService` protobuf service.
- `CreateRun`, `StreamRun`, `CancelRun`, and `GetRun` RPCs.
- A run request envelope with `request_id`, `session_id`, `actor`, `input`,
  `options`, and `metadata`.
- A run event envelope with monotonic `sequence`, `run_id`, `request_id`,
  `session_id`, `occurred_at`, and typed event payloads.
- Event types for `run.accepted`, `run.started`, `message.delta`,
  `thinking.delta`, `tool.call`, `tool.result`, `run.completed`,
  `run.cancelled`, and `run.failed`.
- Bearer-token authentication using the existing gateway pairing token model.
- Cancellation by `run_id`, mapped to the existing cancellation-token path.
- Per-session serialization using the existing gateway `SessionActorQueue`.

Out of scope for this MVP:

- Multi-instance run routing.
- Persistent replayable event logs.
- gRPC-Web.
- Full service identity authorization or SPIFFE policy.
- Queue-backed async job admission.
- Public HTTP/SSE parity work.

## Chosen Approach

Implement gRPC inside `crates/zeroclaw-gateway` as a focused module rather than
creating a new workspace crate or mixing tonic handlers into the Axum router.

Reasons:

- `zeroclaw-gateway` already owns auth, sessions, cancellation, rate limiting,
  dashboard APIs, and `Agent::turn_streamed` integration.
- The MVP can share runtime setup and security policy without exposing internal
  Rust traits directly.
- A separate module keeps tonic service code isolated from the existing Axum
  HTTP/WebSocket gateway.

The CLI will expose a separate `zeroclaw grpc` command for the MVP. Running
gRPC separately from `zeroclaw gateway start` avoids accidental public exposure
and keeps operational rollout explicit.

## Protobuf Contract

Package: `zeroclaw.v1`

Service:

```proto
service AgentService {
  rpc CreateRun(CreateRunRequest) returns (CreateRunResponse);
  rpc StreamRun(StreamRunRequest) returns (stream RunEvent);
  rpc CancelRun(CancelRunRequest) returns (CancelRunResponse);
  rpc GetRun(GetRunRequest) returns (GetRunResponse);
}
```

Core request fields:

- `protocol`: string, default semantic value `zeroclaw.v1`.
- `request_id`: caller-provided idempotency key for admission deduplication.
- `session_id`: caller-provided conversation/session id.
- `actor`: caller/user identity from Server 1.
- `input`: currently `MESSAGE` plus UTF-8 text content.
- `options`: stream flag, optional model override, optional allowed tool names,
  optional timeout in milliseconds.
- `metadata`: arbitrary string map for `client`, `trace_id`, tenant, shard, or
  product context.

Run identity:

- `run_id`: server-generated UUID.
- MVP keeps run state in memory.
- `CreateRun` returns an existing `run_id` when the same authenticated caller
  repeats a `request_id` within the process lifetime.

Event payloads:

- `RunAccepted`: queue/admission metadata.
- `RunStarted`: provider and model labels.
- `MessageDelta`: assistant text delta.
- `ThinkingDelta`: model reasoning/thinking delta where available.
- `ToolCall`: tool call id, name, JSON arguments.
- `ToolResult`: tool call id, name, string output.
- `RunCompleted`: final assistant text.
- `RunCancelled`: cancellation reason.
- `RunFailed`: structured error.

Structured error:

- `code`: stable machine-readable code.
- `message`: sanitized human-readable message.
- `retryable`: whether Server 1 may retry.
- `details`: string map for safe operational details.

## Runtime Flow

1. Server 1 calls `CreateRun`.
2. The gRPC service authenticates the `authorization: Bearer <token>` metadata
   against the existing `PairingGuard`.
3. The service validates `request_id`, `session_id`, and non-empty message
   content.
4. The service creates an in-memory run record and records `run.accepted`.
5. A background task acquires the gateway `SessionActorQueue` guard for
   `gw_<session_id>`.
6. The task builds `Agent::from_config`, hydrates session history from the
   existing session backend, appends the user message, and calls
   `Agent::turn_streamed`.
7. Each `TurnEvent` is mapped to a protobuf `RunEvent` and sent through a
   per-run broadcast channel.
8. On success the service persists the final assistant response and emits
   `run.completed`.
9. On cancellation it emits `run.cancelled`.
10. On failure it emits `run.failed` with sanitized error details.

`StreamRun` subscribes to the run broadcast channel and streams events until a
terminal event is emitted. Each run retains the last 1,024 emitted events in
memory. When `after_sequence` is set, the service replays retained events with a
larger sequence before subscribing to live events. If `after_sequence` is older
than the retained buffer, the RPC returns `OutOfRange` and the caller must use
`GetRun` for terminal state.

`CancelRun` cancels the stored cancellation token. The operation is idempotent:
already-terminal or unknown runs return a non-panicking response.

## Authentication

The MVP uses gRPC metadata:

```text
authorization: Bearer <paired-token>
```

If `gateway.require_pairing = false`, requests are accepted without a token,
matching existing gateway behavior. If pairing is required and the token is
missing or invalid, the RPC returns `Unauthenticated`.

mTLS is handled outside the MVP by the deployment layer. The first gRPC server
does not add service-identity policy beyond the bearer-token check.

## Operational Defaults

- gRPC host defaults to `gateway.host`.
- gRPC port defaults to `gateway.port + 1`.
- The command accepts `--host` and `--port` overrides.
- Binding to a public address follows the existing gateway public-bind warning
  policy.
- Request body protection relies on protobuf message size limits added in tonic
  server configuration.

## File Layout

Planned files:

- `crates/zeroclaw-gateway/proto/zeroclaw/v1/agent.proto`:
  protobuf contract.
- `crates/zeroclaw-gateway/build.rs`:
  tonic/prost code generation.
- `crates/zeroclaw-gateway/src/grpc.rs`:
  gRPC service implementation and run registry.
- `crates/zeroclaw-gateway/src/lib.rs`:
  exports `grpc` module when enabled.
- `crates/zeroclaw-gateway/Cargo.toml`:
  tonic/prost dependencies and build dependency.
- `Cargo.toml`:
  root feature forwarding if the gRPC feature is gated.
- `src/lib.rs`:
  CLI command enum additions.
- `src/main.rs`:
  `zeroclaw grpc` command wiring.
- `tests/component/grpc.rs` or module-level tests:
  protocol validation, event mapping, auth, idempotency, and cancel behavior.

## Testing Strategy

Unit tests:

- Validate request conversion rejects missing `request_id`, missing
  `session_id`, and empty input content.
- Validate `TurnEvent` to `RunEvent` mapping.
- Validate idempotent `CreateRun` returns the same `run_id` for duplicate
  authenticated `request_id`.
- Validate auth accepts/denies based on `PairingGuard`.
- Validate `CancelRun` is idempotent for unknown and terminal runs.

Build checks:

- `cargo fmt --all -- --check`
- `cargo test -p zeroclaw-gateway grpc`
- `cargo clippy -p zeroclaw-gateway --all-targets -- -D warnings`
- If the gateway-scoped checks pass, run the repository-level checks from
  `AGENTS.md`: `cargo clippy --all-targets -- -D warnings` and `cargo test`.

## Risks

- The MVP run registry is process-local. Multi-instance Server 2 deployments
  need either sticky routing, shared run state, or an external queue/event store.
- gRPC dependency additions increase compile time and binary size.
- Reusing gateway session persistence means gRPC and WebSocket sessions share
  the `gw_<session_id>` namespace by design.
- Long-running streams need deployment-level deadlines and keepalive tuning.

## Rollback

The feature is isolated to a new command and gateway module. Rollback is to stop
using `zeroclaw grpc` and remove the gRPC feature/module changes without
affecting existing HTTP, WebSocket, SSE, or webhook gateway routes.
