## Title

Session Root Agent And Message-Only Worker MCP

## Status

Completed

## Date

2026-03-20

## Summary

Herd now creates and maintains one visible Root agent per tmux session/tab. Root agents are highlighted in red, use a stable logical id `root:<session_id>`, are repaired automatically if they disappear, and are protected from normal close actions.

The checked-in MCP configuration remains a single `server:herd` entry. Root and worker agents both launch against that same entry, but the Herd MCP bridge now switches behavior by role:

1. Root agents get the full Herd MCP tool surface.
2. Worker agents get only message tools.
3. The backend enforces the same permission boundary for agent-originated CLI/socket calls.

Messaging now uses:

- `message_direct`
- `message_public`
- `message_network`
- `message_root`

`message_public` replaces `message_chatter` as the supported contract.

## Context

Before this change:

1. Herd-managed agents all used the same unrestricted MCP surface.
2. There was no per-session Root agent.
3. Agent-originated CLI/socket calls could use privileged Herd commands directly.
4. `message_root`, `message_network`, and `list_network` did not exist.

The target behavior for this PRD was:

1. Every session/tab has one Root agent.
2. Root is the only agent allowed to use the full Herd tool surface.
3. Worker agents are message-only.
4. Root remains alive and is repaired automatically by Herd.
5. The CLI, socket API, MCP bridge, docs, and tests all reflect that model.

For this PRD, “network” is the current session-local creator-tree connected component. The later port-graph model is not part of this change.

## Goals

1. Add a stable per-session Root agent lifecycle.
2. Split the MCP behavior into worker and root modes without duplicating the checked-in `.mcp.json` entry.
3. Enforce root-only non-message operations at the backend.
4. Add `message_public`, `message_network`, `message_root`, and `list_network`.
5. Keep Root visible, red, and non-closable.
6. Update docs, skill text, and tests to the new message-first coordination model.

## Non-goals

1. Replacing the creator-tree network with the later generic port graph.
2. Changing work ownership or collaborator rules in this PRD.
3. Removing the CLI or raw socket interfaces.
4. Rewriting the transport to `MCP -> CLI -> Socket`; the MCP bridge still talks directly to the socket.

## Implementation Summary

The shipped design is:

1. `.mcp.json` contains one checked-in MCP server entry:
   - `server:herd`
2. Herd-managed agent launches always pass:
   - `--mcp-config <repo-root>/.mcp.json`
   - `--teammate-mode tmux`
   - `--dangerously-load-development-channels server:herd`
3. Herd injects:
   - `HERD_AGENT_ID`
   - `HERD_AGENT_ROLE`
   - `HERD_SESSION_ID`
   - `HERD_PANE_ID`
   - `HERD_SOCK`
4. The MCP bridge inspects `HERD_AGENT_ROLE` / `HERD_AGENT_ID`:
   - Root mode exposes the full Herd MCP surface.
   - Worker mode exposes only message tools.
5. The backend still validates agent role on socket/CLI requests, so non-root agents cannot bypass the message-only restriction.

## Acceptance Criteria

1. Every session has one visible red Root agent with stable id `root:<session_id>`.
2. New sessions create their Root agent automatically.
3. App bootstrap repairs missing Root agents for existing sessions.
4. Root and worker agents both launch against `server:herd`.
5. Worker MCP exposes only message tools.
6. Root MCP exposes the full Herd tool surface.
7. Non-root agent-originated CLI/socket requests are rejected for non-message commands.
8. `message_chatter` is replaced by `message_public` as the supported contract.
9. `message_root` delivers only to the session Root agent and logs `Agent X -> Root: ...`.
10. `message_network` delivers to the other agents in the sender’s current creator-tree network and logs `Agent X -> Network: ...`.
11. `list_network` returns the current creator-tree network for the caller’s session.
12. Root agent tiles cannot be closed through normal Herd UI or backend close commands.

## Phased Red/Green Plan

### Phase 0: PRD And Failing Coverage

#### Objective

Create the PRD and add failing tests for the new root lifecycle, message surface, and permission boundary.

#### Red

1. Add failing tests for:
   - root agent spawn on new session creation
   - root agent bootstrap repair on startup
   - worker-vs-root MCP mode selection
   - backend rejection of non-message worker commands
   - `message_public`, `message_network`, and `message_root`

#### Green

1. Create this PRD.
2. Land the failing tests and fixtures for the later phases.

#### Exit Criteria

1. The PRD exists in `prd/`.
2. The red test set fails for the expected reasons.

### Phase 1: Root Agent Registry And Lifecycle

#### Objective

Add root-vs-worker roles, root-agent spawning, and keepalive/repair behavior.

#### Red

1. Add failing tests for:
   - `agent_role` persistence and serialization
   - root spawn with stable id `root:<session_id>`
   - root non-closable behavior
   - root repair after death or unregister

#### Green

1. Add `agent_role: root | worker` to the registry model.
2. Spawn and repair root agents:
   - on new session creation
   - on bootstrap
   - after death or unregister
3. Make root tiles visible, red, and protected from close actions.

#### Exit Criteria

1. Every session has exactly one root agent.
2. Root ids are stable per session.
3. Root agents are visible and protected from normal close actions.

### Phase 2: MCP Role Split And Launch Modes

#### Objective

Split the MCP behavior into worker and root modes while keeping one checked-in `server:herd` entry.

#### Red

1. Add failing tests for:
   - worker mode still exposing full tools
   - root mode not exposing the full tools
   - root/worker launches missing the correct env/role mode

#### Green

1. Keep a single `server:herd` entry in `.mcp.json`.
2. Drive MCP mode from `HERD_AGENT_ROLE` / `HERD_AGENT_ID`.
3. Register only message tools in worker mode.
4. Keep the full current tool set in root mode.
5. Update root/worker launches to inject the right role metadata.

#### Exit Criteria

1. Worker agents launch against `server:herd` in message-only mode.
2. Root agents launch against `server:herd` in full-tool mode.
3. Tool registration differs correctly by role.

### Phase 3: Backend Permission Boundary And Message Surface

#### Objective

Enforce root-only non-message access for agents and add the new message commands.

#### Red

1. Add failing tests for:
   - worker CLI/socket calls to non-message commands
   - `message_public` replacing `message_chatter`
   - `message_root` delivery and log formatting
   - `message_network` delivery and log formatting
   - root-only `list_network`

#### Green

1. Rename the supported chatter command to `message_public`.
2. Add `message_root`, `message_network`, and `list_network` to socket and CLI.
3. Enforce that non-root agent-originated requests may call only:
   - `agent_register`
   - `agent_unregister`
   - `agent_events_subscribe`
   - `agent_ping_ack`
   - `message_direct`
   - `message_public`
   - `message_network`
   - `message_root`
4. Define network as the sender window’s current creator-tree component inside its session.

#### Exit Criteria

1. Non-root agents are message-only at the backend.
2. Root agents retain full access.
3. The new message commands and `list_network` work as specified.

### Phase 4: UI, Docs, And Regression

#### Objective

Finish the root tile visuals, update docs and skill text, and run the adjacent regression suites.

#### Red

1. Add failing tests for:
   - red root highlighting
   - root close protection
   - worker-vs-root session state projection
   - stale docs/skill guidance

#### Green

1. Update tile styling and projection for root agents.
2. Update docs and the local Herd skill to the root-agent/message-only model.
3. Mark the PRD complete only after targeted and adjacent regression suites pass.

#### Exit Criteria

1. The UI exposes root agents correctly.
2. The docs and local skill describe the new contract.
3. This PRD is marked `Completed`.

## Execution Checklist

- [x] Phase 0 complete
- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Phase 4 complete
- [x] Integration/regression checks complete
- [x] Documentation/status updated

## Command Log

1. `./node_modules/.bin/vitest run --config vitest.integration.config.ts tests/integration/worker-root-mcp.test.ts --reporter=verbose`
   - result: `passed`
   - notes: verified root lifecycle, worker permission boundary, and `message_root` / `message_network`
2. `cargo test --manifest-path src-tauri/Cargo.toml commands::tests::root_agent_launch_command_uses_root_mcp_server -- --nocapture`
   - result: `passed`
   - notes: verified worker/root launch command shape
3. `cargo test --manifest-path src-tauri/Cargo.toml cli::tests -- --nocapture`
   - result: `passed`
   - notes: verified CLI message and list command surface
4. `cargo test --manifest-path src-tauri/Cargo.toml`
   - result: `passed`
   - notes: full Rust suite
5. `cargo check --manifest-path src-tauri/Cargo.toml`
   - result: `passed`
   - notes: backend build verification
6. `npm run check`
   - result: `passed`
   - notes: frontend typecheck verification
