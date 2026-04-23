# Axi Orchestrator Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework ZeroClaw toward the proposed axi multi-user orchestrator architecture by introducing stable orchestration contracts, a unified context-building pipeline, descriptor-driven capability assembly, controlled session execution, and explicit safety/recovery planes.

**Architecture:** The refactor keeps the existing trait-based runtime but inserts a new `orchestrator` layer between entrypoints and the current agent loop. The migration is incremental: first land stable contracts and snapshots, then route session execution through a `Session Runtime Bundle`, then extract `ContextBuilder`, then unify skills/hooks/plugins/MCP/tools under descriptor-driven assembly, and finally add multi-user butler/host supervision and stronger recovery boundaries.

**Tech Stack:** Rust 2024, Tokio, Serde, Axum, rusqlite, existing ZeroClaw traits/modules

---

### Task 1: Introduce Orchestrator Contracts

**Files:**
- Create: `src/orchestrator/mod.rs`
- Create: `src/orchestrator/contracts.rs`
- Modify: `src/lib.rs`
- Test: `src/orchestrator/contracts.rs`

- [ ] **Step 1: Write the failing contract tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_message_serializes_with_enqueue_policy_and_memory_directives() {
        let message = InboundMessage {
            message_id: "msg-1".into(),
            channel_type: "http".into(),
            session_hint: Some("session-a".into()),
            enqueue_policy: EnqueuePolicy::InterruptAndEnqueue,
            auth_material_ref: Some("auth/ref".into()),
            request_payload: serde_json::json!({"input": "hello"}),
            memory_loading_directives: MemoryLoadingDirectives::all_enabled(),
            trace_context: serde_json::json!({"trace_id": "trace-1"}),
            created_at: "2026-04-22T00:00:00Z".into(),
        };

        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["enqueue_policy"], "interrupt_and_enqueue");
        assert_eq!(json["memory_loading_directives"]["user_private_long_term"], true);
    }

    #[test]
    fn public_only_memory_directives_disable_private_scopes() {
        let directives = MemoryLoadingDirectives::public_only();
        assert!(directives.public_short_term);
        assert!(directives.public_long_term);
        assert!(!directives.user_private_short_term);
        assert!(!directives.user_private_long_term);
    }

    #[test]
    fn context_build_reason_uses_snake_case_contract_names() {
        let json = serde_json::to_string(&ContextBuildReason::SkillBodyReentry).unwrap();
        assert_eq!(json, "\"skill_body_reentry\"");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test inbound_message_serializes_with_enqueue_policy_and_memory_directives --lib`
Expected: FAIL because `src/orchestrator/contracts.rs` does not yet define the orchestrator contract types.

- [ ] **Step 3: Write the minimal contract implementation**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnqueuePolicy {
    Enqueue,
    InterruptAndEnqueue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLoadingDirectives {
    pub public_short_term: bool,
    pub public_long_term: bool,
    pub user_private_short_term: bool,
    pub user_private_long_term: bool,
}

impl MemoryLoadingDirectives {
    pub fn all_enabled() -> Self {
        Self {
            public_short_term: true,
            public_long_term: true,
            user_private_short_term: true,
            user_private_long_term: true,
        }
    }

    pub fn public_only() -> Self {
        Self {
            public_short_term: true,
            public_long_term: true,
            user_private_short_term: false,
            user_private_long_term: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundMessage {
    pub message_id: String,
    pub channel_type: String,
    pub session_hint: Option<String>,
    pub enqueue_policy: EnqueuePolicy,
    pub auth_material_ref: Option<String>,
    pub request_payload: serde_json::Value,
    pub memory_loading_directives: MemoryLoadingDirectives,
    pub trace_context: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub message_id: String,
    pub correlation_id: String,
    pub user_id: String,
    pub session_id: String,
    pub channel_type: String,
    pub response_payload: serde_json::Value,
    pub safety_labels: Vec<String>,
    pub audit_result: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBuildReason {
    SessionBootstrap,
    TurnGeneration,
    SkillBodyReentry,
    ToolResultReentry,
    AgentResultReentry,
    MemoryAppendReentry,
    RetryRebuild,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test orchestrator::contracts --lib`
Expected: PASS for the new contract tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/orchestrator/mod.rs src/orchestrator/contracts.rs
git commit -m "feat: add orchestrator contract types"
```

### Task 2: Add Session Runtime Bundle and Turn Queue Contracts

**Files:**
- Create: `src/orchestrator/session.rs`
- Modify: `src/gateway/session_queue.rs`
- Modify: `src/channels/session_backend.rs`
- Modify: `src/channels/session_sqlite.rs`
- Test: `src/orchestrator/session.rs`

- [ ] **Step 1: Write the failing session bundle tests**

```rust
#[test]
fn session_runtime_bundle_captures_frozen_snapshots() {
    let bundle = SessionRuntimeBundle {
        session_id: "session-a".into(),
        turn_queue_depth: 0,
        active_turn_id: None,
        capability_snapshot_version: "cap-v1".into(),
        tool_surface_version: "tool-v1".into(),
        skill_catalog_version: "skill-v1".into(),
        memory_snapshot_version: "mem-v1".into(),
        context_build_reason: ContextBuildReason::SessionBootstrap,
    };

    assert_eq!(bundle.session_id, "session-a");
    assert_eq!(bundle.context_build_reason, ContextBuildReason::SessionBootstrap);
}

#[test]
fn session_phase_contract_tracks_cancelling_state() {
    assert_eq!(SessionPhase::Cancelling.as_str(), "cancelling");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test session_runtime_bundle_captures_frozen_snapshots --lib`
Expected: FAIL because the bundle and phase types do not exist yet.

- [ ] **Step 3: Implement the minimal session orchestration contract**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Running,
    WaitingTool,
    WaitingAgent,
    Cancelling,
    Failed,
    Rebuilding,
    Closed,
}

impl SessionPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::WaitingTool => "waiting_tool",
            Self::WaitingAgent => "waiting_agent",
            Self::Cancelling => "cancelling",
            Self::Failed => "failed",
            Self::Rebuilding => "rebuilding",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRuntimeBundle {
    pub session_id: String,
    pub turn_queue_depth: usize,
    pub active_turn_id: Option<String>,
    pub capability_snapshot_version: String,
    pub tool_surface_version: String,
    pub skill_catalog_version: String,
    pub memory_snapshot_version: String,
    pub context_build_reason: ContextBuildReason,
}
```

- [ ] **Step 4: Extend the persistent session state contract**

```rust
pub struct SessionState {
    pub state: String,
    pub turn_id: Option<String>,
    pub turn_started_at: Option<DateTime<Utc>>,
    pub queue_depth: usize,
    pub cancel_requested: bool,
}
```

- [ ] **Step 5: Run tests to verify the session contracts pass**

Run: `cargo test session_phase_contract_tracks_cancelling_state --lib`
Expected: PASS and no regressions in `src/channels/session_sqlite.rs` state tests.

- [ ] **Step 6: Commit**

```bash
git add src/orchestrator/session.rs src/gateway/session_queue.rs src/channels/session_backend.rs src/channels/session_sqlite.rs
git commit -m "feat: add session runtime bundle contracts"
```

### Task 3: Extract a Unified ContextBuilder Layer

Implementation note on 2026-04-22:
- The initial landing uses a single `src/orchestrator/context.rs` module instead of separate `context_builder.rs` / `context_item.rs` files.
- The minimal slice is intended to route system prompt, memory context, and turn user payload through `ContextBuilder` first.
- Task 3 is now functionally complete in the repo:
  - `Agent` prompt + memory + hook contributions flow through `ContextBuilder`.
  - dispatcher and `loop_.rs` tool-result reentry flow through `ContextBuilder`.
  - `channels::build_system_prompt_*` finalizes system prompt output through `ContextBuilder`.
  - `before_prompt_build` / `before_llm_call` remain only as legacy compatibility surfaces and are no longer the preferred extension path.

**Files:**
- Create: `src/orchestrator/context_builder.rs`
- Create: `src/orchestrator/context_item.rs`
- Create: `src/orchestrator/context_builders/mod.rs`
- Modify: `src/agent/agent.rs`
- Modify: `src/agent/prompt.rs`
- Modify: `src/agent/memory_loader.rs`
- Modify: `src/hooks/traits.rs`
- Test: `src/orchestrator/context_builder.rs`

- [ ] **Step 1: Write the failing context builder tests**

```rust
#[test]
fn context_builder_collects_items_from_multiple_sources() {
    let result = ContextBuilder::new()
        .with_reason(ContextBuildReason::TurnGeneration)
        .add_item(ContextItem::inline("system", "policy", 10))
        .add_item(ContextItem::inline("memory", "remember this", 20))
        .build();

    assert_eq!(result.selected_context_items.len(), 2);
    assert_eq!(result.reason, ContextBuildReason::TurnGeneration);
}

#[test]
fn lower_budget_items_are_rejected_when_budget_is_exhausted() {
    let result = ContextBuilder::new()
        .with_budget(10)
        .add_item(ContextItem::inline("system", "always include", 8))
        .add_item(ContextItem::inline("skill", "overflow", 5))
        .build();

    assert_eq!(result.selected_context_items.len(), 1);
    assert_eq!(result.rejected_context_items.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test context_builder_collects_items_from_multiple_sources --lib`
Expected: FAIL because the new builder and item types are not yet wired.

- [ ] **Step 3: Implement the minimal `ContextItem` and `ContextBuilder` types**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub item_id: String,
    pub source_type: String,
    pub loading_reason: String,
    pub budget_weight: usize,
    pub inline_content: String,
}

impl ContextItem {
    pub fn inline(source_type: &str, inline_content: &str, budget_weight: usize) -> Self {
        Self {
            item_id: format!("{source_type}-{budget_weight}"),
            source_type: source_type.into(),
            loading_reason: "direct".into(),
            budget_weight,
            inline_content: inline_content.into(),
        }
    }
}

pub struct ContextBuildResult {
    pub reason: ContextBuildReason,
    pub selected_context_items: Vec<ContextItem>,
    pub rejected_context_items: Vec<ContextItem>,
}
```

- [ ] **Step 4: Move current prompt assembly behind the builder**

```rust
let context_result = ContextBuilder::new()
    .with_reason(ContextBuildReason::TurnGeneration)
    .extend(system_items)
    .extend(memory_items)
    .extend(skill_items)
    .build();

let prompt = self.prompt_builder.build_from_context(&context_result.selected_context_items)?;
```

- [ ] **Step 5: Update hook contracts to return declarative context contributions**

```rust
pub struct ContextHookResult {
    pub proposed_items: Vec<ContextItem>,
    pub ranking_hints: Vec<String>,
    pub redaction_hints: Vec<String>,
    pub budget_hints: Vec<String>,
    pub diagnostics: Vec<String>,
}
```

- [ ] **Step 6: Run focused tests**

Run: `cargo test context_builder --lib`
Expected: PASS for the new builder tests and no failures in prompt-related unit tests.

- [ ] **Step 7: Commit**

```bash
git add src/orchestrator/context_item.rs src/orchestrator/context_builder.rs src/orchestrator/context_builders/mod.rs src/agent/agent.rs src/agent/prompt.rs src/agent/memory_loader.rs src/hooks/traits.rs
git commit -m "feat: add unified context builder pipeline"
```

### Task 4: Unify Skills, Hooks, Plugins, MCP, and Commands Behind Descriptors

Implementation note on 2026-04-22:
- The initial landing uses `src/orchestrator/descriptors/{manifest,runtime_descriptor}.rs` as a pure contract/projection layer.
- This slice intentionally does not rewire the existing skill loader, hook runner, or tool registry yet; it establishes descriptor validation and projection first.
- The current bridge points are:
  - `SkillDescriptor::from_skill(&crate::skills::Skill, role)`
  - `ToolSurfaceItem::from_tool_spec(...)`
  - `CapabilityRuntimeDescriptor::project()` for command + MCP tool surfaces, skill catalog, agent catalog, hook catalog, and plugin pipeline views.

**Files:**
- Create: `src/orchestrator/descriptors/mod.rs`
- Create: `src/orchestrator/descriptors/manifest.rs`
- Create: `src/orchestrator/descriptors/runtime_descriptor.rs`
- Modify: `src/skills/mod.rs`
- Modify: `src/plugins/mod.rs`
- Modify: `src/tools/traits.rs`
- Modify: `src/hooks/traits.rs`
- Test: `src/orchestrator/descriptors/runtime_descriptor.rs`

- [ ] **Step 1: Write the failing descriptor validation tests**

```rust
#[test]
fn runtime_descriptor_requires_manager_executor_and_supervisor_roles() {
    let descriptor = CapabilityRuntimeDescriptor {
        manifest: DescriptorManifest::new("capability-a", "0.1.0"),
        skills: vec![SkillDescriptor::new("executor", SkillRole::Executor)],
        commands: vec![],
        hooks: vec![],
        agents: vec![],
        mcp_servers: vec![],
        plugins: vec![],
    };

    let error = descriptor.validate().unwrap_err();
    assert!(error.to_string().contains("manager"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test runtime_descriptor_requires_manager_executor_and_supervisor_roles --lib`
Expected: FAIL because descriptor types and validation do not exist yet.

- [ ] **Step 3: Implement the minimal descriptor types**

```rust
pub struct CapabilityRuntimeDescriptor {
    pub manifest: DescriptorManifest,
    pub skills: Vec<SkillDescriptor>,
    pub commands: Vec<CommandDescriptor>,
    pub hooks: Vec<HookDescriptor>,
    pub agents: Vec<AgentDescriptor>,
    pub mcp_servers: Vec<McpServerDescriptor>,
    pub plugins: Vec<PluginDescriptor>,
}

impl CapabilityRuntimeDescriptor {
    pub fn validate(&self) -> anyhow::Result<()> {
        let has_manager = self.skills.iter().any(|skill| skill.role == SkillRole::Manager);
        let has_executor = self.skills.iter().any(|skill| skill.role == SkillRole::Executor);
        let has_supervisor = self.skills.iter().any(|skill| skill.role == SkillRole::Supervisor);

        anyhow::ensure!(has_manager, "capability skills must include a manager role");
        anyhow::ensure!(has_executor, "capability skills must include an executor role");
        anyhow::ensure!(has_supervisor, "capability skills must include a supervisor role");
        Ok(())
    }
}
```

- [ ] **Step 4: Add projection helpers for tool, skill, agent, and hook views**

```rust
pub struct DescriptorProjection {
    pub capability_pipeline: Vec<String>,
    pub skill_catalog: Vec<SkillDescriptor>,
    pub tool_surface: Vec<ToolSpec>,
    pub agent_catalog: Vec<AgentDescriptor>,
}
```

- [ ] **Step 5: Run tests to verify descriptor validation passes**

Run: `cargo test runtime_descriptor --lib`
Expected: PASS for descriptor validation and no regressions in `skills` or `plugins` tests.

- [ ] **Step 6: Commit**

```bash
git add src/orchestrator/descriptors/mod.rs src/orchestrator/descriptors/manifest.rs src/orchestrator/descriptors/runtime_descriptor.rs src/skills/mod.rs src/plugins/mod.rs src/tools/traits.rs src/hooks/traits.rs
git commit -m "feat: add capability runtime descriptor layer"
```

### Task 5: Add Safety Gates and Explicit Tool/Outbound Audits

Implementation note on 2026-04-22:
- The first landing adds `src/orchestrator/safety.rs` with:
  - `ToolSafetyDecision`
  - `ToolSafetyReviewGate`
  - `OutboundAuditDecision`
  - `OutboundSafetyAuditGate`
- Current runtime integration is intentionally narrow:
  - `agent/loop_.rs` now rejects tool calls that are not present in the current visible tool surface.
  - browser-style tools are marked as `Sandbox` by the gate contract.
  - `tools/browser_delegate.rs` now applies the existing sandbox backend to its spawned CLI subprocess and fails closed if sandbox wrapping cannot be applied.
  - `channels/mod.rs` now runs outbound responses through the outbound audit gate for label generation and includes the resulting labels in `runtime_trace` egress metadata, without changing the existing sanitize-and-send behavior.
- Deeper follow-up work for this task remains open:
  - route additional sandbox-marked tools into distinct isolated execution paths
  - connect outbound audit labels to structured audit logging / gateway egress metadata beyond runtime trace
  - consolidate existing `SecurityPolicy::enforce_tool_operation` checks behind the same gate contract

**Files:**
- Create: `src/orchestrator/safety.rs`
- Modify: `src/gateway/mod.rs`
- Modify: `src/tools/traits.rs`
- Modify: `src/security/policy.rs`
- Modify: `src/security/audit.rs`
- Test: `src/orchestrator/safety.rs`

- [ ] **Step 1: Write the failing safety gate tests**

```rust
#[test]
fn tool_safety_gate_rejects_tool_not_present_in_tool_surface() {
    let decision = ToolSafetyReviewGate::default().review(
        "shell",
        &["read_file".to_string(), "web_fetch".to_string()],
    );

    assert_eq!(decision, ToolSafetyDecision::Reject);
}

#[test]
fn outbound_safety_gate_flags_private_memory_labels() {
    let decision = OutboundSafetyAuditGate::default().review(
        serde_json::json!({"text": "secret"}),
        &["user_private_long_term".to_string()],
    );

    assert_eq!(decision.labels, vec!["private-memory-redaction".to_string()]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test tool_safety_gate_rejects_tool_not_present_in_tool_surface --lib`
Expected: FAIL because the gate types do not exist yet.

- [ ] **Step 3: Implement the minimal safety gate layer**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSafetyDecision {
    Allow,
    Reject,
    Sandbox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundAuditDecision {
    pub allowed: bool,
    pub labels: Vec<String>,
}
```

- [ ] **Step 4: Route gateway tool execution and outbound delivery through the gates**

```rust
let tool_decision = tool_gate.review(tool_name, visible_tools);
match tool_decision {
    ToolSafetyDecision::Allow => run_tool(),
    ToolSafetyDecision::Sandbox => run_tool_in_isolation(),
    ToolSafetyDecision::Reject => return Err(anyhow::anyhow!("tool blocked by safety gate")),
}
```

- [ ] **Step 5: Run focused safety tests**

Run: `cargo test safety_gate --lib`
Expected: PASS and no regressions in gateway security tests.

- [ ] **Step 6: Commit**

```bash
git add src/orchestrator/safety.rs src/gateway/mod.rs src/tools/traits.rs src/security/policy.rs src/security/audit.rs
git commit -m "feat: add orchestrator safety gates"
```

### Task 6: Add Butler/User Host Supervision and Recovery Metadata

Implementation note on 2026-04-22:
- The first Task 6 landing stays at the orchestrator contract layer and does not yet rewire daemon or gateway startup.
- Added minimal orchestration types:
  - `src/orchestrator/butler.rs` -> `ButlerRuntimeHandle`
  - `src/orchestrator/host.rs` -> `UserHost`
  - `src/orchestrator/supervisor.rs` -> `ButlerDirectory`
  - `src/orchestrator/recovery.rs` -> `RecoveryCursor`
- This slice establishes:
  - one-active-butler-per-user registration semantics
  - explicit binding between user, butler, and host ids
  - session membership on a user host
  - recovery cursor tracking for descriptor version sets and the last turn id
- Remaining Task 6 work stays open:
  - integrate these contracts into daemon/gateway runtime startup and recovery flows
  - persist recovery cursors through session/backend state transitions
  - connect butler/host supervision to real multi-user runtime ownership rather than contract-only handles

**Files:**
- Create: `src/orchestrator/supervisor.rs`
- Create: `src/orchestrator/butler.rs`
- Create: `src/orchestrator/host.rs`
- Create: `src/orchestrator/recovery.rs`
- Modify: `src/daemon/mod.rs`
- Modify: `src/gateway/mod.rs`
- Modify: `src/channels/session_backend.rs`
- Test: `src/orchestrator/supervisor.rs`

- [ ] **Step 1: Write the failing supervision tests**

```rust
#[test]
fn butler_directory_enforces_one_active_butler_per_user() {
    let mut directory = ButlerDirectory::default();
    assert!(directory.register("user-a", "butler-1").is_ok());
    assert!(directory.register("user-a", "butler-2").is_err());
}

#[test]
fn recovery_cursor_captures_session_runtime_versions() {
    let cursor = RecoveryCursor {
        user_id: "user-a".into(),
        session_id: "session-a".into(),
        descriptor_version_set: vec!["cap-a@1.0.0".into()],
        last_turn_id: Some("turn-9".into()),
    };

    assert_eq!(cursor.session_id, "session-a");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test butler_directory_enforces_one_active_butler_per_user --lib`
Expected: FAIL because the supervision types do not exist yet.

- [ ] **Step 3: Implement the minimal supervision layer**

```rust
#[derive(Default)]
pub struct ButlerDirectory {
    active: std::collections::HashMap<String, String>,
}

impl ButlerDirectory {
    pub fn register(&mut self, user_id: &str, butler_id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.active.contains_key(user_id),
            "user already has an active butler"
        );
        self.active.insert(user_id.into(), butler_id.into());
        Ok(())
    }
}
```

- [ ] **Step 4: Add recovery metadata persistence hooks**

```rust
pub struct RecoveryCursor {
    pub user_id: String,
    pub session_id: String,
    pub descriptor_version_set: Vec<String>,
    pub last_turn_id: Option<String>,
}
```

- [ ] **Step 5: Run focused supervision tests**

Run: `cargo test butler_directory --lib`
Expected: PASS and no regressions in daemon/gateway startup tests.

- [ ] **Step 6: Commit**

```bash
git add src/orchestrator/supervisor.rs src/orchestrator/butler.rs src/orchestrator/host.rs src/orchestrator/recovery.rs src/daemon/mod.rs src/gateway/mod.rs src/channels/session_backend.rs
git commit -m "feat: add butler supervision and recovery metadata"
```

### Task 7: Add Documentation and Rollout Notes

**Files:**
- Create: `docs/architecture/2026-04-22-axi-orchestrator-migration-roadmap.md`
- Modify: `docs/architecture/zeroclaw_architecture.md`
- Modify: `docs/maintainers/repo-map.md`

- [ ] **Step 1: Document current-to-target architecture mapping**

```md
## Current ZeroClaw -> Target Axi Mapping

- `src/main.rs`, `src/gateway`, `src/channels`: Entry Layer
- `src/orchestrator/contracts.rs`, `src/orchestrator/message_bus.rs`: Message Bus Layer
- `src/auth`, `src/identity`: Auth & Identity Layer
- `src/orchestrator/butler.rs`, `src/orchestrator/host.rs`, `src/orchestrator/supervisor.rs`: Agent Orchestrator Layer
- `src/orchestrator/descriptors/*`: Capability Assembly Layer
- `src/orchestrator/context_builder.rs`, `src/agent/*`: Dialogue Execution Layer
- `src/orchestrator/safety.rs`, `src/security/*`: Safety Governance Layer
```

- [ ] **Step 2: Document rollout invariants**

```md
- Do not allow any model-visible context to bypass `ContextBuilder`.
- Do not permit user-private memory classes to enter public distribution paths.
- Keep session turn execution serial even when enqueueing or interrupting.
- Keep tool and outbound safety audits as distinct gates.
```

- [ ] **Step 3: Run doc linting and format checks**

Run: `cargo fmt --all -- --check`
Expected: PASS if any Rust docs/examples touched code formatting.

- [ ] **Step 4: Commit**

```bash
git add docs/architecture/2026-04-22-axi-orchestrator-migration-roadmap.md docs/architecture/zeroclaw_architecture.md docs/maintainers/repo-map.md
git commit -m "docs: add axi orchestrator migration roadmap"
```

## Self-Review

- Spec coverage:
  - Multi-user orchestrator, butler/user host, session serialization, enqueue policy, memory directives, context builder, descriptor assembly, safety gates, recovery, and market-facing hooks are each mapped to at least one task.
  - High-risk targets (`src/gateway/**`, `src/security/**`, `src/tools/**`) are isolated into dedicated tasks instead of mixed with low-risk docs work.
  - The plan intentionally stages the work so ZeroClaw can ship stable contracts before invasive runtime rewiring.

- Placeholder scan:
  - No `TODO`, `TBD`, or “implement later” steps remain.
  - Each task names concrete files, commands, and the minimum target code shape.

- Type consistency:
  - `InboundMessage`, `OutboundMessage`, `ContextBuildReason`, `SessionRuntimeBundle`, `CapabilityRuntimeDescriptor`, `ToolSafetyDecision`, and `RecoveryCursor` use the same names across all tasks.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-22-axi-orchestrator-refactor.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
