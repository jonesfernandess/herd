Status: In Progress

# Browser Socket Commands PRD

## Context

The Unix socket API still uses `shell_spawn`, while the rest of the command surface has moved toward noun-first naming. The backend already has browser-window helpers and browser webview navigation commands, but the socket API does not expose first-class browser lifecycle commands.

## Goals

- Rename the canonical socket command `shell_spawn` to `shell_create`.
- Add first-class browser socket commands for create, destroy, navigate, and loading a local file.
- Update internal callers, tests, and docs to the new command names.

## Non-goals

- Reworking browser tile rendering or child-webview synchronization.
- Adding queued navigation state before a browser webview exists.
- Redesigning the broader socket protocol response schema.

## Scope

- `src-tauri/src/socket/protocol.rs`
- `src-tauri/src/socket/server.rs`
- `src-tauri/src/browser.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/cli.rs`
- `mcp-server/src/index.ts`
- `tests/integration/*`
- `docs/socket-and-test-driver.md`

## Risks

- Browser navigation currently depends on a live browser child webview, so socket navigation may fail until the UI tile has synced once.
- Renaming a command can break older callers if compatibility aliases are removed outright.
- Local file loading must normalize paths safely and produce a valid `file://` URL.

## Acceptance Criteria

- `shell_create` is the canonical socket command and existing internal callers use it.
- `browser_create`, `browser_destroy`, `browser_navigate`, and `browser_load` are accepted by the socket server.
- `browser_load` resolves a local file path to a browser-loadable URL.
- CLI help and payload generation reflect the new commands.
- Socket docs list the new canonical API.

## Phased Plan

### Phase 1: Red

- Add CLI/unit coverage for `shell create` and browser command payloads.
- Update integration callers to use the new shell command name and expose browser helpers.

### Phase 2: Green

- Implement protocol variants and handlers for the new commands.
- Add local-file URL support in the browser backend.
- Update docs and MCP callers.

### Phase 3: Verify

- Run targeted Rust and TypeScript verification.
- Fix any regressions found in the touched paths.

## Checklist

- [ ] Add PRD
- [ ] Add failing tests
- [ ] Implement backend changes
- [ ] Update callers and docs
- [ ] Verify

## Command Log

- Pending during implementation
