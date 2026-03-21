## Title

Work Tiles As First-Class Canvas Tiles

## Status

Completed

## Date

2026-03-20

## Context

Work items currently render on the canvas as computed overlay cards. They do not own persisted layout entries, cannot be dragged independently, and cannot be deleted from the canvas. Terminal tiles already have a title bar, drag behavior, and persisted layout in SQLite-backed state, so the work-item canvas model should converge on that same pattern instead of staying synthetic.

## Goals

1. Give work tiles a title/drag bar like other tiles.
2. Make work tiles independently draggable on the canvas.
3. Persist work-tile layout through the existing layout state path.
4. Allow work tiles to be deleted from the canvas.
5. Cover the layout, delete, and interaction flows with unit and integration tests.

## Non-goals

1. Turning work tiles into tmux-backed panes.
2. Adding resize handles to work tiles in this change.
3. Changing the work review flow beyond making the tile behave like a first-class canvas object.

## Scope

In scope:
- work tile layout keys in the shared layout store
- work deletion backend/frontend wiring
- work-card titlebar drag/delete UI
- targeted test coverage

Out of scope:
- work stage/file content model changes
- terminal tile interaction changes unrelated to work tiles

## Risks And Mitigations

1. Tmux snapshot reconciliation could drop non-window layout keys.
   - Mitigation: explicitly preserve work layout entries across snapshot refreshes.
2. Deleting a work item could leave stale layout entries behind.
   - Mitigation: remove the matching layout key locally and from persisted layout state as part of delete.
3. Dragging work tiles could interfere with existing card content interactions.
   - Mitigation: restrict drag start to the title bar and keep content controls stop-propagating.

## Acceptance Criteria

1. Work tiles render with a visible title bar.
2. Dragging a work tile by its title bar updates its canvas position.
3. Work-tile positions persist across state refreshes/reloads.
4. Deleting a work tile removes the work item and its persisted layout entry.
5. Work cards still support review actions and content interaction after the titlebar change.

## Phased Plan

### Phase 1: Persisted Work-Tile Layout

#### Objective

Make work tiles first-class layout entries instead of computed-only overlays.

#### Red

1. Add failing unit coverage proving work layout entries are lost on state reconciliation and that canvas work cards ignore persisted layout entries.
2. Capture the failure signal from the current synthetic `buildCanvasWorkCards` path.

#### Green

1. Introduce stable work layout keys.
2. Preserve work layout entries during tmux snapshot reconciliation.
3. Use persisted layout entries when building canvas work cards.
4. Create default work layout entries for new work items.

#### Exit Criteria

1. Work cards use persisted layout when present.
2. Work layout survives tmux snapshot refreshes.

### Phase 2: Delete Flow

#### Objective

Add a backend/frontend delete path for work items and clean up layout state.

#### Red

1. Add failing backend tests for deleting a work item and cleaning up related state.
2. Add failing frontend/unit coverage for removing the layout entry when a work item disappears.

#### Green

1. Add a backend work delete operation and Tauri command.
2. Add a frontend delete API wrapper.
3. Remove the matching work layout key from in-memory and persisted layout state on delete.

#### Exit Criteria

1. A deleted work item no longer appears in work lists or on the canvas.
2. Its layout key is removed from persisted state.

### Phase 3: Titlebar Drag/Delete UI And Regression Coverage

#### Objective

Give work tiles the same basic canvas affordances as regular tiles and prove them in tests.

#### Red

1. Add failing integration coverage for dragging a work tile and deleting it from the canvas.
2. Capture the current failure signal from the static card UI.

#### Green

1. Add a work-tile title bar with drag affordance and delete control.
2. Drag only from the title bar; keep content interactions intact.
3. Add targeted integration coverage for drag/delete behavior.

#### Exit Criteria

1. Work tiles can be dragged and deleted directly from the canvas.
2. Review controls still work after the titlebar change.

## Execution Checklist

- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Integration/regression checks complete
- [x] Documentation/status updated
