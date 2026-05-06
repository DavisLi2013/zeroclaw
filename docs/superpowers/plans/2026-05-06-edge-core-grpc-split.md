# Edge/Core gRPC Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `zeroclaw-edge.exe` and `zeroclaw-core.exe` so every WebUI, REST chat, WebSocket, webhook, and channel request enters edge, edge communicates only with core over gRPC for agent execution, and only core calls the agent runtime.

**Architecture:** `zeroclaw-edge.exe` owns gateway surfaces and depends on a `CoreAgentClient` abstraction whose production implementation is gRPC-only. `zeroclaw-core.exe` starts only `zeroclaw.v1.AgentService` and is the only split binary allowed to call `Agent::from_config`, `Agent::from_config_with_session_cwd`, `turn_streamed`, or `process_message`.

**Tech Stack:** Rust, Tokio, Axum, tonic gRPC, existing `zeroclaw-gateway`, existing `zeroclaw-runtime`, existing `zeroclaw-config`.

---

## Non-Negotiable Boundary

`zeroclaw-edge.exe` must not directly call agent/runtime execution APIs in production code. Edge may only call core through gRPC.

Forbidden in edge production paths:

```text
zeroclaw_runtime::agent::Agent
Agent::from_config
Agent::from_config_with_session_cwd
Agent::turn_streamed
zeroclaw_runtime::agent::process_message
```

Allowed in edge:

```text
CoreAgentClient trait
GrpcCoreAgentClient production implementation
MockCoreAgentClient test implementation
RunEvent -> WebUI/REST/channel response mapping
```

Allowed in core:

```text
GrpcAgentService
Agent::from_config
Agent::turn_streamed
provider/tool/runtime execution
```

## File Structure

- Create `crates/zeroclaw-gateway/src/core_client.rs`: shared request/event/result types and `CoreAgentClient` trait.
- Create `crates/zeroclaw-gateway/src/core_client_grpc.rs`: edge-side gRPC client for `zeroclaw.v1.AgentService`.
- Modify `crates/zeroclaw-gateway/src/lib.rs`: add `core_client` to `AppState`, initialize it, and route REST/webhook/channel chat through it.
- Modify `crates/zeroclaw-gateway/src/ws.rs`: route WebSocket chat through `CoreAgentClient`.
- Modify `crates/zeroclaw-gateway/src/grpc.rs`: keep this as core-side gRPC service; expose protobuf helpers needed by `core_client_grpc`.
- Modify `crates/zeroclaw-config/src/schema.rs`: add `[gateway.core]` config.
- Create `src/bin/zeroclaw-core.rs`: starts only core gRPC.
- Create `src/bin/zeroclaw-edge.rs`: starts gateway edge and requires a core gRPC endpoint.
- Modify `src/lib.rs`: expose shared startup helpers for the split binaries.
- Modify `docs/superpowers/specs/2026-05-06-edge-core-grpc-split-design.md`: keep design synchronized.

## Task 1: Add CoreAgentClient Types

**Files:**
- Create: `crates/zeroclaw-gateway/src/core_client.rs`
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: `crates/zeroclaw-gateway/src/core_client.rs`

- [x] **Step 1: Write the failing test**

Add this test in `core_client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_delta_accumulates_final_text() {
        let mut collected = CoreRunCollected::default();
        collected.apply(&CoreRunEvent::MessageDelta {
            delta: "hello".to_string(),
        });
        collected.apply(&CoreRunEvent::MessageDelta {
            delta: " world".to_string(),
        });

        assert_eq!(collected.final_text, "hello world");
    }
}
```

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway core_client::tests::run_event_delta_accumulates_final_text
```

Expected: fail because `core_client` module and types do not exist.

- [x] **Step 3: Add minimal core client types**

Create `crates/zeroclaw-gateway/src/core_client.rs`:

```rust
use std::pin::Pin;

use futures_util::Stream;

pub type CoreRunStream =
    Pin<Box<dyn Stream<Item = anyhow::Result<CoreRunEvent>> + Send + 'static>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreRunRequest {
    pub request_id: String,
    pub session_id: String,
    pub actor_id: String,
    pub input: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreRunEvent {
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
pub struct CoreCancelResult {
    pub accepted: bool,
}

#[derive(Default)]
pub struct CoreRunCollected {
    pub final_text: String,
}

impl CoreRunCollected {
    pub fn apply(&mut self, event: &CoreRunEvent) {
        if let CoreRunEvent::MessageDelta { delta } = event {
            self.final_text.push_str(delta);
        }
        if let CoreRunEvent::Completed { final_text } = event {
            self.final_text = final_text.clone();
        }
    }
}

#[async_trait::async_trait]
pub trait CoreAgentClient: Send + Sync {
    async fn run_chat_streamed(&self, request: CoreRunRequest)
        -> anyhow::Result<CoreRunStream>;

    async fn cancel_run(&self, run_id: &str, reason: &str)
        -> anyhow::Result<CoreCancelResult>;
}
```

Modify `crates/zeroclaw-gateway/src/lib.rs`:

```rust
pub mod core_client;
```

- [x] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test -p zeroclaw-gateway core_client::tests::run_event_delta_accumulates_final_text
```

Expected: pass.

## Task 2: Add GrpcCoreAgentClient Event Mapping

**Files:**
- Create: `crates/zeroclaw-gateway/src/core_client_grpc.rs`
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: `crates/zeroclaw-gateway/src/core_client_grpc.rs`

- [x] **Step 1: Write the failing mapping test**

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
            crate::core_client::CoreRunEvent::MessageDelta {
                delta: "hello".to_string()
            }
        );
    }
}
```

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway core_client_grpc::tests::maps_message_delta_event
```

Expected: fail because `core_client_grpc` does not exist.

- [x] **Step 3: Implement mapping**

Create `crates/zeroclaw-gateway/src/core_client_grpc.rs` with:

```rust
use crate::core_client::CoreRunEvent;
use crate::grpc::pb;

pub fn map_grpc_event(event: pb::RunEvent) -> anyhow::Result<CoreRunEvent> {
    match event.payload {
        Some(pb::run_event::Payload::Started(started)) => Ok(CoreRunEvent::RunStarted {
            provider: started.provider,
            model: started.model,
        }),
        Some(pb::run_event::Payload::MessageDelta(delta)) => {
            Ok(CoreRunEvent::MessageDelta { delta: delta.delta })
        }
        Some(pb::run_event::Payload::ThinkingDelta(delta)) => {
            Ok(CoreRunEvent::ThinkingDelta { delta: delta.delta })
        }
        Some(pb::run_event::Payload::ToolCall(call)) => Ok(CoreRunEvent::ToolCall {
            id: call.id,
            name: call.name,
            args: serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::Value::Null),
        }),
        Some(pb::run_event::Payload::ToolResult(result)) => Ok(CoreRunEvent::ToolResult {
            id: result.id,
            name: result.name,
            output: result.output,
        }),
        Some(pb::run_event::Payload::Completed(done)) => {
            Ok(CoreRunEvent::Completed { final_text: done.final_text })
        }
        Some(pb::run_event::Payload::Cancelled(cancelled)) => {
            Ok(CoreRunEvent::Cancelled { reason: cancelled.reason })
        }
        Some(pb::run_event::Payload::Failed(failed)) => {
            let error = failed.error.unwrap_or(pb::RunError {
                code: "unknown".to_string(),
                message: "run failed".to_string(),
                retryable: false,
                details: Default::default(),
            });
            Ok(CoreRunEvent::Failed {
                code: error.code,
                message: error.message,
            })
        }
        Some(pb::run_event::Payload::Accepted(_)) => Ok(CoreRunEvent::RunStarted {
            provider: String::new(),
            model: String::new(),
        }),
        None => anyhow::bail!("gRPC run event has no payload"),
    }
}
```

Modify `lib.rs`:

```rust
pub mod core_client_grpc;
```

- [x] **Step 4: Run the mapping test**

Run:

```bash
cargo test -p zeroclaw-gateway core_client_grpc::tests::maps_message_delta_event
```

Expected: pass.

## Task 3: Add Gateway Core Configuration

**Files:**
- Modify: `crates/zeroclaw-config/src/schema.rs`
- Test: config schema tests in `schema.rs`

- [x] **Step 1: Write failing config default test**

Add a test near gateway config tests:

```rust
#[test]
fn gateway_core_config_defaults_to_no_endpoint() {
    let config = Config::default();
    assert!(config.gateway.core.endpoint.is_none());
    assert_eq!(config.gateway.core.timeout_ms, 600_000);
}
```

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-config gateway_core_config_defaults_to_no_endpoint
```

Expected: fail because `gateway.core` does not exist.

- [x] **Step 3: Add config fields**

Add:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct GatewayCoreConfig {
    pub endpoint: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_ms: u64,
}

impl Default for GatewayCoreConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            bearer_token: None,
            timeout_ms: 600_000,
        }
    }
}
```

Add this field to `GatewayConfig`:

```rust
pub core: GatewayCoreConfig,
```

- [x] **Step 4: Run config tests**

Run:

```bash
cargo test -p zeroclaw-config gateway_core_config_defaults_to_no_endpoint
```

Expected: pass.

## Task 4: Put CoreAgentClient in AppState

**Files:**
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: existing `AppState` clone/test-state tests

- [x] **Step 1: Write failing AppState client test**

Add:

```rust
#[test]
fn app_state_contains_core_agent_client() {
    fn assert_client<T: Send + Sync>() {}
    assert_client::<Arc<dyn crate::core_client::CoreAgentClient>>();
}
```

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway app_state_contains_core_agent_client
```

Expected: fail because `AppState` has no core client field.

- [x] **Step 3: Add core client field and construction helper**

Add to `AppState`:

```rust
pub core_client: Arc<dyn core_client::CoreAgentClient>,
```

Add helper:

```rust
fn build_core_client(config: &Config) -> Result<Arc<dyn core_client::CoreAgentClient>> {
    let endpoint = config
        .gateway
        .core
        .endpoint
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("gateway.core.endpoint is required for edge mode"))?;

    Ok(Arc::new(core_client_grpc::GrpcCoreAgentClient::new(
        endpoint.to_string(),
        config.gateway.core.bearer_token.clone(),
        std::time::Duration::from_millis(config.gateway.core.timeout_ms),
    )?))
}
```

During `run_gateway`, set:

```rust
let core_client = build_core_client(&config)?;
```

and include it in `AppState`.

- [x] **Step 4: Update test AppState builders**

Every test creating `AppState` must pass a mock that implements `CoreAgentClient`.

Use this test-only mock:

```rust
#[derive(Default)]
struct MockCoreAgentClient;

#[async_trait::async_trait]
impl crate::core_client::CoreAgentClient for MockCoreAgentClient {
    async fn run_chat_streamed(
        &self,
        _request: crate::core_client::CoreRunRequest,
    ) -> anyhow::Result<crate::core_client::CoreRunStream> {
        let stream = tokio_stream::iter(vec![Ok(crate::core_client::CoreRunEvent::Completed {
            final_text: "mock response".to_string(),
        })]);
        Ok(Box::pin(stream))
    }

    async fn cancel_run(
        &self,
        _run_id: &str,
        _reason: &str,
    ) -> anyhow::Result<crate::core_client::CoreCancelResult> {
        Ok(crate::core_client::CoreCancelResult { accepted: true })
    }
}
```

Then set:

```rust
core_client: Arc::new(MockCoreAgentClient::default()),
```

- [x] **Step 5: Run gateway test**

Run:

```bash
cargo test -p zeroclaw-gateway app_state_contains_core_agent_client
```

Expected: pass.

## Task 5: Migrate REST/Webhook/Channel Chat to CoreAgentClient

**Files:**
- Modify: `crates/zeroclaw-gateway/src/lib.rs`
- Test: gateway webhook tests

- [x] **Step 1: Write failing behavior test**

Add a test client that records calls and install it in a test `AppState`. Test `handle_webhook` with a message and assert the client saw that input.

Expected test assertion:

```rust
assert_eq!(client.calls.lock().unwrap().as_slice(), &["hello from webhook"]);
```

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway webhook_uses_core_agent_client
```

Expected: fail because `run_gateway_chat_with_tools` still calls provider/runtime directly.

- [x] **Step 3: Change `run_gateway_chat_with_tools`**

Replace local provider/runtime dispatch with:

```rust
let request = core_client::CoreRunRequest {
    request_id: uuid::Uuid::new_v4().to_string(),
    session_id: session_id.unwrap_or("webhook").to_string(),
    actor_id: "gateway".to_string(),
    input: message.to_string(),
};

let mut stream = state.core_client.run_chat_streamed(request).await?;
let mut collected = core_client::CoreRunCollected::default();
while let Some(event) = stream.next().await {
    collected.apply(&event?);
}
Ok(collected.final_text)
```

- [x] **Step 4: Run webhook tests**

Run:

```bash
cargo test -p zeroclaw-gateway webhook_uses_core_agent_client
```

Expected: pass.

## Task 6: Migrate WebSocket Chat to CoreAgentClient

**Files:**
- Modify: `crates/zeroclaw-gateway/src/ws.rs`
- Test: WebSocket unit tests or focused mapping tests

- [x] **Step 1: Write failing WebSocket event mapping test**

Add a pure function:

```rust
fn ws_message_from_core_event(event: crate::core_client::CoreRunEvent)
    -> Option<serde_json::Value>
```

Test:

```rust
#[test]
fn ws_message_from_core_event_maps_delta() {
    let value = ws_message_from_core_event(CoreRunEvent::MessageDelta {
        delta: "hi".to_string(),
    })
    .unwrap();

    assert_eq!(value["type"], "chunk");
    assert_eq!(value["content"], "hi");
}
```

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway ws_message_from_core_event_maps_delta
```

Expected: fail because the function does not exist.

- [x] **Step 3: Implement mapping function**

Add:

```rust
fn ws_message_from_core_event(event: CoreRunEvent) -> Option<serde_json::Value> {
    match event {
        CoreRunEvent::MessageDelta { delta } => {
            Some(serde_json::json!({ "type": "chunk", "content": delta }))
        }
        CoreRunEvent::ThinkingDelta { delta } => {
            Some(serde_json::json!({ "type": "thinking", "content": delta }))
        }
        CoreRunEvent::ToolCall { id, name, args } => {
            Some(serde_json::json!({ "type": "tool_call", "id": id, "name": name, "args": args }))
        }
        CoreRunEvent::ToolResult { id, name, output } => {
            Some(serde_json::json!({ "type": "tool_result", "id": id, "name": name, "output": output }))
        }
        CoreRunEvent::Cancelled { .. } => Some(serde_json::json!({ "type": "aborted" })),
        CoreRunEvent::Failed { message, .. } => {
            Some(serde_json::json!({ "type": "error", "message": message }))
        }
        CoreRunEvent::RunStarted { .. } | CoreRunEvent::Completed { .. } => None,
    }
}
```

- [x] **Step 4: Replace direct local agent path**

Remove WebSocket production calls to:

```text
Agent::from_config_with_session_cwd
agent.turn_streamed
```

For each user message, build `CoreRunRequest`, call
`state.core_client.run_chat_streamed`, forward mapped events to the socket, and
update session persistence with accumulated final text.

- [x] **Step 5: Run WebSocket mapping test**

Run:

```bash
cargo test -p zeroclaw-gateway ws_message_from_core_event_maps_delta
```

Expected: pass.

## Task 7: Add GrpcCoreAgentClient Client Calls

**Files:**
- Modify: `crates/zeroclaw-gateway/src/core_client_grpc.rs`
- Test: `crates/zeroclaw-gateway/src/core_client_grpc.rs`

- [x] **Step 1: Write failing request conversion test**

Add:

```rust
#[test]
fn builds_create_run_request_from_core_request() {
    let request = CoreRunRequest {
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

- [x] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p zeroclaw-gateway builds_create_run_request_from_core_request
```

Expected: fail because `build_create_run_request` does not exist.

- [x] **Step 3: Implement conversion**

Add:

```rust
pub fn build_create_run_request(request: CoreRunRequest) -> pb::CreateRunRequest {
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

- [x] **Step 4: Implement actual gRPC client**

Add a tonic client wrapper for the manually maintained service paths. Use
`tonic::client::Grpc` with path strings:

```text
/zeroclaw.v1.AgentService/CreateRun
/zeroclaw.v1.AgentService/StreamRun
/zeroclaw.v1.AgentService/CancelRun
```

Attach metadata:

```text
authorization: Bearer <gateway.core.bearer_token>
```

- [x] **Step 5: Run gRPC client tests**

Run:

```bash
cargo test -p zeroclaw-gateway core_client_grpc
```

Expected: pass.

## Task 8: Add Edge Boundary Regression Tests

**Files:**
- Test: `crates/zeroclaw-gateway/src/ws.rs`
- Test: `crates/zeroclaw-gateway/src/lib.rs`

- [x] **Step 1: Add scan-based boundary test**

Add a test that reads edge source files and rejects forbidden agent calls:

```rust
#[test]
fn edge_sources_do_not_call_agent_runtime_directly() {
    let files = [
        "crates/zeroclaw-gateway/src/ws.rs",
        "crates/zeroclaw-gateway/src/lib.rs",
    ];
    let forbidden = [
        "Agent::from_config",
        "Agent::from_config_with_session_cwd",
        ".turn_streamed(",
        "zeroclaw_runtime::agent::process_message",
    ];

    for file in files {
        let contents = std::fs::read_to_string(file).unwrap();
        for needle in forbidden {
            assert!(
                !contents.contains(needle),
                "{file} must not contain forbidden edge agent call {needle}"
            );
        }
    }
}
```

- [x] **Step 2: Run the test to verify it fails before migration**

Run:

```bash
cargo test -p zeroclaw-gateway edge_sources_do_not_call_agent_runtime_directly
```

Expected: fail until WebSocket and REST/webhook/channel paths are migrated.

- [x] **Step 3: Re-run after Tasks 5 and 6**

Run:

```bash
cargo test -p zeroclaw-gateway edge_sources_do_not_call_agent_runtime_directly
```

Expected: pass after direct calls are removed.

## Task 9: Add zeroclaw-core.exe

**Files:**
- Modify: `src/lib.rs`
- Create: `src/bin/zeroclaw-core.rs`
- Test: compile check

- [x] **Step 1: Run failing binary check**

Run:

```bash
cargo check --bin zeroclaw-core
```

Expected: fail because `zeroclaw-core` does not exist yet.

- [x] **Step 2: Add public split-binary helpers**

Add to `src/lib.rs`:

```rust
#[cfg(feature = "agent-runtime")]
pub fn init_cli_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).try_init();
}

#[cfg(feature = "agent-runtime")]
pub fn apply_runtime_project_root(
    config: &mut Config,
    project_root: &std::path::Path,
) -> anyhow::Result<()> {
    crate::config::project_root::apply_project_root(config, project_root)?;
    observability::runtime_trace::init_from_config(&config.observability, &config.workspace_dir);
    Ok(())
}
```

- [x] **Step 3: Add core binary**

Create `src/bin/zeroclaw-core.rs`:

```rust
use clap::Parser;
use std::path::PathBuf;
use zeroclaw::Config;

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
    zeroclaw::init_cli_tracing();
    let args = Args::parse();
    let mut config = Box::pin(Config::load_or_init()).await?;
    config.apply_env_overrides();
    zeroclaw::apply_runtime_project_root(&mut config, &args.project_root)?;
    zeroclaw_gateway::grpc::run_grpc_server(&args.host, args.port, config).await
}
```

- [x] **Step 4: Run binary check**

Run:

```bash
cargo check --bin zeroclaw-core
```

Expected: pass.

## Task 10: Add zeroclaw-edge.exe

**Files:**
- Create: `src/bin/zeroclaw-edge.rs`
- Test: compile check

- [x] **Step 1: Run failing binary check**

Run:

```bash
cargo check --bin zeroclaw-edge
```

Expected: fail because the binary does not exist.

- [x] **Step 2: Add edge binary**

Create `src/bin/zeroclaw-edge.rs`:

```rust
use clap::Parser;
use std::path::PathBuf;
use zeroclaw::Config;

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
    zeroclaw::init_cli_tracing();
    let args = Args::parse();
    let mut config = Box::pin(Config::load_or_init()).await?;
    config.apply_env_overrides();
    zeroclaw::apply_runtime_project_root(&mut config, &args.project_root)?;
    config.gateway.core.endpoint = Some(args.core_grpc);
    config.gateway.core.bearer_token = args.core_token;
    zeroclaw_gateway::run_gateway(&args.host, args.port, config, None, None, None).await
}
```

- [x] **Step 3: Run binary check**

Run:

```bash
cargo check --bin zeroclaw-edge
```

Expected: pass.

## Task 11: Verify Split Boundary

**Files:**
- Modify tests added in earlier tasks
- Documentation only if commands change

- [x] **Step 1: Run formatting**

Run:

```bash
cargo fmt --all -- --check
```

Expected: pass.

- [x] **Step 2: Run focused tests**

Run:

```bash
cargo test -p zeroclaw-config gateway_core_config_defaults_to_no_endpoint
cargo test -p zeroclaw-gateway core_client
cargo test -p zeroclaw-gateway core_client_grpc
cargo test -p zeroclaw-gateway grpc
cargo test -p zeroclaw-gateway edge_sources_do_not_call_agent_runtime_directly
```

Expected: pass.

- [x] **Step 3: Run binary checks**

Run:

```bash
cargo check --bin zeroclaw-core
cargo check --bin zeroclaw-edge
```

Expected: pass.

- [x] **Step 4: Run gateway clippy**

Run:

```bash
cargo clippy -p zeroclaw-gateway --all-targets -- -D warnings
```

Expected: pass.

- [x] **Step 5: Record manual smoke command**

Manual smoke sequence:

```bash
target/debug/zeroclaw-core.exe --project-root D:\workspace\axi-research\zeroclaw --host 127.0.0.1 --port 42618
target/debug/zeroclaw-edge.exe --project-root D:\workspace\axi-research\zeroclaw --host 127.0.0.1 --port 42617 --core-grpc http://127.0.0.1:42618
```

Expected: WebUI connects to edge on `42617`; no browser or WebUI traffic is sent to core on `42618`; all agent execution is visible only in the core process.

## Self-Review

- The plan covers executable split, gRPC core client, WebSocket migration, REST/webhook/channel migration, config, tests, binary entrypoints, and verification.
- `zeroclaw-edge.exe` has no local-agent fallback in production. Tests use only `MockCoreAgentClient`.
- `zeroclaw-core.exe` is the only split binary that calls agent/runtime execution APIs.
