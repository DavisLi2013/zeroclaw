# Project-Root Sessions

ZeroClaw can run against multiple independent projects as long as each session or runtime is created with an explicit `project_root`.

This applies to:

- `zeroclaw agent`
- `zeroclaw daemon`
- ACP `session/new`

## Why This Exists

Without an explicit project root, workspace-scoped behavior depends on global config discovery and can drift across sessions. Requiring `project_root` makes the runtime boundary explicit and repeatable.

For a validated `project_root`, ZeroClaw binds runtime workspace behavior to that directory:

- file reads and writes
- shell working directory
- `AGENTS.md`, `SOUL.md`, `IDENTITY.md`, and skill loading
- prompt `Workspace` context
- memory and other workspace-scoped runtime state

## Validation Rules

`project_root` must:

- be present
- not be empty
- already exist
- be a directory

ZeroClaw canonicalizes the path before using it. This keeps path comparisons stable for security-policy checks and ACP session metadata.

## CLI Usage

## `zeroclaw agent`

`--project-root` is required.

```bash
zeroclaw agent --project-root /path/to/project
zeroclaw agent --project-root /path/to/project -m "summarize recent changes"
```

Behavior:

- the session workspace becomes `/path/to/project`
- file tools can only operate inside that project root unless policy allowlists say otherwise
- shell commands run with that directory as the working directory

## `zeroclaw daemon`

`--project-root` is required.

```bash
zeroclaw daemon --project-root /path/to/project
zeroclaw daemon --project-root /path/to/project --host 127.0.0.1 --port 42617
```

A daemon instance is bound to one project root. To run multiple projects at the same time, start multiple daemon processes with different `--project-root` values.

Example:

```bash
zeroclaw daemon --project-root /srv/app-a --port 42617
zeroclaw daemon --project-root /srv/app-b --port 42618
```

## ACP Usage

ACP does not infer workspace from `cwd` anymore for session creation. `session/new` must send `project_root`.

Accepted field names:

- `project_root`
- `projectRoot`

Example request:

```json
{"jsonrpc":"2.0","id":1,"method":"session/new","params":{"project_root":"D:/workspace/app-a"}}
```

Example response:

```json
{"jsonrpc":"2.0","result":{"sessionId":"...","workspaceDir":"D:\\workspace\\app-a"},"id":1}
```

If `project_root` is missing or invalid, ACP returns `INVALID_PARAMS`.

## Isolation Model

Isolation is per session/process:

- one `agent` invocation = one project root
- one `daemon` process = one project root
- one ACP session = one project root

There is no in-session project switching in this model.

## Notes

- Config discovery still happens normally at startup.
- For session-bound entrypoints, the selected `project_root` becomes the runtime workspace after validation.
- Session-bound config state is redirected to `project_root/.zeroclaw/config.toml`.

