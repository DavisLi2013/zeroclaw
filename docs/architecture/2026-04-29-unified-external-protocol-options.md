# Proposal: Unified External Protocol Options for ZeroClaw

## Status

This is a proposal document for choosing a unified external protocol. It does
not describe a current stable runtime contract and does not require code changes
by itself.

## Goal

Define a single public contract that external applications can use to invoke
ZeroClaw. The protocol should let clients submit work, stream results, manage
sessions, observe tool activity, cancel running turns, and integrate with the
existing gateway security model.

## Additional Deployment Scenario (Server-to-Server)

This document originally focuses on "external applications" as direct clients of
the gateway. There is also an important server-to-server (S2S) deployment pattern
to optimize for:

```text
Millions of end users
  -> Server 1 (edge / product backend)
    -> (this unified protocol)
      -> Server 2 (internal agent execution tier, runs zeroclaw.exe)
```

In this S2S pattern, Server 1 is an infrastructure-controlled client, not a
browser. The key constraints and goals shift slightly:

- The most important link is the **internal Server1 -> Server2 hop**.
- Operational requirements usually include:
  - **high QPS**, tight p99 latency targets, and connection pooling
  - **mTLS**, service identity, and per-service authorization (not only end-user tokens)
  - **backpressure** and load shedding when Server 2 is saturated
  - clear **idempotency** and retry semantics across network failures
  - multi-instance routing without sticky sessions becoming a bottleneck
  - better typed contracts / generated clients (often preferred for S2S)

Because the "public" client population (end users) and the "internal" S2S client
(Server 1) have very different requirements, a single transport may be a poor
fit for both. This motivates a layered design where the *semantic contract* stays
unified, while the *transport bindings* differ for public vs internal hops.

The target users are:

- Web applications and backend services that call ZeroClaw as an agent runtime.
- Interactive clients such as IDE extensions, dashboards, mobile apps, and local
  control panels.
- Future SDKs that need a stable request, event, error, and authentication model.

## Step-by-step Context Review

### 1. Current entry surfaces

ZeroClaw already has a gateway crate: `crates/zeroclaw-gateway`.

The current gateway is Axum-based and already exposes several relevant surfaces:

- HTTP REST-style API under `/api/*`.
- Gateway health and metrics routes.
- Pairing and device management routes.
- WebSocket chat at `/ws/chat`.
- SSE events at `/api/events`.
- Webhook ingress for external event sources.
- TLS, request body limits, request timeouts, idempotency support, and rate
  limiting.

This means the right design should extend the existing gateway contract. A
parallel external server would duplicate authentication, session persistence,
rate limiting, observability, and security boundaries.

### 2. Current runtime boundary

The internal architecture is trait-driven:

- `Provider` handles model backends.
- `Channel` handles messaging ingress and egress.
- `Tool` handles callable capabilities.
- `Memory` handles durable context.
- `Observer` handles events and telemetry.
- `RuntimeAdapter` handles execution environment concerns.

The external protocol should not expose those Rust traits directly. It should
expose a transport-neutral agent invocation contract that the gateway maps onto
the existing runtime.

### 3. Non-negotiable protocol properties

Any option should keep these invariants:

- Versioned public API: start with `/v1` or protocol name `zeroclaw.v1`.
- Explicit session identity: every turn has a `session_id`.
- Explicit request identity: every client request has a `request_id` or
  `idempotency_key`.
- Bearer-token authentication based on the current pairing model.
- Structured errors with stable `code`, `message`, `retryable`, and `details`.
- Streaming events for text deltas, tool calls, tool results, approvals,
  completion, cancellation, and errors.
- Transport-independent event envelope so HTTP/SSE, WebSocket, and gRPC can
  share the same semantic contract.
- No bypass around tool security gates, approval flow, workspace boundaries, or
  audit logging.

## Shared Protocol Model

All three options should share this logical model even if the wire format
differs.

### Request envelope

```json
{
  "protocol": "zeroclaw.v1",
  "request_id": "req_01J...",
  "session_id": "sess_01J...",
  "actor": {
    "id": "external-app:user-123",
    "display_name": "User 123"
  },
  "input": {
    "type": "message",
    "content": "Summarize this repository and propose next steps."
  },
  "options": {
    "stream": true,
    "model": null,
    "allowed_tools": null,
    "autonomy": null
  },
  "metadata": {
    "client": "example-web-app",
    "trace_id": "trace_01J..."
  }
}
```

### Event envelope

```json
{
  "protocol": "zeroclaw.v1",
  "request_id": "req_01J...",
  "session_id": "sess_01J...",
  "sequence": 7,
  "event": {
    "type": "message.delta",
    "content": "The repository is organized around..."
  },
  "timestamp": "2026-04-29T00:00:00Z"
}
```

### Required event types

| Event type | Meaning |
|---|---|
| `run.accepted` | Gateway accepted and queued the request |
| `run.started` | Runtime started processing the turn |
| `message.delta` | Partial assistant text |
| `tool.call` | Runtime is about to invoke a tool |
| `tool.result` | Tool invocation completed |
| `approval.required` | Human approval is required before continuing |
| `approval.resolved` | Approval was granted or denied |
| `memory.write` | Runtime wrote durable memory or session history |
| `run.completed` | Turn completed successfully |
| `run.cancelled` | Turn was cancelled |
| `run.failed` | Turn failed with a structured error |

### Error shape

```json
{
  "error": {
    "code": "AUTH_REQUIRED",
    "message": "Bearer token is required.",
    "retryable": false,
    "details": {}
  }
}
```

## Option A: HTTP/HTTPS API as the Canonical Protocol

### HTTP/SSE Shape

Make HTTP/HTTPS the primary public protocol, documented as OpenAPI. Use JSON for
requests and responses. Use SSE for streaming turn events.

Proposed route family:

```text
POST   /v1/runs
GET    /v1/runs/{run_id}
POST   /v1/runs/{run_id}/cancel
GET    /v1/runs/{run_id}/events
GET    /v1/sessions
POST   /v1/sessions
GET    /v1/sessions/{session_id}
GET    /v1/sessions/{session_id}/messages
DELETE /v1/sessions/{session_id}
GET    /v1/tools
GET    /v1/health
```

Synchronous clients can call `POST /v1/runs` with `stream=false` and receive the
final result in one response. Streaming clients can call `POST /v1/runs` with
`stream=true` and then consume `GET /v1/runs/{run_id}/events` through SSE.

### HTTP/SSE Fit With Current ZeroClaw

This is the closest to the existing gateway:

- The gateway already has HTTP routes.
- The gateway already has `/api/events` SSE.
- The gateway already has session and memory routes.
- The gateway already uses bearer-token auth for `/api/*`.
- OpenAPI can document the contract without forcing clients into a specific SDK.

### HTTP/SSE Advantages

- Best default for external applications.
- Easy to call from browsers, backend services, scripts, and low-code tools.
- Easy to document, test, cache, log, proxy, rate-limit, and secure.
- Can reuse the existing gateway and current pairing model.
- Smallest conceptual migration from the current codebase.

### HTTP/SSE Trade-offs

- Bidirectional interaction is less natural than WebSocket.
- Client must combine REST calls and SSE subscriptions for full streaming.
- Fine-grained interactive control, such as mid-turn approval UX, needs careful
  event and callback design.

### HTTP/SSE Recommended Use

Choose this if the first priority is broad compatibility and a stable external
developer API.

## Option B: WebSocket as the Canonical Protocol

### WebSocket Shape

Make WebSocket the primary public protocol. Clients connect once and exchange
typed JSON frames over `wss://host/v1/agent`.

Proposed frame family:

```text
client -> server: connect
client -> server: run.start
client -> server: run.cancel
client -> server: approval.resolve
client -> server: session.rename

server -> client: session.started
server -> client: run.accepted
server -> client: run.started
server -> client: message.delta
server -> client: tool.call
server -> client: tool.result
server -> client: approval.required
server -> client: run.completed
server -> client: run.failed
```

The existing `/ws/chat` route already uses a `zeroclaw.v1` subprotocol and sends
typed JSON messages such as session start, chunks, tool calls, tool results, and
done events. This option formalizes and expands that behavior into the canonical
external contract.

### WebSocket Fit With Current ZeroClaw

This fits the existing interactive gateway path:

- `/ws/chat` already exists.
- WebSocket token extraction already supports bearer auth, subprotocol bearer
  tokens, and query-token fallback.
- Session persistence already exists for gateway WebSocket chat.
- Streaming assistant output and tool events already match the WebSocket mental
  model.

### WebSocket Advantages

- Best real-time experience for IDEs, dashboards, mobile apps, and control UIs.
- Natural bidirectional channel for approvals, cancellation, interrupts, and
  live tool progress.
- Lower overhead for long interactive sessions.
- Can keep client state synchronized without polling.

### WebSocket Trade-offs

- Harder to integrate from basic server-to-server workflows.
- Harder to document and test than OpenAPI.
- Load balancers, reverse proxies, reconnects, and sticky sessions need more
  operational care.
- Long-lived connections complicate multi-instance deployment unless session
  ownership is explicit.

### WebSocket Recommended Use

Choose this if ZeroClaw is primarily consumed by interactive clients that need
low-latency streaming, live cancellation, and approval control.

## Option C: gRPC as the Canonical Protocol

### gRPC Shape

Make gRPC the primary public protocol with protobuf-defined services. Add an
HTTP/JSON bridge later for browser and webhook-style clients.

Proposed service shape:

```proto
service ZeroClawAgent {
  rpc CreateRun(CreateRunRequest) returns (CreateRunResponse);
  rpc StreamRun(StreamRunRequest) returns (stream RunEvent);
  rpc CancelRun(CancelRunRequest) returns (CancelRunResponse);
  rpc ListSessions(ListSessionsRequest) returns (ListSessionsResponse);
  rpc GetSession(GetSessionRequest) returns (Session);
  rpc ListTools(ListToolsRequest) returns (ListToolsResponse);
}
```

The event stream should map one-to-one to the shared `RunEvent` model described
above.

### gRPC Fit With Current ZeroClaw

This is the least aligned with the current gateway implementation:

- The gateway currently uses Axum HTTP/WebSocket, not tonic/prost.
- Existing clients and dashboard routes are already HTTP-oriented.
- gRPC would be an additional transport stack and likely a new crate or gateway
  module.

### gRPC Advantages

- Strong typed contract for generated SDKs.
- Good fit for internal service mesh and infrastructure-controlled deployments.
- First-class streaming RPC support.
- Clear backward-compatibility discipline through protobuf field evolution.

### gRPC Trade-offs

- Highest implementation and dependency cost.
- Browser clients need gRPC-Web or a JSON bridge.
- Operational model is less familiar for many product integrations.
- Adds another public protocol stack that must share auth, rate limits,
  observability, and security policy with the existing gateway.

### gRPC Recommended Use

Choose this only if the primary consumers are typed internal services, enterprise
SDKs, or infrastructure platforms that already standardize on gRPC.

## Option D: Layered Protocol (Public HTTP/SSE or WebSocket, Internal gRPC + mTLS)

### Shape

Keep one unified semantic model (the request envelope, event envelope, and error
shape) but define **two first-class transport bindings**:

- **Public edge binding (client-facing)**
  - HTTP/HTTPS JSON + SSE (and optionally a WebSocket binding for interactive UIs)
  - optimized for broad compatibility and simple integration
- **Internal S2S binding (Server1 -> Server2)**
  - gRPC over HTTP/2 with **mTLS**
  - optimized for high-throughput, low overhead, connection reuse, and typed
    generated clients

In this model, Server 1 never speaks directly to the "public" binding. Instead,
Server 1 uses the internal gRPC binding when calling Server 2.

### Internal gRPC binding details (server-streaming RunEvent)

The internal binding should preserve the same semantic model as the HTTP/SSE and
WebSocket bindings: one request envelope, one event envelope, one error model.
gRPC provides a natural representation of "SSE-like" streaming via
server-streaming RPCs.

#### Run event streaming

Use a server-streaming RPC that emits the shared `RunEvent` messages in-order:

```proto
service ZeroClawAgent {
  rpc CreateRun(CreateRunRequest) returns (CreateRunResponse);
  rpc StreamRun(StreamRunRequest) returns (stream RunEvent);
  rpc CancelRun(CancelRunRequest) returns (CancelRunResponse);
}
```

Mapping guidance:

- `StreamRun` is the gRPC equivalent of `GET /v1/runs/{run_id}/events` (SSE).
- Each `RunEvent` maps 1:1 to the shared event envelope (`sequence`, `event.type`,
  payload, timestamp).
- Message ordering is defined by `sequence` and must be monotonic for a given
  `run_id`.

Completion behavior:

- On success, the server should emit `run.completed` and then gracefully end the
  stream.
- On failure, the server should emit `run.failed` (with the structured error
  payload) and then end the stream.

#### Cancellation semantics

Cancellation needs to work in both directions:

- **Client-initiated cancel**
  - Preferred: client calls `CancelRun(run_id, ...)`.
  - Additionally: client may cancel the `StreamRun` RPC context; the server may
    treat that as a signal to stop streaming and (optionally) cancel the run
    depending on policy.

- **Server-initiated termination**
  - If the run is cancelled internally, the stream should emit `run.cancelled`
    and then end.

Important distinction:

- Cancelling the stream does not necessarily mean cancelling the run unless
  explicitly defined. For S2S, the safest default is:
  - `CancelRun` cancels the run.
  - Dropping the stream only stops event delivery.

#### Timeouts / deadlines

Define timeouts as gRPC deadlines:

- The caller (Server 1) sets a deadline on `CreateRun` and `StreamRun`.
- The callee (Server 2) enforces the deadline and should emit `run.failed` with a
  structured timeout error when possible.

Operational guidance:

- Use a short deadline for `CreateRun` (admission control + queuing decision).
- Use a longer deadline for `StreamRun` (covers the full run) or renew by
  reconnecting if the run is expected to exceed typical RPC deadlines.

#### Retries and idempotency

Because gRPC calls are often retried by clients and infrastructure, the internal
binding must define explicit retry and deduplication rules.

- **Idempotency key**
  - Require `request_id` (or `idempotency_key`) on `CreateRun`.
  - Server 2 deduplicates by `(caller_identity, idempotency_key)` within a
    retention window, returning the existing `run_id` for duplicates.

- **What is safe to retry**
  - Safe: `CreateRun` *only if* an idempotency key is provided and enforced.
  - Safe: `StreamRun` (reconnect) should be allowed.
  - Safe: `CancelRun` should be idempotent.
  - Unsafe: any "mutating" RPC without idempotency.

- **Retry on which failures**
  - Prefer retry on transport-level failures where no application acceptance has
    been confirmed.
  - Avoid blind retries on `RESOURCE_EXHAUSTED` or explicit overload signals; use
    backoff and obey server-provided limits.

#### Reconnect, resume, and event gaps

To make `StreamRun` robust under network resets, define a resume mechanism:

- `StreamRunRequest` includes `run_id` and optional `after_sequence`.
- Server starts streaming from the next available event after `after_sequence`.
- If the requested sequence is no longer available, the server should return a
  structured error indicating the client must fall back to fetching the final
  run state or restarting the run.

### Fit with current ZeroClaw

- Aligns with the current gateway investment (HTTP/WebSocket/SSE) for public
  integrations.
- Adds a purpose-built internal contract for S2S without forcing browsers and
  scripts to adopt gRPC.
- Avoids the ambiguity of calling something "the" canonical protocol when the
  deployment actually needs two different operational profiles.

### Advantages

- Best overall fit for the described "Server1 -> Server2" architecture.
- Enables **service-mesh grade security** (mTLS, SPIFFE-like identities) on the
  internal hop.
- Better backpressure behavior on the internal hop via HTTP/2 flow control and
  server-side streaming semantics.
- Clear separation of concerns:
  - public edge: compatibility and developer experience
  - internal hop: performance, reliability, and infra policy

### Trade-offs

- Not a single transport; requires maintaining parity across two bindings.
- Requires strong discipline to keep event semantics and error mapping identical.
- Increases gateway complexity if both bindings are implemented in the same
  binary (auth, rate limits, audit, observability must stay consistent).

### Recommended when

Choose this when ZeroClaw is deployed as an internal execution tier and the main
caller is another backend service (Server 1).

## Option E: Asynchronous Job Protocol (Queue + Event Stream Callback)

### Shape

For long-running, bursty, or expensive agent runs, a synchronous request/stream
protocol may not be the best default. In that case, define an **asynchronous job
submission contract**:

- Server 1 submits a job:
  - via HTTP `POST /v1/jobs` or gRPC `CreateJob`
  - receives `job_id`
- Server 2 processes jobs from a queue (or an internal dispatcher) with explicit
  concurrency limits.
- Results/events are delivered via one of:
  - Server 1 polls `GET /v1/jobs/{job_id}` + `GET /v1/jobs/{job_id}/events`
  - Server 2 pushes to a Server 1 webhook/callback endpoint
  - Server 1 consumes a dedicated result topic/stream

This option is compatible with Option A/B/C/D; it is primarily a **work
admission and reliability pattern**, not just a transport choice.

### Advantages

- Strongest **load isolation** and **backpressure** story. Queue depth becomes a
  visible control surface.
- Natural fit for retries, dead-letter queues, and delayed processing.
- Better tail latency stability under traffic spikes (Server 2 is protected).

### Trade-offs

- Harder product experience for strictly interactive use cases.
- Requires a queue/streaming system operationally.
- End-to-end tracing is more complex across async boundaries.

### Recommended when

Choose this when agent runs are expensive, can take seconds to minutes, and the
top priority is stability under bursty load.

## Comparison Matrix

| Criterion | Option A: HTTP/SSE | Option B: WebSocket | Option C: gRPC | Option D: Layered | Option E: Async jobs |
|---|---:|---:|---:|---:|---:|
| Fit with current gateway | High | High | Low | High | Medium |
| Browser compatibility | High | High | Medium with gRPC-Web | High | High (for submit + status) |
| Backend service compatibility (S2S) | High | Medium | High | High | High |
| Interactive streaming | Medium | High | High | High | Low to medium |
| OpenAPI/documentation simplicity | High | Medium | Low | Medium | Medium |
| Typed SDK generation | Medium | Low | High | High (internal) | High (if gRPC) |
| Backpressure / load isolation | Medium | Medium | High | High | Very high |
| Operational simplicity | High | Medium | Medium | Medium | Low to medium |
| Implementation risk | Low to medium | Medium | High | Medium to high | Medium to high |
| Best fit for Server1 -> Server2 | Medium | Medium | High | **Highest** | High (for long runs) |

## Recommendation

If ZeroClaw is consumed directly by external applications (including browsers and
scripts), Option A remains the best first public contract.

For the server-to-server deployment pattern described above (Server 1 calls
Server 2 running `zeroclaw.exe`), a layered approach is typically a better fit.

Use Option D as the recommended architecture for S2S deployments:

```text
Public binding (north-south): HTTP/HTTPS JSON API + SSE event stream
Internal binding (east-west): gRPC over HTTP/2 + mTLS
One shared semantic model (events, errors, idempotency)
```

Keep the shared event model transport-neutral so Option B can remain an official
WebSocket binding without redefining semantics:

```text
HTTP/SSE binding: /v1/runs + /v1/runs/{id}/events
WebSocket binding: wss://host/v1/agent
gRPC binding (internal): zeroclaw.v1.Agent (CreateRun / StreamRun / CancelRun)
Same request envelope, same event envelope, same error model
```

If runs are long-running or traffic is highly bursty, adopt Option E as a second
dimension (async job admission) on top of Option A/D.

## Suggested Evolution Path

### Phase 1: Specify the shared semantic model

Deliverables:

- Transport-neutral schema for request envelopes, event envelopes, and errors.
- Authentication rules based on current pairing tokens.
- Idempotency rules for `POST /v1/runs`.
- Cancellation and timeout behavior.

### Phase 2: Specify the public HTTP/SSE binding

Deliverables:

- OpenAPI document for `/v1/*`.
- Map `/v1/runs` to the same runtime path that `/ws/chat` uses.
- Reuse existing session persistence.
- Reuse existing SSE broadcast or add per-run event streams.
- Reuse gateway rate limiting, body limits, and timeout policy.
- Keep `/api/*` dashboard routes separate from `/v1/*` public protocol routes.

### Phase 3: Add official WebSocket binding

Deliverables:

- Formalize `wss://host/v1/agent`.
- Reuse the same event envelope as HTTP/SSE.
- Define reconnect and resume behavior.
- Define approval and cancellation frames.

### Phase 4: Add internal S2S binding (recommended for Server1 -> Server2)

Deliverables:

- Protobuf schema generated from the same logical model.
- mTLS and service identity rules (internal auth distinct from pairing tokens).
- Rate limits and backpressure policy specific to internal callers.
- Observability parity with the HTTP gateway.

### Phase 5: Decide whether async job admission is needed

Deliverables:

- Job submission contract (`job_id`, status model, events model).
- Callback vs polling vs result-stream decision.
- Retry and deduplication rules for at-least-once delivery.

## Decision Guidance

Choose one of these:

1. **Recommended default:** HTTP/HTTPS + SSE first.
   This gives ZeroClaw the broadest, easiest external API while staying close to
   the current gateway.

2. **Interactive-client priority:** WebSocket first.
   This is better if the first external consumers are IDEs, dashboards, or apps
   that require continuous bidirectional control.

3. **Enterprise typed-service priority:** gRPC first.
   This is better only when generated typed SDKs and service-mesh conventions
   are more important than simple browser and script access.

4. **Server-to-server execution tier (recommended for Server1 -> Server2):**
   Layered public + internal bindings.
   Use HTTP/SSE (and optionally WebSocket) for north-south compatibility, and
   gRPC + mTLS for east-west S2S performance and infra-grade security.

5. **Long-running / bursty workloads:** add async job admission.
   Keep the same semantic model, but introduce queue-backed job submission and a
   results delivery strategy (polling, callback, or result stream) to maximize
   stability under traffic spikes.

## Non-goals For The First Protocol PR

- Do not replace the existing dashboard API.
- Do not remove `/ws/chat`.
- Do not weaken pairing, bearer-token auth, tool security, or approval gates.
- Do not expose raw Rust trait internals over the public protocol.
- Do not add speculative configuration keys without concrete protocol behavior.
- Do not introduce gRPC dependencies unless Option C is explicitly selected.

## Open Choice For Maintainers

The main decision is whether ZeroClaw wants its first public external contract
to optimize for broad integration or for rich interactive control:

- Broad integration: choose Option A.
- Rich interactive control: choose Option B.
- Typed service infrastructure: choose Option C.

My recommendation is Option A, with Option B designed as a compatible second
transport binding over the same event model.
