## Title

Port Networks And Derived Work Ownership

## Status

Completed

## Date

2026-03-20

## Context

Herd currently treats “network” as the creator-tree component of tmux windows. Canvas links come from `parent_window_id`, work ownership is stored in SQLite on the work item row, and collaborator state still exists in the work model, CLI, socket API, UI, and tests.

The target behavior is:

1. Networks are session-local graphs defined only by manual tile port connections.
2. Every tile has 4 visible ports.
3. Agent, Root Agent, Shell, Output, and Browser-terminal tiles use read/write ports by default except where constrained.
4. Work and Browser tiles use `left = read_write` and `top/right/bottom = read`.
5. Work ownership is derived only from the Agent connected to the Work tile `left` read/write port.
6. Collaborators are removed from the supported product model.
7. Workers may call `list_network` directly and the local CLI/socket graph remains session-scoped.
8. Creator lineage remains visible provenance only and never affects network membership.

## Goals

1. Add a persistent session-local network connection graph.
2. Render and edit tile connections directly on the canvas through visible ports.
3. Replace stored work ownership with graph-derived ownership.
4. Remove collaborator behavior from the supported contract.
5. Add CLI/socket support for network connect, disconnect, list, and message operations.
6. Preserve existing creator-lineage visuals without using them for networking.

## Non-goals

1. Changing the root-agent/message-only MCP model from the prior PRD.
2. Making worker MCP expose non-message tools.
3. Replacing lineage links with ports for provenance.
4. Cross-session networks or cross-session ownership.

## Scope

In scope:

1. SQLite schema and runtime state for network connections.
2. Backend graph resolution, validation, and work ownership derivation.
3. Socket/CLI changes for network operations and work-owner command removal.
4. Canvas ports, connection rendering, and drag interaction.
5. UI updates for derived ownership and Browser control indication.
6. Test-driver, unit, and integration coverage updates.

Out of scope:

1. Multi-owner work.
2. Message routing to non-Agent tiles.
3. New agent types beyond current `claude`.

## Risks And Mitigations

1. Port and lineage visuals can become conflated.
   - Mitigation: keep manual network edges and lineage links as separate rendered layers and separate projection fields.
2. Removing owner/collaborator state can break existing work flows.
   - Mitigation: derive `owner_agent_id` on reads so UI can remain stable while mutation commands are rewritten.
3. Disconnect or dead-agent cleanup can leave stale ownership.
   - Mitigation: derive ownership from the graph at read time and remove dead-agent edges centrally.
4. Port mutation can conflict with the earlier root-agent MCP boundary.
   - Mitigation: keep worker MCP message-only, but validate local CLI/socket graph and work mutations by session locality and derived ownership.

## Acceptance Criteria

1. Every visible tile exposes 4 visible ports.
2. Networks are computed only from manual port connections inside the current session.
3. Creator lineage remains visible but does not affect `list_network` or `message_network`.
4. Work `owner_agent_id` is derived only from the Agent connected to the Work tile `left` read/write port.
5. Disconnecting that port clears ownership immediately.
6. Collaborator fields and commands are removed from the supported product surface.
7. `herd list network` returns the caller’s current session-local connected component.
8. `herd message network` sends only to other Agent tiles in that component and logs `Agent X -> Network: ...`.
9. `list_network`, `network_connect`, and `network_disconnect` are session-local, and work stage mutation is allowed only for the derived owner.
10. The canvas supports drag-to-connect and disconnect for valid port pairs.

## Phased Plan

### Phase 0: PRD And Failing Coverage

#### Objective

Create the PRD and add failing checks for the graph rewrite, derived ownership, and collaborator removal.

#### Red

1. Add failing tests for:
   - port capability maps by tile kind
   - session-local manual network components
   - derived work ownership from a Work tile port
   - missing `network_connect` / `network_disconnect`
   - stale collaborator API/CLI/UI expectations

Expected failure signal:

1. network membership still follows creator lineage
2. work ownership is still stored
3. collaborators still appear in payloads and UI

#### Green

1. Create this PRD.
2. Land the failing tests and fixtures needed for the later phases.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `npm run test:unit -- src/lib/stores/appState.test.ts`

#### Exit Criteria

1. The PRD exists in `prd/`.
2. The red test set fails for the expected reasons.

### Phase 1: Graph Foundation And SQLite Storage

#### Objective

Add persistent manual network connections and graph resolution.

#### Red

1. Add failing Rust tests for:
   - per-tile port capability resolution
   - one-edge-per-port enforcement
   - invalid `read -> read` rejection
   - Work/Browser `left` port requiring an Agent
   - singleton isolated networks
   - session-local connected components

Expected failure signal:

1. no network table exists
2. no graph API exists
3. invalid connections are accepted or unsupported

#### Green

1. Add SQLite storage for manual network connections.
2. Add runtime state helpers for:
   - port capabilities
   - connect
   - disconnect
   - list component
   - dead-agent edge cleanup
3. Keep creator lineage data untouched, but exclude it from graph calculations.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml db::tests`
2. `cargo test --manifest-path src-tauri/Cargo.toml state::tests`

#### Exit Criteria

1. Manual graph state persists in SQLite.
2. Connected components are correct and session-local.
3. Invalid connection shapes are rejected centrally.

### Phase 2: Work Ownership Rewrite

#### Objective

Replace stored work ownership and collaborators with graph-derived ownership.

#### Red

1. Add failing Rust and integration tests for:
   - derived owner from the Work `left` port
   - immediate owner loss on disconnect
   - dead-agent ownership cleanup
   - non-owner work mutation rejection
   - disappearance of collaborator fields and commands

Expected failure signal:

1. work items still persist owner/collaborator state as authoritative data
2. non-owners can still mutate or collaborator flows still exist

#### Green

1. Remove collaborator behavior from the supported work model.
2. Stop using stored owner data as the source of truth.
3. Derive `owner_agent_id` from the graph at read time.
4. Gate stage start/complete on the derived owner only.
5. Clear ownership automatically when the connection or owning agent disappears.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml work::tests`
2. `npm run test:integration -- tests/integration/work-registry.test.ts`

#### Exit Criteria

1. Ownership is fully graph-derived.
2. Collaborator behavior is gone from the supported contract.
3. Work stage mutations honor the derived owner only.

### Phase 3: CLI, Socket, And Permission Boundary

#### Objective

Expose the graph through CLI/socket and remove the old owner/collaborator commands.

#### Red

1. Add failing tests for:
   - `list_network`
   - `message_network`
   - `network_connect`
   - `network_disconnect`
   - workers calling `list_network`
   - session-local graph mutation through CLI/socket
   - removal of explicit owner claim/release and collaborator commands

Expected failure signal:

1. network mutation commands are missing
2. workers cannot list their network
3. old work owner/collaborator commands are still present

#### Green

1. Add socket commands:
   - `network_connect`
   - `network_disconnect`
2. Add CLI commands:
   - `herd network connect`
   - `herd network disconnect`
3. Keep:
   - `herd list network`
   - `herd message network`
4. Remove explicit work owner/collaborator commands from the supported CLI/socket contract.
5. Keep worker MCP message-only, while the local CLI/socket graph surface remains session-local.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml cli::tests`
2. `cargo test --manifest-path src-tauri/Cargo.toml socket::tests`
3. `npm run test:integration -- tests/integration/worker-root-mcp.test.ts`

#### Exit Criteria

1. CLI and socket expose the new graph mutation surface.
2. Local network mutation is session-scoped and the old explicit owner/collaborator commands are gone.
3. Old explicit owner/collaborator commands are removed from the supported contract.

### Phase 4: Canvas Ports And Interaction

#### Objective

Render ports and allow users to connect and disconnect tiles directly on the canvas.

#### Red

1. Add failing frontend/integration tests for:
   - 4 visible ports on each tile
   - port mode differences by tile kind
   - drag-to-connect
   - disconnect from occupied ports
   - Work owner display updating from graph changes
   - Browser controller display

Expected failure signal:

1. no ports render
2. no graph edges render
3. ownership does not update in the UI

#### Green

1. Render ports on Terminal, Browser, and Work tiles.
2. Render manual network edges separately from creator-lineage links.
3. Add drag-to-connect and disconnect interaction.
4. Show derived owner/controller state in Work/Browser UI.
5. Extend test-driver projection with network edges and port metadata.

Verification commands:

1. `npm run check`
2. `npm run test:unit -- src/lib/stores/appState.test.ts`
3. `npm run test:integration -- tests/integration/test-driver.test.ts`

#### Exit Criteria

1. Ports and manual network edges are visible on the canvas.
2. Users can connect and disconnect valid port pairs.
3. Work owner and Browser controller state update live from the graph.

### Phase 5: Cleanup, Docs, And Regression

#### Objective

Remove stale collaborator/owner material, update docs, and lock the new behavior with regression coverage.

#### Red

1. Add failing regression checks for:
   - creator lineage not affecting networks
   - manual-created tiles using hidden session-root lineage
   - only Agent-created Agent tiles using creator-agent lineage
   - stale docs/skill references to collaborators and owner claim/release

Expected failure signal:

1. docs still describe the old work and network model
2. regressions allow lineage to leak into networking

#### Green

1. Update README, socket docs, skill text, and tests to the graph-based model.
2. Keep lineage projection and tests, but separate it clearly from network projection.
3. Mark the PRD complete only after targeted and adjacent suites pass.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `cargo check --manifest-path src-tauri/Cargo.toml`
3. `npm run check`
4. `npm run test:integration -- tests/integration/work-registry.test.ts`
5. `npm run test:integration -- tests/integration/test-driver.test.ts`

#### Exit Criteria

1. Docs match the shipped graph-based model.
2. Regression suites pass.
3. This PRD is marked `Completed`.

## Execution Checklist

- [x] Phase 0 complete
- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Phase 4 complete
- [x] Phase 5 complete
- [x] Integration/regression checks complete
- [x] Documentation/status updated

## Command Log

1. `cargo test --manifest-path src-tauri/Cargo.toml`
   - result: `passed`
   - notes: full Rust unit suite green after graph, work, and DB cleanup
2. `npm run check`
   - result: `passed`
   - notes: Svelte and TypeScript clean
3. `npm run test:unit -- src/lib/stores/appState.test.ts`
   - result: `passed`
   - notes: state/store projections green with network data
4. `npx vitest run --config vitest.integration.config.ts tests/integration/work-registry.test.ts --reporter=verbose`
   - result: `passed`
   - notes: session privacy, derived ownership, dead-owner cleanup, network messaging, and welcome bootstrap verified
5. `npx vitest run --config vitest.integration.config.ts tests/integration/test-driver.test.ts -t "renders tile ports with the right modes and supports drag-connect plus disconnect on the canvas" --reporter=verbose`
   - result: `passed`
   - notes: UI port rendering and drag connect/disconnect verified
