# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **Shared instructions live in [`AGENTS.md`](./AGENTS.md).**
> This file contains only Claude Code-specific directives.

## Project Overview

ZeroClaw is a personal AI assistant runtime written in Rust. It's a single binary that connects to LLM providers (Anthropic, OpenAI, Ollama, ~20 others), communicates through 30+ channels (Discord, Telegram, Matrix, email, voice, webhooks, CLI), and acts through tools (shell, browser, HTTP, hardware, custom MCP servers). Everything runs locally with user-owned keys and data.

**Philosophy**: You own the agent. You own the data. You own the machine it runs on.

**Important**: Read and follow all instructions in `AGENTS.md` for project conventions, risk tiers, workflow rules, and anti-patterns.

## Essential Commands

### Build and Test

```bash
# Build (default features)
cargo build

# Build with all features (for CI)
cargo build --features ci-all --locked

# Build for release (optimized for size)
cargo build --release

# Fast release build (parallel codegen, requires 16GB+ RAM)
cargo build --profile release-fast

# Run all tests (unit + component + integration + system)
cargo test --locked

# Run specific test levels
cargo test --lib                    # unit tests only
cargo test --test component         # component tests
cargo test --test integration       # integration tests
cargo test --test system            # system tests
cargo test --test live -- --ignored # live tests (requires API keys)

# Run a single test by name
cargo test <test_name>
```

### Quality Gates

```bash
# Format check
cargo fmt --all -- --check

# Lint (correctness only)
cargo clippy --locked --all-targets -- -D clippy::correctness

# Lint (strict - all warnings)
cargo clippy --locked --all-targets -- -D warnings

# Full quality gate (format + lint)
./scripts/ci/rust_quality_gate.sh

# Full CI battery (lint + test + build + security + docker)
./dev/ci.sh all
```

### Local Development

```bash
# Enable pre-push hook (runs fmt, clippy, tests before push)
git config core.hooksPath .githooks

# Run the agent interactively
cargo run -- agent

# Run onboarding wizard
cargo run -- onboard

# Install as system service
cargo run -- service install
cargo run -- service start
```

## Architecture

ZeroClaw is a layered Rust workspace with trait-based extension points.

### Workspace Structure

```
zeroclaw/
├── src/                          # Main binary (CLI entrypoint)
├── crates/
│   ├── zeroclaw-api/            # Public traits (Provider, Channel, Tool, Memory)
│   ├── zeroclaw-runtime/        # Agent loop, security, SOP, cron, onboarding
│   ├── zeroclaw-config/         # TOML schema, secrets, autonomy levels
│   ├── zeroclaw-providers/      # LLM clients (Anthropic, OpenAI, Ollama, ...)
│   ├── zeroclaw-channels/       # 30+ messaging integrations
│   ├── zeroclaw-tools/          # Tool implementations (shell, browser, HTTP, ...)
│   ├── zeroclaw-memory/         # Conversation memory, embeddings, SQLite
│   ├── zeroclaw-gateway/        # REST/WebSocket gateway + web dashboard
│   ├── zeroclaw-hardware/       # GPIO, I2C, SPI, USB hardware abstraction
│   ├── zeroclaw-tui/            # Terminal UI
│   ├── zeroclaw-plugins/        # WASM plugin system
│   ├── zeroclaw-tool-call-parser/ # Tool call parsing/normalization
│   ├── zeroclaw-infra/          # Tracing, metrics, logging
│   └── zeroclaw-macros/         # Derive macros
├── docs/book/src/               # Documentation (mdBook)
├── tests/
│   ├── component/               # Component tests (one subsystem)
│   ├── integration/             # Integration tests (multiple components)
│   ├── system/                  # System tests (full request/response)
│   ├── live/                    # Live tests (real APIs, #[ignore]'d)
│   ├── manual/                  # Human-driven test scripts
│   └── support/                 # Shared test infrastructure
└── .claude/skills/              # AI coding assistant skills
```

### Core Traits (Extension Points)

All extension points are defined in `crates/zeroclaw-api/src/`:

- **`Provider`** — LLM client interface with streaming capability flags
- **`Channel`** — inbound/outbound messaging surface
- **`Tool`** — agent-callable capabilities
- **`Memory`** — conversation memory backends
- **`Observer`** — observability/metrics
- **`Peripheral`** — hardware boards (GPIO, I2C, SPI)

To add a new provider/channel/tool: implement the trait and register in the factory module.

### Request Lifecycle

```
User → Channel → Runtime → Security Policy → Provider (LLM)
                    ↓
                  Tools ← Security Approval
                    ↓
                Provider (with tool results)
                    ↓
                Channel → User
```

Full detail: `docs/book/src/architecture/request-lifecycle.md`

## Testing Taxonomy

ZeroClaw uses a five-level testing hierarchy:

| Level | Boundary | Location | Command |
|-------|----------|----------|---------|
| **Unit** | Single function/struct | `#[cfg(test)]` in `src/` | `cargo test --lib` |
| **Component** | One subsystem | `tests/component/` | `cargo test --test component` |
| **Integration** | Multiple components | `tests/integration/` | `cargo test --test integration` |
| **System** | Full request/response | `tests/system/` | `cargo test --test system` |
| **Live** | Real external APIs | `tests/live/` | `cargo test --test live -- --ignored` |

**Shared test infrastructure**: `tests/support/` contains `MockProvider`, `EchoTool`, `TestChannel`, `build_agent()`, and JSON trace fixtures.

**When writing tests**:
- Use the lowest level that proves what you need to prove
- Component tests for single subsystem isolation
- Integration tests for multiple components wired together
- System tests for full end-to-end message flow
- Live tests only when real API keys are required (mark with `#[ignore]`)

## Feature Flags

ZeroClaw uses Cargo features for modular compilation:

- **`default`** — sensible core build (agent-runtime, gateway, tui-onboarding, observability-prometheus)
- **`agent-runtime`** — full agent runtime (without this, you get kernel only: config + providers + memory + CLI chat)
- **`ci-all`** — everything enabled (for CI)
- **`channel-<name>`** — opt-in per channel (e.g., `channel-discord`, `channel-matrix`)
- **`gateway`** — HTTP/WebSocket gateway + web dashboard
- **`tui-onboarding`** — terminal UI onboarding wizard
- **`hardware`** — hardware subsystem (GPIO, I2C, SPI)
- **`browser-native`** — native browser automation (Fantoccini)
- **`plugins-wasm`** — WASM plugin system
- **`sandbox-landlock`** / **`sandbox-bubblewrap`** — OS-level sandboxing
- **`observability-prometheus`** / **`observability-otel`** — metrics backends

Build with specific features:
```bash
cargo build --features channel-discord,channel-telegram
cargo build --no-default-features --features agent-runtime
```

## Code Conventions

### Rust Style

- **Edition**: Rust 2024 (MSRV: 1.87)
- **Formatting**: `cargo fmt --all` (enforced in CI)
- **Linting**: Clippy with `#![warn(clippy::all, clippy::pedantic)]` at crate root
- **Error handling**: Propagate errors with `?`; avoid `unwrap()`/`expect()` in production paths
- **Async runtime**: Tokio with minimal features
- **Logging**: Use `tracing` spans/events, not `println!`

### Localization

- **User-facing output** (CLI messages, tool descriptions, onboarding prompts) must use `fl!()` / Fluent strings
- **Log messages**, `tracing::` spans/events, and panic messages stay in English
- Never use bare string literals for user-facing text

### Security

- Never commit secrets, API keys, tokens, or personal data
- Use `.env` for local development (git-ignored)
- Pre-commit hook runs `gitleaks` if installed
- High-risk changes (security policy, access control, gateway, tools) require extra scrutiny

## Workflow Rules

1. **Read before write** — inspect existing code, factory wiring, and tests before editing
2. **One concern per PR** — avoid mixing feature + refactor + infra
3. **Implement minimal patch** — no speculative abstractions or unused config keys
4. **Branch from `master`** — all PRs target `master` (not `main`)
5. **Use conventional commits** — `feat:`, `fix:`, `docs:`, `chore:`, etc.
6. **Complete PR template** — `.github/pull_request_template.md` is mandatory
7. **Validation evidence required** — include actual command output, not "CI will check"
8. **Small PRs preferred** — aim for `size: XS/S/M`

### Pre-Push Checklist

```bash
# Format
cargo fmt --all

# Lint
cargo clippy --locked --all-targets -- -D warnings

# Test
cargo test --locked

# Or run full quality gate
./scripts/ci/rust_quality_gate.sh
```

## Risk Tiers

- **Low risk**: docs/chore/tests-only changes
- **Medium risk**: most `crates/*/src/**` behavior changes without boundary/security impact
- **High risk**: `crates/zeroclaw-runtime/src/security/`, `crates/zeroclaw-gateway/`, `crates/zeroclaw-tools/`, `.github/workflows/`, access-control boundaries

When uncertain, classify as higher risk and ask for confirmation before proceeding.

## Anti-Patterns

- ❌ Do not add heavy dependencies for minor convenience
- ❌ Do not silently weaken security policy or access constraints
- ❌ Do not add speculative config/feature flags "just in case"
- ❌ Do not mix massive formatting changes with functional changes
- ❌ Do not modify unrelated modules "while here"
- ❌ Do not bypass failing checks without explicit explanation
- ❌ Do not suppress unused production code with `#[allow(dead_code)]` — delete it or wire it
- ❌ Do not leave `unwrap()`/`expect()` in production paths — propagate errors
- ❌ Do not include personal identity or sensitive information in test data, examples, or commits

## Documentation

- **User docs**: `docs/book/src/` (mdBook format)
- **API docs**: Rustdoc in each crate (`cargo doc --open`)
- **Architecture**: `docs/book/src/architecture/overview.md`
- **Contributing**: `docs/book/src/contributing/how-to.md`
- **Security**: `docs/book/src/security/overview.md`

Build docs locally:
```bash
cd docs/book
mdbook serve
```

## Skills

AI coding assistant skills live in `.claude/skills/`. Key skills:

- **`github-pr-review-session`** — PR review co-pilot (trigger: `review 1234`)
- **`changelog-generation`** — Generate changelogs (trigger: `generate changelog`)
- **`github-issue-triage`** — Issue triage and backlog management (trigger: `triage issues`)
- **`zeroclaw`** — Operational guide for ZeroClaw CLI/API (trigger: `check agent status`)

## Additional Resources

- **README**: `README.md` — quick start, install, architecture diagram
- **AGENTS.md**: `AGENTS.md` — cross-tool agent instructions (shared with all AI assistants)
- **CONTRIBUTING**: `CONTRIBUTING.md` — contribution flow, PR rules, testing
- **SECURITY**: `SECURITY.md` — security policy, responsible disclosure
- **Architecture deep dive**: `docs/book/src/architecture/crates.md`
- **Request lifecycle**: `docs/book/src/architecture/request-lifecycle.md`

## Quick Reference

```bash
# Build
cargo build                                    # default features
cargo build --features ci-all --locked         # all features

# Test
cargo test --locked                            # all tests
cargo test --lib                               # unit tests only
cargo test --test component                    # component tests

# Quality
cargo fmt --all -- --check                     # format check
cargo clippy --locked --all-targets -- -D warnings  # lint
./scripts/ci/rust_quality_gate.sh              # full quality gate

# CI
./dev/ci.sh all                                # full CI battery
./dev/ci.sh lint                               # lint only
./dev/ci.sh test                               # test only

# Run
cargo run -- agent                             # interactive chat
cargo run -- onboard                           # onboarding wizard
cargo run -- service install                   # install as service
```

## Notes

- **Default branch**: `master` (not `main`)
- **MSRV**: Rust 1.87
- **License**: Dual MIT OR Apache 2.0
- **Platform**: Linux, macOS, Windows (cross-platform)
- **Config**: `~/.zeroclaw/config.toml`
- **Secrets**: Encrypted by default with key at `~/.zeroclaw/.secret_key`
