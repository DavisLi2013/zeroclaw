# Proposal: ZeroClaw to Axi Orchestrator Migration Roadmap

## Status

This is a proposal and migration roadmap, not the current runtime contract.

## Goal

Map the proposed axi multi-user orchestrator architecture onto the current ZeroClaw codebase without destabilizing the existing single-user `agent` / `gateway` flows during the transition.

## Current Baseline

ZeroClaw already has several pieces that the target architecture needs, but they are spread across runtime layers and still assembled ad hoc:

- Entry and channel surfaces already exist in [src/main.rs](/D:/workspace/axi-research/zeroclaw/src/main.rs:1), [src/gateway/mod.rs](/D:/workspace/axi-research/zeroclaw/src/gateway/mod.rs:1), and [src/channels/session_backend.rs](/D:/workspace/axi-research/zeroclaw/src/channels/session_backend.rs:1).
- Session serialization already exists as a per-session semaphore queue in [src/gateway/session_queue.rs](/D:/workspace/axi-research/zeroclaw/src/gateway/session_queue.rs:1).
- Session persistence and basic session state tracking already exist in [src/channels/session_sqlite.rs](/D:/workspace/axi-research/zeroclaw/src/channels/session_sqlite.rs:1).
- Prompt assembly, memory loading, and tool registration are still assembled directly inside [src/agent/agent.rs](/D:/workspace/axi-research/zeroclaw/src/agent/agent.rs:1) and [src/agent/prompt.rs](/D:/workspace/axi-research/zeroclaw/src/agent/prompt.rs:1).
- Hook lifecycle interception already exists in [src/hooks/traits.rs](/D:/workspace/axi-research/zeroclaw/src/hooks/traits.rs:1), but it is not yet aligned to a `ContextBuilder`-centric contract.
- Skills and plugins are governed separately in [src/skills/mod.rs](/D:/workspace/axi-research/zeroclaw/src/skills/mod.rs:1) and [src/plugins/mod.rs](/D:/workspace/axi-research/zeroclaw/src/plugins/mod.rs:1), which is the opposite of the target descriptor-unification model.
- Memory has a stable trait surface in [src/memory/traits.rs](/D:/workspace/axi-research/zeroclaw/src/memory/traits.rs:1), but not the four-class governance model from the target architecture.

## Target Mapping

The target axi architecture maps onto ZeroClaw in seven layers plus two gates.

### Layer 1: Entry Layer

Current module targets:

- `src/main.rs`
- `src/gateway/*`
- `src/channels/*`

Migration direction:

- Keep `CLI` and HTTP/OpenAI-compatible entry surfaces.
- Normalize all ingress into an orchestrator-owned `InboundMessage` contract instead of handing raw request structures directly to `Agent::from_config`.

### Layer 2: Message Bus Layer

Current gap:

- No repo-wide internal `MessageBus` abstraction exists yet.

Migration direction:

- Add `src/orchestrator/message_bus.rs`.
- Start with in-process bounded queues and explicit `InboundMessageQueue` / `OutboundMessageQueue` contracts.
- Route gateway websocket/http session work through bus messages instead of directly entering the current agent loop.

### Layer 3: Auth & Identity Layer

Current module targets:

- `src/auth/*`
- `src/identity.rs`
- gateway pairing / auth helpers in `src/gateway/*`

Migration direction:

- Resolve a stable `user_id` before session execution starts.
- Move pairing/auth/session binding output into an authenticated orchestrator message contract.

### Layer 4: Agent Orchestrator Layer

Current gap:

- No `Butler Directory`, `Butler Runtime`, `User Host`, or orchestrator supervisor boundary exists.

Migration direction:

- Add `src/orchestrator/supervisor.rs`, `src/orchestrator/butler.rs`, `src/orchestrator/host.rs`, and `src/orchestrator/session.rs`.
- Keep one active butler per stable user.
- Treat the current `Agent` as a lower execution primitive under the new orchestrator, not as the top-level runtime.

### Layer 5: Capability Assembly Layer

Current module targets:

- `src/skills/*`
- `src/plugins/*`
- `src/tools/*`
- `src/hooks/*`

Migration direction:

- Introduce `CapabilityRuntimeDescriptor` and projection types in `src/orchestrator/descriptors/*`.
- Stop treating skills, plugins, MCP servers, hooks, and commands as separate packaging systems.
- Project them into `skill_catalog`, `tool_surface`, `agent_catalog`, and `capability_pipeline` snapshots per session.

### Layer 6: Agent Dialogue Execution Layer

Current module targets:

- `src/agent/agent.rs`
- `src/agent/prompt.rs`
- `src/agent/memory_loader.rs`

Migration direction:

- Extract a unified `ContextBuilder` into `src/orchestrator/context_builder.rs`.
- Treat current prompt-building, memory loading, skill loading, tool result reinjection, and agent result reinjection as context source builders.
- Make `Agent` consume a finalized `ContextPackage` instead of assembling model-visible state itself.

### Layer 7: Safety Governance & Egress Layer

Current module targets:

- `src/security/*`
- tool execution checks in `src/tools/*`
- gateway response handling in `src/gateway/*`

Migration direction:

- Split safety into `Tool Safety Review Gate` and `Outbound Safety Audit Gate`.
- Keep tool execution approval and outbound response audit distinct.
- Make both gates explicit orchestrator-layer components instead of distributed checks hidden inside handlers.

## Non-Negotiable Invariants

- No component other than `ContextBuilder` may write model-visible context.
- Session turns remain serial even when supporting `enqueue` and `interrupt_and_enqueue`.
- User-private memory classes never enter public distribution paths.
- Market-facing hooks stay on a small, stable whitelist; internal lifecycle phases remain internal.
- Tool gating and outbound gating remain distinct audit boundaries.

## Four-Phase Migration

### Phase 1: Stable Contracts and Session Invariants

Ship first:

- `InboundMessage`
- `OutboundMessage`
- `EnqueuePolicy`
- `MemoryLoadingDirectives`
- `ContextBuildReason`
- `SessionRuntimeBundle`
- expanded session state with queue depth and cancellation intent

Why first:

- These types let the rest of the runtime converge on the same vocabulary before invasive rewiring starts.

### Phase 2: ContextBuilder Extraction

Ship next:

- `ContextItem`
- `ContextBuilder`
- builder contributions from system config, policy, transcript, memory, skills, tool results, and agent results
- hook outputs converted to declarative context contributions

Why second:

- The target architecture makes `ContextBuilder` the central invariant. Until it exists, descriptor unification and memory governance cannot be enforced consistently.

### Phase 3: Descriptor-Driven Assembly

Ship next:

- `CapabilityRuntimeDescriptor`
- descriptor manifest validation
- projection into skill/tool/agent/hook/runtime snapshots
- role-completeness validation for `manager` / `executor` / `supervisor`

Why third:

- Skills, hooks, plugins, MCP, and commands can only be governed together once they share a stable package model.

### Phase 4: Butler/User Host Supervision and Recovery

Ship last:

- `ButlerDirectory`
- `UserHost`
- `OrchestratorSupervisor`
- recovery cursors and version-set replay
- session monitor injection points

Why last:

- This is the highest-risk behavior change and should land only after contracts, snapshots, and gates already exist.

## First Execution Slice

The first code slice should stay low risk and additive:

1. Add `src/orchestrator/contracts.rs`.
2. Export the new module from [src/lib.rs](/D:/workspace/axi-research/zeroclaw/src/lib.rs:1).
3. Add tests for serialization, enqueue policy naming, and memory loading directives.
4. Do not change the existing gateway/agent execution path yet.

This slice gives later phases a stable contract surface without destabilizing the current runtime.

## Immediate File Targets

- `src/orchestrator/mod.rs`
- `src/orchestrator/contracts.rs`
- `src/orchestrator/session.rs`
- `src/orchestrator/context_item.rs`
- `src/orchestrator/context_builder.rs`
- `src/orchestrator/descriptors/*`
- `src/orchestrator/safety.rs`
- `src/orchestrator/butler.rs`
- `src/orchestrator/host.rs`
- `src/orchestrator/supervisor.rs`
- `src/orchestrator/recovery.rs`

## Explicit Non-Goals for the First Slice

- No container-per-user runtime yet.
- No bash or general shell enablement changes.
- No dynamic code-generation-and-execute path.
- No market rollout changes yet.
- No invasive gateway rewiring before the orchestration contracts compile and test cleanly.
