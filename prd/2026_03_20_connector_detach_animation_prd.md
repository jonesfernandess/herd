## Title

Connector Detach Animation And Recessed Port Styling

## Status

Completed

## Date

2026-03-20

## Context

Network connectors currently render as curved links and snap to nearby ports, but the ports still read as floating dots instead of recessed frame features. Occupied-port drags also keep the original connection semantics centered on the grabbed tile, which makes disconnect gestures feel wrong and gives no release animation when a detached link is dropped.

## Goals

1. Restyle tile ports so they read as recessed frame indents instead of floating dots.
2. Add a clear hover affordance for available ports.
3. Make occupied-port drags detach from the grabbed tile and stay anchored to the opposite endpoint.
4. Animate canceling a detached drag so the loose end retracts back to the anchored tile and disappears.

## Non-goals

1. Changing backend network validation rules.
2. Reworking creator-lineage connectors.
3. Adding physics or multi-step routing for network edges.

## Scope

In scope:

1. Frontend port styling in tile components.
2. Frontend network drag state for detached occupied links.
3. Frontend connector cancel animation and rendering filters.
4. Targeted store/unit tests for occupied-link detach behavior.

Out of scope:

1. Backend persistence model changes.
2. New socket or test-driver protocol changes.

## Risks And Mitigations

1. Detach behavior can accidentally reconnect the wrong endpoint.
   - Mitigation: store the original connection and explicitly preserve the opposite endpoint while dragging an occupied link.
2. Release animation can overlap the persisted connection until backend refresh catches up.
   - Mitigation: hide the detached connection from normal rendering while dragging and during the transient retract animation.
3. Recessed port styling can look inconsistent across tile types.
   - Mitigation: keep the port geometry shared and inherit the tile frame palette from existing component borders.

## Acceptance Criteria

1. Ports render as recessed edge indents rather than floating circles.
2. Hovering a port produces a visible cue without obscuring the frame.
3. Dragging from an occupied port visually detaches the link from that tile and anchors the loose end to the opposite tile.
4. Releasing an occupied detached link without reconnecting retracts the loose end back to the anchored tile and removes the link.
5. Reconnecting an occupied detached link preserves the opposite endpoint and connects it to the new target.

## Phased Plan

### Phase 0: PRD And Failing Coverage

#### Objective

Document the interaction model and add failing checks for occupied-link detach semantics.

#### Red

1. Add failing tests that show:
   - occupied drags still preserve the grabbed endpoint instead of the opposite endpoint
   - canceling an occupied drag only disconnects, with no detached-link state to animate

Expected failure signal:

1. `completeNetworkPortDrag` reconnects from the grabbed endpoint
2. drag state does not expose the anchored opposite endpoint for occupied links

#### Green

1. Create this PRD.
2. Add targeted tests covering anchored detach and cancel behavior.

Verification commands:

1. `npm run test:unit -- src/lib/stores/appState.test.ts`

#### Exit Criteria

1. This PRD exists in `prd/`.
2. The new targeted tests fail for the expected interaction reasons before implementation.

### Phase 1: Detach Interaction And Rendering

#### Objective

Implement occupied-link detach semantics and the retract animation.

#### Red

1. Existing tests fail or no retract animation state exists.

#### Green

1. Update network drag state to preserve the opposite endpoint for occupied drags.
2. Hide detached live links from the normal render pass.
3. Add a transient retract animation state for canceling a detached drag.
4. Render the detached draft and retract animation from the anchored endpoint.

Verification commands:

1. `npm run test:unit -- src/lib/stores/appState.test.ts`
2. `npm run check`

#### Exit Criteria

1. Occupied drags behave as detached links.
2. Canceling a detached drag produces the retract animation.

### Phase 2: Port Styling

#### Objective

Restyle the ports as recessed frame indents with hover feedback.

#### Red

1. Ports still render as floating circular handles.

#### Green

1. Update port geometry and CSS to read as recessed edge sockets.
2. Add hover and snapped-state affordances that fit the tile frame style.

Verification commands:

1. `npm run check`

#### Exit Criteria

1. Ports visually read as part of the tile frame and expose a clear hover cue.

## Execution Checklist

- [x] Phase 0 complete
- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Integration/regression checks complete
- [x] Documentation/status updated

## Command Log

1. `sed -n '1,220p' /Users/skryl/.codex/skills/phased-prd-red-green/SKILL.md`
   - result: pass
   - notes: loaded the required skill workflow
2. `rg -n "tile-port|network-draft|network-line|completeNetworkPortDrag|beginNetworkPortDrag" src/lib`
   - result: pass
   - notes: identified the connector render and drag paths to change
3. `npm run test:unit -- src/lib/stores/appState.test.ts`
   - result: fail
   - notes: occupied drags still preserved the grabbed endpoint and had no retract animation state
4. `npm run test:unit -- src/lib/stores/appState.test.ts`
   - result: pass
   - notes: occupied drags now detach from the opposite endpoint and start a retract animation on cancel
5. `npm run check`
   - result: pass
   - notes: Svelte and TypeScript checks passed with the new canvas animation and port styling
