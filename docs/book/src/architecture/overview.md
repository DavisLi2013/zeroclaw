# Architecture Overview

ZeroClaw is a layered Rust workspace. At the top is the agent runtime; below it are pluggable providers, channels, tools, and memory; supporting crates handle config, sandboxing, and hardware.

## High-level shape

```mermaid
flowchart TB
    subgraph External["External world"]
        UI["CLI / chat platforms / gateway clients / ACP IDEs"]
        LLM["LLM providers<br/>Anthropic · OpenAI · Ollama · ..."]
        FS["Filesystem · shell · network"]
    end

    subgraph Edges["Edge crates — talk to the outside"]
        CH["zeroclaw-channels<br/>30+ messaging integrations"]
        GW["zeroclaw-gateway<br/>REST · WebSocket · dashboard"]
        PR["zeroclaw-providers<br/>LLM clients · fallback · routing"]
        TL["zeroclaw-tools<br/>browser · HTTP · PDF · hardware"]
    end

    subgraph Core["Core"]
        RT["zeroclaw-runtime<br/>agent loop · security · SOP · cron · onboarding"]
        MEM["zeroclaw-memory<br/>SQLite · embeddings · consolidation"]
        CFG["zeroclaw-config<br/>schema · autonomy · secrets"]
    end

    UI --> CH
    UI --> GW
    CH --> RT
    GW --> RT
    RT --> PR
    RT --> TL
    RT --> MEM
    RT --> CFG
    PR --> LLM
    TL --> FS
```

## Crates in scope

| Crate | Layer | Role |
|---|---|---|
| **Shared** | | |
| `zeroclaw-api` | Shared | Public traits — `Provider`, `Channel`, `Tool`, `Memory`, `Observer`, `Peripheral`. The kernel ABI |
| `zeroclaw-config` | Shared | TOML schema, secrets encryption, autonomy levels, workspace resolution |
| `zeroclaw-infra` | Shared | Tracing, metrics, structured logging |
| `zeroclaw-macros` | Shared | Derive macros for config, tool registration |
| `zeroclaw-tool-call-parser` | Shared | Model-side tool-call syntax parsing and normalisation |
| **Edge** | | |
| `zeroclaw-gateway` | Edge | HTTP / WebSocket gateway, web dashboard, webhook ingress |
| `zeroclaw-channels` | Edge | 30+ messaging integrations (Discord, Slack, Telegram, Matrix, email, voice, …) |
| `zeroclaw-tui` | Edge | Terminal UI |
| **Core** | | |
| `zeroclaw-runtime` | Core | Agent loop, security policy enforcement, SOP engine, cron scheduler, onboarding wizard |
| `zeroclaw-providers` | Core | All LLM client impls (Anthropic, OpenAI, Ollama, …) plus the router and fallback wrapper |
| `zeroclaw-tools` | Core | Callable tool implementations the agent invokes (browser, HTTP, PDF, hardware probes) |
| `zeroclaw-memory` | Core | Conversation memory, embeddings, vector retrieval |
| `zeroclaw-plugins` | Core | Dynamic plugin loading |
| `zeroclaw-hardware` | Core | Hardware abstraction layer (GPIO, I2C, SPI, USB) |
| `aardvark-sys` | Core | Low-level bindings for Aardvark I2C/SPI adapter |
| `robot-kit` | Core | Specialised hardware support for robot peripherals |

The microkernel roadmap (RFC #5574) is actively splitting `zeroclaw-runtime` further — the kernel layer will shrink to the agent loop and policy enforcement, with everything else moving behind feature flags.

## Request lifecycle (short)

```mermaid
sequenceDiagram
    participant U as User
    participant CH as Channel
    participant RT as Runtime
    participant SEC as Security
    participant PR as Provider
    participant TL as Tool

    U->>CH: message / DM / webhook
    CH->>RT: deliver_message(ctx)
    RT->>PR: chat(messages, tools)
    PR-->>RT: stream: text · tool_call
    RT->>SEC: validate(tool_call)
    SEC-->>RT: approved / blocked
    RT->>TL: invoke(args)
    TL-->>RT: result
    RT->>PR: chat(..., + tool_result)
    PR-->>RT: stream: text (final)
    RT-->>CH: reply (partial / final)
    CH-->>U: message
```

Full detail: [Request lifecycle](./request-lifecycle.md).

## Extension points

Three trait-based extension points live in `zeroclaw-api` (shared layer):

- **`Provider`** — implement for a new LLM endpoint. See [Custom providers](../providers/custom.md).
- **`Channel`** — implement for a new messaging platform. Inbound and outbound are separate hooks.
- **`Tool`** — implement for a new capability the agent can invoke. See [Developing → Plugin protocol](../developing/plugin-protocol.md).

All three are registered at startup via factory functions; the kernel doesn't know the concrete types. Compile-time feature flags decide which implementations ship in a given binary.

## Where to read next

- [Crates](./crates.md) — per-crate deep dive
- [Request lifecycle](./request-lifecycle.md) — streaming, tool calls, approvals
- [Model Providers → Overview](../providers/overview.md)
- [Security → Overview](../security/overview.md)
