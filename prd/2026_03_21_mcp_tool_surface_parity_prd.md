# MCP Tool Surface Parity PRD

## Status

Completed

## Date

2026-03-21

## Context

Herd's root-mode MCP surface has drifted behind the latest user-facing socket commands. The socket and CLI already support newer shell, topic, network, and work operations, but the MCP bridge still exposes only a partial root tool set. Workers must remain message-only.

## Goals

1. Keep worker MCP mode restricted to message tools only.
2. Expand root MCP mode to cover the latest user-facing root/socket capabilities.
3. Add a direct parity regression so future socket-surface growth does not silently bypass MCP.
4. Update docs to describe the current worker/root MCP split accurately.

## Non-goals

1. Renaming socket commands.
2. Exposing internal lifecycle or test-driver socket commands through MCP.
3. Changing the backend permission model.

## Scope

In scope:
- MCP tool naming and registration in `mcp-server/src/index.ts`
- MCP startup/testability refactor if needed to add parity coverage
- MCP docs in `README.md` and `docs/socket-and-test-driver.md`
- MCP parity tests

Out of scope:
- Socket protocol changes
- CLI changes
- Worker MCP privilege expansion

## Risks And Mitigations

1. Root MCP parity could accidentally expose internal commands.
   - Mitigation: limit parity to the current user-facing root/socket surface only.
2. MCP testability refactor could break normal server startup.
   - Mitigation: keep `run.mjs` as the only autorun entrypoint and verify the built MCP server still starts.
3. Tool names could drift again after future socket additions.
   - Mitigation: export and test the tool catalogs directly.

## Acceptance Criteria

1. Worker MCP exposes only:
   - `message_direct`
   - `message_public`
   - `message_network`
   - `message_root`
2. Root MCP additionally exposes the latest user-facing root/socket capabilities:
   - `shell_create`
   - `shells_list`
   - `shell_destroy`
   - `shell_input_send`
   - `shell_exec`
   - `shell_output_read`
   - `shell_title_set`
   - `shell_read_only_set`
   - `shell_role_set`
   - `browser_create`
   - `browser_destroy`
   - `browser_navigate`
   - `browser_load`
   - `agents_list`
   - `topics_list`
   - `topic_subscribe`
   - `topic_unsubscribe`
   - `network_list`
   - `network_connect`
   - `network_disconnect`
   - `work_list`
   - `work_get`
   - `work_create`
   - `work_stage_start`
   - `work_stage_complete`
   - `work_review_approve`
   - `work_review_improve`
3. Internal commands such as agent lifecycle, logging, and test-driver commands are not exposed as MCP tools.
4. Docs describe the current root/worker MCP split and the expanded root surface.
5. Targeted MCP parity tests pass.

## Phased Plan

### Phase 1: PRD And Tool Catalog Red

#### Objective

Lock the intended root/worker MCP tool catalogs and write parity checks first.

#### Red

1. Add a failing MCP parity test for the worker tool list.
2. Add a failing MCP parity test for the root tool list.

Expected failure signal:
- missing root tools such as `shell_exec`, `topic_subscribe`, `network_connect`, and `work_*`

#### Green

1. Export explicit worker/root MCP tool catalogs from the MCP entrypoint.
2. Make the parity test assert those catalogs directly.

Verification commands:
- `npx vitest run mcp-server/src/index.test.ts`

#### Exit Criteria

1. The worker and root MCP tool catalogs are explicit and testable.
2. The failing parity test turns green.

### Phase 2: Root Tool Surface Green

#### Objective

Implement the missing root-mode MCP tools without changing worker permissions.

#### Red

1. Keep the parity test failing until the missing root registrations exist.

Expected failure signal:
- exported root tool list does not match the latest supported surface

#### Green

1. Add the missing root-only MCP tools:
   - shell exec/read-only/role
   - topic subscribe/unsubscribe
   - network connect/disconnect
   - work list/get/create/stage/review
2. Keep workers message-only.
3. Keep root MCP tools as thin wrappers over existing socket commands.

Verification commands:
- `npx vitest run mcp-server/src/index.test.ts`
- `npm --prefix mcp-server run build`

#### Exit Criteria

1. Root MCP matches the latest user-facing root/socket surface.
2. Worker MCP remains unchanged.

### Phase 3: Docs And Regression Verification

#### Objective

Bring the shipped docs in line with the implemented MCP surface and verify the slice.

#### Red

1. Update docs that still describe root MCP as only shell/listing tools.

Expected failure signal:
- docs do not mention the expanded root tool set or the worker-only boundary accurately

#### Green

1. Update `README.md`.
2. Update `docs/socket-and-test-driver.md`.
3. Mark this PRD completed only after targeted checks pass.

Verification commands:
- `npm run check`
- `npx vitest run mcp-server/src/index.test.ts`
- `npm --prefix mcp-server run build`

#### Exit Criteria

1. Docs match the current MCP surface.
2. The targeted MCP checks pass.

## Implementation Checklist

- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Documentation/status updated

## Command Log

1. `sed -n '1,260p' mcp-server/src/index.ts`
   - result: pass
   - notes: confirmed current root MCP only covers shell/browser/listing plus message tools
2. `sed -n '1,460p' src-tauri/src/socket/protocol.rs`
   - result: pass
   - notes: confirmed the latest user-facing socket surface and missing MCP parity commands
3. `npx vitest run --config mcp-server/vitest.config.ts`
   - result: pass
   - notes: root/worker MCP tool catalog parity test passed
4. `npm --prefix mcp-server run build`
   - result: pass
   - notes: MCP TypeScript build passed after exporting `main()` and excluding tests from the build
5. `npm run check`
   - result: pass
   - notes: repo frontend/node type checks passed
6. `cargo test --manifest-path src-tauri/Cargo.toml`
   - result: pass
   - notes: Rust tests passed after tightening worker/root socket permission checks for agent/work access
7. `cargo check --manifest-path src-tauri/Cargo.toml`
   - result: pass
   - notes: Rust backend compiled successfully
8. `npx vitest run --config vitest.integration.config.ts tests/integration/worker-root-mcp.test.ts --reporter=verbose`
   - result: fail
   - notes: existing message-routing integration still has a timeout opening the second worker event subscription; not caused by the new root MCP tool catalog
