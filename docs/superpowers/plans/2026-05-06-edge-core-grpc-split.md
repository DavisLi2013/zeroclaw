# Edge/Core gRPC Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `zeroclaw-edge.exe` and `zeroclaw-core.exe` so WebUI, REST chat, WebSocket, webhook, and channel traffic enters edge and all agent execution is forwarded to core over gRPC.

**Architecture:** Add a gateway-local `AgentBackend` boundary, keep a local backend for compatibility, add a gRPC backend for edge, and make the two new binaries select the correct mode. Core starts only `zeroclaw.v1.AgentService`; edge starts the gateway surfaces and talks to core through `GrpcAgentBackend`.

**Tech Stack:** Rust, Tokio, Axum, tonic gRPC, existing `zeroclaw-gateway`, existing `zeroclaw-runtime`, existing `zeroclaw-config`.

---

## File Structure

- Create `crates/zeroclaw-gateway/src/agent_backend.rs`: shared request/event/result types and `AgentBackend` trait.
- Create `crates/zeroclaw-gateway/src/agent_backend_local.rs`: local compatibility backend around current `Agent::turn_streamed` behavior.
- Create `crates/zeroclaw-gateway/src/agent_backend_grpc.rs`: edge-side gRPC backend around `grpc::pb::AgentService`.
- Modify `crates/zeroclaw-gateway/src/lib.rs`: add backend to `AppState`, initialize backend, route REST/webhook/channel chat through it.
- Modify `crates/zeroclaw-gateway/src/ws.rs`: route WebSocket chat through `AgentBackend`.
- Modify `crates/zeroclaw-gateway/src/grpc.rs`: expose any protobuf/client helpers needed by `agent_backend_grpc`.
- Modify `crates/zeroclaw-config/src/schema.rs`: add gateway agent backend config.
- Create `src/bin/zeroclaw-core.rs`: starts only gRPC core.
- Create `src/bin/zeroclaw-edge.rs`: starts gateway edge with gRPC backend.
- Modify `Cargo.toml`: register the two binaries if needed by current package layout.
- Modify `docs/superpowers/specs/2026-05-06-edge-core-grpc-split-design.md`: keep architecture notes synchronized when implementation choices change.

## Task 1: Add Gateway Agent Backend Types

**Files:**
- Create: `crates/zeroclaw-gateway/src/agent_backend.rs`
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: `crates/zeroclaw-gateway/src/agent_backend.rs`

- [ ] **Step 1: Write the failing test**

Add this test in `agent_backend.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_delta_accumulates_final_text() {
        let mut collected = AgentRunCollected::default();
        collected.apply(&AgentRunEvent::MessageDelta {
            delta: "hello".to_string(),
        });
        collected.apply(&AgentRunEvent::MessageDelta {
            delta: " world".to_string(),
        });

        assert_eq!(collected.final_text, "hello world");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend::tests::run_event_delta_accumulates_final_text
```

Expected: fail because `agent_backend` module and types do not exist.

- [ ] **Step 3: Add minimal backend types**

Create `crates/zeroclaw-gateway/src/agent_backend.rs`:

```rust
use std::pin::Pin;

use futures_util::Stream;

pub type AgentRunStream =
    Pin<Box<dyn Stream<Item = anyhow::Result<AgentRunEvent>> + Send + 'static>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRunRequest {
    pub request_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub input: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentRunEvent {
    RunStarted { provider: String, model: String },
    MessageDelta { delta: String },
    ThinkingDelta { delta: String },
    ToolCall { id: String, name: String, args: serde_json::Value },
    ToolResult { id: String, name: String, output: String },
    Completed { final_text: String },
    Cancelled { reason: String },
    Failed { code: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCancelResult {
    pub accepted: bool,
}

#[derive(Default)]
pub struct AgentRunCollected {
    pub final_text: String,
}

impl AgentRunCollected {
    pub fn apply(&mut self, event: &AgentRunEvent) {
        if let AgentRunEvent::MessageDelta { delta } = event {
            self.final_text.push_str(delta);
        }
        if let AgentRunEvent::Completed { final_text } = event {
            self.final_text = final_text.clone();
        }
    }
}

#[async_trait::async_trait]
pub trait AgentBackend: Send + Sync {
    async fn run_chat_streamed(&self, request: AgentRunRequest)
        -> anyhow::Result<AgentRunStream>;

    async fn cancel_run(&self, run_id: &str, reason: &str)
        -> anyhow::Result<AgentCancelResult>;
}
```

Modify `crates/zeroclaw-gateway/src/lib.rs`:

```rust
pub mod agent_backend;
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend::tests::run_event_delta_accumulates_final_text
```

Expected: pass.

## Task 2: Add LocalAgentBackend for Compatibility

**Files:**
- Create: `crates/zeroclaw-gateway/src/agent_backend_local.rs`
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: `crates/zeroclaw-gateway/src/agent_backend_local.rs`

- [ ] **Step 1: Write the failing test**

Add a test that uses a tiny stream adapter without constructing a real provider:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_backend::{AgentRunEvent, AgentRunRequest};

    #[tokio::test]
    async fn local_backend_rejects_empty_input_before_agent_init() {
        let backend = LocalAgentBackend::new(zeroclaw_config::schema::Config::default());
        let err = backend
            .run_chat_streamed(AgentRunRequest {
                request_id: "req-a".to_string(),
                session_id: "session-a".to_string(),
                actor_id: "user-a".to_string(),
                input: " ".to_string(),
            })
            .await
            .unwrap_err();

        assert!(err.to_string().contains("input is required"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend_local::tests::local_backend_rejects_empty_input_before_agent_init
```

Expected: fail because `LocalAgentBackend` does not exist.

- [ ] **Step 3: Implement minimal local backend shell**

Create `agent_backend_local.rs` with validation and the local runtime call shape:

```rust
use futures_util::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use zeroclaw_config::schema::Config;
use zeroclaw_runtime::agent::{Agent, TurnEvent};

use crate::agent_backend::{
    AgentBackend, AgentCancelResult, AgentRunEvent, AgentRunRequest, AgentRunStream,
};

pub struct LocalAgentBackend {
    config: Config,
}

impl LocalAgentBackend {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl AgentBackend for LocalAgentBackend {
    async fn run_chat_streamed(&self, request: AgentRunRequest) -> anyhow::Result<AgentRunStream> {
        if request.input.trim().is_empty() {
            anyhow::bail!("input is required");
        }

        let mut agent = Agent::from_config(&self.config).await?;
        agent.set_memory_session_id(Some(request.session_id.clone()));

        let (turn_tx, turn_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);
        let input = request.input.clone();

        tokio::spawn(async move {
            let _ = agent.turn_streamed(&input, turn_tx, None).await;
        });

        let stream = ReceiverStream::new(turn_rx).map(|event| match event {
            TurnEvent::Chunk { delta } => Ok(AgentRunEvent::MessageDelta { delta }),
            TurnEvent::Thinking { delta } => Ok(AgentRunEvent::ThinkingDelta { delta }),
            TurnEvent::ToolCall { id, name, args } => Ok(AgentRunEvent::ToolCall {
                id,
                name,
                args,
            }),
            TurnEvent::ToolResult { id, name, output } => {
                Ok(AgentRunEvent::ToolResult { id, name, output })
            }
        });

        Ok(Box::pin(stream))
    }

    async fn cancel_run(&self, _run_id: &str, _reason: &str) -> anyhow::Result<AgentCancelResult> {
        Ok(AgentCancelResult { accepted: false })
    }
}
```

Modify `lib.rs`:

```rust
pub mod agent_backend_local;
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend_local
```

Expected: pass.

## Task 3: Add GrpcAgentBackend Event Mapping

**Files:**
- Create: `crates/zeroclaw-gateway/src/agent_backend_grpc.rs`
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: `crates/zeroclaw-gateway/src/agent_backend_grpc.rs`

- [ ] **Step 1: Write the failing mapping test**

Add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::pb;

    #[test]
    fn maps_message_delta_event() {
        let event = pb::RunEvent {
            run_id: "run-a".to_string(),
            request_id: "req-a".to_string(),
            session_id: "session-a".to_string(),
            sequence: 3,
            occurred_at: None,
            event_type: "message.delta".to_string(),
            payload: Some(pb::run_event::Payload::MessageDelta(pb::MessageDelta {
                delta: "hello".to_string(),
            })),
        };

        let mapped = map_grpc_event(event).unwrap();
        assert_eq!(
            mapped,
            crate::agent_backend::AgentRunEvent::MessageDelta {
                delta: "hello".to_string()
            }
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend_grpc::tests::maps_message_delta_event
```

Expected: fail because `agent_backend_grpc` does not exist.

- [ ] **Step 3: Implement mapping and client skeleton**

Create `agent_backend_grpc.rs` with:

```rust
use crate::agent_backend::AgentRunEvent;
use crate::grpc::pb;

pub fn map_grpc_event(event: pb::RunEvent) -> anyhow::Result<AgentRunEvent> {
    match event.payload {
        Some(pb::run_event::Payload::Started(started)) => Ok(AgentRunEvent::RunStarted {
            provider: started.provider,
            model: started.model,
        }),
        Some(pb::run_event::Payload::MessageDelta(delta)) => {
            Ok(AgentRunEvent::MessageDelta { delta: delta.delta })
        }
        Some(pb::run_event::Payload::ThinkingDelta(delta)) => {
            Ok(AgentRunEvent::ThinkingDelta { delta: delta.delta })
        }
        Some(pb::run_event::Payload::ToolCall(call)) => Ok(AgentRunEvent::ToolCall {
            id: call.id,
            name: call.name,
            args: serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::Value::Null),
        }),
        Some(pb::run_event::Payload::ToolResult(result)) => Ok(AgentRunEvent::ToolResult {
            id: result.id,
            name: result.name,
            output: result.output,
        }),
        Some(pb::run_event::Payload::Completed(done)) => {
            Ok(AgentRunEvent::Completed { final_text: done.final_text })
        }
        Some(pb::run_event::Payload::Cancelled(cancelled)) => {
            Ok(AgentRunEvent::Cancelled { reason: cancelled.reason })
        }
        Some(pb::run_event::Payload::Failed(failed)) => {
            let error = failed.error.unwrap_or(pb::RunError {
                code: "unknown".to_string(),
                message: "run failed".to_string(),
                retryable: false,
                details: Default::default(),
            });
            Ok(AgentRunEvent::Failed {
                code: error.code,
                message: error.message,
            })
        }
        Some(pb::run_event::Payload::Accepted(_)) => Ok(AgentRunEvent::RunStarted {
            provider: String::new(),
            model: String::new(),
        }),
        None => anyhow::bail!("gRPC run event has no payload"),
    }
}
```

Modify `lib.rs`:

```rust
pub mod agent_backend_grpc;
```

- [ ] **Step 4: Run the mapping test**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend_grpc::tests::maps_message_delta_event
```

Expected: pass.

## Task 4: Add Gateway Backend Configuration

**Files:**
- Modify: `crates/zeroclaw-config/src/schema.rs`
- Test: existing config schema tests or module tests in `schema.rs`

- [ ] **Step 1: Write failing config default test**

Add a test near gateway config tests:

```rust
#[test]
fn gateway_agent_backend_defaults_to_local() {
    let config = Config::default();
    assert_eq!(config.gateway.agent_backend.kind, "local");
    assert!(config.gateway.agent_backend.endpoint.is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-config gateway_agent_backend_defaults_to_local
```

Expected: fail because `agent_backend` field does not exist.

- [ ] **Step 3: Add config fields**

Add:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct GatewayAgentBackendConfig {
    pub kind: String,
    pub endpoint: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_ms: u64,
}

impl Default for GatewayAgentBackendConfig {
    fn default() -> Self {
        Self {
            kind: "local".to_string(),
            endpoint: None,
            bearer_token: None,
            timeout_ms: 600_000,
        }
    }
}
```

Add this field to `GatewayConfig`:

```rust
pub agent_backend: GatewayAgentBackendConfig,
```

- [ ] **Step 4: Run config tests**

Run:

```bash
cargo test -p zeroclaw-config gateway_agent_backend_defaults_to_local
```

Expected: pass.

## Task 5: Put AgentBackend in AppState

**Files:**
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: existing `AppState` clone/test-state tests

- [ ] **Step 1: Write failing AppState backend test**

Add:

```rust
#[test]
fn app_state_contains_agent_backend() {
    fn assert_backend<T: Send + Sync>() {}
    assert_backend::<Arc<dyn crate::agent_backend::AgentBackend>>();
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway app_state_contains_agent_backend
```

Expected: fail because `AppState` has no backend field or module import.

- [ ] **Step 3: Add backend field and construction helper**

Add to `AppState`:

```rust
pub agent_backend: Arc<dyn agent_backend::AgentBackend>,
```

Add helper:

```rust
fn build_agent_backend(config: &Config) -> Result<Arc<dyn agent_backend::AgentBackend>> {
    match config.gateway.agent_backend.kind.as_str() {
        "local" => Ok(Arc::new(agent_backend_local::LocalAgentBackend::new(config.clone()))),
        "grpc" => Ok(Arc::new(agent_backend_grpc::GrpcAgentBackend::from_config(config)?)),
        other => anyhow::bail!("unsupported gateway.agent_backend.kind: {other}"),
    }
}
```

During `run_gateway`, set:

```rust
let agent_backend = build_agent_backend(&config)?;
```

and include it in `AppState`.

- [ ] **Step 4: Update test AppState builders**

Every test creating `AppState` must pass:

```rust
agent_backend: Arc::new(crate::agent_backend_local::LocalAgentBackend::new(Config::default())),
```

- [ ] **Step 5: Run gateway tests**

Run:

```bash
cargo test -p zeroclaw-gateway app_state_contains_agent_backend
```

Expected: pass.

## Task 6: Migrate REST/Webhook/Channel Chat to AgentBackend

**Files:**
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: gateway webhook tests

- [ ] **Step 1: Write failing behavior test**

Add a test backend that records calls and install it in a test `AppState`. Test `handle_webhook` with a message and assert the backend saw that input.

Expected test assertion:

```rust
assert_eq!(backend.calls.lock().unwrap().as_slice(), &["hello from webhook"]);
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway webhook_uses_agent_backend
```

Expected: fail because `run_gateway_chat_with_tools` still calls provider/runtime directly.

- [ ] **Step 3: Change `run_gateway_chat_with_tools`**

Replace local provider/runtime dispatch with:

```rust
let request = agent_backend::AgentRunRequest {
    request_id: uuid::Uuid::new_v4().to_string(),
    session_id: session_id.unwrap_or("webhook").to_string(),
    actor_id: "gateway".to_string(),
    input: message.to_string(),
};

let mut stream = state.agent_backend.run_chat_streamed(request).await?;
let mut collected = agent_backend::AgentRunCollected::default();
while let Some(event) = stream.next().await {
    collected.apply(&event?);
}
Ok(collected.final_text)
```

- [ ] **Step 4: Run webhook tests**

Run:

```bash
cargo test -p zeroclaw-gateway webhook_uses_agent_backend
```

Expected: pass.

## Task 7: Migrate WebSocket Chat to AgentBackend

**Files:**
- Modify: `crates/zeroclaw-gateway/src/ws.rs`
- Test: WebSocket unit tests or focused backend mapping tests

- [ ] **Step 1: Write failing WebSocket event mapping test**

Add a pure function:

```rust
fn ws_message_from_agent_event(event: crate::agent_backend::AgentRunEvent)
    -> Option<serde_json::Value>
```

Test:

```rust
#[test]
fn ws_message_from_agent_event_maps_delta() {
    let value = ws_message_from_agent_event(AgentRunEvent::MessageDelta {
        delta: "hi".to_string(),
    })
    .unwrap();

    assert_eq!(value["type"], "chunk");
    assert_eq!(value["content"], "hi");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway ws_message_from_agent_event_maps_delta
```

Expected: fail because the function does not exist.

- [ ] **Step 3: Implement mapping function**

Add:

```rust
fn ws_message_from_agent_event(event: AgentRunEvent) -> Option<serde_json::Value> {
    match event {
        AgentRunEvent::MessageDelta { delta } => {
            Some(serde_json::json!({ "type": "chunk", "content": delta }))
        }
        AgentRunEvent::ThinkingDelta { delta } => {
            Some(serde_json::json!({ "type": "thinking", "content": delta }))
        }
        AgentRunEvent::ToolCall { id, name, args } => {
            Some(serde_json::json!({ "type": "tool_call", "id": id, "name": name, "args": args }))
        }
        AgentRunEvent::ToolResult { id, name, output } => {
            Some(serde_json::json!({ "type": "tool_result", "id": id, "name": name, "output": output }))
        }
        AgentRunEvent::Cancelled { .. } => Some(serde_json::json!({ "type": "aborted" })),
        AgentRunEvent::Failed { message, .. } => {
            Some(serde_json::json!({ "type": "error", "message": message }))
        }
        AgentRunEvent::RunStarted { .. } | AgentRunEvent::Completed { .. } => None,
    }
}
```

- [ ] **Step 4: Replace direct `Agent::from_config_with_session_cwd` path**

Change WebSocket setup so it no longer creates a persistent `Agent`. For each message, build `AgentRunRequest`, call `state.agent_backend.run_chat_streamed`, forward mapped events to the socket, and update session persistence with accumulated final text.

- [ ] **Step 5: Run WebSocket mapping test**

Run:

```bash
cargo test -p zeroclaw-gateway ws_message_from_agent_event_maps_delta
```

Expected: pass.

## Task 8: Add GrpcAgentBackend Client Calls

**Files:**
- Modify: `crates/zeroclaw-gateway/src/agent_backend_grpc.rs`
- Test: `crates/zeroclaw-gateway/src/agent_backend_grpc.rs`

- [ ] **Step 1: Write failing request conversion test**

Add:

```rust
#[test]
fn builds_create_run_request_from_agent_request() {
    let request = AgentRunRequest {
        request_id: "req-a".to_string(),
        session_id: "session-a".to_string(),
        actor_id: "user-a".to_string(),
        input: "hello".to_string(),
    };

    let grpc = build_create_run_request(request);
    assert_eq!(grpc.request_id, "req-a");
    assert_eq!(grpc.session_id, "session-a");
    assert_eq!(grpc.input.unwrap().text, "hello");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway builds_create_run_request_from_agent_request
```

Expected: fail because `build_create_run_request` does not exist.

- [ ] **Step 3: Implement conversion**

Add:

```rust
pub fn build_create_run_request(request: AgentRunRequest) -> pb::CreateRunRequest {
    pb::CreateRunRequest {
        protocol: "zeroclaw.v1".to_string(),
        request_id: request.request_id,
        session_id: request.session_id,
        actor: Some(pb::Actor {
            actor_id: request.actor_id,
            actor_type: "edge-user".to_string(),
            display_name: String::new(),
            metadata: Default::default(),
        }),
        input: Some(pb::RunInput {
            kind: pb::run_input::InputKind::Message as i32,
            text: request.input,
        }),
        options: Some(pb::RunOptions {
            stream: true,
            model: String::new(),
            allowed_tools: Vec::new(),
            timeout_ms: 0,
        }),
        metadata: Default::default(),
    }
}
```

- [ ] **Step 4: Implement actual client after conversion is green**

Add a tonic client wrapper for the manually maintained service paths. Use `tonic::client::Grpc` with path strings:

```text
/zeroclaw.v1.AgentService/CreateRun
/zeroclaw.v1.AgentService/StreamRun
/zeroclaw.v1.AgentService/CancelRun
```

Attach metadata:

```text
authorization: Bearer <gateway.agent_backend.bearer_token>
```

- [ ] **Step 5: Run gRPC backend tests**

Run:

```bash
cargo test -p zeroclaw-gateway agent_backend_grpc
```

Expected: pass.

## Task 9: Add zeroclaw-core.exe

**Files:**
- Create: `src/bin/zeroclaw-core.rs`
- Test: CLI parsing or smoke test

- [ ] **Step 1: Write failing binary compile check**

Run:

```bash
cargo check --bin zeroclaw-core
```

Expected: fail because the binary does not exist.

- [ ] **Step 2: Add core binary**

Create `src/bin/zeroclaw-core.rs`:

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    project_root: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 42618)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    zeroclaw::init_tracing();
    let args = Args::parse();
    let mut config = zeroclaw_config::schema::Config::load().await?;
    zeroclaw::apply_runtime_project_root(&mut config, &args.project_root)?;
    zeroclaw_gateway::grpc::run_grpc_server(&args.host, args.port, config).await
}
```

If `init_tracing` or `apply_runtime_project_root` is private, extract public helpers in `src/lib.rs` rather than duplicating logic.

- [ ] **Step 3: Run binary check**

Run:

```bash
cargo check --bin zeroclaw-core
```

Expected: pass.

## Task 10: Add zeroclaw-edge.exe

**Files:**
- Create: `src/bin/zeroclaw-edge.rs`
- Test: CLI parsing or smoke test

- [ ] **Step 1: Write failing binary compile check**

Run:

```bash
cargo check --bin zeroclaw-edge
```

Expected: fail because the binary does not exist.

- [ ] **Step 2: Add edge binary**

Create `src/bin/zeroclaw-edge.rs`:

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    project_root: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 42617)]
    port: u16,
    #[arg(long)]
    core_grpc: String,
    #[arg(long)]
    core_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    zeroclaw::init_tracing();
    let args = Args::parse();
    let mut config = zeroclaw_config::schema::Config::load().await?;
    zeroclaw::apply_runtime_project_root(&mut config, &args.project_root)?;
    config.gateway.agent_backend.kind = "grpc".to_string();
    config.gateway.agent_backend.endpoint = Some(args.core_grpc);
    config.gateway.agent_backend.bearer_token = args.core_token;
    zeroclaw_gateway::run_gateway(&args.host, args.port, config, None, None, None).await
}
```

- [ ] **Step 3: Run binary check**

Run:

```bash
cargo check --bin zeroclaw-edge
```

Expected: pass.

## Task 11: Verify Split Boundary

**Files:**
- Modify tests added in earlier tasks
- Documentation only if commands change

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --all -- --check
```

Expected: pass.

- [ ] **Step 2: Run focused tests**

Run:

```bash
cargo test -p zeroclaw-config gateway_agent_backend_defaults_to_local
cargo test -p zeroclaw-gateway agent_backend
cargo test -p zeroclaw-gateway agent_backend_grpc
cargo test -p zeroclaw-gateway grpc
```

Expected: pass.

- [ ] **Step 3: Run binary checks**

Run:

```bash
cargo check --bin zeroclaw-core
cargo check --bin zeroclaw-edge
```

Expected: pass.

- [ ] **Step 4: Run gateway clippy**

Run:

```bash
cargo clippy -p zeroclaw-gateway --all-targets -- -D warnings
```

Expected: pass.

- [ ] **Step 5: Record manual smoke command**

Manual smoke sequence:

```bash
target/debug/zeroclaw-core.exe --project-root D:\workspace\axi-research\zeroclaw --host 127.0.0.1 --port 42618
target/debug/zeroclaw-edge.exe --project-root D:\workspace\axi-research\zeroclaw --host 127.0.0.1 --port 42617 --core-grpc http://127.0.0.1:42618
```

Expected: WebUI connects to edge on `42617`; no browser or WebUI traffic is sent to core on `42618`.

## Self-Review

- The plan covers executable split, backend abstraction, gRPC client, WebSocket migration, REST/webhook/channel migration, config, tests, and verification.
- The first implementation keeps local backend only for compatibility and tests; `zeroclaw-edge.exe` forces gRPC backend.
- The plan intentionally does not move all business session ownership out of core in the first phase. That is a separate follow-up after the two-process boundary is verified.
