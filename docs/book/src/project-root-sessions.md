# Project-Root Sessions

ZeroClaw can run against multiple independent projects by binding each session or runtime to a project root.

This applies to:

- `zeroclaw agent`
- `zeroclaw daemon`
- ACP `session/new` with `project_root` / `projectRoot`

## Why This Exists

Without an explicit project root, workspace-scoped behavior depends on global config discovery and can drift across sessions. Passing `project_root` makes the runtime boundary explicit and repeatable.

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

ACP accepts `project_root` for project-bound session creation. This is the preferred form for editor and IDE clients because it binds both the file/shell boundary and workspace-scoped runtime state to the project.

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

If `project_root` is invalid, ACP returns `INVALID_PARAMS`.

Compatibility fields are still accepted:

- `cwd`
- `workspaceDir`
- `workspace_dir`

These fields only set the per-session file/shell boundary. Memory, identity, cron, and other persistent runtime state remain under the server's configured `workspace_dir`. If neither `project_root` nor a compatibility cwd field is present, ACP uses the server launch directory as a compatibility fallback.

When both forms are present, `project_root` wins.

## Isolation Model

Isolation is per session/process:

- one `agent` invocation = one project root
- one `daemon` process = one project root
- one ACP session with `project_root` = one project root
- one ACP session with only `cwd` = one temporary sandbox root backed by the server workspace

There is no in-session project switching in this model.

## Notes

- Config discovery still happens normally at startup.
- For project-root-bound entrypoints, the selected `project_root` becomes the runtime workspace after validation.
- Project-root-bound config state is redirected to `project_root/.zeroclaw/config.toml`.
- ACP `cwd` compatibility mode does not redirect config state; it only changes the session tool boundary.
