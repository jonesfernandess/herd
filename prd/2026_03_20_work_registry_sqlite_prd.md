## Title

SQLite-Backed Herd State And Work Registry

## Status

Completed

## Date

2026-03-20

## Context

Herd currently persists tile geometry to `tmp/herd-state.json`, keeps agent/topic/chatter state primarily in memory, and has no work registry. The next feature set requires a durable registry for work items, owners, collaborators, stage review state, and agent coordination metadata. The file-backed geometry path is not a good foundation for that.

The target architecture is:
- SQLite as the only durable store for Herd state, agent/topic/chatter history, and work metadata
- markdown stage files under `work/` for human/agent-authored content
- grouped socket/CLI work commands layered on top of the database
- a `WORK` sidebar section and `+ WORK` toolbar action
- per-session privacy for agent registry, topic registry, chatter, and work views

This PRD is intentionally phased so the database foundation lands before the work registry and review UI.

## Goals

1. Replace JSON-backed local Herd state with SQLite.
2. Move chatter persistence into SQLite.
3. Add a durable work registry with owner/collaborator rules and stage review flow.
4. Keep stage content in markdown files under `work/`.
5. Preserve the grouped CLI and `category_command` socket naming.
6. Keep agent/work/topic/chatter APIs private to the caller's tmux session.

## Non-goals

1. Migrating existing `tmp/herd-state.json` or chatter log data into SQLite.
2. Adding a separate document editor inside Herd.
3. Allowing non-owners to update work status through Herd.
4. Showing cross-tab work items in the sidebar; sidebar scope stays local to the active session.
5. Exposing cross-session agent, topic, chatter, or work data through the user-facing or agent-facing APIs.

## Scope

In scope:
- SQLite runtime database path and schema
- tile-state and chatter persistence
- agent/topic/work registry tables
- work socket/CLI commands
- toolbar/sidebar/review UI for work items
- welcome-message update for agent onboarding

Out of scope:
- importing legacy JSON/log data
- collaborative text editing semantics for the markdown files
- global cross-session work editing from the UI

## Risks And Mitigations

1. SQLite becomes a second source of truth during rollout.
   - Mitigation: stop reading/writing `tmp/herd-state.json` once the DB path lands; do a clean break.
2. Liveness-derived ownership can become stale across app restarts.
   - Mitigation: on boot, mark agents dead and clear owners/collaborators while keeping work history.
3. Work rules can be bypassed by direct file edits.
   - Mitigation: Herd governs metadata and status through the CLI/socket; markdown files remain direct-edit artifacts by design.
4. Large cross-cutting changes can break the CLI/UI contract.
   - Mitigation: stage the work behind red/green phase gates with targeted tests per phase.

## Acceptance Criteria

1. Herd persists local state to SQLite under `tmp/` instead of `tmp/herd-state.json`.
2. Chatter history is stored in SQLite and still appears in the debug/activity UI.
3. A `+ WORK` button creates a work item for the active session.
4. The sidebar includes a `WORK` section showing only active-session items.
5. Work items enforce one owner, multiple collaborators, and the requested ownership/collaboration limits.
6. Work stages follow `plan -> prd -> artifact` and `ready -> in_progress -> completed -> approved`.
7. Only the owner can perform Herd-managed work updates.
8. User review uses `Approve` or `Improve(comment)` and advances or reopens the stage correctly.
9. Dead owners are unassigned automatically; dead collaborators are removed automatically.
10. The grouped CLI and socket APIs expose the work registry.
11. Agent, topic, chatter, and work APIs only return data from the caller's current tmux session.

## Phased Plan

### Phase 0: SQLite Foundation For State And Chatter

#### Objective

Introduce the SQLite runtime database, create the base schema, and move tile-state/chatter persistence off the JSON/log files.

#### Red

1. Add failing Rust tests for:
   - runtime database path resolution
   - tile-state save/load round-trip through SQLite
   - chatter entry append/load round-trip through SQLite
2. Run targeted tests and capture the failure signal caused by missing database code and continued JSON/file persistence.

Expected failure signal:
- missing SQLite module/functions
- tile-state round-trip fails
- chatter persistence fails

#### Green

1. Add a SQLite module and runtime DB path helper.
2. Create the initial schema for tile state, chatter, agents, topics, and work.
3. Replace `persist.rs` JSON persistence with SQLite-backed tile-state persistence.
4. Replace file-backed chatter persistence with SQLite-backed chatter persistence.
5. Keep live subscribers in memory; only persisted records go to SQLite.

Verification commands:
1. `cargo test --manifest-path src-tauri/Cargo.toml db::tests`
2. `cargo test --manifest-path src-tauri/Cargo.toml runtime::tests`
3. `cargo check --manifest-path src-tauri/Cargo.toml`

#### Exit Criteria

1. Herd no longer depends on `tmp/herd-state.json` for layout persistence.
2. Chatter persistence no longer depends on `tmp/herd-chatter.log`.
3. The database schema is created automatically at runtime.

### Phase 1: Agent, Topic, And Liveness State In SQLite

#### Objective

Persist agent/topic metadata and make startup/liveness reconciliation database-backed.

#### Red

1. Add failing tests for:
   - agent/topic persistence round-trip
   - startup clearing of agent alive state
   - startup clearing of work owner/collaborator assignment placeholders
2. Add failing tests for dead-agent cleanup touching persisted ownership metadata.

Expected failure signal:
- agents/topics only exist in memory
- restart loses registry metadata
- owner/collaborator cleanup does not persist

#### Green

1. Persist agent/topic records to SQLite.
2. Load persisted registry metadata on startup.
3. On startup, mark agents dead and clear owner/collaborator assignments.
4. Keep chatter/topic derivation behavior unchanged from the user’s perspective.

Verification commands:
1. `cargo test --manifest-path src-tauri/Cargo.toml state::tests`
2. `cargo test --manifest-path src-tauri/Cargo.toml socket::tests`
3. `cargo check --manifest-path src-tauri/Cargo.toml`

#### Exit Criteria

1. Agent/topic metadata survives restart in SQLite.
2. Restart clears live assignment state safely.
3. Chatter/activity views still render from the restored store.

### Phase 2: Work Registry Backend, Socket, CLI, And Files

#### Objective

Add the work registry, markdown workspace files, and the work socket/CLI contract.

#### Red

1. Add failing tests for:
   - work creation and stage file creation under `work/`
   - owner and collaborator rule enforcement
   - stage lifecycle transitions
   - review approve/improve behavior
   - work CLI serialization to `work_*` socket commands

Expected failure signal:
- missing work tables and commands
- no work files created
- no ownership enforcement
- invalid stage transitions pass or valid ones fail

#### Green

1. Add work tables and data access in SQLite.
2. Create stage markdown files in `work/session-<session-number>/<work-id>/`.
3. Add `work_*` socket commands and grouped CLI `work` commands.
4. Enforce:
   - one owner per work item
   - one owned item per agent across the runtime
   - one collaborator assignment per agent across the runtime
   - same-session assignment only
   - owner-only updates
5. Add automatic topic creation for each work item.

Verification commands:
1. `cargo test --manifest-path src-tauri/Cargo.toml cli::tests`
2. `cargo test --manifest-path src-tauri/Cargo.toml socket::tests`
3. `cargo test --manifest-path src-tauri/Cargo.toml state::tests`
4. `cargo check --manifest-path src-tauri/Cargo.toml`

#### Exit Criteria

1. Work items are durable in SQLite and stage files exist on disk.
2. CLI/socket work commands are implemented and enforced.
3. Ownership/collaboration rules are enforced by Herd.

### Phase 3: Work UI, Review Flow, And Welcome Message

#### Objective

Expose work creation, browsing, and review in the toolbar/sidebar and update the agent onboarding message.

#### Red

1. Add failing frontend tests for:
   - `+ WORK`
   - `WORK` sidebar section limited to the current session
   - approval-ready ordering/highlighting
   - detail/review panel behavior
   - approve/improve actions and comment requirement
2. Add failing tests for the updated welcome DM text and work-oriented onboarding behavior.

Expected failure signal:
- no work UI
- no review flow
- welcome text still describes only PRD/chatter discovery

#### Green

1. Add `+ WORK` to the toolbar.
2. Add `WORK` to the `TREE` sidebar and show only current-session work items.
3. Add detail/review UI with file links, preview, and `Approve`/`Improve`.
4. Pin and highlight items awaiting approval.
5. Update the welcome DM to the requested workflow.

Verification commands:
1. `npm run test:unit -- src/lib/stores/appState.test.ts`
2. `npm run check`
3. targeted integration tests for work UI flows

#### Exit Criteria

1. The work UI is usable end-to-end.
2. Approval-ready items are surfaced correctly.
3. The welcome flow matches the new work model.

## Implementation Checklist

- [x] Phase 0 complete
- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Integration/regression checks complete
- [x] Documentation/status updated

## Command Log

1. `cargo test --manifest-path src-tauri/Cargo.toml db::tests`
   - result: pass
   - notes: schema initialization verified after the red failure on the missing SQLite stub
2. `cargo test --manifest-path src-tauri/Cargo.toml runtime::tests`
   - result: pass
   - notes: runtime DB filename/path helper verified
3. `cargo test --manifest-path src-tauri/Cargo.toml persist::tests`
   - result: pass
   - notes: tile-state and chatter round-trip through SQLite
4. `cargo check --manifest-path src-tauri/Cargo.toml`
   - result: pass
   - notes: Rust compilation green with existing non-blocking warnings
5. `cargo test --manifest-path src-tauri/Cargo.toml`
   - result: pass
   - notes: full Rust suite green after the SQLite foundation changes
6. `npm run check`
   - result: pass
   - notes: frontend store/types/sidebar/toolbar work wiring validated
7. `cargo test --manifest-path src-tauri/Cargo.toml work::tests`
   - result: pass
   - notes: work creation, ownership rules, and stage/review lifecycle
8. `cargo test --manifest-path src-tauri/Cargo.toml cli::tests`
   - result: pass
   - notes: grouped `herd work ...` CLI serialization
9. `cargo test --manifest-path src-tauri/Cargo.toml`
   - result: pass
   - notes: full Rust suite green after work backend/socket/UI invoke wiring
10. `npm run test:unit -- src/lib/stores/appState.test.ts`
   - result: pass
   - notes: existing store suite still green after adding the work slice
