import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AppStateTree, TmuxSnapshot, WorkItem } from '../types';

const tauriMocks = vi.hoisted(() => ({
  approveWorkItem: vi.fn(),
  connectNetworkTiles: vi.fn(),
  createWorkItem: vi.fn(),
  deleteWorkItem: vi.fn(),
  disconnectNetworkPort: vi.fn(),
  sendDirectMessageCommand: vi.fn(),
  sendPublicMessageCommand: vi.fn(),
  getAgentDebugState: vi.fn(),
  getClaudeMenuDataForPane: vi.fn(),
  getLayoutState: vi.fn(),
  getTmuxState: vi.fn(),
  getWorkItems: vi.fn(),
  improveWorkItem: vi.fn(),
  killPane: vi.fn(),
  killSession: vi.fn(),
  killWindow: vi.fn(),
  newSession: vi.fn(),
  newWindow: vi.fn(),
  readWorkStagePreview: vi.fn(),
  renameSession: vi.fn(),
  renameWindow: vi.fn(),
  resizeWindow: vi.fn(),
  saveLayoutState: vi.fn(),
  sendRootMessageCommand: vi.fn(),
  selectSession: vi.fn(),
  selectWindow: vi.fn(),
  setPaneTitle: vi.fn(),
  spawnBrowserWindow: vi.fn(),
  spawnAgentWindow: vi.fn(),
  writePane: vi.fn(),
}));

vi.mock('../tauri', () => tauriMocks);

import {
  __resetWindowResizeTrackingForTest,
  agentInfos,
  applyPaneReadOnlyToState,
  applyPaneRoleToState,
  applyAgentDebugStateToState,
  applyTmuxSnapshot,
  applyTmuxSnapshotToState,
  appState,
  activeNetworkDrag,
  appendChatterEntryToState,
  applyWorkItemsToState,
  autoArrange,
  beginNetworkPortDrag,
  beginSidebarRename,
  bootstrapAppState,
  buildCanvasWorkCards,
  buildContextMenuItems,
  buildCanvasConnections,
  buildAgentActivityEntries,
  clientDeltaToWorldDelta,
  buildRenderedNetworkConnections,
  buildSidebarItems,
  buildSidebarRenameCommand,
  calculateWindowSizeRequest,
  completeNetworkPortDrag,
  dismissContextMenuInState,
  initialAppState,
  networkReleaseAnimation,
  openCanvasContextMenuInState,
  openPaneContextMenuInState,
  parseCommandBarCommand,
  reduceContextMenuSelection,
  reportPaneViewport,
  reduceIntent,
  executeCommandBarCommand,
  activeSessionWorkItems,
  topicInfos,
  updateNetworkPortDrag,
} from './appState';

function freshState(): AppStateTree {
  return JSON.parse(JSON.stringify(initialAppState)) as AppStateTree;
}

function baseSnapshot(): TmuxSnapshot {
  return {
    version: 1,
    server_name: 'herd',
    active_session_id: '$1',
    active_window_id: '@1',
    active_pane_id: '%1',
    sessions: [
      { id: '$1', name: 'Main', active: true, window_ids: ['@1', '@2'], active_window_id: '@1', root_cwd: '/Users/skryl/Dev/herd' },
      { id: '$2', name: 'Build', active: false, window_ids: ['@3'], active_window_id: '@3', root_cwd: '/Users/skryl/Dev/herd/src-tauri' },
    ],
    windows: [
      { id: '@1', session_id: '$1', session_name: 'Main', index: 0, name: 'shell', active: true, cols: 80, rows: 24, pane_ids: ['%1'] },
      { id: '@2', session_id: '$1', session_name: 'Main', index: 1, name: 'logs', active: false, cols: 90, rows: 28, pane_ids: ['%2'] },
      { id: '@3', session_id: '$2', session_name: 'Build', index: 0, name: 'build', active: true, cols: 100, rows: 30, pane_ids: ['%3'] },
    ],
    panes: [
      { id: '%1', session_id: '$1', window_id: '@1', window_index: 0, pane_index: 0, cols: 80, rows: 24, title: 'shell', command: 'zsh', active: true, dead: false },
      { id: '%2', session_id: '$1', window_id: '@2', window_index: 1, pane_index: 0, cols: 90, rows: 28, title: 'logs', command: 'tail', active: false, dead: false },
      { id: '%3', session_id: '$2', window_id: '@3', window_index: 0, pane_index: 0, cols: 100, rows: 30, title: 'build', command: 'npm', active: true, dead: false },
    ],
  };
}

function snapshotWithMainWindowCount(count: number): TmuxSnapshot {
  const snapshot = baseSnapshot();
  if (count <= 2) return snapshot;

  const extraWindowIds: string[] = [];
  const extraWindows = [];
  const extraPanes = [];

  for (let offset = 0; offset < count - 2; offset += 1) {
    const windowNumber = offset + 4;
    const paneNumber = offset + 4;
    const windowId = `@${windowNumber}`;
    const paneId = `%${paneNumber}`;
    extraWindowIds.push(windowId);
    extraWindows.push({
      id: windowId,
      session_id: '$1',
      session_name: 'Main',
      index: offset + 2,
      name: `shell-${offset + 3}`,
      active: false,
      cols: 80,
      rows: 24,
      pane_ids: [paneId],
    });
    extraPanes.push({
      id: paneId,
      session_id: '$1',
      window_id: windowId,
      window_index: offset + 2,
      pane_index: 0,
      cols: 80,
      rows: 24,
      title: `shell-${offset + 3}`,
      command: 'zsh',
      active: false,
      dead: false,
    });
  }

  return {
    ...snapshot,
    sessions: [
      {
        ...snapshot.sessions[0],
        window_ids: ['@1', '@2', ...extraWindowIds],
      },
      snapshot.sessions[1],
    ],
    windows: [...snapshot.windows, ...extraWindows],
    panes: [...snapshot.panes, ...extraPanes],
  };
}

function entriesOverlap(
  left: { x: number; y: number; width: number; height: number },
  right: { x: number; y: number; width: number; height: number },
): boolean {
  return (
    left.x < right.x + right.width &&
    left.x + left.width > right.x &&
    left.y < right.y + right.height &&
    left.y + left.height > right.y
  );
}

beforeEach(() => {
  appState.set(freshState());
  activeNetworkDrag.set(null);
  networkReleaseAnimation.set(null);
  __resetWindowResizeTrackingForTest();
  Object.values(tauriMocks).forEach((mockFn) => mockFn.mockReset());
  tauriMocks.getClaudeMenuDataForPane.mockResolvedValue({ commands: [], skills: [] });
  tauriMocks.getAgentDebugState.mockResolvedValue({ agents: [], topics: [], chatter: [], agent_logs: [], connections: [] });
  tauriMocks.getWorkItems.mockResolvedValue([]);
  tauriMocks.resizeWindow.mockResolvedValue(undefined);
  tauriMocks.spawnAgentWindow.mockResolvedValue(undefined);
  tauriMocks.connectNetworkTiles.mockResolvedValue(undefined);
  tauriMocks.disconnectNetworkPort.mockResolvedValue(null);
});

function sampleWorkItem(overrides: Partial<WorkItem> = {}): WorkItem {
  return {
    work_id: 'work-s1-001',
    session_id: '$1',
    title: 'Socket refactor',
    topic: '#work-s1-001',
    owner_agent_id: null,
    current_stage: 'plan',
    stages: [
      { stage: 'plan', status: 'ready', file_path: '/tmp/plan.md' },
      { stage: 'prd', status: 'ready', file_path: '/tmp/prd.md' },
      { stage: 'artifact', status: 'ready', file_path: '/tmp/artifact.md' },
    ],
    reviews: [],
    created_at: 1,
    updated_at: 1,
    ...overrides,
  };
}

describe('applyTmuxSnapshotToState', () => {
  it('hydrates tmux sessions, windows, and tile layout from the snapshot', () => {
    const next = applyTmuxSnapshotToState(freshState(), baseSnapshot());

    expect(next.tmux.serverName).toBe('herd');
    expect(next.tmux.activeSessionId).toBe('$1');
    expect(next.tmux.activeWindowId).toBe('@1');
    expect(next.tmux.activePaneId).toBe('%1');
    expect(next.tmux.sessionOrder).toEqual(['$1', '$2']);
    expect(next.tmux.windowOrder).toEqual(['@1', '@2', '@3']);
    expect(next.tmux.sessions['$1'].root_cwd).toBe('/Users/skryl/Dev/herd');
    expect(next.ui.selectedPaneId).toBe('%1');
    expect(Object.keys(next.layout.entries)).toEqual(['@1', '@2', '@3']);
  });

  it('drops stale layout entries and preserves read-only pane metadata', () => {
    const withSnapshot = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    withSnapshot.layout.entries['@1'] = { x: 10, y: 20, width: 500, height: 300 };
    withSnapshot.layout.entries['@9'] = { x: 1, y: 1, width: 1, height: 1 };
    withSnapshot.layout.entries['work:work-s1-001'] = { x: 1400, y: 120, width: 360, height: 320 };
    const readOnlyState = applyPaneReadOnlyToState(withSnapshot, '%2', true);

    const next = applyTmuxSnapshotToState(readOnlyState, {
      ...baseSnapshot(),
      version: 2,
      sessions: [
        { id: '$1', name: 'Main', active: true, window_ids: ['@1'], active_window_id: '@1' },
      ],
      windows: [
        { id: '@1', session_id: '$1', session_name: 'Main', index: 0, name: 'shell', active: true, cols: 80, rows: 24, pane_ids: ['%1'] },
      ],
      panes: [baseSnapshot().panes[0]],
      active_session_id: '$1',
      active_window_id: '@1',
      active_pane_id: '%1',
    });

    expect(next.layout.entries['@1']).toEqual({ x: 10, y: 20, width: 500, height: 300 });
    expect(next.layout.entries['@9']).toBeUndefined();
    expect(next.layout.entries['work:work-s1-001']).toEqual({ x: 1400, y: 120, width: 360, height: 320 });
    expect(next.tmux.panes['%2']).toBeUndefined();
  });

  it('preserves tile layout entries when switching tabs between sessions', () => {
    const initial = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    initial.layout.entries['@1'] = { x: 10, y: 20, width: 500, height: 300 };
    initial.layout.entries['@3'] = { x: 700, y: 40, width: 640, height: 400 };

    const switched = applyTmuxSnapshotToState(initial, {
      ...baseSnapshot(),
      version: 2,
      active_session_id: '$2',
      active_window_id: '@3',
      active_pane_id: '%3',
      sessions: [
        { id: '$1', name: 'Main', active: false, window_ids: ['@1', '@2'], active_window_id: '@1' },
        { id: '$2', name: 'Build', active: true, window_ids: ['@3'], active_window_id: '@3' },
      ],
      windows: [
        { ...baseSnapshot().windows[0], active: true },
        { ...baseSnapshot().windows[1], active: false },
        { ...baseSnapshot().windows[2], active: true },
      ],
      panes: [
        { ...baseSnapshot().panes[0], active: true },
        { ...baseSnapshot().panes[1], active: false },
        { ...baseSnapshot().panes[2], active: true },
      ],
    });

    expect(switched.layout.entries['@1']).toEqual({ x: 10, y: 20, width: 500, height: 300 });
    expect(switched.layout.entries['@3']).toEqual({ x: 700, y: 40, width: 640, height: 400 });
    expect(switched.tmux.activeSessionId).toBe('$2');
    expect(switched.ui.selectedPaneId).toBe('%3');
  });

  it('places new child windows next to their parent window', () => {
    const next = applyTmuxSnapshotToState(freshState(), {
      ...baseSnapshot(),
      version: 2,
      sessions: [
        { id: '$1', name: 'Main', active: true, window_ids: ['@1', '@2', '@4'], active_window_id: '@1' },
        { id: '$2', name: 'Build', active: false, window_ids: ['@3'], active_window_id: '@3' },
      ],
      windows: [
        { ...baseSnapshot().windows[0] },
        { ...baseSnapshot().windows[1] },
        { ...baseSnapshot().windows[2] },
        {
          id: '@4',
          session_id: '$1',
          session_name: 'Main',
          index: 2,
          name: 'agent',
          active: false,
          cols: 80,
          rows: 24,
          pane_ids: ['%4'],
          parent_window_id: '@1',
        },
      ],
      panes: [
        ...baseSnapshot().panes,
        {
          id: '%4',
          session_id: '$1',
          window_id: '@4',
          window_index: 2,
          pane_index: 0,
          cols: 80,
          rows: 24,
          title: 'agent',
          command: 'claude',
          active: false,
          dead: false,
        },
      ],
    });

    expect(next.layout.entries['@4'].x).toBeGreaterThan(next.layout.entries['@1'].x + next.layout.entries['@1'].width);
    expect(Math.abs(next.layout.entries['@4'].y - next.layout.entries['@1'].y)).toBeLessThanOrEqual(60);
  });
});

describe('network connectors', () => {
  it('builds curved network connector control points', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 0, y: 0, width: 240, height: 160 };
    state.layout.entries['@2'] = { x: 420, y: 40, width: 260, height: 180 };
    state.network.connections = [{
      session_id: '$1',
      from_tile_id: '%1',
      from_port: 'right',
      to_tile_id: '%2',
      to_port: 'left',
    }];

    const [connection] = buildRenderedNetworkConnections(state);
    expect(connection).toMatchObject({
      fromTileId: '%1',
      fromPort: 'right',
      toTileId: '%2',
      toPort: 'left',
      x1: 240,
      y1: 80,
      x2: 420,
      y2: 130,
    });
    expect(connection.cx1).toBeGreaterThan(connection.x1);
    expect(connection.cx2).toBeLessThan(connection.x2);
  });

  it('snaps network drags to the nearest valid target port and completes on mouseup', async () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 0, y: 0, width: 240, height: 160 };
    state.layout.entries['@2'] = { x: 420, y: 40, width: 260, height: 180 };
    appState.set(state);

    beginNetworkPortDrag('%1', 'right', 240, 80);
    updateNetworkPortDrag(405, 136);

    expect(get(activeNetworkDrag)).toMatchObject({
      snappedTileId: '%2',
      snappedPort: 'left',
      snappedX: 420,
      snappedY: 130,
    });

    await completeNetworkPortDrag();

    expect(tauriMocks.connectNetworkTiles).toHaveBeenCalledWith('%1', 'right', '%2', 'left');
  });

  it('detaches occupied drags from the opposite endpoint', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 0, y: 0, width: 240, height: 160 };
    state.layout.entries['@2'] = { x: 420, y: 40, width: 260, height: 180 };
    state.network.connections = [{
      session_id: '$1',
      from_tile_id: '%1',
      from_port: 'right',
      to_tile_id: '%2',
      to_port: 'left',
    }];
    appState.set(state);

    beginNetworkPortDrag('%1', 'right', 240, 80);

    expect(get(activeNetworkDrag)).toMatchObject({
      tileId: '%2',
      port: 'left',
      grabbedTileId: '%1',
      grabbedPort: 'right',
      startX: 420,
      startY: 130,
      currentX: 240,
      currentY: 80,
      startedOccupied: true,
    });
  });

  it('reconnects occupied drags from the anchored endpoint', async () => {
    const snapshot = baseSnapshot();
    snapshot.sessions[0].window_ids.push('@4');
    snapshot.windows.push({
      id: '@4',
      session_id: '$1',
      session_name: 'Main',
      index: 2,
      name: 'worker',
      active: false,
      cols: 80,
      rows: 24,
      pane_ids: ['%4'],
    });
    snapshot.panes.push({
      id: '%4',
      session_id: '$1',
      window_id: '@4',
      window_index: 2,
      pane_index: 0,
      cols: 80,
      rows: 24,
      title: 'worker',
      command: 'zsh',
      active: false,
      dead: false,
    });

    const state = applyTmuxSnapshotToState(freshState(), snapshot);
    state.layout.entries['@1'] = { x: 0, y: 0, width: 240, height: 160 };
    state.layout.entries['@2'] = { x: 420, y: 40, width: 260, height: 180 };
    state.layout.entries['@4'] = { x: 780, y: 40, width: 260, height: 180 };
    state.network.connections = [{
      session_id: '$1',
      from_tile_id: '%1',
      from_port: 'right',
      to_tile_id: '%2',
      to_port: 'left',
    }];
    appState.set(state);

    beginNetworkPortDrag('%1', 'right', 240, 80);
    await completeNetworkPortDrag('%4', 'left');

    expect(tauriMocks.connectNetworkTiles).toHaveBeenCalledWith('%2', 'left', '%4', 'left');
  });

  it('starts a retract animation when an occupied drag is released without reconnecting', async () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 0, y: 0, width: 240, height: 160 };
    state.layout.entries['@2'] = { x: 420, y: 40, width: 260, height: 180 };
    state.network.connections = [{
      session_id: '$1',
      from_tile_id: '%1',
      from_port: 'right',
      to_tile_id: '%2',
      to_port: 'left',
    }];
    appState.set(state);

    beginNetworkPortDrag('%1', 'right', 240, 80);
    updateNetworkPortDrag(332, 108);
    await completeNetworkPortDrag();

    expect(tauriMocks.disconnectNetworkPort).toHaveBeenCalledWith('%2', 'left');
    expect(get(networkReleaseAnimation)).toMatchObject({
      anchorTileId: '%2',
      anchorPort: 'left',
      anchorX: 420,
      anchorY: 130,
    });
  });
});

describe('work state', () => {
  it('bootstraps current-session work items from tauri', async () => {
    tauriMocks.getLayoutState.mockResolvedValue({});
    tauriMocks.getTmuxState.mockResolvedValue(baseSnapshot());
    tauriMocks.getAgentDebugState.mockResolvedValue({ agents: [], topics: [], chatter: [], agent_logs: [], connections: [] });
    tauriMocks.getWorkItems.mockResolvedValue([
      sampleWorkItem(),
      sampleWorkItem({
        work_id: 'work-s1-002',
        title: 'PRD review',
        current_stage: 'prd',
        stages: [
          { stage: 'plan', status: 'approved', file_path: '/tmp/plan-2.md' },
          { stage: 'prd', status: 'completed', file_path: '/tmp/prd-2.md' },
          { stage: 'artifact', status: 'ready', file_path: '/tmp/artifact-2.md' },
        ],
        updated_at: 20,
      }),
    ]);

    await bootstrapAppState();

    const state = get(appState);
    expect(state.work.order).toEqual(['work-s1-001', 'work-s1-002']);
    expect(state.work.items['work-s1-002'].title).toBe('PRD review');
  });

  it('keeps work state on tmux snapshot updates and exposes current-session items', () => {
    const seeded = applyWorkItemsToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      [
        sampleWorkItem(),
        sampleWorkItem({
          work_id: 'work-s2-001',
          session_id: '$2',
          title: 'Artifact polish',
          topic: '#work-s2-001',
        }),
      ],
    );
    appState.set(seeded);

    const switched = applyTmuxSnapshotToState(seeded, {
      ...baseSnapshot(),
      version: 2,
      active_session_id: '$2',
      active_window_id: '@3',
      active_pane_id: '%3',
      sessions: [
        { id: '$1', name: 'Main', active: false, window_ids: ['@1', '@2'], active_window_id: '@1', root_cwd: '/Users/skryl/Dev/herd' },
        { id: '$2', name: 'Build', active: true, window_ids: ['@3'], active_window_id: '@3', root_cwd: '/Users/skryl/Dev/herd/src-tauri' },
      ],
    });
    appState.set(switched);

    expect(switched.work.order).toEqual(['work-s1-001', 'work-s2-001']);
    expect(get(activeSessionWorkItems).map((item) => item.work_id)).toEqual(['work-s2-001']);
  });
});

describe('session-scoped agent debug state', () => {
  it('normalizes pointer movement by the current canvas zoom', () => {
    expect(clientDeltaToWorldDelta(40, 20, 2)).toEqual({ dx: 20, dy: 10 });
    expect(clientDeltaToWorldDelta(40, 20, 0.5)).toEqual({ dx: 80, dy: 40 });
  });

  it('keeps only active-session agents, topics, and chatter from debug snapshots', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const next = applyAgentDebugStateToState(state, {
      agents: [
        {
          agent_id: 'agent-1',
          agent_type: 'claude',
          agent_role: 'worker',
          tile_id: '%1',
          window_id: '@1',
          session_id: '$1',
          title: 'Agent',
          display_name: 'Agent 1',
          alive: true,
          chatter_subscribed: true,
          topics: ['#work-s1-001'],
        },
        {
          agent_id: 'agent-2',
          agent_type: 'claude',
          agent_role: 'worker',
          tile_id: '%3',
          window_id: '@3',
          session_id: '$2',
          title: 'Agent',
          display_name: 'Agent 2',
          alive: true,
          chatter_subscribed: true,
          topics: ['#work-s2-001'],
        },
      ],
      agent_logs: [],
      connections: [],
      topics: [
        { session_id: '$1', name: '#work-s1-001', subscriber_count: 1, last_activity_at: 10 },
        { session_id: '$2', name: '#work-s2-001', subscriber_count: 1, last_activity_at: 20 },
      ],
      chatter: [
        {
          session_id: '$1',
          kind: 'public',
          from_agent_id: 'agent-1',
          from_display_name: 'Agent 1',
          message: 'hello from main',
          to_agent_id: null,
          to_display_name: null,
          topics: ['#work-s1-001'],
          mentions: [],
          timestamp_ms: 1,
          public: true,
          display_text: 'Agent 1 -> Chatter: hello from main',
        },
        {
          session_id: '$2',
          kind: 'public',
          from_agent_id: 'agent-2',
          from_display_name: 'Agent 2',
          message: 'hello from build',
          to_agent_id: null,
          to_display_name: null,
          topics: ['#work-s2-001'],
          mentions: [],
          timestamp_ms: 2,
          public: true,
          display_text: 'Agent 2 -> Chatter: hello from build',
        },
      ],
    });

    expect(Object.keys(next.agents)).toEqual(['agent-1']);
    expect(Object.keys(next.topics)).toEqual(['#work-s1-001']);
    expect(next.chatter.map((entry) => entry.session_id)).toEqual(['$1']);
  });

  it('ignores chatter append events from other sessions and derives active-session registry views', () => {
    const seeded = applyAgentDebugStateToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      {
        agents: [
          {
            agent_id: 'agent-1',
            agent_type: 'claude',
            agent_role: 'worker',
            tile_id: '%1',
            window_id: '@1',
            session_id: '$1',
            title: 'Agent',
            display_name: 'Agent 1',
            alive: true,
            chatter_subscribed: true,
            topics: ['#work-s1-001'],
          },
          {
            agent_id: 'agent-2',
            agent_type: 'claude',
            agent_role: 'worker',
            tile_id: '%3',
            window_id: '@3',
            session_id: '$2',
            title: 'Agent',
            display_name: 'Agent 2',
            alive: true,
            chatter_subscribed: true,
            topics: ['#work-s2-001'],
          },
        ],
        agent_logs: [],
        connections: [],
        topics: [
          { session_id: '$1', name: '#work-s1-001', subscriber_count: 1, last_activity_at: 10 },
          { session_id: '$2', name: '#work-s2-001', subscriber_count: 1, last_activity_at: 20 },
        ],
        chatter: [],
      },
    );
    appState.set(seeded);

    appState.update((state) =>
      appendChatterEntryToState(state, {
        session_id: '$2',
        kind: 'public',
        from_agent_id: 'agent-2',
        from_display_name: 'Agent 2',
        message: 'foreign',
        to_agent_id: null,
        to_display_name: null,
        topics: ['#work-s2-001'],
        mentions: [],
        timestamp_ms: 99,
        public: true,
        display_text: 'Agent 2 -> Chatter: foreign',
      }),
    );

    expect(get(agentInfos).map((agent) => agent.agent_id)).toEqual(['agent-1']);
    expect(get(topicInfos).map((topic) => topic.name)).toEqual(['#work-s1-001']);
    expect(get(appState).chatter).toEqual([]);
  });

  it('merges persisted agent logs into agent activity entries in timestamp order', () => {
    const state = applyAgentDebugStateToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      {
        agents: [
          {
            agent_id: 'agent-1',
            agent_type: 'claude',
            agent_role: 'worker',
            tile_id: '%1',
            window_id: '@1',
            session_id: '$1',
            title: 'Agent',
            display_name: 'Agent 1',
            alive: true,
            chatter_subscribed: true,
            topics: [],
          },
        ],
        topics: [],
        chatter: [
          {
            session_id: '$1',
            kind: 'direct',
            from_agent_id: 'agent-2',
            from_display_name: 'Agent 2',
            to_agent_id: 'agent-1',
            to_display_name: 'Agent 1',
            message: 'hello',
            topics: [],
            mentions: [],
            timestamp_ms: 20,
            public: false,
            display_text: 'Agent 2 -> Agent 1: hello',
          },
        ],
        agent_logs: [
          {
            session_id: '$1',
            agent_id: 'agent-1',
            tile_id: '%1',
            kind: 'incoming_hook',
            text: 'MCP hook [system] Port connected: %1:left <-> work:work-s1-001:left',
            timestamp_ms: 10,
          },
          {
            session_id: '$1',
            agent_id: 'agent-1',
            tile_id: '%1',
            kind: 'outgoing_call',
            text: 'MCP call message_direct {"to_agent_id":"agent-2","message":"hello"}',
            timestamp_ms: 30,
          },
        ],
        connections: [],
      },
    );

    expect(buildAgentActivityEntries(state, '%1')).toEqual([
      {
        kind: 'incoming_hook',
        text: 'MCP hook [system] Port connected: %1:left <-> work:work-s1-001:left',
        timestamp_ms: 10,
      },
      {
        kind: 'incoming_dm',
        text: 'Agent 2 -> Agent 1: hello',
        timestamp_ms: 20,
      },
      {
        kind: 'outgoing_call',
        text: 'MCP call message_direct {"to_agent_id":"agent-2","message":"hello"}',
        timestamp_ms: 30,
      },
    ]);
  });
});

describe('reduceIntent', () => {
  it('maps new shell controls to tmux window creation in the active session', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const result = reduceIntent(state, { type: 'new-shell' });
    expect(result.effects).toEqual([{ type: 'new-window', sessionId: '$1' }]);
  });

  it('maps new tab controls to tmux session creation', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const result = reduceIntent(state, { type: 'new-tab' });
    expect(result.effects).toEqual([{ type: 'new-session' }]);
  });

  it('maps close tile control to a tmux kill effect when other windows remain', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    expect(reduceIntent(state, { type: 'close-selected-pane' }).effects).toEqual([
      { type: 'kill-window', windowId: '@1' },
    ]);
  });

  it('opens a confirmation dialog before closing a root agent pane', () => {
    const seeded = applyPaneRoleToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      '%1',
      'root_agent',
    );

    const result = reduceIntent(seeded, { type: 'close-selected-pane' });
    expect(result.effects).toEqual([]);
    expect(result.state.ui.closePaneConfirmation).toEqual({
      paneId: '%1',
      title: 'CLOSE ROOT AGENT',
      message: 'Close this Root agent? Herd will restart it automatically.',
      confirmLabel: 'Close Root Agent',
    });
  });

  it('confirms a root agent close by killing its window', () => {
    const seeded = applyPaneRoleToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      '%1',
      'root_agent',
    );
    const withDialog = reduceIntent(seeded, { type: 'close-selected-pane' }).state;

    const result = reduceIntent(withDialog, { type: 'confirm-close-pane' });
    expect(result.effects).toEqual([{ type: 'kill-window', windowId: '@1' }]);
    expect(result.state.ui.closePaneConfirmation).toBeNull();
  });

  it('requests confirmation before closing the last window because it would kill the session', () => {
    const state = applyTmuxSnapshotToState(freshState(), {
      ...baseSnapshot(),
      version: 2,
      active_session_id: '$2',
      active_window_id: '@3',
      active_pane_id: '%3',
      sessions: [
        { id: '$1', name: 'Main', active: false, window_ids: ['@1', '@2'], active_window_id: '@1' },
        { id: '$2', name: 'Build', active: true, window_ids: ['@3'], active_window_id: '@3' },
      ],
    });

    const closePane = reduceIntent(state, { type: 'close-selected-pane' });
    expect(closePane.effects).toEqual([]);
    expect(closePane.state.ui.closeTabConfirmation).toEqual({
      sessionId: '$2',
      sessionName: 'Build',
      paneCount: 1,
    });
  });

  it('requests confirmation for multi-pane tab closes', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const closeTab = reduceIntent(state, { type: 'close-active-tab' });
    expect(closeTab.effects).toEqual([]);
    expect(closeTab.state.ui.closeTabConfirmation).toEqual({
      sessionId: '$1',
      sessionName: 'Main',
      paneCount: 2,
    });
  });

  it('kills the active tab immediately when only one pane would be removed', () => {
    const state = applyTmuxSnapshotToState(freshState(), {
      ...baseSnapshot(),
      version: 2,
      active_session_id: '$2',
      active_window_id: '@3',
      active_pane_id: '%3',
      sessions: [
        { id: '$1', name: 'Main', active: false, window_ids: ['@1', '@2'], active_window_id: '@1' },
        { id: '$2', name: 'Build', active: true, window_ids: ['@3'], active_window_id: '@3' },
      ],
    });

    const closeTab = reduceIntent(state, { type: 'close-active-tab' });
    expect(closeTab.effects).toEqual([{ type: 'kill-session', sessionId: '$2' }]);
    expect(closeTab.state.ui.closeTabConfirmation).toBeNull();
  });

  it('confirms and cancels pending tab closes through ui state', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const requested = reduceIntent(state, { type: 'close-active-tab' }).state;

    const cancelled = reduceIntent(requested, { type: 'cancel-close-active-tab' });
    expect(cancelled.effects).toEqual([]);
    expect(cancelled.state.ui.closeTabConfirmation).toBeNull();

    const confirmed = reduceIntent(requested, { type: 'confirm-close-active-tab' });
    expect(confirmed.effects).toEqual([{ type: 'kill-session', sessionId: '$1' }]);
    expect(confirmed.state.ui.closeTabConfirmation).toBeNull();
  });

  it('maps next and previous tab controls to tmux session selection', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    expect(reduceIntent(state, { type: 'select-next-tab' }).effects).toEqual([
      { type: 'select-session', sessionId: '$2' },
    ]);
    expect(reduceIntent(state, { type: 'select-prev-tab' }).effects).toEqual([
      { type: 'select-session', sessionId: '$2' },
    ]);
  });

  it('updates local ui state for overlays and mode changes', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state = reduceIntent(state, { type: 'toggle-sidebar' }).state;
    state = reduceIntent(state, { type: 'toggle-debug' }).state;
    state = reduceIntent(state, { type: 'open-command-bar' }).state;
    state = reduceIntent(state, { type: 'open-help' }).state;
    state = reduceIntent(state, { type: 'enter-input-mode' }).state;

    expect(state.ui.sidebarOpen).toBe(true);
    expect(state.ui.debugPaneOpen).toBe(true);
    expect(state.ui.commandBarOpen).toBe(true);
    expect(state.ui.helpOpen).toBe(true);
    expect(state.ui.mode).toBe('input');

    state = reduceIntent(state, { type: 'exit-input-mode' }).state;
    expect(state.ui.mode).toBe('command');
  });

  it('maps typed input to a pane write effect and keeps move/reset local', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const send = reduceIntent(state, { type: 'send-input', data: 'ls\r' });
    expect(send.effects).toEqual([{ type: 'write-pane', paneId: '%1', data: 'ls\r' }]);

    state = reduceIntent(state, { type: 'move-selected-pane', dx: 25, dy: 15 }).state;
    expect(state.layout.entries['@1'].x).toBeGreaterThan(0);
    expect(state.layout.entries['@1'].y).toBeGreaterThan(0);

    state.ui.canvas = { panX: 100, panY: 200, zoom: 2 };
    state = reduceIntent(state, { type: 'reset-canvas' }).state;
    expect(state.ui.canvas).toEqual({ panX: 0, panY: 0, zoom: 1 });
    expect(state.ui.zoomBookmark).toBeNull();
  });

  it('toggles focused zoom for the selected pane and restores the prior canvas', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.canvas = { panX: 33, panY: 44, zoom: 0.75 };

    state = reduceIntent(state, {
      type: 'toggle-selected-zoom',
      viewportWidth: 1000,
      viewportHeight: 600,
    }).state;

    expect(state.ui.zoomBookmark).toEqual({
      mode: 'focused',
      paneId: '%1',
      previousCanvas: { panX: 33, panY: 44, zoom: 0.75 },
    });
    expect(state.ui.canvas.zoom).toBeCloseTo(1.2);
    expect(state.ui.canvas.panX).toBeCloseTo(-4);
    expect(state.ui.canvas.panY).toBeCloseTo(-60);

    state = reduceIntent(state, {
      type: 'toggle-selected-zoom',
      viewportWidth: 1000,
      viewportHeight: 600,
    }).state;

    expect(state.ui.canvas).toEqual({ panX: 33, panY: 44, zoom: 0.75 });
    expect(state.ui.zoomBookmark).toBeNull();
  });

  it('toggles fullscreen zoom and keeps the original canvas bookmark when switching zoom modes', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.canvas = { panX: 12, panY: 24, zoom: 0.9 };

    state = reduceIntent(state, {
      type: 'toggle-selected-zoom',
      viewportWidth: 1000,
      viewportHeight: 600,
    }).state;

    state = reduceIntent(state, {
      type: 'toggle-selected-fullscreen-zoom',
      viewportWidth: 1000,
      viewportHeight: 600,
    }).state;

    expect(state.ui.zoomBookmark).toEqual({
      mode: 'fullscreen',
      paneId: '%1',
      previousCanvas: { panX: 12, panY: 24, zoom: 0.9 },
    });
    expect(state.ui.canvas.zoom).toBeCloseTo(1.5);
    expect(state.ui.canvas.panX).toBeCloseTo(-130);
    expect(state.ui.canvas.panY).toBeCloseTo(-150);

    state = reduceIntent(state, {
      type: 'toggle-selected-fullscreen-zoom',
      viewportWidth: 1000,
      viewportHeight: 600,
    }).state;

    expect(state.ui.canvas).toEqual({ panX: 12, panY: 24, zoom: 0.9 });
    expect(state.ui.zoomBookmark).toBeNull();
  });

  it('maps rename controls to session and window naming effects', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    expect(reduceIntent(state, { type: 'rename-selected-pane', name: 'server' }).effects).toEqual([
      { type: 'rename-window', windowId: '@1', name: 'server' },
    ]);
    expect(reduceIntent(state, { type: 'rename-active-tab', name: 'Ops' }).effects).toEqual([
      { type: 'rename-session', sessionId: '$1', name: 'Ops' },
    ]);
  });

  it('keeps the tmux tree local to the active tab and focuses panes without switching tabs', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const sidebarItems = buildSidebarItems(state);
    expect(sidebarItems.every((item) => item.sessionId === '$1')).toBe(true);

    const paneIndex = sidebarItems.findIndex((item) => item.paneId === '%2');
    const result = reduceIntent(state, { type: 'set-sidebar-selection', index: paneIndex });

    expect(result.state.ui.sidebarSelectedIdx).toBe(paneIndex);
    expect(result.state.ui.selectedPaneId).toBe('%2');
    expect(result.effects).toEqual([{ type: 'select-window', windowId: '@2' }]);
  });

  it('moves focus between sidebar sections and uses section-local j/k navigation', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state = applyWorkItemsToState(state, [
      sampleWorkItem(),
      sampleWorkItem({
        work_id: 'work-s1-002',
        title: 'Artifact polish',
        topic: '#work-s1-002',
      }),
    ]);
    state = applyAgentDebugStateToState(state, {
      agents: [
        {
          agent_id: 'agent-1',
          agent_type: 'claude',
          agent_role: 'worker',
          tile_id: '%2',
          window_id: '@2',
          session_id: '$1',
          title: 'Agent',
          display_name: 'Agent 1',
          alive: true,
          chatter_subscribed: true,
          topics: ['#work-s1-002'],
        },
      ],
      agent_logs: [],
      connections: [],
      topics: [],
      chatter: [],
    });

    state = reduceIntent(state, { type: 'move-sidebar-section', delta: -1 }).state;
    expect(state.ui.sidebarSection).toBe('agents');
    expect(state.ui.selectedPaneId).toBe('%2');

    state = reduceIntent(state, { type: 'move-sidebar-section', delta: -1 }).state;
    expect(state.ui.sidebarSection).toBe('work');
    expect(state.ui.selectedWorkId).toBe('work-s1-001');

    state = reduceIntent(state, { type: 'move-sidebar-selection', delta: 1 }).state;
    expect(state.ui.selectedWorkId).toBe('work-s1-002');

    state = reduceIntent(state, { type: 'move-sidebar-section', delta: -1 }).state;
    expect(state.ui.sidebarSection).toBe('settings');

    state = reduceIntent(state, { type: 'move-sidebar-section', delta: 1 }).state;
    expect(state.ui.sidebarSection).toBe('work');
    expect(state.ui.selectedWorkId).toBe('work-s1-002');
  });

  it('shows a root-specific close label in the pane context menu', () => {
    const seeded = openPaneContextMenuInState(
      applyPaneRoleToState(
        applyTmuxSnapshotToState(freshState(), baseSnapshot()),
        '%1',
        'root_agent',
      ),
      '%1',
      420,
      240,
    );

    expect(buildContextMenuItems(seeded)).toEqual([
      { id: 'close-shell', label: 'Close Root Agent', kind: 'action', disabled: false },
    ]);
  });
});

describe('buildCanvasConnections', () => {
  it('builds a connection line for parent-child windows in the active tab', () => {
    const state = applyTmuxSnapshotToState(freshState(), {
      ...baseSnapshot(),
      windows: [
        { ...baseSnapshot().windows[0] },
        { ...baseSnapshot().windows[1], parent_window_id: '@1', parent_window_source: 'hook' },
        { ...baseSnapshot().windows[2] },
      ],
    });

    const connections = buildCanvasConnections(state);
    expect(connections).toHaveLength(1);
    expect(connections[0].parentWindowId).toBe('@1');
    expect(connections[0].childWindowId).toBe('@2');
  });

  it('does not draw manual parent-child lineage lines', () => {
    const state = applyTmuxSnapshotToState(freshState(), {
      ...baseSnapshot(),
      windows: [
        { ...baseSnapshot().windows[0] },
        { ...baseSnapshot().windows[1], parent_window_id: '@1', parent_window_source: 'manual' },
        { ...baseSnapshot().windows[2] },
      ],
    });

    expect(buildCanvasConnections(state)).toHaveLength(0);
  });
});

describe('buildCanvasWorkCards', () => {
  it('does not auto-select work just because the active tab has work items', () => {
    const state = applyWorkItemsToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      [sampleWorkItem()],
    );

    expect(state.ui.selectedWorkId).toBeNull();
    expect(state.ui.sidebarSection).toBe('tmux');
  });

  it('builds one work card per active-session item and places them to the right of terminal tiles', () => {
    const state = applyWorkItemsToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      [
        sampleWorkItem(),
        sampleWorkItem({
          work_id: 'work-s1-002',
          title: 'Artifact polish',
          topic: '#work-s1-002',
        }),
        sampleWorkItem({
          work_id: 'work-s2-001',
          session_id: '$2',
          title: 'Build PRD',
          topic: '#work-s2-001',
        }),
      ],
    );

    state.layout.entries['@1'] = { x: 100, y: 80, width: 640, height: 400 };
    state.layout.entries['@2'] = { x: 860, y: 120, width: 640, height: 400 };
    state.layout.entries['@3'] = { x: 40, y: 60, width: 640, height: 400 };

    const cards = buildCanvasWorkCards(state);

    expect(cards).toHaveLength(2);
    expect(cards.map((card) => card.workId)).toEqual(['work-s1-001', 'work-s1-002']);
    expect(cards[0].x).toBeGreaterThan(1500);
    expect(cards[0].y).toBe(80);
    expect(cards[1].y).toBeGreaterThan(cards[0].y + cards[0].height);
  });

  it('creates a persisted layout entry for new work items and uses that layout on the canvas', () => {
    const state = applyWorkItemsToState(
      applyTmuxSnapshotToState(freshState(), baseSnapshot()),
      [sampleWorkItem()],
    );

    expect(state.layout.entries['work:work-s1-001']).toEqual({
      x: expect.any(Number),
      y: expect.any(Number),
      width: 360,
      height: 320,
    });

    state.layout.entries['work:work-s1-001'] = {
      x: 2220,
      y: 420,
      width: 480,
      height: 360,
    };

    const cards = buildCanvasWorkCards(state);
    expect(cards).toEqual([
      {
        workId: 'work-s1-001',
        x: 2220,
        y: 420,
        width: 480,
        height: 360,
      },
    ]);
  });
});

describe('context menu state', () => {
  it('opens a canvas context menu with click-derived world coordinates and dismisses it locally', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.canvas = { panX: 100, panY: 50, zoom: 2 };

    state = openCanvasContextMenuInState(state, 320, 250);
    expect(state.ui.contextMenu).toEqual({
      open: true,
      target: 'canvas',
      paneId: null,
      clientX: 320,
      clientY: 250,
      worldX: 110,
      worldY: 100,
      claudeCommands: [],
      claudeSkills: [],
      loadingClaudeCommands: false,
      claudeCommandsError: null,
    });

    state = dismissContextMenuInState(state);
    expect(state.ui.contextMenu).toBeNull();
  });

  it('opens a pane context menu, selects the pane, and derives regular-tile actions', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state = openPaneContextMenuInState(state, '%2', 640, 360);

    expect(state.ui.selectedPaneId).toBe('%2');
    expect(state.ui.contextMenu?.target).toBe('pane');
    expect(state.ui.contextMenu?.paneId).toBe('%2');
    expect(buildContextMenuItems(state)).toEqual([
      { id: 'close-shell', label: 'Close Shell', kind: 'action', disabled: false },
    ]);
  });

  it('maps canvas New Shell selection to tmux window creation and a pending click placement', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.canvas = { panX: 100, panY: 50, zoom: 2 };
    state = openCanvasContextMenuInState(state, 320, 250);

    expect(buildContextMenuItems(state).map((item) => item.id)).toEqual([
      'new-shell',
      'new-agent',
      'new-browser',
      'new-work',
    ]);

    const selected = reduceContextMenuSelection(state, 'new-shell');
    expect(selected.effects).toEqual([{ type: 'new-window', sessionId: '$1' }]);
    expect(selected.state.ui.contextMenu).toBeNull();
    expect(selected.state.ui.pendingSpawnPlacement).toEqual({
      sessionId: '$1',
      worldX: 110,
      worldY: 100,
    });
  });

  it('maps canvas Agent, Browser, and Work selections to their matching actions', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.canvas = { panX: 100, panY: 50, zoom: 2 };
    state = openCanvasContextMenuInState(state, 320, 250);

    expect(reduceContextMenuSelection(state, 'new-agent').effects).toEqual([
      { type: 'new-agent-window', sessionId: '$1' },
    ]);

    expect(reduceContextMenuSelection(state, 'new-browser').effects).toEqual([
      { type: 'new-browser-window', sessionId: '$1' },
    ]);

    expect(reduceContextMenuSelection(state, 'new-work').effects).toEqual([
      {
        type: 'open-work-dialog',
        placement: { sessionId: '$1', worldX: 110, worldY: 100 },
      },
    ]);
  });

  it('applies the pending click placement to the next created window instead of default layout', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.canvas = { panX: 100, panY: 50, zoom: 2 };
    state = openCanvasContextMenuInState(state, 320, 250);
    state = reduceContextMenuSelection(state, 'new-shell').state;

    const next = applyTmuxSnapshotToState(state, {
      ...baseSnapshot(),
      version: 2,
      sessions: [
        { id: '$1', name: 'Main', active: true, window_ids: ['@1', '@2', '@4'], active_window_id: '@1' },
        { id: '$2', name: 'Build', active: false, window_ids: ['@3'], active_window_id: '@3' },
      ],
      windows: [
        ...baseSnapshot().windows,
        {
          id: '@4',
          session_id: '$1',
          session_name: 'Main',
          index: 2,
          name: 'shell-3',
          active: false,
          cols: 80,
          rows: 24,
          pane_ids: ['%4'],
        },
      ],
      panes: [
        ...baseSnapshot().panes,
        {
          id: '%4',
          session_id: '$1',
          window_id: '@4',
          window_index: 2,
          pane_index: 0,
          cols: 80,
          rows: 24,
          title: 'shell-3',
          command: 'zsh',
          active: false,
          dead: false,
        },
      ],
    });

    expect(next.layout.entries['@4']).toMatchObject({
      x: 120,
      y: 100,
    });
    expect(next.ui.pendingSpawnPlacement).toBeNull();
  });

  it('maps regular Close Shell selection through the existing close path', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state = openPaneContextMenuInState(state, '%2', 640, 360);

    const selected = reduceContextMenuSelection(state, 'close-shell');
    expect(selected.effects).toEqual([{ type: 'kill-window', windowId: '@2' }]);
    expect(selected.state.ui.contextMenu).toBeNull();
    expect(selected.state.ui.selectedPaneId).toBe('%2');
  });

  it('builds Claude-specific items for explicit Claude panes and excludes output panes', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state = applyPaneRoleToState(state, '%1', 'claude');
    state = openPaneContextMenuInState(state, '%1', 600, 320);

    expect(buildContextMenuItems(state)).toEqual([
      { id: 'claude-skills', label: 'Skills', kind: 'submenu', disabled: false, children: [{ id: 'skills-loading', label: 'Loading…', kind: 'status', disabled: true }] },
      { id: 'separator-skills', label: '', kind: 'separator', disabled: true },
      { id: 'close-shell', label: 'Close Shell', kind: 'action', disabled: false },
      { id: 'separator-claude', label: '', kind: 'separator', disabled: true },
      { id: 'claude-label', label: 'Claude Commands', kind: 'label', disabled: true },
      { id: 'claude-loading', label: 'Loading…', kind: 'status', disabled: true },
    ]);

    state = {
      ...state,
      ui: {
        ...state.ui,
        contextMenu: state.ui.contextMenu && {
          ...state.ui.contextMenu,
          loadingClaudeCommands: false,
          claudeCommands: [
            { name: 'clear', execution: 'execute', source: 'builtin' },
            { name: 'model', execution: 'insert', source: 'builtin' },
            { name: 'codex', execution: 'execute', source: 'skill' },
          ],
          claudeSkills: [
            { name: 'codex', execution: 'execute', source: 'skill' },
          ],
        },
      },
    };

    expect(buildContextMenuItems(state)).toEqual([
      {
        id: 'claude-skills',
        label: 'Skills',
        kind: 'submenu',
        disabled: false,
        children: [{ id: 'claude-command:codex', label: '/codex', kind: 'action', disabled: false }],
      },
      { id: 'separator-skills', label: '', kind: 'separator', disabled: true },
      { id: 'close-shell', label: 'Close Shell', kind: 'action', disabled: false },
      { id: 'separator-claude', label: '', kind: 'separator', disabled: true },
      { id: 'claude-label', label: 'Claude Commands', kind: 'label', disabled: true },
      { id: 'claude-command:clear', label: '/clear', kind: 'action', disabled: false },
      { id: 'claude-command:model', label: '/model', kind: 'action', disabled: false },
    ]);

    state = applyPaneRoleToState(state, '%1', 'output');
    expect(buildContextMenuItems(state)).toEqual([
      { id: 'close-shell', label: 'Close Shell', kind: 'action', disabled: false },
    ]);
  });

  it('routes Claude command items to execute-or-insert pane writes', () => {
    let state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state = applyPaneRoleToState(state, '%1', 'claude');
    state = openPaneContextMenuInState(state, '%1', 600, 320);
    state = {
      ...state,
      ui: {
        ...state.ui,
        contextMenu: state.ui.contextMenu && {
          ...state.ui.contextMenu,
          loadingClaudeCommands: false,
          claudeCommands: [
            { name: 'clear', execution: 'execute', source: 'builtin' },
            { name: 'model', execution: 'insert', source: 'builtin' },
          ],
          claudeSkills: [],
        },
      },
    };

    const executed = reduceContextMenuSelection(state, 'claude-command:clear');
    expect(executed.state.ui.contextMenu).toBeNull();
    expect(executed.state.ui.selectedPaneId).toBe('%1');
    expect(executed.effects).toEqual([{ type: 'write-pane', paneId: '%1', data: '/clear\r' }]);

    const inserted = reduceContextMenuSelection(state, 'claude-command:model');
    expect(inserted.effects).toEqual([{ type: 'write-pane', paneId: '%1', data: '/model ' }]);
  });
});

describe('parseCommandBarCommand', () => {
  it('maps shell, close, and closeall command bar verbs', () => {
    expect(parseCommandBarCommand('sh')).toEqual({ type: 'intent', intent: { type: 'new-shell' } });
    expect(parseCommandBarCommand('close')).toEqual({ type: 'intent', intent: { type: 'close-selected-pane' } });
    expect(parseCommandBarCommand('qa')).toEqual({ type: 'close-all' });
  });

  it('maps tab command bar verbs', () => {
    expect(parseCommandBarCommand('tn')).toEqual({ type: 'new-tab', name: undefined });
    expect(parseCommandBarCommand('tabnew Build')).toEqual({ type: 'new-tab', name: 'Build' });
    expect(parseCommandBarCommand('tc')).toEqual({ type: 'intent', intent: { type: 'close-active-tab' } });
    expect(parseCommandBarCommand('tr Ops')).toEqual({
      type: 'intent',
      intent: { type: 'rename-active-tab', name: 'Ops' },
    });
  });

  it('maps sudo command bar verbs', () => {
    expect(parseCommandBarCommand('sudo please inspect local work')).toEqual({
      type: 'sudo',
      message: 'please inspect local work',
    });
    expect(parseCommandBarCommand('sudo')).toEqual({ type: 'none' });
  });

  it('maps dm and cm command bar verbs', () => {
    expect(parseCommandBarCommand('dm 10 hi there')).toEqual({
      type: 'dm',
      target: '10',
      message: 'hi there',
    });
    expect(parseCommandBarCommand('cm hey all!')).toEqual({
      type: 'cm',
      message: 'hey all!',
    });
    expect(parseCommandBarCommand('dm')).toEqual({ type: 'none' });
    expect(parseCommandBarCommand('dm 10')).toEqual({ type: 'none' });
    expect(parseCommandBarCommand('cm')).toEqual({ type: 'none' });
  });
});

describe('executeCommandBarCommand', () => {
  it('routes sudo through the root message invoke', async () => {
    tauriMocks.sendRootMessageCommand.mockResolvedValue(undefined);

    await executeCommandBarCommand('sudo please inspect local work');

    expect(tauriMocks.sendRootMessageCommand).toHaveBeenCalledWith('please inspect local work');
  });

  it('routes dm and cm through the user message invokes', async () => {
    tauriMocks.sendDirectMessageCommand.mockResolvedValue(undefined);
    tauriMocks.sendPublicMessageCommand.mockResolvedValue(undefined);

    await executeCommandBarCommand('dm 10 hi there');
    await executeCommandBarCommand('cm hey all!');

    expect(tauriMocks.sendDirectMessageCommand).toHaveBeenCalledWith('10', 'hi there');
    expect(tauriMocks.sendPublicMessageCommand).toHaveBeenCalledWith('hey all!');
  });
});

describe('autoArrange', () => {
  it('anchors the first arrangement on the selected tile and persists the new layout', async () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 100, y: 100, width: 640, height: 400 };
    state.layout.entries['@2'] = { x: 880, y: 60, width: 640, height: 400 };
    state.ui.selectedPaneId = '%1';
    appState.set(state);

    await autoArrange('$1');

    const next = get(appState);
    expect(next.layout.entries['@1']).toEqual({ x: 100, y: 100, width: 640, height: 400 });
    expect(next.layout.entries['@2']).toEqual({ x: 100, y: -700, width: 640, height: 400 });
    expect(next.ui.arrangementModeBySession['$1']).toBe('circle');
    expect(next.ui.arrangementCycleBySession['$1']).toBe(1);
    expect(tauriMocks.saveLayoutState).toHaveBeenCalledTimes(2);
    expect(tauriMocks.saveLayoutState).toHaveBeenNthCalledWith(1, '@1', 100, 100, 640, 400);
    expect(tauriMocks.saveLayoutState).toHaveBeenNthCalledWith(2, '@2', 100, -700, 640, 400);
  });

  it('advances through the remaining arrangement cycle on repeated calls', async () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 100, y: 100, width: 640, height: 400 };
    state.layout.entries['@2'] = { x: 880, y: 60, width: 640, height: 400 };
    state.ui.selectedPaneId = '%1';
    appState.set(state);

    await autoArrange('$1');
    const first = get(appState).layout.entries['@2'];

    await autoArrange('$1');
    const second = get(appState).layout.entries['@2'];

    await autoArrange('$1');
    const third = get(appState).layout.entries['@2'];

    await autoArrange('$1');
    const fourth = get(appState).layout.entries['@2'];

    await autoArrange('$1');
    const fifth = get(appState).layout.entries['@2'];

    expect(first).toEqual({ x: 100, y: -700, width: 640, height: 400 });
    expect(second).toEqual({ x: 500, y: -580, width: 640, height: 400 });
    expect(third).toEqual({ x: 100, y: 540, width: 640, height: 400 });
    expect(fourth).toEqual({ x: 780, y: 100, width: 640, height: 400 });
    expect(fifth).toEqual({ x: 780, y: 100, width: 640, height: 400 });
  });

  it('adds circle and snowflake radial arrangements around the selected tile', async () => {
    const state = applyTmuxSnapshotToState(freshState(), snapshotWithMainWindowCount(7));
    for (const windowId of state.tmux.sessions['$1'].window_ids) {
      state.layout.entries[windowId] = { x: 100, y: 100, width: 640, height: 400 };
    }
    state.ui.selectedPaneId = '%1';
    appState.set(state);

    await autoArrange('$1');
    const circle = get(appState).layout.entries;

    await autoArrange('$1');
    const snowflake = get(appState).layout.entries;

    await autoArrange('$1');
    await autoArrange('$1');
    await autoArrange('$1');
    const spiral = get(appState).layout.entries;

    expect(circle['@1']).toEqual({ x: 100, y: 100, width: 640, height: 400 });
    expect(circle['@2']).toEqual({ x: 100, y: -700, width: 640, height: 400 });
    expect(circle['@4']).toEqual({ x: 780, y: -300, width: 640, height: 400 });
    expect(circle['@5']).toEqual({ x: 780, y: 500, width: 640, height: 400 });

    expect(snowflake['@1']).toEqual({ x: 100, y: 100, width: 640, height: 400 });
    expect(snowflake['@2']).toEqual({ x: 500, y: -580, width: 640, height: 400 });
    expect(snowflake['@4']).toEqual({ x: 900, y: 100, width: 640, height: 400 });
    expect(snowflake['@5']).toEqual({ x: 500, y: 780, width: 640, height: 400 });

    expect(spiral['@1']).toEqual({ x: 100, y: 100, width: 640, height: 400 });
    expect(spiral['@2']).toEqual({ x: 780, y: 100, width: 640, height: 400 });
    expect(spiral['@4']).toEqual({ x: 780, y: 540, width: 640, height: 400 });
  });

  it('keeps all arranged windows non-overlapping across the full cycle', async () => {
    const state = applyTmuxSnapshotToState(freshState(), snapshotWithMainWindowCount(7));
    for (const windowId of state.tmux.sessions['$1'].window_ids) {
      state.layout.entries[windowId] = { x: 100, y: 100, width: 640, height: 400 };
    }
    state.ui.selectedPaneId = '%1';
    appState.set(state);

    for (let cycle = 0; cycle < 5; cycle += 1) {
      await autoArrange('$1');
      const entries = get(appState).layout.entries;
      const arranged = state.tmux.sessions['$1'].window_ids.map((windowId) => entries[windowId]);

      for (let left = 0; left < arranged.length; left += 1) {
        for (let right = left + 1; right < arranged.length; right += 1) {
          expect(entriesOverlap(arranged[left], arranged[right])).toBe(false);
        }
      }
    }
  });

  it('reapplies the current arrangement mode when a new shell appears in the same session', async () => {
    const state = applyTmuxSnapshotToState(freshState(), snapshotWithMainWindowCount(4));
    for (const windowId of state.tmux.sessions['$1'].window_ids) {
      state.layout.entries[windowId] = { x: 100, y: 100, width: 640, height: 400 };
    }
    state.ui.selectedPaneId = '%1';
    appState.set(state);

    await autoArrange('$1');
    const arranged = get(appState);
    const next = applyTmuxSnapshotToState(arranged, snapshotWithMainWindowCount(5));

    expect(next.ui.arrangementModeBySession['$1']).toBe('circle');
    expect(next.ui.arrangementCycleBySession['$1']).toBe(1);
    expect(next.layout.entries['@1']).toEqual({ x: 100, y: 100, width: 640, height: 400 });
    expect(next.layout.entries['@2']).toEqual({ x: 100, y: -700, width: 640, height: 400 });
    expect(next.layout.entries['@4']).toEqual({ x: 900, y: 100, width: 640, height: 400 });
    expect(next.layout.entries['@5']).toEqual({ x: 100, y: 900, width: 640, height: 400 });
    expect(next.layout.entries['@6']).toEqual({ x: -700, y: 100, width: 640, height: 400 });
  });
});

describe('sidebar rename helpers', () => {
  it('builds a session rename command from the selected tree item', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const sessionIndex = buildSidebarItems(state).findIndex((item) => item.type === 'session' && item.sessionId === '$1');
    state.ui.sidebarSelectedIdx = sessionIndex;

    expect(buildSidebarRenameCommand(state)).toBe('tr Main');
  });

  it('builds a window rename command from the selected tree item', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const windowIndex = buildSidebarItems(state).findIndex((item) => item.type === 'window' && item.windowId === '@2');
    state.ui.sidebarSelectedIdx = windowIndex;

    expect(buildSidebarRenameCommand(state)).toBe('rename logs');
  });

  it('opens the command bar with the selected tree item rename command', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    const windowIndex = buildSidebarItems(state).findIndex((item) => item.type === 'window' && item.windowId === '@2');
    state.ui.sidebarSelectedIdx = windowIndex;
    appState.set(state);

    beginSidebarRename();

    const next = get(appState);
    expect(next.ui.commandBarOpen).toBe(true);
    expect(next.ui.commandText).toBe('rename logs');
  });
});

describe('window sizing helpers', () => {
  it('computes a tmux window size request from the owning pane viewport', () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.ui.paneViewportHints['%1'] = { cols: 100, rows: 30, pixelWidth: 750, pixelHeight: 480 };

    expect(calculateWindowSizeRequest(state, '@1')).toEqual({ cols: 100, rows: 30 });
  });

  it('reports pane viewport measurements without resizing tmux unless explicitly requested', async () => {
    appState.set(applyTmuxSnapshotToState(freshState(), baseSnapshot()));

    await reportPaneViewport('%1', 100, 30, 750, 480);

    const state = get(appState);
    expect(state.ui.paneViewportHints['%1']).toEqual({
      cols: 100,
      rows: 30,
      pixelWidth: 750,
      pixelHeight: 480,
    });
    expect(tauriMocks.resizeWindow).not.toHaveBeenCalled();
  });

  it('resizes the owning tmux window when explicitly requested', async () => {
    appState.set(applyTmuxSnapshotToState(freshState(), baseSnapshot()));

    await reportPaneViewport('%1', 100, 30, 750, 480, true);

    expect(tauriMocks.resizeWindow).toHaveBeenCalledWith('@1', 100, 30);
  });

  it('persists snapped tile dimensions after tmux reports the actual window size', async () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 0, y: 0, width: 640, height: 400 };
    appState.set(state);
    await reportPaneViewport('%1', 100, 24, 750, 480, true);

    const resizingState = get(appState);
    resizingState.ui.paneViewportHints['%1'] = { cols: 80, rows: 20, pixelWidth: 600, pixelHeight: 320 };
    appState.set(resizingState);

    applyTmuxSnapshot({
      ...baseSnapshot(),
      version: 2,
      windows: [
        { ...baseSnapshot().windows[0], cols: 100, rows: 24 },
        baseSnapshot().windows[1],
        baseSnapshot().windows[2],
      ],
      panes: [
        { ...baseSnapshot().panes[0], cols: 100, rows: 24 },
        baseSnapshot().panes[1],
        baseSnapshot().panes[2],
      ],
    });

    expect(get(appState).layout.entries['@1']).toEqual({ x: 0, y: 0, width: 790, height: 464 });
    expect(tauriMocks.saveLayoutState).toHaveBeenCalledWith('@1', 0, 0, 790, 464);
  });

  it('does not snap unrelated tiles when another tile in the session is resized', async () => {
    const state = applyTmuxSnapshotToState(freshState(), baseSnapshot());
    state.layout.entries['@1'] = { x: 0, y: 0, width: 640, height: 400 };
    state.layout.entries['@2'] = { x: 500, y: 0, width: 540, height: 360 };
    state.ui.paneViewportHints['%1'] = { cols: 80, rows: 20, pixelWidth: 600, pixelHeight: 320 };
    state.ui.paneViewportHints['%2'] = { cols: 80, rows: 20, pixelWidth: 600, pixelHeight: 320 };
    appState.set(state);

    await reportPaneViewport('%1', 100, 24, 750, 480, true);
    const resizingState = get(appState);
    resizingState.ui.paneViewportHints['%1'] = { cols: 80, rows: 20, pixelWidth: 600, pixelHeight: 320 };
    appState.set(resizingState);

    applyTmuxSnapshot({
      ...baseSnapshot(),
      version: 2,
      windows: [
        { ...baseSnapshot().windows[0], cols: 100, rows: 24 },
        { ...baseSnapshot().windows[1], cols: 100, rows: 24 },
        baseSnapshot().windows[2],
      ],
      panes: [
        { ...baseSnapshot().panes[0], cols: 100, rows: 24 },
        { ...baseSnapshot().panes[1], cols: 100, rows: 24 },
        baseSnapshot().panes[2],
      ],
    });

    expect(get(appState).layout.entries['@1']).toEqual({ x: 0, y: 0, width: 790, height: 464 });
    expect(get(appState).layout.entries['@2']).toEqual({ x: 500, y: 0, width: 540, height: 360 });
  });
});
