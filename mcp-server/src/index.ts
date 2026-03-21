import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { spawnSync } from "node:child_process";
import * as net from "node:net";
import * as readline from "node:readline";
import { z } from "zod";

const HERD_AGENT_ID = process.env.HERD_AGENT_ID || "";
const HERD_PANE_ID = process.env.TMUX_PANE || process.env.HERD_PANE_ID || "";
const HERD_SESSION_ID = process.env.HERD_SESSION_ID || "";
const HERD_AGENT_ROLE =
  process.env.HERD_AGENT_ROLE
  || (HERD_AGENT_ID.startsWith("root:") ? "root" : "")
  || (process.env.HERD_MCP_MODE === "root" ? "root" : "worker");
const HERD_MCP_MODE = process.env.HERD_MCP_MODE || (HERD_AGENT_ROLE === "root" ? "root" : "worker");
const IS_ROOT_MODE = HERD_MCP_MODE === "root";

const MESSAGE_TOOLS = {
  direct: "message_direct",
  public: "message_public",
  network: "message_network",
  root: "message_root",
  sudo: "sudo",
} as const;

const SHARED_TOOLS = {
  networkList: "network_list",
} as const;

const ROOT_TOOLS = {
  agentCreate: "agent_create",
  shellCreate: "shell_create",
  shellsList: "shells_list",
  shellDestroy: "shell_destroy",
  shellInputSend: "shell_input_send",
  shellExec: "shell_exec",
  shellOutputRead: "shell_output_read",
  shellTitleSet: "shell_title_set",
  shellReadOnlySet: "shell_read_only_set",
  shellRoleSet: "shell_role_set",
  browserCreate: "browser_create",
  browserDestroy: "browser_destroy",
  browserNavigate: "browser_navigate",
  browserLoad: "browser_load",
  agentsList: "agents_list",
  topicsList: "topics_list",
  topicSubscribe: "topic_subscribe",
  topicUnsubscribe: "topic_unsubscribe",
  sessionList: "session_list",
  tileList: "tile_list",
  tileGet: "tile_get",
  tileMove: "tile_move",
  tileResize: "tile_resize",
  networkConnect: "network_connect",
  networkDisconnect: "network_disconnect",
  workList: "work_list",
  workGet: "work_get",
  workCreate: "work_create",
  workStageStart: "work_stage_start",
  workStageComplete: "work_stage_complete",
  workReviewApprove: "work_review_approve",
  workReviewImprove: "work_review_improve",
} as const;

export const MESSAGE_TOOL_NAMES = Object.freeze([...Object.values(MESSAGE_TOOLS)]);
export const SHARED_TOOL_NAMES = Object.freeze([...Object.values(SHARED_TOOLS)]);
export const WORKER_TOOL_NAMES = Object.freeze([...MESSAGE_TOOL_NAMES, ...SHARED_TOOL_NAMES]);
export const ROOT_ONLY_TOOL_NAMES = Object.freeze([...Object.values(ROOT_TOOLS)]);
export const ROOT_TOOL_NAMES = Object.freeze([...WORKER_TOOL_NAMES, ...ROOT_ONLY_TOOL_NAMES]);

type SocketResponse = { ok: boolean; data?: unknown; error?: string };
type AgentLogAppendKind = "incoming_hook" | "outgoing_call";
type HerdToolSchema = Record<string, z.ZodTypeAny>;
const TILE_TYPE_SCHEMA = z.enum(["shell", "agent", "browser", "work"]).optional();
type AgentStreamEnvelope = {
  type: "event";
  event: {
    kind: "direct" | "public" | "network" | "root" | "system" | "ping";
    from_agent_id?: string | null;
    from_display_name: string;
    to_agent_id?: string | null;
    to_display_name?: string | null;
    message: string;
    topics?: string[];
    mentions?: string[];
    replay?: boolean;
    ping_id?: string | null;
    timestamp_ms: number;
  };
};

function resolveSessionId() {
  if (HERD_SESSION_ID) {
    return HERD_SESSION_ID;
  }
  const result = spawnSync("tmux", ["display-message", "-p", "#{session_id}"], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return "";
  }
  return result.stdout.trim();
}

function resolveSocketPath() {
  if (process.env.HERD_SOCK) {
    return process.env.HERD_SOCK;
  }
  const sessionId = resolveSessionId();
  if (!sessionId) {
    return "/tmp/herd.sock";
  }
  const result = spawnSync("tmux", ["show-environment", "-t", sessionId, "HERD_SOCK"], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return "/tmp/herd.sock";
  }
  const line = result.stdout.trim();
  if (!line.startsWith("HERD_SOCK=")) {
    return "/tmp/herd.sock";
  }
  return line.slice("HERD_SOCK=".length) || "/tmp/herd.sock";
}

const SOCKET_PATH = resolveSocketPath();

async function sendCommand(command: Record<string, unknown>): Promise<SocketResponse> {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(SOCKET_PATH);
    const rl = readline.createInterface({ input: socket });
    let responded = false;

    socket.on("connect", () => {
      socket.write(JSON.stringify(command) + "\n");
    });

    rl.on("line", (line) => {
      if (responded) return;
      responded = true;
      rl.close();
      socket.destroy();
      try {
        resolve(JSON.parse(line) as SocketResponse);
      } catch {
        reject(new Error("Invalid JSON response from Herd"));
      }
    });

    socket.on("error", (err) => {
      if (responded) return;
      responded = true;
      reject(new Error(`Cannot connect to Herd at ${SOCKET_PATH}: ${err.message}`));
    });

    setTimeout(() => {
      if (responded) return;
      responded = true;
      rl.close();
      socket.destroy();
      reject(new Error("Timeout connecting to Herd"));
    }, 5000);
  });
}

function summarizeLogPayload(payload: Record<string, unknown>) {
  const json = JSON.stringify(payload);
  if (!json) {
    return "{}";
  }
  return json.length > 400 ? `${json.slice(0, 397)}...` : json;
}

function jsonText(value: unknown) {
  return value === undefined ? "{}" : JSON.stringify(value, null, 2);
}

async function appendAgentLog(kind: AgentLogAppendKind, text: string) {
  if (!HERD_AGENT_ID) {
    return;
  }
  try {
    const response = await sendCommand({
      command: "agent_log_append",
      agent_id: HERD_AGENT_ID,
      kind,
      text,
      timestamp_ms: Date.now(),
    });
    if (!response.ok) {
      console.error("Failed to append Herd agent log:", response.error);
    }
  } catch (error) {
    console.error("Failed to append Herd agent log:", error);
  }
}

async function sendToolCommand(
  toolName: string,
  toolArgs: Record<string, unknown>,
  command: Record<string, unknown>,
) {
  await appendAgentLog("outgoing_call", `MCP call ${toolName} ${summarizeLogPayload(toolArgs)}`);
  return sendCommand(command);
}

function errorResult(msg: string) {
  return { content: [{ type: "text" as const, text: `Error: ${msg}` }], isError: true };
}

function safeMetaValue(value: unknown): string | undefined {
  if (value === null || value === undefined) return undefined;
  if (Array.isArray(value)) return value.join(",");
  return String(value);
}

function buildChannelMeta(event: AgentStreamEnvelope["event"]) {
  const entries = {
    kind: event.kind,
    from_agent_id: event.from_agent_id,
    from_display_name: event.from_display_name,
    to_agent_id: event.to_agent_id,
    to_display_name: event.to_display_name,
    topics: event.topics?.join(","),
    mentions: event.mentions?.join(","),
    replay: event.replay ? "true" : "false",
    timestamp_ms: String(event.timestamp_ms),
  };
  return Object.fromEntries(
    Object.entries(entries)
      .map(([key, value]) => [key, safeMetaValue(value)])
      .filter((entry): entry is [string, string] => Boolean(entry[1])),
  );
}

async function pushChannelEvent(server: McpServer, event: AgentStreamEnvelope["event"]) {
  if (event.kind === "ping") {
    await sendCommand({
      command: "agent_ping_ack",
      agent_id: HERD_AGENT_ID,
    }).catch((error) => {
      console.error("Failed to ack Herd ping:", error);
    });
    return;
  }

  await server.server.notification({
    method: "notifications/claude/channel",
    params: {
      content: event.message,
      meta: buildChannelMeta(event),
    },
  });
  await appendAgentLog("incoming_hook", `MCP hook [${event.kind}] ${event.message}`);
}

function subscribeAgentEvents(server: McpServer, agentId: string) {
  const socket = net.createConnection(SOCKET_PATH);
  const rl = readline.createInterface({ input: socket });
  let initialized = false;

  socket.on("connect", () => {
    socket.write(JSON.stringify({ command: "agent_events_subscribe", agent_id: agentId }) + "\n");
  });

  socket.on("error", (error) => {
    console.error("Herd event subscription error:", error.message);
  });

  rl.on("line", async (line) => {
    if (!initialized) {
      initialized = true;
      try {
        const response = JSON.parse(line) as SocketResponse;
        if (!response.ok) {
          console.error("Herd event subscription failed:", response.error);
          socket.destroy();
        }
      } catch (error) {
        console.error("Invalid Herd subscription response:", error);
        socket.destroy();
      }
      return;
    }

    try {
      const envelope = JSON.parse(line) as AgentStreamEnvelope;
      if (envelope.type !== "event") return;
      await pushChannelEvent(server, envelope.event);
    } catch (error) {
      console.error("Failed to process Herd agent event:", error);
    }
  });

  return () => {
    rl.close();
    socket.destroy();
  };
}

const server = new McpServer(
  {
    name: "herd",
    version: "0.1.0",
  },
  {
    capabilities: {
      experimental: {
        "claude/channel": {},
      },
    },
    instructions:
      'Messages arrive as <channel source="herd" kind="..."> with metadata including from_agent_id, from_display_name, to_agent_id, to_display_name, topics, mentions, replay, and timestamp_ms. kind="direct" is private coordination. kind="public" is session-wide chatter. kind="network" is local network coordination. kind="root" is traffic for the session root agent. kind="system" is Herd lifecycle information. Treat replay="true" as historical context rather than a fresh request, and treat replay="false" as live traffic. If you want Herd or other agents to see your reply, respond through the Herd messaging tools such as message_direct, message_public, message_network, or message_root. Plain assistant text in the local session does not publish a reply back onto the Herd channels.',
  },
);

function senderContext() {
  return {
    sender_agent_id: HERD_AGENT_ID || undefined,
    sender_pane_id: HERD_PANE_ID || undefined,
  };
}

function registerTool(
  name: string,
  description: string,
  schema: HerdToolSchema,
  handler: (args: any) => Promise<{ content: Array<{ type: "text"; text: string }>; isError?: boolean }>,
) {
  server.tool(name, description, schema, handler);
}

function registerMessageTools() {
  registerTool(
    MESSAGE_TOOLS.direct,
    "Send a direct message to another agent in the current session.",
    {
      to_agent_id: z.string(),
      message: z.string(),
    },
    async ({ to_agent_id, message }) => {
      try {
        const resp = await sendToolCommand(
          MESSAGE_TOOLS.direct,
          { to_agent_id, message },
          {
            command: "message_direct",
            to_agent_id,
            message,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Direct message sent" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    MESSAGE_TOOLS.public,
    "Send a public message to the current session chatter stream.",
    {
      message: z.string(),
      topics: z.array(z.string()).optional(),
      mentions: z.array(z.string()).optional(),
    },
    async ({ message, topics, mentions }) => {
      try {
        const resp = await sendToolCommand(
          MESSAGE_TOOLS.public,
          { message, topics: topics ?? [], mentions: mentions ?? [] },
          {
            command: "message_public",
            message,
            topics: topics ?? [],
            mentions: mentions ?? [],
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Public message sent" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    MESSAGE_TOOLS.network,
    "Send a message to all other agents on the sender's local network.",
    {
      message: z.string(),
    },
    async ({ message }) => {
      try {
        const resp = await sendToolCommand(
          MESSAGE_TOOLS.network,
          { message },
          {
            command: "message_network",
            message,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Network message sent" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    MESSAGE_TOOLS.root,
    "Send a message to the current session root agent.",
    {
      message: z.string(),
    },
    async ({ message }) => {
      try {
        const resp = await sendToolCommand(
          MESSAGE_TOOLS.root,
          { message },
          {
            command: "message_root",
            message,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Root message sent" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    MESSAGE_TOOLS.sudo,
    "Send a privileged request to the current session root agent.",
    {
      message: z.string(),
    },
    async ({ message }) => {
      try {
        const resp = await sendToolCommand(
          MESSAGE_TOOLS.sudo,
          { message },
          {
            command: "message_root",
            message,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Root message sent" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );
}

function registerSharedTools() {
  registerTool(
    SHARED_TOOLS.networkList,
    "List tiles on the sender's current session network component.",
    { tile_type: TILE_TYPE_SCHEMA },
    async ({ tile_type }) => {
      try {
        const resp = await sendToolCommand(
          SHARED_TOOLS.networkList,
          tile_type ? { tile_type } : {},
          {
            command: "network_list",
            tile_type,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );
}

function registerRootTools() {
  registerTool(
    ROOT_TOOLS.agentCreate,
    "Create a new worker agent on the Herd canvas.",
    {},
    async () => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.agentCreate,
          {},
          {
            command: "agent_create",
            parent_session_id: HERD_SESSION_ID || undefined,
            parent_pane_id: HERD_PANE_ID || undefined,
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellCreate,
    "Create a new terminal shell on the Herd canvas.",
    {
      x: z.number().optional(),
      y: z.number().optional(),
      width: z.number().optional(),
      height: z.number().optional(),
      title: z.string().optional(),
    },
    async (params) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellCreate,
          params,
          {
            command: "shell_create",
            x: params.x,
            y: params.y,
            width: params.width,
            height: params.height,
            parent_session_id: HERD_SESSION_ID || undefined,
            parent_pane_id: HERD_PANE_ID || undefined,
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");

        const data = resp.data as { pane_id?: string } | undefined;
        if (params.title && data?.pane_id) {
          await sendCommand({
            command: "shell_title_set",
            session_id: data.pane_id,
            title: params.title,
            ...senderContext(),
          });
        }

        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(ROOT_TOOLS.shellsList, "List all active Herd shells.", {}, async () => {
    try {
      const resp = await sendToolCommand(ROOT_TOOLS.shellsList, {}, { command: "shell_list", ...senderContext() });
      if (!resp.ok) return errorResult(resp.error || "Unknown error");
      return { content: [{ type: "text", text: jsonText(resp.data) }] };
    } catch (err) {
      return errorResult(String(err));
    }
  });

  registerTool(
    ROOT_TOOLS.shellDestroy,
    "Destroy a shell by pane id.",
    { pane_id: z.string() },
    async ({ pane_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellDestroy,
          { pane_id },
          { command: "shell_destroy", session_id: pane_id, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Shell destroyed" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellInputSend,
    "Send text input to a shell.",
    { pane_id: z.string(), input: z.string() },
    async ({ pane_id, input }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellInputSend,
          { pane_id, input },
          { command: "shell_input_send", session_id: pane_id, input, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Input sent" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellExec,
    "Execute a shell command inside a Herd tile.",
    { pane_id: z.string(), command: z.string() },
    async ({ pane_id, command }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellExec,
          { pane_id, command },
          {
            command: "shell_exec",
            session_id: pane_id,
            shell_command: command,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellOutputRead,
    "Read recent terminal output from a shell.",
    { pane_id: z.string() },
    async ({ pane_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellOutputRead,
          { pane_id },
          { command: "shell_output_read", session_id: pane_id, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        const output = (resp.data as { output?: string } | undefined)?.output ?? "";
        return { content: [{ type: "text", text: output || "(no output)" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellTitleSet,
    "Set the display title of a Herd tile.",
    { pane_id: z.string(), title: z.string() },
    async ({ pane_id, title }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellTitleSet,
          { pane_id, title },
          { command: "shell_title_set", session_id: pane_id, title, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Title updated" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellReadOnlySet,
    "Set whether a Herd tile is read-only.",
    { pane_id: z.string(), read_only: z.boolean() },
    async ({ pane_id, read_only }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellReadOnlySet,
          { pane_id, read_only },
          {
            command: "shell_read_only_set",
            session_id: pane_id,
            read_only,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Read-only state updated" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.shellRoleSet,
    "Set the logical role of a Herd tile.",
    { pane_id: z.string(), role: z.string() },
    async ({ pane_id, role }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.shellRoleSet,
          { pane_id, role },
          { command: "shell_role_set", session_id: pane_id, role, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Role updated" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.browserCreate,
    "Create a new browser tile on the Herd canvas.",
    {},
    async () => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.browserCreate,
          {},
          {
            command: "browser_create",
            parent_session_id: HERD_SESSION_ID || undefined,
            parent_pane_id: HERD_PANE_ID || undefined,
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.browserDestroy,
    "Destroy a browser tile by pane id.",
    { pane_id: z.string() },
    async ({ pane_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.browserDestroy,
          { pane_id },
          { command: "browser_destroy", pane_id, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: "Browser destroyed" }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.browserNavigate,
    "Navigate an existing browser tile to a URL.",
    { pane_id: z.string(), url: z.string() },
    async ({ pane_id, url }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.browserNavigate,
          { pane_id, url },
          { command: "browser_navigate", pane_id, url, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.browserLoad,
    "Load a local file into an existing browser tile.",
    { pane_id: z.string(), path: z.string() },
    async ({ pane_id, path }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.browserLoad,
          { pane_id, path },
          { command: "browser_load", pane_id, path, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(ROOT_TOOLS.agentsList, "List agents in the current session.", {}, async () => {
    try {
      const resp = await sendToolCommand(ROOT_TOOLS.agentsList, {}, { command: "agent_list", ...senderContext() });
      if (!resp.ok) return errorResult(resp.error || "Unknown error");
      return { content: [{ type: "text", text: jsonText(resp.data) }] };
    } catch (err) {
      return errorResult(String(err));
    }
  });

  registerTool(ROOT_TOOLS.topicsList, "List topics in the current session.", {}, async () => {
    try {
      const resp = await sendToolCommand(ROOT_TOOLS.topicsList, {}, { command: "topics_list", ...senderContext() });
      if (!resp.ok) return errorResult(resp.error || "Unknown error");
      return { content: [{ type: "text", text: jsonText(resp.data) }] };
    } catch (err) {
      return errorResult(String(err));
    }
  });

  registerTool(
    ROOT_TOOLS.topicSubscribe,
    "Subscribe an agent to a topic in the current session.",
    { agent_id: z.string(), topic: z.string() },
    async ({ agent_id, topic }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.topicSubscribe,
          { agent_id, topic },
          { command: "topic_subscribe", agent_id, topic },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.topicUnsubscribe,
    "Unsubscribe an agent from a topic in the current session.",
    { agent_id: z.string(), topic: z.string() },
    async ({ agent_id, topic }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.topicUnsubscribe,
          { agent_id, topic },
          { command: "topic_unsubscribe", agent_id, topic },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.sessionList,
    "List all tiles in the current session, optionally filtered by tile type.",
    { tile_type: TILE_TYPE_SCHEMA },
    async ({ tile_type }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.sessionList,
          tile_type ? { tile_type } : {},
          {
            command: "session_list",
            tile_type,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.tileList,
    "List tiles in the current session, optionally filtered by tile type.",
    { tile_type: TILE_TYPE_SCHEMA },
    async ({ tile_type }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.tileList,
          tile_type ? { tile_type } : {},
          {
            command: "tile_list",
            tile_type,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.tileGet,
    "Get a single tile from the current session, including type-specific details.",
    { tile_id: z.string() },
    async ({ tile_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.tileGet,
          { tile_id },
          {
            command: "tile_get",
            tile_id,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.tileMove,
    "Move a tile on the Herd canvas.",
    {
      tile_id: z.string(),
      x: z.number(),
      y: z.number(),
    },
    async ({ tile_id, x, y }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.tileMove,
          { tile_id, x, y },
          {
            command: "tile_move",
            tile_id,
            x,
            y,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.tileResize,
    "Resize a tile on the Herd canvas.",
    {
      tile_id: z.string(),
      width: z.number(),
      height: z.number(),
    },
    async ({ tile_id, width, height }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.tileResize,
          { tile_id, width, height },
          {
            command: "tile_resize",
            tile_id,
            width,
            height,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.networkConnect,
    "Connect two tile ports on the current session network.",
    {
      from_tile_id: z.string(),
      from_port: z.string(),
      to_tile_id: z.string(),
      to_port: z.string(),
    },
    async ({ from_tile_id, from_port, to_tile_id, to_port }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.networkConnect,
          { from_tile_id, from_port, to_tile_id, to_port },
          {
            command: "network_connect",
            from_tile_id,
            from_port,
            to_tile_id,
            to_port,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data ?? { ok: true }) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.networkDisconnect,
    "Disconnect the current edge attached to a tile port.",
    { tile_id: z.string(), port: z.string() },
    async ({ tile_id, port }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.networkDisconnect,
          { tile_id, port },
          { command: "network_disconnect", tile_id, port, ...senderContext() },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data ?? { ok: true }) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(ROOT_TOOLS.workList, "List work items in the current session.", {}, async () => {
    try {
      const resp = await sendToolCommand(
        ROOT_TOOLS.workList,
        {},
        {
          command: "work_list",
          scope: "current_session",
          agent_id: HERD_AGENT_ID || undefined,
          session_id: HERD_SESSION_ID || undefined,
          sender_pane_id: HERD_PANE_ID || undefined,
        },
      );
      if (!resp.ok) return errorResult(resp.error || "Unknown error");
      return { content: [{ type: "text", text: jsonText(resp.data) }] };
    } catch (err) {
      return errorResult(String(err));
    }
  });

  registerTool(
    ROOT_TOOLS.workGet,
    "Get a work item from the current session.",
    { work_id: z.string() },
    async ({ work_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.workGet,
          { work_id },
          {
            command: "work_get",
            work_id,
            agent_id: HERD_AGENT_ID || undefined,
            session_id: HERD_SESSION_ID || undefined,
            sender_pane_id: HERD_PANE_ID || undefined,
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.workCreate,
    "Create a new work item in the current session.",
    { title: z.string() },
    async ({ title }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.workCreate,
          { title },
          {
            command: "work_create",
            title,
            session_id: HERD_SESSION_ID || undefined,
            ...senderContext(),
          },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.workStageStart,
    "Mark a work item's current stage as in progress for the given owner agent.",
    { work_id: z.string(), agent_id: z.string() },
    async ({ work_id, agent_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.workStageStart,
          { work_id, agent_id },
          { command: "work_stage_start", work_id, agent_id },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.workStageComplete,
    "Mark a work item's current stage as completed for the given owner agent.",
    { work_id: z.string(), agent_id: z.string() },
    async ({ work_id, agent_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.workStageComplete,
          { work_id, agent_id },
          { command: "work_stage_complete", work_id, agent_id },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.workReviewApprove,
    "Approve the current stage of a work item.",
    { work_id: z.string() },
    async ({ work_id }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.workReviewApprove,
          { work_id },
          { command: "work_review_approve", work_id },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );

  registerTool(
    ROOT_TOOLS.workReviewImprove,
    "Send a work item stage back to in-progress with an improvement comment.",
    { work_id: z.string(), comment: z.string() },
    async ({ work_id, comment }) => {
      try {
        const resp = await sendToolCommand(
          ROOT_TOOLS.workReviewImprove,
          { work_id, comment },
          { command: "work_review_improve", work_id, comment },
        );
        if (!resp.ok) return errorResult(resp.error || "Unknown error");
        return { content: [{ type: "text", text: jsonText(resp.data) }] };
      } catch (err) {
        return errorResult(String(err));
      }
    },
  );
}

registerMessageTools();
registerSharedTools();
if (IS_ROOT_MODE) {
  registerRootTools();
}

export async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error(`Herd MCP server running (socket: ${SOCKET_PATH})`);

  if (HERD_AGENT_ID && HERD_PANE_ID) {
    const registration = await sendCommand({
      command: "agent_register",
      agent_id: HERD_AGENT_ID,
      agent_type: "claude",
      agent_role: HERD_AGENT_ROLE,
      pane_id: HERD_PANE_ID,
      agent_pid: Number(process.ppid) || undefined,
      title: HERD_AGENT_ROLE === "root" ? "Root" : "Agent",
    });
    if (!registration.ok) {
      console.error("Failed to register Herd agent:", registration.error);
    } else {
      subscribeAgentEvents(server, HERD_AGENT_ID);
    }
  }
}
