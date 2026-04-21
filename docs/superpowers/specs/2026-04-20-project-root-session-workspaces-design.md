# Project Root Session Workspaces Design

## Goal

Make runtime workspace scope explicit and session-bound so ZeroClaw can run against multiple independent projects without leaking file access, tool side effects, memory, or prompt context across directories.

## Confirmed Requirements

- `agent`, `daemon`, and `acp` are in scope.
- Session/project binding must use an explicit `project_root` parameter.
- `project_root` is required, not inferred from the current directory.
- Read and write capabilities must both be restricted to the selected project root.
- Runtime prompt context, tool execution, memory, cache, traces, and identity/personality file loading must all follow the selected project root.
- Independent sessions/projects must remain isolated from each other.

## Chosen Approach

Use the existing runtime `workspace_dir` flow as the single source of truth, but change how it is sourced:

- `agent` CLI gets a required `--project-root`.
- `daemon` CLI gets a required `--project-root`.
- ACP `session/new` gets a required `project_root` / `projectRoot`.
- At runtime, the chosen project root is canonicalized and applied to a cloned `Config`.
- The derived config then flows through the existing `SecurityPolicy`, tool registry, prompt builder, runtime adapters, memory setup, and observability initialization.

This avoids introducing a second parallel concept such as `runtime_workspace_dir`.

## Runtime Semantics

For a validated `project_root`:

- `config.workspace_dir = canonical(project_root)`
- `SecurityPolicy.workspace_dir = config.workspace_dir`
- shell/tool working directory = `config.workspace_dir`
- prompt workspace section = `config.workspace_dir`
- `AGENTS.md`, `SOUL.md`, `IDENTITY.md`, and skills are loaded from `config.workspace_dir`
- memory/traces/tool state continue to resolve from the runtime workspace flow that already uses `config.workspace_dir`

To keep config-adjacent runtime state project-local, the runtime config path is rewritten to:

- `project_root/.zeroclaw/config.toml`

This keeps config-derived state next to the selected project instead of the global user profile for these session-bound entrypoints.

## Entry Point Behavior

### agent

- `zeroclaw agent --project-root <PATH>` is required.
- Startup fails fast if the path is missing, does not exist, or is not a directory.
- The validated project root is applied before building tools, memory, and prompts.

### daemon

- `zeroclaw daemon --project-root <PATH>` is required.
- The daemon instance is bound to that project root for all gateway/channels/heartbeat/scheduler work performed by that process.
- Multiple projects are handled by running multiple daemon instances, each with its own explicit project root.

### acp

- `session/new` requires `project_root` (snake_case) and also accepts `projectRoot` (camelCase).
- ACP no longer falls back to `cwd`, `workspaceDir`, or global config workspace for new sessions.
- Each ACP session clones the base config, applies its own project root, and builds an agent from that session-local config.

## Validation Rules

`project_root` validation must:

- reject empty values
- require the path to exist
- require it to be a directory
- canonicalize the directory so later path checks are stable

## Compatibility / Non-Goals

- This change does not alter `onboard` or config discovery rules.
- This change does not introduce in-session project switching.
- This change does not redesign all runtime state layout beyond routing session-bound entrypoints through the explicit project root.

## Risks

- Some runtime state historically tied to the global config directory now becomes project-local for session-bound entrypoints.
- Existing scripts that launch `agent` or `daemon` without a project argument will need to add `--project-root`.
- ACP clients must send `project_root` during session creation.

## Test Plan

- CLI parse tests for required `--project-root` on `agent` and `daemon`
- unit tests for project-root validation and config application
- ACP tests for rejecting missing `project_root`
- ACP tests for applying a valid `project_root` to the session config and returned session metadata

