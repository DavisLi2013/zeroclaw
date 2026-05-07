# Crates

The workspace is organized into three categories: **edge** crates handle user interaction and gateways, **core** crates implement agent execution and tools, and **shared** crates provide common utilities. Each crate has its own rustdoc — see [API (rustdoc)](../api.md).

## Shared Layer

### `zeroclaw-api`

The kernel ABI. Defines public traits that both edge and core depend on:

- `Provider` — LLM client interface with streaming capability flags
- `Channel` — inbound/outbound messaging surface
- `Tool` — agent-callable capabilities
- `Memory` — conversation memory backends
- `Observer` — observability/metrics
- `Peripheral` — hardware boards (GPIO, I2C, SPI)

The runtime depends only on these traits, not on concrete implementations. This is what makes provider/channel/tool additions a matter of implementing a trait rather than patching the core.

### `zeroclaw-config`

TOML schema and its validation. Handles:

- Autonomy level enum (`ReadOnly` / `Supervised` / `Full`)
- Encrypted secrets store (local key file)
- Workspace resolution (env vars, Homebrew paths, XDG, container detection)
- Schema versioning and migration

All user-facing config keys are documented in [Reference → Config](../reference/config.md), which is generated from this crate.

### `zeroclaw-infra`

Tracing, metrics, structured logging. All crates emit events via this layer.

### `zeroclaw-macros`

Derive macros for config schema, tool registration, and channel registration. Saves boilerplate across the workspace.

### `zeroclaw-tool-call-parser`

Model-side tool-call syntax parsing. Handles variations between providers:

- OpenAI-style `tool_calls` JSON
- Anthropic-style `<tool_use>` blocks
- Qwen/Ollama's function-call formats
- Native tool-call streaming deltas

## Edge Layer

Edge crates are used by `zeroclaw-edge.exe` to handle user interaction, messaging, and gateway services.

### `zeroclaw-gateway`

HTTP/WebSocket gateway. Exposes the runtime over:

- REST API (sessions, memory, status, cron management)
- WebSocket for streaming responses
- Web dashboard (static assets + auth)
- Webhook endpoints (inbound from channels that push)

Pairing is required by default; `[gateway.allow_public_bind = true]` enables binding to `0.0.0.0`.

### `zeroclaw-channels`

30+ messaging integrations. See [Channels → Overview](../channels/overview.md) for the catalogue.

All channels implement the `Channel` trait from `zeroclaw-api`. Each is feature-gated — a minimal build includes only the channels you compile in.

The `orchestrator/` submodule handles message streaming, draft updates, multi-message splits, and the ACP server.

### `zeroclaw-tui`

Terminal UI. Optional — compile with `--features tui`.

## Core Layer

Core crates are used by `zeroclaw-core.exe` to execute agent logic, manage tools, and interact with LLM providers.

### `zeroclaw-runtime`

The agent loop, security-policy enforcement, SOP engine, cron scheduler, onboarding wizard. Depends on other core crates.

Notable submodules:

- `agent/` — the main request/response loop, streaming, tool-call orchestration
- `security/` — policy types, sandbox detection, OTP, emergency stop
- `sop/` — Standard Operating Procedure engine (see [SOP → Overview](../sop/index.md))
- `onboard/` — the first-run wizard (`wizard.rs`)
- `memory/` — wraps `zeroclaw-memory` with runtime-level caching and consolidation schedules
- `service/` — systemd / launchctl / Windows Service integration

### `zeroclaw-providers`

All LLM client implementations plus the routing and fallback wrappers. See [Model Providers → Overview](../providers/overview.md) for the list.

Structure:

- `traits.rs` — re-exports from `zeroclaw-api` plus provider-internal helpers
- `anthropic.rs`, `openai.rs`, `ollama.rs`, … — one file per native provider
- `compatible.rs` — a single OpenAI-compatible implementation reused by 20+ providers (Groq, Mistral, xAI, Venice, etc.)
- `router.rs` — multi-provider router that routes by task hint
- `reliable.rs` — fallback-chain wrapper
- `streaming.rs` — SSE parsing, token estimation, tool-call deltas

### `zeroclaw-tools`

Callable tools the agent invokes. Not to be confused with CLI `zeroclaw` subcommands.

Includes: `browser`, `http`, `pdf_extract`, `web_search`, `shell`, `file_read`, `file_write`, `hardware_probe`, and more. See [Tools → Overview](../tools/overview.md).

Each tool is registered via factory and described to the model via Fluent-localised strings.

### `zeroclaw-memory`

Conversation memory and retrieval. SQLite is the default backend; PostgreSQL is available behind `--features memory-postgres` for multi-instance deployments that need a shared, concurrent-write store. Optional:

- Embedding backends (OpenAI, Ollama, local)
- Vector retrieval over stored conversations (pgvector when on PostgreSQL)
- Memory consolidation (summaries, fact extraction)

### `zeroclaw-plugins`

Dynamic plugin loader for out-of-process tool implementations. See [Developing → Plugin protocol](../developing/plugin-protocol.md).

### `zeroclaw-hardware`

Hardware abstraction — GPIO, I2C, SPI, USB. Platform-gated. See [Hardware → Overview](../hardware/index.md).

### `aardvark-sys`

Low-level bindings for Aardvark I2C/SPI adapter. Used by `zeroclaw-hardware`.

### `robot-kit`

Specialised hardware support for robot peripherals. Used by the `hardware` submodule.

## Feature flags

The microkernel roadmap (RFC #5574) defines a feature-flag taxonomy. The practical upshot for a user:

- `default` — a sensible core build
- `ci-all` — everything on, for CI
- `channel-<name>` — opt-in per channel (e.g. `channel-matrix`, `channel-discord`)
- `provider-<name>` — opt-in per provider
- `hardware` — enable hardware subsystem
- `tui` — terminal UI

Run `cargo metadata --format-version 1 | jq '.workspace_members'` or read the top-level `Cargo.toml` for the full list.
