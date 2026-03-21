import net from 'node:net';
import readline from 'node:readline';

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import type { AgentInfo, TestDriverProjection } from '../../src/lib/types';
import { HerdTestClient } from './client';
import { createIsolatedTab, waitFor } from './helpers';
import { startIntegrationRuntime, type HerdIntegrationRuntime } from './runtime';

interface SocketResponse<T = unknown> {
  ok: boolean;
  data?: T;
  error?: string;
}

interface AgentChannelEvent {
  kind: 'direct' | 'public' | 'network' | 'root' | 'system' | 'ping';
  from_agent_id?: string | null;
  from_display_name: string;
  to_agent_id?: string | null;
  to_display_name?: string | null;
  message: string;
  topics: string[];
  mentions: string[];
  replay: boolean;
  ping_id?: string | null;
  timestamp_ms: number;
}

interface AgentStreamEnvelope {
  type: 'event';
  event: AgentChannelEvent;
}

interface AgentEventSubscription {
  nextEvent: (timeoutMs?: number) => Promise<AgentChannelEvent>;
  close: () => void;
}

async function collectAgentEvents(
  subscription: AgentEventSubscription,
  predicate: (events: AgentChannelEvent[]) => boolean,
  timeoutMs = 10_000,
): Promise<AgentChannelEvent[]> {
  const deadline = Date.now() + timeoutMs;
  const events: AgentChannelEvent[] = [];
  while (Date.now() <= deadline) {
    const remaining = Math.max(1, deadline - Date.now());
    events.push(await subscription.nextEvent(remaining));
    if (predicate(events)) {
      return events;
    }
  }
  throw new Error(`timed out waiting for expected agent events: ${JSON.stringify(events)}`);
}

async function openAgentEventSubscription(socketPath: string, agentId: string): Promise<AgentEventSubscription> {
  const socket = net.createConnection(socketPath);
  const lines = readline.createInterface({ input: socket });
  const bufferedLines: string[] = [];
  let lineResolver: ((line: string) => void) | null = null;
  let lineRejecter: ((error: Error) => void) | null = null;

  const rejectPending = (error: Error) => {
    if (!lineRejecter) {
      return;
    }
    const reject = lineRejecter;
    lineResolver = null;
    lineRejecter = null;
    reject(error);
  };

  lines.on('line', (line) => {
    if (lineResolver) {
      const resolve = lineResolver;
      lineResolver = null;
      lineRejecter = null;
      resolve(line);
      return;
    }
    bufferedLines.push(line);
  });

  socket.on('error', (error) => rejectPending(error instanceof Error ? error : new Error(String(error))));
  socket.on('close', () => rejectPending(new Error(`agent event subscription for ${agentId} closed unexpectedly`)));

  await new Promise<void>((resolve, reject) => {
    socket.on('connect', resolve);
    socket.on('error', reject);
  });

  const nextLine = (timeoutMs = 10_000): Promise<string> =>
    new Promise((resolve, reject) => {
      if (bufferedLines.length > 0) {
        resolve(bufferedLines.shift()!);
        return;
      }
      const timer = setTimeout(() => {
        if (lineResolver === resolve) {
          lineResolver = null;
          lineRejecter = null;
        }
        reject(new Error(`timed out waiting for subscription line for ${agentId}`));
      }, timeoutMs);
      lineResolver = (line) => {
        clearTimeout(timer);
        resolve(line);
      };
      lineRejecter = (error) => {
        clearTimeout(timer);
        reject(error);
      };
    });

  socket.write(`${JSON.stringify({ command: 'agent_events_subscribe', agent_id: agentId })}\n`);
  const firstLine = await nextLine();
  const response = JSON.parse(firstLine) as SocketResponse<{ agent: AgentInfo }>;
  if (!response.ok) {
    lines.close();
    socket.destroy();
    throw new Error(response.error ?? `agent event subscription failed for ${agentId}`);
  }

  return {
    nextEvent: async (timeoutMs = 10_000) => {
      const line = await nextLine(timeoutMs);
      const envelope = JSON.parse(line) as AgentStreamEnvelope;
      if (envelope.type !== 'event') {
        throw new Error(`unexpected agent stream envelope: ${line}`);
      }
      return envelope.event;
    },
    close: () => {
      lines.close();
      socket.destroy();
    },
  };
}

async function waitForActiveTab(client: HerdTestClient, sessionId: string) {
  await client.toolbarSelectTab(sessionId);
  return waitFor(
    `session ${sessionId} to become active`,
    () => client.getProjection(),
    (projection) => projection.active_tab_id === sessionId,
    30_000,
    150,
  );
}

async function spawnWorkerShellInActiveTab(client: HerdTestClient): Promise<string> {
  const before = await client.getProjection();
  const knownPaneIds = new Set(before.active_tab_terminals.map((terminal) => terminal.id));
  await client.sendCommand({
    command: 'shell_create',
    parent_session_id: before.active_tab_id,
    parent_pane_id: before.selected_pane_id,
  });
  const projection = await waitFor(
    'worker shell create in active tab',
    () => client.getProjection(),
    (nextProjection) => nextProjection.active_tab_terminals.some((terminal) => !knownPaneIds.has(terminal.id)),
    30_000,
    150,
  );
  const created = projection.active_tab_terminals.find((terminal) => !knownPaneIds.has(terminal.id));
  if (!created) {
    throw new Error('failed to locate spawned worker shell pane');
  }
  return created.id;
}

async function spawnWorkerAgentInActiveTab(client: HerdTestClient): Promise<{ paneId: string; agentId: string }> {
  const before = await waitFor(
    'live root agent before worker agent create',
    () => client.getProjection(),
    (projection) => Boolean(rootAgentForProjection(projection)),
    60_000,
    150,
  );
  const root = rootAgentForProjection(before);
  if (!root) {
    throw new Error('no live root agent available for agent_create');
  }
  const knownPaneIds = new Set(before.active_tab_terminals.map((terminal) => terminal.id));
  const knownAgentIds = new Set(before.agents.map((agent) => agent.agent_id));
  await client.sendCommand({
    command: 'agent_create',
    parent_session_id: before.active_tab_id,
    parent_pane_id: root.tile_id,
  });
  const projection = await waitFor(
    'worker agent create in active tab',
    () => client.getProjection(),
    (nextProjection) =>
      nextProjection.active_tab_terminals.some((terminal) => !knownPaneIds.has(terminal.id) && terminal.kind === 'claude')
      && nextProjection.agents.some(
        (agent) =>
          !knownAgentIds.has(agent.agent_id)
          && agent.agent_role === 'worker'
          && agent.alive
          && agent.session_id === before.active_tab_id,
      ),
    60_000,
    150,
  );
  const createdTerminal = projection.active_tab_terminals.find(
    (terminal) => !knownPaneIds.has(terminal.id) && terminal.kind === 'claude',
  );
  const createdAgent = projection.agents.find(
    (agent) =>
      !knownAgentIds.has(agent.agent_id)
      && agent.agent_role === 'worker'
      && agent.alive
      && agent.tile_id === createdTerminal?.id,
  );
  if (!createdTerminal || !createdAgent) {
    throw new Error('failed to locate spawned worker agent');
  }
  return { paneId: createdTerminal.id, agentId: createdAgent.agent_id };
}

function rootAgentForProjection(projection: TestDriverProjection): AgentInfo | undefined {
  return projection.agents.find((agent) => agent.agent_role === 'root' && agent.alive);
}

describe.sequential('worker/root mcp and permissions', () => {
  let runtime: HerdIntegrationRuntime;
  let client: HerdTestClient;

  beforeAll(async () => {
    runtime = await startIntegrationRuntime();
    client = runtime.client;
  });

  afterAll(async () => {
    await runtime.stop();
  });

  it('creates and repairs a red root agent for each session', async () => {
    const firstProjection = await waitFor(
      'bootstrap root agent',
      () => client.getProjection(),
      (projection) => Boolean(rootAgentForProjection(projection)),
      60_000,
      250,
    );
    const firstRoot = rootAgentForProjection(firstProjection);
    expect(firstRoot).toBeTruthy();
    expect(firstRoot?.agent_role).toBe('root');
    expect(firstRoot?.agent_id).toBe(`root:${firstProjection.active_tab_id}`);

    const newTabProjection = await createIsolatedTab(client, 'root-agent-tab');
    const sessionId = newTabProjection.active_tab_id!;
    const sessionProjection = await waitForActiveTab(client, sessionId);
    const sessionRoot = rootAgentForProjection(sessionProjection);
    expect(sessionRoot?.agent_id).toBe(`root:${sessionId}`);

    await client.sendCommand({ command: 'agent_unregister', agent_id: sessionRoot!.agent_id });
    const repaired = await waitFor(
      'root agent repair',
      () => client.getProjection(),
      (projection) => {
        if (projection.active_tab_id !== sessionId) return false;
        const root = rootAgentForProjection(projection);
        const visibleRootTiles = projection.active_tab_terminals.filter((terminal) => terminal.kind === 'root_agent');
        return root?.agent_id === `root:${sessionId}` && root.alive && visibleRootTiles.length === 1;
      },
      60_000,
      250,
    );
    expect(rootAgentForProjection(repaired)?.agent_id).toBe(`root:${sessionId}`);
    expect(repaired.active_tab_terminals.filter((terminal) => terminal.kind === 'root_agent')).toHaveLength(1);
  });

  it('allows closing a root agent with confirmation and auto-restarts it', async () => {
    const projection = await waitFor(
      'existing root agent before close',
      () => client.getProjection(),
      (nextProjection) => Boolean(rootAgentForProjection(nextProjection)),
      60_000,
      250,
    );
    const root = rootAgentForProjection(projection);
    expect(root).toBeTruthy();

    await client.tileClose(root!.tile_id);
    const confirmProjection = await waitFor(
      'root close confirmation dialog',
      () => client.getProjection(),
      (nextProjection) =>
        nextProjection.close_pane_confirmation?.paneId === root!.tile_id
        && nextProjection.close_pane_confirmation.confirmLabel === 'Close Root Agent',
      30_000,
      150,
    );
    expect(confirmProjection.close_pane_confirmation?.title).toBe('CLOSE ROOT AGENT');

    await client.pressKeys([{ key: 'Enter' }]);
    const restarted = await waitFor(
      'root agent restart after confirmed close',
      () => client.getProjection(),
      (nextProjection) => {
        const nextRoot = rootAgentForProjection(nextProjection);
        const visibleRootTiles = nextProjection.active_tab_terminals.filter((terminal) => terminal.kind === 'root_agent');
        return (
          nextRoot?.agent_id === root!.agent_id
          && nextRoot.tile_id === root!.tile_id
          && visibleRootTiles.length === 1
          && nextProjection.close_pane_confirmation === null
        );
      },
      60_000,
      250,
    );

    expect(rootAgentForProjection(restarted)?.agent_id).toBe(root!.agent_id);
    expect(restarted.active_tab_terminals.filter((terminal) => terminal.kind === 'root_agent')).toHaveLength(1);
  });

  it('restarts a root agent when its process dies unexpectedly', async () => {
    const projection = await waitFor(
      'existing root agent before kill',
      () => client.getProjection(),
      (nextProjection) => Boolean(rootAgentForProjection(nextProjection)?.agent_pid),
      60_000,
      250,
    );
    const root = rootAgentForProjection(projection);
    expect(root?.agent_pid).toBeTruthy();

    process.kill(root!.agent_pid!, 'SIGKILL');

    const restarted = await waitFor(
      'root agent restart after process death',
      () => client.getProjection(),
      (nextProjection) => {
        const nextRoot = rootAgentForProjection(nextProjection);
        const visibleRootTiles = nextProjection.active_tab_terminals.filter((terminal) => terminal.kind === 'root_agent');
        return (
          nextRoot?.agent_id === root!.agent_id
          && nextRoot.alive
          && nextRoot.agent_pid != null
          && nextRoot.agent_pid !== root!.agent_pid
          && visibleRootTiles.length === 1
        );
      },
      60_000,
      250,
    );

    expect(rootAgentForProjection(restarted)?.agent_id).toBe(root!.agent_id);
    expect(rootAgentForProjection(restarted)?.agent_pid).not.toBe(root!.agent_pid);
    expect(restarted.active_tab_terminals.filter((terminal) => terminal.kind === 'root_agent')).toHaveLength(1);
  });

  it('creates actual worker agents through agent_create instead of plain shells', async () => {
    const created = await spawnWorkerAgentInActiveTab(client);
    const projection = await client.getProjection();
    const terminal = projection.active_tab_terminals.find((candidate) => candidate.id === created.paneId);
    const agent = projection.agents.find((candidate) => candidate.agent_id === created.agentId);
    expect(terminal?.kind).toBe('claude');
    expect(terminal?.agentId).toBe(created.agentId);
    expect(terminal?.parentWindowId).toBe(rootAgentForProjection(projection)?.window_id);
    expect(agent?.agent_role).toBe('worker');
    expect(agent?.tile_id).toBe(created.paneId);
    expect(
      projection.active_tab_connections.some((connection) => connection.child_window_id === terminal?.windowId),
    ).toBe(false);
  });

  it('enforces worker message-only permissions at the backend', async () => {
    const projection = await createIsolatedTab(client, 'worker-perms');
    const workerPaneId = await spawnWorkerShellInActiveTab(client);
    await client.agentRegister('agent-worker-perms', workerPaneId, 'Worker Perms');
    const workerSubscription = await openAgentEventSubscription(runtime.socketPath, 'agent-worker-perms');

    try {
      await expect(
        client.sendCommand({
          command: 'agent_list',
          sender_agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).rejects.toThrow(/root/i);

      await expect(
        client.sendCommand({
          command: 'shell_list',
          sender_agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).rejects.toThrow(/root/i);

      await expect(
        client.sendCommand({
          command: 'work_list',
          agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).rejects.toThrow(/root/i);

      await expect(
        client.sendCommand({
          command: 'session_list',
          sender_agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).rejects.toThrow(/root/i);

      await expect(
        client.sendCommand({
          command: 'tile_list',
          sender_agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).rejects.toThrow(/root/i);

      await expect(
        client.sendCommand({
          command: 'tile_move',
          tile_id: workerPaneId,
          x: 500,
          y: 200,
          sender_agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).rejects.toThrow(/root/i);

      await expect(
        client.sendCommand({
          command: 'message_public',
          message: 'worker public message',
          sender_agent_id: 'agent-worker-perms',
          sender_pane_id: workerPaneId,
        }),
      ).resolves.toBeNull();
    } finally {
      workerSubscription.close();
    }

    expect(projection.active_tab_id).toBeTruthy();
  });

  it('lists current-session tiles for root and supports tile-type filters', async () => {
    const projection = await createIsolatedTab(client, 'session-list');
    const sessionId = projection.active_tab_id!;
    const rootProjection = await waitFor(
      'root agent in session-list tab',
      () => client.getProjection(),
      (nextProjection) => nextProjection.active_tab_id === sessionId && Boolean(rootAgentForProjection(nextProjection)),
      60_000,
      250,
    );
    const rootAgent = rootAgentForProjection(rootProjection)!;
    const workerPaneId = await spawnWorkerShellInActiveTab(client);
    const work = await client.workCreate('Session list work', rootAgent.tile_id);

    const full = await client.sessionList(rootAgent.tile_id, rootAgent.agent_id);
    expect(full.session_id).toBe(sessionId);
    expect(full.tiles.find((tile) => tile.tile_id === rootAgent.tile_id)).toMatchObject({
      kind: 'root_agent',
      pane_id: rootAgent.tile_id,
      details: {
        agent_id: rootAgent.agent_id,
        agent_role: 'root',
        display_name: 'Root',
      },
    });
    expect(full.tiles.some((tile) => tile.tile_id === workerPaneId && tile.kind === 'shell')).toBe(true);
    expect(full.tiles.some((tile) => tile.tile_id === `work:${work.work_id}` && tile.kind === 'work')).toBe(true);

    const workOnly = await client.sessionList(rootAgent.tile_id, rootAgent.agent_id, 'work');
    expect(workOnly.tiles).toHaveLength(1);
    expect(workOnly.tiles[0]).toMatchObject({
      tile_id: `work:${work.work_id}`,
      session_id: sessionId,
      kind: 'work',
      title: work.title,
      width: expect.any(Number),
      height: expect.any(Number),
      details: {
        work_id: work.work_id,
        topic: work.topic,
      },
    });
  });

  it('lists, gets, moves, and resizes tiles for root', async () => {
    const projection = await createIsolatedTab(client, 'tile-api');
    const sessionId = projection.active_tab_id!;
    const rootProjection = await waitFor(
      'root agent in tile-api tab',
      () => client.getProjection(),
      (nextProjection) => nextProjection.active_tab_id === sessionId && Boolean(rootAgentForProjection(nextProjection)),
      60_000,
      250,
    );
    const rootAgent = rootAgentForProjection(rootProjection)!;
    const workerPaneId = await spawnWorkerShellInActiveTab(client);
    const work = await client.workCreate('Tile api work', rootAgent.tile_id);

    const tiles = await client.tileList(rootAgent.tile_id, rootAgent.agent_id);
    const workerTile = tiles.find((tile) => tile.tile_id === workerPaneId);
    const workTileId = `work:${work.work_id}`;
    const workTile = tiles.find((tile) => tile.tile_id === workTileId);

    expect(workerTile).toMatchObject({
      tile_id: workerPaneId,
      session_id: sessionId,
      kind: 'shell',
      pane_id: workerPaneId,
      x: expect.any(Number),
      y: expect.any(Number),
      width: expect.any(Number),
      height: expect.any(Number),
      details: {
        window_name: expect.any(String),
      },
    });
    expect(workTile).toMatchObject({
      tile_id: workTileId,
      kind: 'work',
      width: expect.any(Number),
      height: expect.any(Number),
      details: {
        work_id: work.work_id,
        topic: work.topic,
      },
    });

    const rootTile = await client.tileGet(rootAgent.tile_id, rootAgent.tile_id, rootAgent.agent_id);
    expect(rootTile).toMatchObject({
      tile_id: rootAgent.tile_id,
      session_id: sessionId,
      kind: 'root_agent',
      pane_id: rootAgent.tile_id,
      window_id: rootAgent.window_id,
      details: {
        agent_id: rootAgent.agent_id,
        agent_role: 'root',
        display_name: 'Root',
      },
    });

    const moved = await client.tileMove(workerPaneId, 1180, 260, rootAgent.tile_id, rootAgent.agent_id);
    expect(moved).toMatchObject({
      tile_id: workerPaneId,
      x: 1180,
      y: 260,
    });

    await waitFor(
      'moved tile layout reflected in frontend state',
      () => client.getStateTree(),
      (stateTree) => {
        const entry = moved.window_id ? stateTree.layout.entries[moved.window_id] : null;
        return entry?.x === 1180 && entry?.y === 260;
      },
      30_000,
      150,
    );

    const resized = await client.tileResize(workerPaneId, 760, 520, rootAgent.tile_id, rootAgent.agent_id);
    expect(resized).toMatchObject({
      tile_id: workerPaneId,
      width: 760,
      height: 520,
    });

    await waitFor(
      'resized tile layout reflected in frontend state',
      () => client.getStateTree(),
      (stateTree) => {
        const entry = resized.window_id ? stateTree.layout.entries[resized.window_id] : null;
        return entry?.width === 760 && entry?.height === 520;
      },
      30_000,
      150,
    );

    const resizedWork = await client.tileResize(workTileId, 420, 340, rootAgent.tile_id, rootAgent.agent_id);
    expect(resizedWork).toMatchObject({
      tile_id: workTileId,
      width: 420,
      height: 340,
      details: {
        work_id: work.work_id,
      },
    });
  });

  it('routes message_root and message_network inside the sender session', async () => {
    const projection = await createIsolatedTab(client, 'worker-msgs');
    const sessionId = projection.active_tab_id!;
    const root = await waitFor(
      'root agent in worker-msgs tab',
      () => client.getProjection(),
      (nextProjection) => nextProjection.active_tab_id === sessionId && Boolean(rootAgentForProjection(nextProjection)),
      60_000,
      250,
    );
    const rootAgent = rootAgentForProjection(root)!;
    const rootSubscription = await openAgentEventSubscription(runtime.socketPath, rootAgent.agent_id);

    const firstWorkerPane = await spawnWorkerShellInActiveTab(client);
    const secondWorkerPane = await spawnWorkerShellInActiveTab(client);
    await client.agentRegister('agent-network-a', firstWorkerPane, 'Worker A');
    await client.agentRegister('agent-network-b', secondWorkerPane, 'Worker B');
    const firstWorkerSubscription = await openAgentEventSubscription(runtime.socketPath, 'agent-network-a');
    const secondWorkerSubscription = await openAgentEventSubscription(runtime.socketPath, 'agent-network-b');
    const senderProjection = await waitFor(
      'registered worker display name',
      () => client.getProjection(),
      (nextProjection) =>
        nextProjection.active_tab_id === sessionId
        && nextProjection.agents.some((agent) => agent.agent_id === 'agent-network-a'),
      30_000,
      150,
    );
    const senderLabel =
      senderProjection.agents.find((agent) => agent.agent_id === 'agent-network-a')?.display_name ?? 'Agent';

    try {
      await client.sendCommand({
        command: 'message_root',
        message: 'need session help',
        sender_agent_id: 'agent-network-a',
        sender_pane_id: firstWorkerPane,
      });
      const rootEvents = await collectAgentEvents(
        rootSubscription,
        (events) =>
          events.some(
            (event) =>
              event.kind === 'direct'
              && event.message === 'need session help'
              && event.to_agent_id === rootAgent.agent_id,
          ),
        10_000,
      );
      const rootEvent = rootEvents.find((event) => event.message === 'need session help')!;
      expect(rootEvent.message).toBe('need session help');
      expect(rootEvent.to_agent_id).toBe(rootAgent.agent_id);

      await client.sendCommand({
        command: 'message_network',
        message: 'network sync',
        sender_agent_id: 'agent-network-a',
        sender_pane_id: firstWorkerPane,
      });
      const networkEvents = await collectAgentEvents(
        secondWorkerSubscription,
        (events) => events.some((event) => event.kind === 'direct' && event.message === 'network sync'),
        10_000,
      );
      const networkEvent = networkEvents.find((event) => event.message === 'network sync')!;
      expect(networkEvent.message).toBe('network sync');
      expect(networkEvent.kind).toBe('direct');

      const chatterProjection = await waitFor(
        'network/root chatter lines',
        () => client.getProjection(),
        (nextProjection) =>
          nextProjection.active_tab_id === sessionId
          && nextProjection.chatter.some((entry) => entry.display_text === `${senderLabel} -> Root: need session help`)
          && nextProjection.chatter.some((entry) => entry.display_text === `${senderLabel} -> Network: network sync`),
        30_000,
        150,
      );
      expect(
        chatterProjection.chatter.some(
          (entry) => entry.display_text === `${senderLabel} -> Root: need session help`,
        ),
      ).toBe(true);
      expect(
        chatterProjection.chatter.some(
          (entry) => entry.display_text === `${senderLabel} -> Network: network sync`,
        ),
      ).toBe(true);
    } finally {
      rootSubscription.close();
      firstWorkerSubscription.close();
      secondWorkerSubscription.close();
    }
  });

  it('routes command-bar sudo, dm, and cm messages from User', async () => {
    const projection = await createIsolatedTab(client, 'sudo-cmd');
    const sessionId = projection.active_tab_id!;
    const root = await waitFor(
      'root agent in sudo-cmd tab',
      () => client.getProjection(),
      (nextProjection) => nextProjection.active_tab_id === sessionId && Boolean(rootAgentForProjection(nextProjection)),
      60_000,
      250,
    );
    const rootAgent = rootAgentForProjection(root)!;
    const worker = await spawnWorkerAgentInActiveTab(client);
    const workerProjection = await waitFor(
      'worker agent details in sudo-cmd tab',
      () => client.getProjection(),
      (nextProjection) =>
        nextProjection.active_tab_id === sessionId
        && nextProjection.agents.some((agent) => agent.agent_id === worker.agentId && agent.alive),
      60_000,
      150,
    );
    const workerAgent = workerProjection.agents.find((agent) => agent.agent_id === worker.agentId)!;
    const workerDisplayIndex = workerAgent.display_name.replace(/^Agent\s+/, '');
    const rootSubscription = await openAgentEventSubscription(runtime.socketPath, rootAgent.agent_id);
    const workerSubscription = await openAgentEventSubscription(runtime.socketPath, worker.agentId);

    try {
      await client.commandBarOpen();
      await client.commandBarSetText('sudo please inspect this session');
      await client.commandBarSubmit();

      const rootEvents = await collectAgentEvents(
        rootSubscription,
        (events) =>
          events.some(
            (event) =>
              event.kind === 'direct'
              && event.message === 'please inspect this session'
              && event.to_agent_id === rootAgent.agent_id,
          ),
        10_000,
      );
      const rootEvent = rootEvents.find((event) => event.message === 'please inspect this session')!;
      expect(rootEvent.from_display_name).toBe('User');

      await client.commandBarOpen();
      await client.commandBarSetText(`dm ${workerDisplayIndex} hello worker`);
      await client.commandBarSubmit();

      const workerEvents = await collectAgentEvents(
        workerSubscription,
        (events) =>
          events.some(
            (event) =>
              event.kind === 'direct'
              && event.message === 'hello worker'
              && event.to_agent_id === worker.agentId,
          ),
        10_000,
      );
      const workerEvent = workerEvents.find((event) => event.message === 'hello worker')!;
      expect(workerEvent.from_display_name).toBe('User');

      await client.commandBarOpen();
      await client.commandBarSetText('cm hey all!');
      await client.commandBarSubmit();

      const rootBroadcasts = await collectAgentEvents(
        rootSubscription,
        (events) =>
          events.some(
            (event) =>
              event.kind === 'public'
              && event.message === 'hey all!',
          ),
        10_000,
      );
      const rootBroadcast = rootBroadcasts.find((event) => event.message === 'hey all!')!;
      expect(rootBroadcast.from_display_name).toBe('User');

      const chatterProjection = await waitFor(
        'sudo dm cm chatter lines',
        () => client.getProjection(),
        (nextProjection) =>
          nextProjection.active_tab_id === sessionId
          && nextProjection.chatter.some((entry) => entry.display_text === 'User -> Root: please inspect this session')
          && nextProjection.chatter.some(
            (entry) => entry.display_text === `User -> ${workerAgent.display_name}: hello worker`,
          )
          && nextProjection.chatter.some((entry) => entry.display_text === 'User -> Chatter: hey all!'),
        30_000,
        150,
      );
      expect(
        chatterProjection.chatter.some(
          (entry) => entry.display_text === 'User -> Root: please inspect this session',
        ),
      ).toBe(true);
      expect(
        chatterProjection.chatter.some(
          (entry) => entry.display_text === `User -> ${workerAgent.display_name}: hello worker`,
        ),
      ).toBe(true);
      expect(
        chatterProjection.chatter.some(
          (entry) => entry.display_text === 'User -> Chatter: hey all!',
        ),
      ).toBe(true);
    } finally {
      rootSubscription.close();
      workerSubscription.close();
    }
  });
});
