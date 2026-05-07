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
- **Error handling**: Propagate errors with `?`; avoid `unwrap()`/`expect()` in production paths (document the invariant if panic is truly impossible)
- **Async runtime**: Tokio with minimal features
- **Logging**: Use `tracing` spans/events, not `println!`
- **Dead code**: Do not suppress unused production code with `#[allow(dead_code)]` — delete it, wire it into behavior, or track a follow-up issue

### Localization (Critical)

- **User-facing output** (CLI messages, tool descriptions, onboarding prompts) **must** use `fl!()` / Fluent strings — never bare string literals
- **Log messages**, `tracing::` spans/events, and panic messages stay in English with stable `error_key` fields
- Panics and `tracing::` lines are never translated
- The Wiki and internal developer docs are English only

### Security

- Never commit secrets, API keys, tokens, or personal data
- Use `.env` for local development (git-ignored, copy from `.env.example`)
- Pre-commit hook runs `gitleaks` if installed
- High-risk changes (security policy, access control, gateway, tools) require extra scrutiny
- Never commit real names, emails, or PII in test data, examples, docs, or commits (see `docs/book/src/contributing/privacy.md`)

## Workflow Rules

1. **Read before write** — inspect existing code, factory wiring, and tests before editing
2. **One concern per PR** — avoid mixing feature + refactor + infra
3. **Implement minimal patch** — no speculative abstractions or unused config keys
4. **Branch from `master`** — all PRs target `master` (not `main`)
5. **Use conventional commits** — `feat:`, `fix:`, `docs:`, `chore:`, etc.
6. **Complete PR template** — `.github/pull_request_template.md` is mandatory
7. **Validation evidence required** — include actual command output, not "CI will check"
8. **Small PRs preferred** — aim for `size: XS/S/M`
9. **Privacy discipline is a merge gate** — never commit real names, emails, tokens, or PII

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

### Pre-Push Hook (Recommended)

```bash
# Enable the pre-push hook (runs fmt, clippy, tests before every push)
git config core.hooksPath .githooks

# Skip hook for rapid iteration (CI still runs checks)
git push --no-verify
```

### Pre-Push Hook Opt-ins

Set environment variables to enable additional checks for one push:

| Variable | Effect |
|----------|--------|
| `ZEROCLAW_STRICT_LINT=1` | Strict lint pass on the full repo |
| `ZEROCLAW_STRICT_DELTA_LINT=1` | Strict lint on changed Rust lines only |
| `ZEROCLAW_DOCS_LINT=1` | Markdown gate on changed lines |
| `ZEROCLAW_DOCS_LINKS=1` | Link check on added links only |

## Stability Tiers

Every workspace crate carries a stability tier per the Microkernel Architecture RFC:

| Crate | Tier | Notes |
|-------|------|-------|
| `zeroclaw-api` | Experimental | Stable at v1.0.0 (formal milestone) |
| `zeroclaw-config` | Beta | Stable at v0.8.0 |
| `zeroclaw-providers` | Beta | — |
| `zeroclaw-memory` | Beta | — |
| `zeroclaw-infra` | Beta | — |
| `zeroclaw-tool-call-parser` | Beta | Stable at v0.8.0 |
| `zeroclaw-channels` | Experimental | Plugin migration at v1.0.0 |
| `zeroclaw-tools` | Experimental | Plugin migration at v1.0.0 |
| `zeroclaw-runtime` | Experimental | Agent runtime (agent loop, security, cron, SOP, skills, observability) |
| `zeroclaw-gateway` | Experimental | Separate binary at v0.9.0 |
| `zeroclaw-tui` | Experimental | TUI onboarding wizard |
| `zeroclaw-plugins` | Experimental | WASM plugin system — foundation for v1.0.0 plugin ecosystem |
| `zeroclaw-hardware` | Experimental | USB discovery, peripherals, serial |
| `zeroclaw-macros` | Beta | Tightly coupled to config schema |

**Tiers**: 
- **Stable** = covered by breaking-change policy
- **Beta** = breaking changes permitted in MINOR with changelog notes
- **Experimental** = no stability guarantee

Tiers are promoted, never demoted, through deliberate team decision.

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

- **`github-pr-review-session`** — PR review co-pilot; posts reviews as the active `gh` account holder using RFC feedback taxonomy (🔴/🟡/✅/🔵/🟢). Trigger: `review 1234`, `re-review 1234`, `go through the queue`
- **`changelog-generation`** — Generate changelogs between stable tags, resolve contributors via GraphQL. Trigger: `generate changelog`, `release notes for v0.7.x`
- **`github-issue-triage`** — Issue triage and lifecycle management; manages backlog, labels, stale policies. Trigger: `triage issues`, `sweep issues`, `handle issue #N`
- **`github-issue`** — Interactively file structured GitHub issues using repo templates. Trigger: `file issue`, `report bug`, `feature request`
- **`github-pr`** — Open or update GitHub PRs, handle validation evidence, manage PR descriptions. Trigger: `open PR`, `update PR`, `submit for review`
- **`squash-merge`** — Perform conventional squash-merges into master with preserved commit history. Trigger: `squash-merge #123`, `land #789`
- **`zeroclaw`** — Operational guide for ZeroClaw CLI/API (send messages, manage memory/cron, check status, configure channels). Trigger: `check agent status`, `manage memory`, `zeroclaw config`
- **`skill-creator`** — Framework for creating, testing, evaluating, and optimizing new AI skills. Trigger: `create skill`, `improve skill`, `run skill evals`

See `AGENTS.md` for full skill descriptions and trigger patterns.

## Additional Resources

- **AGENTS.md**: `AGENTS.md` — **Read this first**. Cross-tool agent instructions with commands, project snapshot, risk tiers, workflow rules, anti-patterns, and skills
- **README**: `README.md` — quick start, install, architecture diagram
- **CONTRIBUTING**: `CONTRIBUTING.md` — contribution flow, PR rules, testing, secret management
- **SECURITY**: `SECURITY.md` — security policy, responsible disclosure
- **Architecture deep dive**: `docs/book/src/architecture/crates.md`
- **Request lifecycle**: `docs/book/src/architecture/request-lifecycle.md`
- **Privacy & PII rules**: `docs/book/src/contributing/privacy.md` — **mandatory reading**
- **Testing taxonomy**: `docs/book/src/contributing/testing.md`
- **PR review protocol**: `docs/book/src/contributing/pr-review-protocol.md`

## Dev-Operational Contracts

Protected files consumed by AI coding skills and development tooling. Do not move or delete without updating all consuming skills and AGENTS.md:

| Protected file | Consuming skill / tool |
|---|---|
| `docs/book/src/contributing/pr-review-protocol.md` | `github-pr-review-session` — review protocol |
| `docs/book/src/maintainers/changelog-generation.md` | `changelog-generation` — release procedure |
| `docs/book/src/maintainers/reviewer-playbook.md` | `github-issue-triage` — triage governance |
| `docs/book/src/maintainers/pr-workflow.md` | `github-issue-triage` — triage discipline |
| `docs/book/src/contributing/privacy.md` | `github-issue-triage`, PR template — privacy rules |
| `docs/book/src/foundations/fnd-00*.md` | `github-pr-review-session` — RFC reference data |

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
