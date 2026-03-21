# Session Spawn CWD PRD

## Status

In Progress

## Date

2026-03-20

## Context

Herd already stores `HERD_TAB_ROOT_CWD` per tmux session, but new shell creation is not consistently launched from that directory and Claude startup has been coupled to per-subdirectory MCP workarounds. The desired behavior is one configurable spawn directory per tab/session, used by both new shells and new Claude windows. By default, that directory should be the project root.

## Goals

1. Make new shells spawn from the configured session root cwd.
2. Make new Claude windows spawn from the configured session root cwd.
3. Default new sessions to the project root.
4. Expose the session spawn directory in the UI and allow changing it.
5. Remove the `src-tauri` `.mcp.json` copy and `.claude` symlink workaround.

## Non-goals

1. Changing the cwd of existing running panes.
2. Adding a full modal settings system for session configuration.
3. Changing non-Herd Claude launches typed manually by the user.

## Scope

In scope:

1. tmux session root cwd detection/defaulting
2. new shell and Claude spawn behavior
3. active-session cwd editing UI
4. docs and cleanup of `src-tauri` Claude/MCP config workarounds

Out of scope:

1. migrating every historical session root automatically
2. per-pane cwd configuration

## Risks And Mitigations

1. Claude launched from a subdirectory may not resolve MCP config.
   - Mitigation: pass the repo-root `.mcp.json` explicitly with `--mcp-config`.
2. New shell behavior could regress if tmux pane creation still inherits the active pane cwd.
   - Mitigation: respawn newly created panes explicitly into the session root cwd.
3. Removing `src-tauri/.claude` could break skill discovery.
   - Mitigation: keep Claude launches on the configured session cwd, default sessions to the project root, and only remove the workaround after verification.

## Acceptance Criteria

1. New sessions default their spawn cwd to the project root.
2. New shells open in the configured session cwd.
3. New Claude windows open in the configured session cwd.
4. Claude launches still register correctly without `src-tauri/.mcp.json`.
5. The active session spawn cwd is visible and editable in the UI.
6. `src-tauri/.mcp.json` and `src-tauri/.claude` are removed.

## Phased Plan

### Phase 1: Red

Objective:

Add failing coverage for project-root defaults and spawn-cwd behavior.

Red:

1. Add Rust tests for project-root detection and session-root defaulting.
2. Add frontend/store tests for session root cwd flowing through tmux snapshots.
3. Add targeted checks for Claude launch using explicit MCP config rather than a cwd-local copy.

Expected failure signal:

1. session roots still default to the active working directory in `src-tauri`
2. snapshots do not expose a configurable spawn cwd
3. Claude launch strings do not include the explicit MCP config path

Green:

1. Implement the smallest backend/type changes needed to satisfy the new tests.
2. Verify with targeted Rust and unit tests.

Exit criteria:

1. project-root defaulting is covered
2. tmux snapshots carry session root cwd
3. Claude launch command construction is covered

### Phase 2: Green

Objective:

Unify shell and Claude spawn behavior on the session root cwd.

Red:

1. Add failing checks for new shell creation still inheriting the active pane cwd.
2. Add failing checks for Claude launches requiring `src-tauri/.mcp.json`.

Expected failure signal:

1. new panes start in the wrong cwd
2. Claude depends on a local subdirectory MCP config

Green:

1. Respawn new shell panes into the session root cwd.
2. Launch Claude from the session root cwd with explicit `--mcp-config`.
3. Keep the development-channel auto-confirm behavior intact.

Exit criteria:

1. shells and Claude use the same session root cwd
2. Claude registration still works without a subdirectory `.mcp.json`

### Phase 3: UI And Cleanup

Objective:

Expose the session spawn cwd in the UI, then remove the `src-tauri` workarounds.

Red:

1. Add failing checks for missing session root cwd UI.
2. Add failing checks for changing the cwd not emitting updated tmux state.

Expected failure signal:

1. no visible/editable session cwd
2. backend updates do not propagate to the frontend

Green:

1. Add an active-session spawn-directory control in the sidebar.
2. Add a backend setter command that updates tmux session env and emits a fresh snapshot.
3. Remove `src-tauri/.mcp.json` and `src-tauri/.claude`.
4. Update docs and mark the PRD completed.

Exit criteria:

1. the spawn cwd is visible and editable
2. docs match the new launch model
3. `src-tauri` no longer carries local Claude/MCP config copies

## Execution Checklist

- [ ] Phase 1 complete
- [ ] Phase 2 complete
- [ ] Phase 3 complete
- [ ] Integration/regression checks complete
- [ ] Documentation/status updated

## Command Log

1. `cargo test --manifest-path src-tauri/Cargo.toml`
   - result: pending
   - notes: targeted backend verification
2. `npm run test:unit -- src/lib/stores/appState.test.ts`
   - result: pending
   - notes: snapshot/store regression coverage
3. `npm run check`
   - result: pending
   - notes: frontend/type validation
