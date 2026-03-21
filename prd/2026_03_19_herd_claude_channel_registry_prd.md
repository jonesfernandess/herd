## Title

Herd Claude Channel Registry, Agent Messaging, And Topic Activity

## Status

Completed

## Date

2026-03-19

## Context

Herd currently has tmux-authoritative tile management, a local Unix socket control surface, an existing `herd` MCP server, and a frontend debug log. It does not yet have a normalized socket protocol, a Rust CLI mode, agent registration, Claude channel delivery, topic subscriptions, chatter persistence, or an agent activity panel.

The target behavior is:

1. Herd-managed Claude launches are standardized.
2. The existing `herd` MCP server also acts as the Claude channel server.
3. Agents register with Herd using a Herd-issued `HERD_AGENT_ID`.
4. Herd can list agents and topics, route direct messages and public chatter, and track liveness with periodic ping/ack.
5. Global chatter/debug activity is appended to `tmp/herd-chatter.log`.
6. The debug pane exposes `Chatter` and `Logs`.
7. Claude tiles render an activity panel below the shell showing relevant DM/chatter activity.

## Goals

1. Add a `+ Claude` toolbar action next to `+ Shell`.
2. Launch all Herd-managed Claude tiles with `--teammate-mode tmux --dangerously-load-development-channels server:herd`.
3. Normalize the CLI around grouped subcommands such as `list agents`, `message direct`, and `subscribe topic`.
4. Normalize the socket protocol to `category_command` names.
5. Add agent registry, topic registry, chatter persistence, welcome/bootstrap delivery, and ping-based liveness in the Rust backend.
6. Extend the existing `herd` MCP server with Claude channel capability and agent event subscription.
7. Add a repo-local Herd Claude skill that uses the installed `herd` CLI or repo-local `bin/herd`.

## Non-goals

1. Per-agent webhook servers.
2. Bounded chatter history.
3. Replaying historical DMs to newly registered agents.
4. Supporting the old socket command names after the migration.

## Scope

In scope:

1. Rust backend state, protocol, socket server, CLI mode, and runtime paths.
2. Existing `herd` MCP server channel delivery.
3. Frontend toolbar/debug/agent tile updates.
4. Tests and docs/skill updates.

Out of scope:

1. Manual non-Herd Claude launches.
2. Chatter persistence across different runtime IDs.
3. New external services or webhook daemons.

## Architecture Summary

1. Herd launches Claude with a generated `HERD_AGENT_ID`.
2. The existing `herd` MCP server runs inside that Claude environment, declares `claude/channel`, registers the agent through the Herd socket, and subscribes to agent events.
3. Herd stores an in-memory registry keyed by `agent_id`, plus topic subscriptions and liveness metadata.
4. Herd writes all DM/chatter/lifecycle debug entries to `tmp/herd-chatter.log`.
5. The frontend consumes chatter/agent events through Tauri events and the chatter log tail.
6. Claude tiles render a local activity panel derived from DM traffic, outgoing chatter, mentions, and subscribed topics.

## Risks And Mitigations

1. The old socket names are referenced broadly.
   - Mitigation: migrate all first-party callers in the same change set and update docs/tests together.
2. Long-lived socket subscriptions add complexity to the backend.
   - Mitigation: keep the streaming contract agent-specific and line-delimited JSON.
3. Unbounded chatter history can grow large.
   - Mitigation: keep backend append-only persistence and tail-based frontend reads.
4. Topic and mention parsing can drift.
   - Mitigation: parse and normalize them once in the backend and reuse the metadata everywhere.

## Acceptance Criteria

1. Herd-managed Claude launches use the required `claude` flags.
2. Each launch gets a unique `HERD_AGENT_ID`.
3. The existing `herd` MCP server self-registers agents and forwards `notifications/claude/channel`.
4. Socket commands are normalized to `category_command` names.
5. CLI commands are normalized to grouped subcommands.
6. Herd supports agent listing, topic listing, direct messaging, chatter messaging, topic subscription, ping ack, sign-on, sign-off, and one-hour public chatter replay.
7. All chatter/debug activity is appended to `tmp/herd-chatter.log`.
8. The debug pane shows `Chatter` and `Logs`.
9. Agent tiles show a local activity panel beneath the shell.

## Phased Plan

### Phase 1

Objective:

Create the PRD, runtime path additions, and normalized backend protocol/state scaffolding.

Red:

1. Add backend tests for normalized socket command parsing, agent/topic state bookkeeping, and chatter formatting.
2. Confirm the current protocol does not recognize the new names and the new state does not exist.

Expected failure signal:

1. Unknown socket commands.
2. Missing agent registry/topic registry/chatter log helpers.

Green:

1. Add normalized socket command enums and response/event types.
2. Add backend app state for agents, topics, subscriptions, and chatter records.
3. Add runtime chatter log path helpers and append/tail helpers.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `cargo check --manifest-path src-tauri/Cargo.toml`

Exit criteria:

1. Normalized protocol types compile.
2. Backend state and chatter path helpers exist and are tested.

### Phase 2

Objective:

Implement socket command handling, liveness, and event subscription in Rust.

Red:

1. Add backend tests for `agent_register`, `agent_unregister`, `agent_events_subscribe`, `agent_ping_ack`, `list_agents`, `list_topics`, `message_direct`, `message_chatter`, `topic_subscribe`, and `topic_unsubscribe`.
2. Add backend tests for sign-on, sign-off, welcome DM, and public-chatter replay.

Expected failure signal:

1. Missing handlers or incorrect response/event payloads.

Green:

1. Implement the normalized socket handlers.
2. Add periodic ping scheduling and dead-agent transitions.
3. Persist chatter/debug lines to `tmp/herd-chatter.log`.
4. Emit frontend events for agent/chatter updates.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `cargo check --manifest-path src-tauri/Cargo.toml`

Exit criteria:

1. Backend routes agent and topic traffic correctly.
2. Sign-on/off and bootstrap delivery are logged and emitted.

### Phase 3

Objective:

Add the Rust CLI mode, repo `bin/herd` wrapper, and MCP channel integration.

Red:

1. Add CLI tests for grouped commands such as `list agents`, `message direct`, `message chatter`, `message topic`, `subscribe topic`, and `agent ack-ping`.
2. Add MCP-side tests/checks for agent registration, subscription, and channel notifications.

Expected failure signal:

1. CLI mode missing.
2. MCP server not channel-capable or still using legacy socket names.

Green:

1. Add `src-tauri/src/cli.rs` and wire `main.rs` to CLI-vs-GUI dispatch.
2. Add `bin/herd`.
3. Update `mcp-server/src/index.ts` to use normalized commands, register agents, subscribe to events, ack pings, and emit `notifications/claude/channel`.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `cargo check --manifest-path src-tauri/Cargo.toml`
3. `npm run check`

Exit criteria:

1. The CLI works in grouped-subcommand form.
2. The MCP server acts as the Claude channel server.

### Phase 4

Objective:

Implement `+ Claude`, frontend chatter/agent state, debug tabs, and agent tile activity UI.

Red:

1. Add frontend store/integration tests for `+ Claude`, debug tabs, chatter state, agent activity derivation, mentions, and topic relevance.

Expected failure signal:

1. Missing `+ Claude`.
2. No chatter tab or agent activity panel.

Green:

1. Add the `+ Claude` action and Tauri invocation support.
2. Extend frontend state/types for agents, topics, chatter, and debug tabs.
3. Update `DebugPane.svelte`, `Toolbar.svelte`, `TerminalTile.svelte`, and `App.svelte`.

Verification commands:

1. `npm run test:unit`
2. `npm run test:integration`
3. `npm run check`

Exit criteria:

1. The UI surfaces the new agent/chatter model end to end.

### Phase 5

Objective:

Add docs and the repo-local Claude skill, then close the PRD.

Red:

1. Add documentation/consistency checks for the normalized CLI/socket contract and the `/herd` skill.

Expected failure signal:

1. Docs or skill content reference stale commands.

Green:

1. Add `.claude/skills/herd/SKILL.md`.
2. Update `docs/socket-and-test-driver.md` and any relevant docs.
3. Mark the PRD completed after verification passes.

Verification commands:

1. `cargo test --manifest-path src-tauri/Cargo.toml`
2. `npm run test:unit`
3. `npm run test:integration`
4. `npm run check`

Exit criteria:

1. Docs and skill content match the implemented contract.

## Implementation Checklist

- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Phase 4 complete
- [x] Phase 5 complete
- [x] Integration/regression checks complete
- [x] PRD status updated to `Completed`
