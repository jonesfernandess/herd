## Title

Generic Agent Rename And Typed Agent Registry

## Status

Completed

## Date

2026-03-20

## Context

Herd currently uses `Claude` as the generic product-level name for agent tiles, launch actions, test-driver requests, and several user-facing docs even though the app now has a broader agent model. At the same time, the runtime needs an explicit typed registry so future non-Claude agents can coexist without overloading the existing role/title heuristics.

The target result is:
- generic user-facing and registry-facing concepts use `Agent`
- explicit Claude-only runtime pieces stay Claude-specific where they actually refer to the Claude CLI, slash commands, or `notifications/claude/channel`
- the agent registry includes an explicit `agent_type` field, starting with `claude`

## Goals

1. Rename generic product-level `Claude` language to `Agent`.
2. Add and persist `agent_type` in the registry.
3. Keep Claude-specific launch/runtime/menu behavior intact where it is still truly Claude-specific.
4. Update tests, docs, and fixtures so the renamed surface is fully green.

## Non-goals

1. Renaming the actual `claude` CLI binary.
2. Renaming the Claude channel protocol method `notifications/claude/channel`.
3. Replacing existing pane role values such as `claude`.
4. Redesigning the Claude slash-command discovery model.

## Acceptance Criteria

1. The toolbar, invoke path, and test-driver use `Agent` naming for generic launch actions.
2. The agent registry includes `agent_type`, currently `claude`.
3. Generic docs and user-facing references say `Agent` instead of `Claude`.
4. Explicit Claude-only command/runtime references remain only where technically required.
5. Frontend unit checks and Rust backend tests pass after the rename.

## Phased Plan

### Phase 1: Registry Type And Backend Plumbing

#### Objective

Add `agent_type` to the registry model and propagate it through registration, persistence, and snapshots.

#### Red

1. Failing compile/test state from missing `agent_type` in registry fixtures and socket payload handling.

#### Green

1. Add `AgentType::Claude` and thread it through Rust and TypeScript registry models.
2. Update socket registration and persisted agent fixtures to carry `agent_type`.

#### Exit Criteria

1. Agent registry records serialize and deserialize with `agent_type`.
2. Existing agent registration still succeeds with `claude`.

### Phase 2: Generic Rename Sweep

#### Objective

Rename generic launcher, UI, test-driver, and documentation language from `Claude` to `Agent`.

#### Red

1. Failing references, tests, or typechecks caused by mixed old/new naming.

#### Green

1. Rename launcher/test-driver/invoke names and user-facing labels to `Agent`.
2. Update docs and skills to use `+ Agent` and Agent terminology.
3. Keep explicit Claude-only command/menu/runtime references unchanged where required.

#### Exit Criteria

1. Product-level generic naming is consistently `Agent`.
2. Claude-specific technical references remain only where needed.

### Phase 3: Regression Sweep

#### Objective

Bring the frontend and backend suites back to green.

#### Red

1. Targeted unit/type/backend failures after the rename.

#### Green

1. Fix remaining fixture, mock, and docs/test-driver drift.
2. Run targeted TS and Rust verification.

#### Exit Criteria

1. `npm run check` passes.
2. Targeted frontend unit tests pass.
3. Rust tests pass.

## Risks And Mitigations

1. Over-renaming can break true Claude-specific runtime features.
   - Mitigation: keep actual Claude CLI/channel/slash-command code paths explicitly Claude-specific.
2. Partial rename can leave test-driver/docs drift.
   - Mitigation: patch the tests and docs in the same change and verify immediately.

## Implementation Checklist

- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Verification complete
- [x] PRD status updated to `Completed`
