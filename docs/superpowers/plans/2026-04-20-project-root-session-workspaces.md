# Project Root Session Workspaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Require explicit project roots for session-bound entrypoints and route runtime workspace behavior through that directory.

**Architecture:** Reuse the existing `Config.workspace_dir` pipeline by applying a validated `project_root` runtime override before constructing agents, tools, memory, or daemon services. Add the same validation path to ACP session creation so each ACP session is isolated by its own project root.

**Tech Stack:** Rust, clap, tokio, serde_json, existing ZeroClaw config/agent runtime

---

## File Map

- Modify: `src/main.rs`
- Modify: `src/channels/acp_server.rs`
- Modify: `src/config/mod.rs`
- Create: `src/config/project_root.rs`
- Modify: `docs/reference/api/config-reference.md`

### Task 1: Add project-root runtime helper

**Files:**
- Create: `src/config/project_root.rs`
- Modify: `src/config/mod.rs`
- Test: `src/config/project_root.rs`

- [ ] Step 1: Write failing tests for project-root validation and config mutation
- [ ] Step 2: Run the helper tests and verify they fail
- [ ] Step 3: Implement canonicalization and config application
- [ ] Step 4: Run the helper tests and verify they pass

### Task 2: Require project roots for CLI entrypoints

**Files:**
- Modify: `src/main.rs`
- Test: `src/main.rs`

- [ ] Step 1: Write failing clap tests for `agent` and `daemon`
- [ ] Step 2: Run the targeted CLI tests and verify they fail
- [ ] Step 3: Add required `--project-root` and apply it before runtime startup
- [ ] Step 4: Run the targeted CLI tests and verify they pass

### Task 3: Enforce ACP session project roots

**Files:**
- Modify: `src/channels/acp_server.rs`
- Test: `src/channels/acp_server.rs`

- [ ] Step 1: Write failing ACP tests for missing and valid `project_root`
- [ ] Step 2: Run the targeted ACP tests and verify they fail
- [ ] Step 3: Apply the project-root helper to per-session config creation
- [ ] Step 4: Run the targeted ACP tests and verify they pass

### Task 4: Document the new runtime contract

**Files:**
- Modify: `docs/reference/api/config-reference.md`

- [ ] Step 1: Document `agent`/`daemon` `--project-root`
- [ ] Step 2: Document ACP `session/new project_root`
- [ ] Step 3: Document isolation behavior and validation rules

### Task 5: Verify impacted flows

**Files:**
- Test: `src/config/project_root.rs`
- Test: `src/main.rs`
- Test: `src/channels/acp_server.rs`

- [ ] Step 1: Run targeted unit tests for config helper, CLI parsing, and ACP session creation
- [ ] Step 2: Run repository-formatting and lint/test commands as far as the touched area allows
- [ ] Step 3: Review docs and code paths for consistency with the design
