use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::path::Path;
use std::collections::BTreeSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tauri::{AppHandle, Emitter, Manager};

use crate::agent::{
    collect_mentions, collect_topics, format_direct_display, format_network_display,
    format_public_display, format_root_display, format_sign_off_display,
    format_sign_on_display, now_ms, AgentChannelEvent, AgentChannelEventKind, AgentLogEntry,
    AgentLogKind, AgentRole, ChatterEntry, ChatterKind,
};
use crate::persist::TileState;
use crate::state::AppState;
use crate::{network, runtime, tmux, work};

use super::protocol::{SocketCommand, SocketResponse, TestDriverRequest};

const AGENT_PING_INTERVAL: Duration = Duration::from_secs(15);
const AGENT_PING_TIMEOUT: Duration = Duration::from_secs(10);
const AGENT_REPLAY_WINDOW_MS: i64 = 60 * 60 * 1000;
const HERD_WORKER_WELCOME_MESSAGE: &str = "Welcome to Herd. Review the /herd skill, inspect the recent public activity in your session, and coordinate through public, network, direct, or root messages. The root agent manages the full Herd tool surface for this session.";
const HERD_ROOT_WELCOME_MESSAGE: &str = "You are the Root agent for this session. You are responsible for handling messages sent to Root, coordinating session work, and using the full Herd MCP surface on behalf of this session.";
const GRID_SNAP: f64 = 20.0;
const GAP: f64 = 30.0;
const DEFAULT_TILE_WIDTH: f64 = 640.0;
const DEFAULT_TILE_HEIGHT: f64 = 400.0;
const WORK_CARD_WIDTH: f64 = 360.0;
const WORK_CARD_HEIGHT: f64 = 320.0;

fn parse_agent_type(value: Option<&str>) -> Result<crate::agent::AgentType, String> {
    match value.unwrap_or("claude").trim() {
        "" | "claude" => Ok(crate::agent::AgentType::Claude),
        other => Err(format!("unsupported agent type: {other}")),
    }
}

fn parse_agent_role(value: Option<&str>) -> Result<crate::agent::AgentRole, String> {
    match value.unwrap_or("worker").trim() {
        "root" => Ok(crate::agent::AgentRole::Root),
        "" | "worker" => Ok(crate::agent::AgentRole::Worker),
        other => Err(format!("unsupported agent role: {other}")),
    }
}

struct SocketLogger {
    file: std::fs::File,
}

impl SocketLogger {
    fn open() -> Option<Self> {
        let log_path = runtime::socket_log_path().to_string();
        log::info!("Socket traffic logging to {log_path}");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok()
            .map(|file| Self { file })
    }

    fn log(&mut self, direction: &str, data: &str) {
        let now = chrono::Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(self.file, "[{now}] {direction} {}", data.trim());
    }
}

type SharedLogger = Arc<Mutex<Option<SocketLogger>>>;

fn emit_agent_state(app: &AppHandle, state: &AppState) {
    let Ok(session_id) = resolve_ui_session_id(state) else {
        return;
    };
    if let Ok(snapshot) = state.snapshot_agent_debug_state_for_session(&session_id) {
        let _ = app.emit("herd-agent-state", snapshot);
    }
}

fn emit_work_updated(app: &AppHandle, item: &work::WorkItem) {
    let _ = app.emit(
        "herd-work-updated",
        serde_json::json!({
            "session_id": item.session_id,
            "work_id": item.work_id,
        }),
    );
}

fn emit_layout_updated(
    app: &AppHandle,
    tile: &network::SessionTileInfo,
    layout: &TileState,
    request_resize: bool,
) {
    let _ = app.emit(
        "herd-layout-entry",
        serde_json::json!({
            "entry_id": tile
                .window_id
                .as_deref()
                .unwrap_or(tile.tile_id.as_str()),
            "tile_id": tile.tile_id.clone(),
            "pane_id": tile.pane_id.clone(),
            "window_id": tile.window_id.clone(),
            "x": layout.x,
            "y": layout.y,
            "width": layout.width,
            "height": layout.height,
            "request_resize": request_resize,
        }),
    );
}

fn append_chatter_entry(state: &AppState, app: &AppHandle, entry: ChatterEntry) -> Result<(), String> {
    state.append_chatter_entry(entry.clone())?;
    if resolve_ui_session_id(state).ok().as_deref() == Some(entry.session_id.as_str()) {
        let _ = app.emit("herd-chatter-entry", &entry);
    }
    Ok(())
}

fn append_agent_log_entry(state: &AppState, app: &AppHandle, entry: AgentLogEntry) -> Result<(), String> {
    state.append_agent_log_entry(entry)?;
    emit_agent_state(app, state);
    Ok(())
}

struct SenderContext {
    session_id: String,
    sender_agent_id: Option<String>,
    display_name: String,
    sender_agent_role: Option<AgentRole>,
    sender_tile_id: Option<String>,
    sender_window_id: Option<String>,
}

fn resolve_ui_session_id(state: &AppState) -> Result<String, String> {
    if let Some(session_id) = state.last_active_session() {
        return Ok(session_id);
    }
    crate::tmux_state::snapshot(state)?
        .active_session_id
        .ok_or("no active session available".to_string())
}

fn resolve_session_id_for_pane(state: &AppState, pane_id: &str) -> Result<String, String> {
    let snapshot = crate::tmux_state::snapshot(state)?;
    snapshot
        .panes
        .iter()
        .find(|pane| pane.id == pane_id)
        .map(|pane| pane.session_id.clone())
        .ok_or_else(|| format!("no tmux pane found for {pane_id}"))
}

fn resolve_sender_context(
    state: &AppState,
    sender_agent_id: Option<String>,
    sender_pane_id: Option<String>,
) -> Result<SenderContext, String> {
    if let Some(agent_id) = sender_agent_id {
        let agent = live_agent_info(state, &agent_id)?;
        return Ok(SenderContext {
            session_id: agent.session_id,
            sender_agent_id: Some(agent.agent_id),
            display_name: agent.display_name,
            sender_agent_role: Some(agent.agent_role),
            sender_tile_id: Some(agent.tile_id),
            sender_window_id: Some(agent.window_id),
        });
    }

    if let Some(pane_id) = sender_pane_id {
        if let Ok(Some(agent)) = state.agent_info_by_tile(&pane_id) {
            if agent.alive {
                return Ok(SenderContext {
                    session_id: agent.session_id,
                    sender_agent_id: Some(agent.agent_id.clone()),
                    display_name: agent.display_name,
                    sender_agent_role: Some(agent.agent_role),
                    sender_tile_id: Some(agent.tile_id),
                    sender_window_id: Some(agent.window_id),
                });
            }
        }
        return Ok(SenderContext {
            session_id: resolve_session_id_for_pane(state, &pane_id)?,
            sender_agent_id: None,
            display_name: "HERD".to_string(),
            sender_agent_role: None,
            sender_tile_id: Some(pane_id.clone()),
            sender_window_id: crate::tmux_state::snapshot(state)?
                .panes
                .iter()
                .find(|pane| pane.id == pane_id)
                .map(|pane| pane.window_id.clone()),
        });
    }

    Ok(SenderContext {
        session_id: resolve_ui_session_id(state)?,
        sender_agent_id: None,
        display_name: "HERD".to_string(),
        sender_agent_role: None,
        sender_tile_id: None,
        sender_window_id: None,
    })
}

fn resolve_user_sender_context(state: &AppState) -> Result<SenderContext, String> {
    Ok(SenderContext {
        session_id: resolve_ui_session_id(state)?,
        sender_agent_id: None,
        display_name: "User".to_string(),
        sender_agent_role: None,
        sender_tile_id: None,
        sender_window_id: None,
    })
}

fn send_direct_message_from_sender(
    state: &AppState,
    app: &AppHandle,
    sender: SenderContext,
    to_agent_id: String,
    message: String,
) -> Result<(), String> {
    let target = live_agent_info(state, &to_agent_id)?;
    if sender.session_id != target.session_id {
        return Err(format!(
            "agent {} cannot direct-message {} across sessions",
            sender.sender_agent_id.unwrap_or_else(|| sender.display_name.clone()),
            to_agent_id,
        ));
    }
    let to_display_name = target.display_name.clone();
    let event = AgentChannelEvent {
        kind: AgentChannelEventKind::Direct,
        from_agent_id: sender.sender_agent_id.clone(),
        from_display_name: sender.display_name.clone(),
        to_agent_id: Some(to_agent_id.clone()),
        to_display_name: Some(to_display_name.clone()),
        message: message.clone(),
        topics: Vec::new(),
        mentions: Vec::new(),
        replay: false,
        ping_id: None,
        timestamp_ms: now_ms(),
    };
    if let Err(error) = state.send_event_to_agent(&to_agent_id, event) {
        let _ = mark_agent_dead(state, app, &to_agent_id);
        return Err(error);
    }
    let entry = build_direct_entry(
        sender.session_id,
        sender.sender_agent_id,
        sender.display_name,
        to_agent_id,
        to_display_name,
        message,
    );
    append_chatter_entry(state, app, entry)
}

fn send_public_message_from_sender(
    state: &AppState,
    app: &AppHandle,
    sender: SenderContext,
    message: String,
    topics: Vec<String>,
    mentions: Vec<String>,
) -> Result<(), String> {
    let normalized_topics = collect_topics(&message, &topics);
    let normalized_mentions = collect_mentions(&message, &mentions);
    state.touch_topics_in_session(&sender.session_id, &normalized_topics)?;
    let entry = build_chatter_entry(
        sender.session_id,
        sender.sender_agent_id.clone(),
        sender.display_name.clone(),
        message,
        normalized_topics,
        normalized_mentions,
    );
    append_chatter_entry(state, app, entry.clone())?;
    broadcast_public_event(state, app, &entry);
    Ok(())
}

fn send_root_message_from_sender(
    state: &AppState,
    app: &AppHandle,
    sender: SenderContext,
    message: String,
) -> Result<(), String> {
    let root_agent = session_root_agent(state, &sender.session_id)?;
    let event = AgentChannelEvent {
        kind: AgentChannelEventKind::Direct,
        from_agent_id: sender.sender_agent_id.clone(),
        from_display_name: sender.display_name.clone(),
        to_agent_id: Some(root_agent.agent_id.clone()),
        to_display_name: Some(root_agent.display_name.clone()),
        message: message.clone(),
        topics: Vec::new(),
        mentions: Vec::new(),
        replay: false,
        ping_id: None,
        timestamp_ms: now_ms(),
    };
    if let Err(error) = state.send_event_to_agent(&root_agent.agent_id, event) {
        let _ = mark_agent_dead(state, app, &root_agent.agent_id);
        return Err(error);
    }
    let entry = build_root_entry(
        sender.session_id,
        sender.sender_agent_id,
        sender.display_name,
        message,
    );
    append_chatter_entry(state, app, entry)
}

fn resolve_user_message_target(
    state: &AppState,
    session_id: &str,
    target: &str,
) -> Result<crate::agent::AgentInfo, String> {
    let normalized = target.trim();
    if normalized.is_empty() {
        return Err("direct message target may not be empty".to_string());
    }
    if normalized.eq_ignore_ascii_case("root") {
        return session_root_agent(state, session_id);
    }
    let agent_index = normalized.parse::<u64>().ok();
    state
        .list_agents_in_session(session_id)?
        .into_iter()
        .find(|agent| {
            agent.alive
                && (agent.agent_id.eq_ignore_ascii_case(normalized)
                    || agent.display_name.eq_ignore_ascii_case(normalized)
                    || agent_index
                        .map(|index| agent.display_name == format!("Agent {index}"))
                        .unwrap_or(false))
        })
        .ok_or_else(|| format!("no live agent target found for {normalized} in session {session_id}"))
}

fn build_direct_entry(
    session_id: String,
    from_agent_id: Option<String>,
    from_display_name: String,
    to_agent_id: String,
    to_display_name: String,
    message: String,
) -> ChatterEntry {
    ChatterEntry {
        session_id,
        kind: ChatterKind::Direct,
        from_agent_id,
        from_display_name: from_display_name.clone(),
        to_agent_id: Some(to_agent_id),
        to_display_name: Some(to_display_name.clone()),
        message: message.clone(),
        topics: Vec::new(),
        mentions: Vec::new(),
        timestamp_ms: now_ms(),
        public: false,
        display_text: format_direct_display(&from_display_name, &to_display_name, &message),
    }
}

fn build_chatter_entry(
    session_id: String,
    from_agent_id: Option<String>,
    from_display_name: String,
    message: String,
    topics: Vec<String>,
    mentions: Vec<String>,
) -> ChatterEntry {
    ChatterEntry {
        session_id,
        kind: ChatterKind::Public,
        from_agent_id,
        from_display_name: from_display_name.clone(),
        to_agent_id: None,
        to_display_name: None,
        message: message.clone(),
        topics,
        mentions,
        timestamp_ms: now_ms(),
        public: true,
        display_text: format_public_display(&from_display_name, &message),
    }
}

fn build_network_entry(
    session_id: String,
    from_agent_id: Option<String>,
    from_display_name: String,
    message: String,
) -> ChatterEntry {
    ChatterEntry {
        session_id,
        kind: ChatterKind::Network,
        from_agent_id,
        from_display_name: from_display_name.clone(),
        to_agent_id: None,
        to_display_name: None,
        message: message.clone(),
        topics: Vec::new(),
        mentions: Vec::new(),
        timestamp_ms: now_ms(),
        public: false,
        display_text: format_network_display(&from_display_name, &message),
    }
}

fn build_root_entry(
    session_id: String,
    from_agent_id: Option<String>,
    from_display_name: String,
    message: String,
) -> ChatterEntry {
    ChatterEntry {
        session_id,
        kind: ChatterKind::Root,
        from_agent_id,
        from_display_name: from_display_name.clone(),
        to_agent_id: None,
        to_display_name: None,
        message: message.clone(),
        topics: Vec::new(),
        mentions: Vec::new(),
        timestamp_ms: now_ms(),
        public: false,
        display_text: format_root_display(&from_display_name, &message),
    }
}

fn build_sign_on_entry(session_id: &str, display_name: &str) -> ChatterEntry {
    ChatterEntry {
        session_id: session_id.to_string(),
        kind: ChatterKind::SignOn,
        from_agent_id: None,
        from_display_name: display_name.to_string(),
        to_agent_id: None,
        to_display_name: None,
        message: "Signed On".to_string(),
        topics: Vec::new(),
        mentions: Vec::new(),
        timestamp_ms: now_ms(),
        public: true,
        display_text: format_sign_on_display(display_name),
    }
}

fn build_sign_off_entry(session_id: &str, display_name: &str) -> ChatterEntry {
    ChatterEntry {
        session_id: session_id.to_string(),
        kind: ChatterKind::SignOff,
        from_agent_id: None,
        from_display_name: display_name.to_string(),
        to_agent_id: None,
        to_display_name: None,
        message: "Signed Off".to_string(),
        topics: Vec::new(),
        mentions: Vec::new(),
        timestamp_ms: now_ms(),
        public: true,
        display_text: format_sign_off_display(display_name),
    }
}

fn channel_event_from_entry(entry: &ChatterEntry, replay: bool) -> AgentChannelEvent {
    let kind = match entry.kind {
        ChatterKind::Direct => AgentChannelEventKind::Direct,
        ChatterKind::Public => AgentChannelEventKind::Public,
        ChatterKind::Network => AgentChannelEventKind::Network,
        ChatterKind::Root => AgentChannelEventKind::Root,
        ChatterKind::SignOn | ChatterKind::SignOff => AgentChannelEventKind::System,
    };
    AgentChannelEvent {
        kind,
        from_agent_id: entry.from_agent_id.clone(),
        from_display_name: entry.from_display_name.clone(),
        to_agent_id: entry.to_agent_id.clone(),
        to_display_name: entry.to_display_name.clone(),
        message: entry.message.clone(),
        topics: entry.topics.clone(),
        mentions: entry.mentions.clone(),
        replay,
        ping_id: None,
        timestamp_ms: entry.timestamp_ms,
    }
}

fn broadcast_public_event(state: &AppState, app: &AppHandle, entry: &ChatterEntry) {
    let failed = state
        .broadcast_event_in_session(&entry.session_id, channel_event_from_entry(entry, false), false)
        .unwrap_or_default();
    for agent_id in failed {
        let _ = mark_agent_dead(state, app, &agent_id);
    }
}

fn work_ids_touched_by_connections(connections: &[network::NetworkConnection]) -> BTreeSet<String> {
    let mut work_ids = BTreeSet::new();
    for connection in connections {
        if let Some(work_id) = connection.from_tile_id.strip_prefix("work:") {
            work_ids.insert(work_id.to_string());
        }
        if let Some(work_id) = connection.to_tile_id.strip_prefix("work:") {
            work_ids.insert(work_id.to_string());
        }
    }
    work_ids
}

fn process_dead_agent(state: &AppState, app: &AppHandle, info: crate::agent::AgentInfo) -> Result<(), String> {
    match network::disconnect_all_for_tile_at(
        Path::new(runtime::database_path()),
        &info.session_id,
        &info.tile_id,
    ) {
        Ok(removed_connections) => {
            for connection in &removed_connections {
                notify_agents_about_connection_change(state, app, connection, false);
            }
            for work_id in work_ids_touched_by_connections(&removed_connections) {
                if let Ok(item) = work::get_work_item_at(Path::new(runtime::database_path()), &work_id) {
                    emit_work_updated(app, &item);
                }
            }
        }
        Err(error) => {
            log::warn!(
                "Failed to clear dead-agent network edges for {}: {error}",
                info.agent_id
            );
        }
    }
    let entry = build_sign_off_entry(&info.session_id, &info.display_name);
    append_chatter_entry(state, app, entry.clone())?;
    broadcast_public_event(state, app, &entry);
    emit_agent_state(app, state);
    if info.agent_role == AgentRole::Root {
        if let Err(error) = crate::commands::repair_root_agent(app.clone(), &info) {
            log::warn!(
                "Failed to respawn root agent {} for session {}: {error}",
                info.agent_id,
                info.session_id
            );
        }
    }
    Ok(())
}

fn mark_agent_dead(state: &AppState, app: &AppHandle, agent_id: &str) -> Result<(), String> {
    let Some(info) = state.mark_agent_dead(agent_id)? else {
        return Ok(());
    };
    process_dead_agent(state, app, info)
}

fn live_agent_info(state: &AppState, agent_id: &str) -> Result<crate::agent::AgentInfo, String> {
    let Some(info) = state.agent_info(agent_id)? else {
        return Err(format!("unknown agent: {agent_id}"));
    };
    if !info.alive {
        return Err(format!("agent {agent_id} is not alive"));
    }
    Ok(info)
}

fn parse_agent_log_kind(kind: &str) -> Result<AgentLogKind, String> {
    match kind.trim() {
        "incoming_hook" => Ok(AgentLogKind::IncomingHook),
        "outgoing_call" => Ok(AgentLogKind::OutgoingCall),
        other => Err(format!("unsupported agent log kind: {other}")),
    }
}

fn connection_event_message(connection: &network::NetworkConnection, connected: bool) -> String {
    let action = if connected { "connected" } else { "disconnected" };
    format!(
        "Port {action}: {}:{} <-> {}:{}",
        connection.from_tile_id,
        connection.from_port.as_str(),
        connection.to_tile_id,
        connection.to_port.as_str(),
    )
}

fn notify_agents_about_connection_change(
    state: &AppState,
    app: &AppHandle,
    connection: &network::NetworkConnection,
    connected: bool,
) {
    let message = connection_event_message(connection, connected);
    for tile_id in [&connection.from_tile_id, &connection.to_tile_id] {
        let Ok(Some(agent)) = state.agent_info_by_tile(tile_id) else {
            continue;
        };
        if !agent.alive {
            continue;
        }
        let event = AgentChannelEvent {
            kind: AgentChannelEventKind::System,
            from_agent_id: None,
            from_display_name: "HERD".to_string(),
            to_agent_id: Some(agent.agent_id.clone()),
            to_display_name: Some(agent.display_name.clone()),
            message: message.clone(),
            topics: Vec::new(),
            mentions: Vec::new(),
            replay: false,
            ping_id: None,
            timestamp_ms: now_ms(),
        };
        if let Err(error) = state.send_event_to_agent(&agent.agent_id, event) {
            log::warn!("Failed to deliver connection event to {}: {error}", agent.agent_id);
            let _ = mark_agent_dead(state, app, &agent.agent_id);
        }
    }
}

fn ensure_root_sender(context: &SenderContext, action: &str) -> Result<(), String> {
    if matches!(context.sender_agent_role, Some(AgentRole::Worker)) {
        return Err(format!(
            "non-root agents may not call {action}; send a message to Root instead"
        ));
    }
    Ok(())
}

fn ensure_root_agent_by_id(
    state: &AppState,
    agent_id: &str,
    action: &str,
) -> Result<crate::agent::AgentInfo, String> {
    let info = live_agent_info(state, agent_id)?;
    if info.agent_role != AgentRole::Root {
        return Err(format!(
            "non-root agents may not call {action}; send a message to Root instead"
        ));
    }
    Ok(info)
}

fn ensure_root_for_sender(
    state: &AppState,
    sender_agent_id: Option<String>,
    sender_pane_id: Option<String>,
    action: &str,
) -> Result<SenderContext, String> {
    let sender = resolve_sender_context(state, sender_agent_id, sender_pane_id)?;
    ensure_root_sender(&sender, action)?;
    Ok(sender)
}

fn maybe_respawn_root_agent(
    _state: &AppState,
    app: &AppHandle,
    info: &crate::agent::AgentInfo,
) {
    if info.agent_role != AgentRole::Root {
        return;
    }
    if let Err(error) = crate::commands::repair_root_agent(app.clone(), info) {
        log::warn!(
            "Failed to respawn root agent {} for session {}: {error}",
            info.agent_id,
            info.session_id
        );
    }
}

fn session_root_agent(state: &AppState, session_id: &str) -> Result<crate::agent::AgentInfo, String> {
    let Some(info) = state.root_agent_in_session(session_id)? else {
        return Err(format!("no root agent registered for session {session_id}"));
    };
    if !info.alive {
        return Err(format!("root agent for session {session_id} is not alive"));
    }
    Ok(info)
}

pub fn send_root_message(
    state: &AppState,
    app: &AppHandle,
    message: String,
    sender_agent_id: Option<String>,
    sender_pane_id: Option<String>,
) -> Result<(), String> {
    let sender = resolve_sender_context(state, sender_agent_id, sender_pane_id)?;
    send_root_message_from_sender(state, app, sender, message)
}

pub fn send_root_message_as_user(
    state: &AppState,
    app: &AppHandle,
    message: String,
) -> Result<(), String> {
    let sender = resolve_user_sender_context(state)?;
    send_root_message_from_sender(state, app, sender, message)
}

pub fn send_direct_message_as_user(
    state: &AppState,
    app: &AppHandle,
    target: String,
    message: String,
) -> Result<(), String> {
    let sender = resolve_user_sender_context(state)?;
    let target = resolve_user_message_target(state, &sender.session_id, &target)?;
    send_direct_message_from_sender(state, app, sender, target.agent_id, message)
}

pub fn send_public_message_as_user(
    state: &AppState,
    app: &AppHandle,
    message: String,
) -> Result<(), String> {
    let sender = resolve_user_sender_context(state)?;
    send_public_message_from_sender(state, app, sender, message, Vec::new(), Vec::new())
}

fn resolve_create_target(
    snapshot: &crate::tmux_state::TmuxSnapshot,
    parent_session_id: Option<String>,
    parent_pane_id: Option<String>,
) -> (Option<String>, Option<String>) {
    let target_session_id = parent_pane_id
        .as_ref()
        .and_then(|pane_id| {
            snapshot
                .panes
                .iter()
                .find(|pane| &pane.id == pane_id)
                .map(|pane| pane.session_id.clone())
        })
        .or(parent_session_id)
        .or(snapshot.active_session_id.clone());

    let parent_window_id = parent_pane_id.as_ref().and_then(|pane_id| {
        snapshot
            .panes
            .iter()
            .find(|pane| &pane.id == pane_id)
            .map(|pane| pane.window_id.clone())
    });

    (target_session_id, parent_window_id)
}

fn created_tile_response(
    pane_id: String,
    window_id: String,
    parent_window_id: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "pane_id": pane_id,
        "window_id": window_id,
        "parent_window_id": parent_window_id,
    })
}

fn network_tile_kind_for_parts(
    agent: Option<&crate::agent::AgentInfo>,
    window_name: &str,
    pane_title: &str,
) -> network::NetworkTileKind {
    if let Some(agent) = agent {
        return match agent.agent_role {
            AgentRole::Root => network::NetworkTileKind::RootAgent,
            AgentRole::Worker => network::NetworkTileKind::Agent,
        };
    }
    if pane_title.eq_ignore_ascii_case("Browser") || window_name.eq_ignore_ascii_case("Browser") {
        return network::NetworkTileKind::Browser;
    }
    network::NetworkTileKind::Shell
}

fn network_tile_kind_for_pane(
    state: &AppState,
    snapshot: &crate::tmux_state::TmuxSnapshot,
    pane_id: &str,
) -> Result<network::NetworkTileKind, String> {
    let agent = state.agent_info_by_tile(pane_id)?;
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.id == pane_id)
        .ok_or_else(|| format!("unknown tmux pane: {pane_id}"))?;
    let window_name = snapshot
        .windows
        .iter()
        .find(|window| window.id == pane.window_id)
        .map(|window| window.name.as_str())
        .unwrap_or("");
    Ok(network_tile_kind_for_parts(
        agent.as_ref(),
        window_name,
        &pane.title,
    ))
}

fn ensure_browser_pane(state: &AppState, pane_id: &str) -> Result<(), String> {
    let snapshot = crate::tmux_state::snapshot(state)?;
    match network_tile_kind_for_pane(state, &snapshot, pane_id)? {
        network::NetworkTileKind::Browser => Ok(()),
        _ => Err(format!("pane {pane_id} is not a browser tile")),
    }
}

fn snap_to_grid(value: f64) -> f64 {
    (value / GRID_SNAP).round() * GRID_SNAP
}

fn rects_overlap(a: &TileState, b: &TileState) -> bool {
    a.x < b.x + b.width
        && a.x + a.width > b.x
        && a.y < b.y + b.height
        && a.y + a.height > b.y
}

fn find_open_position(
    desired_x: f64,
    desired_y: f64,
    width: f64,
    height: f64,
    occupied_ids: &[String],
    entries: &std::collections::HashMap<String, TileState>,
) -> TileState {
    let overlaps = |candidate: &TileState| {
        occupied_ids.iter().any(|entry_id| {
            entries
                .get(entry_id)
                .map(|entry| rects_overlap(candidate, entry))
                .unwrap_or(false)
        })
    };

    let candidate = TileState {
        x: snap_to_grid(desired_x),
        y: snap_to_grid(desired_y),
        width,
        height,
    };
    if !overlaps(&candidate) {
        return candidate;
    }

    for ring in 1..=20 {
        let step_x = (width + GAP) * ring as f64;
        let step_y = (height + GAP) * ring as f64;
        let candidates = [
            (desired_x + step_x, desired_y),
            (desired_x - step_x, desired_y),
            (desired_x, desired_y + step_y),
            (desired_x, desired_y - step_y),
            (desired_x + step_x, desired_y + step_y),
            (desired_x - step_x, desired_y + step_y),
            (desired_x + step_x, desired_y - step_y),
            (desired_x - step_x, desired_y - step_y),
        ];
        for (x, y) in candidates {
            let candidate = TileState {
                x: snap_to_grid(x),
                y: snap_to_grid(y),
                width,
                height,
            };
            if !overlaps(&candidate) {
                return candidate;
            }
        }
    }

    TileState {
        x: snap_to_grid(desired_x),
        y: snap_to_grid(desired_y + occupied_ids.len() as f64 * (height + GAP)),
        width,
        height,
    }
}

fn session_layout_entries(
    state: &AppState,
    snapshot: &crate::tmux_state::TmuxSnapshot,
    session_id: &str,
    work_items: &[work::WorkItem],
) -> std::collections::HashMap<String, TileState> {
    let persisted = state
        .tile_states
        .lock()
        .map(|entries| entries.clone())
        .unwrap_or_default();
    let mut entries = std::collections::HashMap::new();
    let session_window_ids = snapshot
        .windows
        .iter()
        .filter(|window| window.session_id == session_id)
        .map(|window| window.id.clone())
        .collect::<Vec<_>>();

    for window_id in &session_window_ids {
        if let Some(entry) = persisted.get(window_id) {
            entries.insert(window_id.clone(), entry.clone());
        }
    }
    for item in work_items {
        let entry_id = network::work_tile_id(&item.work_id);
        if let Some(entry) = persisted.get(&entry_id) {
            entries.insert(entry_id, entry.clone());
        }
    }

    for (index, window) in snapshot
        .windows
        .iter()
        .filter(|window| window.session_id == session_id)
        .enumerate()
    {
        if entries.contains_key(&window.id) {
            continue;
        }

        let occupied_ids = session_window_ids
            .iter()
            .filter(|window_id| *window_id != &window.id)
            .cloned()
            .collect::<Vec<_>>();
        let next_entry = if let Some(parent_entry) = window
            .parent_window_id
            .as_ref()
            .and_then(|parent_window_id| entries.get(parent_window_id).cloned())
        {
            find_open_position(
                parent_entry.x + parent_entry.width + GAP + GRID_SNAP,
                parent_entry.y,
                DEFAULT_TILE_WIDTH,
                DEFAULT_TILE_HEIGHT,
                &occupied_ids,
                &entries,
            )
        } else {
            let offset = index as f64 * 40.0;
            find_open_position(
                100.0 + offset,
                100.0 + offset,
                DEFAULT_TILE_WIDTH,
                DEFAULT_TILE_HEIGHT,
                &occupied_ids,
                &entries,
            )
        };
        entries.insert(window.id.clone(), next_entry);
    }

    let max_x = session_window_ids
        .iter()
        .filter_map(|window_id| entries.get(window_id))
        .fold(80.0_f64, |value, entry| value.max(entry.x + entry.width));
    let min_y = session_window_ids
        .iter()
        .filter_map(|window_id| entries.get(window_id))
        .fold(f64::INFINITY, |value, entry| value.min(entry.y));
    let base_x = max_x + GAP * 2.0;
    let base_y = if min_y.is_finite() { min_y } else { 80.0 };

    for (index, item) in work_items.iter().enumerate() {
        let entry_id = network::work_tile_id(&item.work_id);
        if entries.contains_key(&entry_id) {
            continue;
        }
        let occupied_ids = session_window_ids
            .iter()
            .cloned()
            .chain(
                work_items
                    .iter()
                    .filter(|other| other.work_id != item.work_id)
                    .map(|other| network::work_tile_id(&other.work_id)),
            )
            .collect::<Vec<_>>();
        let next_entry = find_open_position(
            base_x,
            base_y + index as f64 * (WORK_CARD_HEIGHT + GAP),
            WORK_CARD_WIDTH,
            WORK_CARD_HEIGHT,
            &occupied_ids,
            &entries,
        );
        entries.insert(entry_id, next_entry);
    }

    entries
}

fn tile_state_from_info(tile: &network::SessionTileInfo) -> TileState {
    TileState {
        x: tile.x,
        y: tile.y,
        width: tile.width,
        height: tile.height,
    }
}

fn tile_with_layout(tile: &network::SessionTileInfo, layout: &TileState) -> network::SessionTileInfo {
    let mut next = tile.clone();
    next.x = layout.x;
    next.y = layout.y;
    next.width = layout.width;
    next.height = layout.height;
    next
}

fn tile_layout_entry_id(tile: &network::SessionTileInfo) -> Result<String, String> {
    if tile.kind == network::NetworkTileKind::Work {
        return Ok(tile.tile_id.clone());
    }
    tile.window_id
        .clone()
        .ok_or_else(|| format!("tile {} is missing a window id", tile.tile_id))
}

fn session_tile_by_id(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    tile_id: &str,
) -> Result<network::SessionTileInfo, String> {
    session_network_tiles(app, state, session_id)?
        .into_iter()
        .find(|tile| tile.tile_id == tile_id)
        .ok_or_else(|| format!("tile {tile_id} is not available from session {session_id}"))
}

fn pane_tile_details(
    pane: &crate::tmux_state::TmuxPane,
    window: &crate::tmux_state::TmuxWindow,
) -> network::PaneTileDetails {
    network::PaneTileDetails {
        window_name: window.name.clone(),
        window_index: window.index,
        pane_index: pane.pane_index,
        cols: pane.cols,
        rows: pane.rows,
        active: pane.active,
        dead: pane.dead,
    }
}

fn session_network_tiles(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<Vec<network::SessionTileInfo>, String> {
    let snapshot = crate::tmux_state::snapshot(state)?;
    let work_items = work::list_work_at(
        Path::new(runtime::database_path()),
        work::WorkListScope::CurrentSession(session_id.to_string()),
    )?;
    let layout_entries = session_layout_entries(state, &snapshot, session_id, &work_items);
    let agents_by_tile = state
        .list_agents_in_session(session_id)?
        .into_iter()
        .map(|agent| (agent.tile_id.clone(), agent))
        .collect::<std::collections::HashMap<_, _>>();
    let mut tiles = Vec::new();
    for window in snapshot.windows.iter().filter(|window| window.session_id == session_id) {
        let Some(pane_id) = window.pane_ids.first() else {
            continue;
        };
        let Some(pane) = snapshot.panes.iter().find(|pane| pane.id == *pane_id) else {
            continue;
        };
        let agent = agents_by_tile.get(pane_id);
        let kind = network_tile_kind_for_parts(agent, &window.name, &pane.title);
        let title = match agent {
            Some(agent) => agent.title.clone(),
            None if !window.name.trim().is_empty() => window.name.clone(),
            None if !pane.title.trim().is_empty() => pane.title.clone(),
            None => pane.id.clone(),
        };
        let details = match kind {
            network::NetworkTileKind::Agent | network::NetworkTileKind::RootAgent => {
                let agent = agent.ok_or_else(|| format!("missing agent record for {}", pane.id))?;
                network::TileDetails::Agent(network::AgentTileDetails {
                    agent_id: agent.agent_id.clone(),
                    agent_type: agent.agent_type,
                    agent_role: agent.agent_role,
                    display_name: agent.display_name.clone(),
                    alive: agent.alive,
                    chatter_subscribed: agent.chatter_subscribed,
                    topics: agent.topics.clone(),
                    agent_pid: agent.agent_pid,
                })
            }
            network::NetworkTileKind::Browser => network::TileDetails::Browser(network::BrowserTileDetails {
                window_name: window.name.clone(),
                window_index: window.index,
                pane_index: pane.pane_index,
                cols: pane.cols,
                rows: pane.rows,
                active: pane.active,
                dead: pane.dead,
                current_url: crate::browser::current_url_for_pane(app, pane_id),
            }),
            network::NetworkTileKind::Shell | network::NetworkTileKind::Output => {
                network::TileDetails::Shell(pane_tile_details(pane, window))
            }
            network::NetworkTileKind::Work => unreachable!("work tiles are built from the work registry"),
        };
        let layout = layout_entries
            .get(&window.id)
            .cloned()
            .unwrap_or(TileState {
                x: 0.0,
                y: 0.0,
                width: DEFAULT_TILE_WIDTH,
                height: DEFAULT_TILE_HEIGHT,
            });
        tiles.push(network::SessionTileInfo {
            tile_id: pane_id.clone(),
            session_id: session_id.to_string(),
            kind,
            title,
            x: layout.x,
            y: layout.y,
            width: layout.width,
            height: layout.height,
            pane_id: Some(pane.id.clone()),
            window_id: Some(window.id.clone()),
            parent_window_id: window.parent_window_id.clone(),
            command: Some(pane.command.clone()),
            details,
        });
    }

    for item in work_items {
        let entry_id = network::work_tile_id(&item.work_id);
        let layout = layout_entries
            .get(&entry_id)
            .cloned()
            .unwrap_or(TileState {
                x: 0.0,
                y: 0.0,
                width: WORK_CARD_WIDTH,
                height: WORK_CARD_HEIGHT,
            });
        tiles.push(network::SessionTileInfo {
            tile_id: entry_id,
            session_id: session_id.to_string(),
            kind: network::NetworkTileKind::Work,
            title: item.title.clone(),
            x: layout.x,
            y: layout.y,
            width: layout.width,
            height: layout.height,
            pane_id: None,
            window_id: None,
            parent_window_id: None,
            command: None,
            details: network::TileDetails::Work(network::WorkTileDetails {
                work_id: item.work_id.clone(),
                topic: item.topic.clone(),
                owner_agent_id: item.owner_agent_id.clone(),
                current_stage: item.current_stage,
                stages: item.stages.clone(),
                reviews: item.reviews.clone(),
                created_at: item.created_at,
                updated_at: item.updated_at,
            }),
        });
    }

    tiles.sort_by(|left, right| left.tile_id.cmp(&right.tile_id));
    Ok(tiles)
}

fn resolve_network_tile_descriptor(
    state: &AppState,
    session_id: &str,
    tile_id: &str,
) -> Result<network::NetworkTileDescriptor, String> {
    if let Some(work_id) = tile_id.strip_prefix("work:") {
        let item = work::get_work_item_at(Path::new(runtime::database_path()), work_id)?;
        if item.session_id != session_id {
            return Err(format!("tile {tile_id} is not in session {session_id}"));
        }
        return Ok(network::NetworkTileDescriptor {
            tile_id: tile_id.to_string(),
            session_id: session_id.to_string(),
            kind: network::NetworkTileKind::Work,
        });
    }

    let snapshot = crate::tmux_state::snapshot(state)?;
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.id == tile_id)
        .ok_or_else(|| format!("unknown tile: {tile_id}"))?;
    if pane.session_id != session_id {
        return Err(format!("tile {tile_id} is not in session {session_id}"));
    }

    Ok(network::NetworkTileDescriptor {
        tile_id: tile_id.to_string(),
        session_id: session_id.to_string(),
        kind: network_tile_kind_for_pane(state, &snapshot, tile_id)?,
    })
}

fn component_for_sender(
    app: &AppHandle,
    state: &AppState,
    sender: &SenderContext,
) -> Result<network::NetworkComponent, String> {
    let Some(start_tile_id) = sender.sender_tile_id.as_deref() else {
        return Ok(network::NetworkComponent {
            session_id: sender.session_id.clone(),
            tiles: Vec::new(),
            connections: Vec::new(),
        });
    };
    let session_tiles = session_network_tiles(app, state, &sender.session_id)?;
    let connections = network::list_connections_at(Path::new(runtime::database_path()), &sender.session_id)?;
    Ok(network::component_for_tile(
        &sender.session_id,
        start_tile_id,
        &session_tiles,
        &connections,
    ))
}

fn session_component(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<network::NetworkComponent, String> {
    Ok(network::NetworkComponent {
        session_id: session_id.to_string(),
        tiles: session_network_tiles(app, state, session_id)?,
        connections: network::list_connections_at(Path::new(runtime::database_path()), session_id)?,
    })
}

fn session_tile_list(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    tile_type: Option<network::TileTypeFilter>,
) -> Result<Vec<network::SessionTileInfo>, String> {
    Ok(network::filter_tiles(
        session_network_tiles(app, state, session_id)?,
        tile_type,
    ))
}

fn update_tile_layout(
    app: &AppHandle,
    state: &AppState,
    tile: &network::SessionTileInfo,
    layout: TileState,
    request_resize: bool,
) -> Result<network::SessionTileInfo, String> {
    let entry_id = tile_layout_entry_id(tile)?;
    state.set_tile_state(&entry_id, layout.clone());
    state.save();
    emit_layout_updated(app, tile, &layout, request_resize);
    Ok(tile_with_layout(tile, &layout))
}

fn resolve_work_session_id(
    state: &AppState,
    explicit_session_id: Option<String>,
    agent_id: Option<String>,
    sender_pane_id: Option<String>,
) -> Result<String, String> {
    if let Some(session_id) = explicit_session_id.filter(|value| !value.trim().is_empty()) {
        if let Some(agent_id) = agent_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let info = live_agent_info(state, agent_id)?;
            if info.session_id != session_id {
                return Err(format!(
                    "agent {agent_id} cannot access work in session {session_id}",
                ));
            }
        }
        if let Some(pane_id) = sender_pane_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let pane_session_id = resolve_session_id_for_pane(state, pane_id)?;
            if pane_session_id != session_id {
                return Err(format!(
                    "pane {pane_id} cannot access work in session {session_id}",
                ));
            }
        }
        return Ok(session_id);
    }
    if let Some(agent_id) = agent_id.filter(|value| !value.trim().is_empty()) {
        let info = live_agent_info(state, &agent_id)?;
        return Ok(info.session_id);
    }
    if let Some(pane_id) = sender_pane_id.filter(|value| !value.trim().is_empty()) {
        return resolve_session_id_for_pane(state, &pane_id);
    }
    if let Some(session_id) = state.last_active_session() {
        return Ok(session_id);
    }
    crate::tmux_state::snapshot(state)?
        .active_session_id
        .ok_or("no active session available for work command".to_string())
}

fn test_driver_enabled() -> bool {
    runtime::test_driver_enabled()
}

fn tmux_control_client_alive(control_pid: Option<libc::pid_t>) -> bool {
    let Some(control_pid) = control_pid else {
        return false;
    };

    let output = match tmux::output(&["list-clients", "-F", "#{client_pid}\t#{client_control_mode}"]) {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };

    let control_pid = control_pid.to_string();
    String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        let mut parts = line.split('\t');
        matches!(
            (parts.next(), parts.next()),
            (Some(client_pid), Some("1")) if client_pid == control_pid
        )
    })
}

fn test_driver_status(state: &AppState) -> serde_json::Value {
    serde_json::json!({
        "enabled": test_driver_enabled(),
        "frontend_ready": state.test_driver_frontend_ready(),
        "bootstrap_complete": state.test_driver_bootstrap_complete(),
        "runtime_id": runtime::runtime_id(),
        "tmux_server_name": runtime::tmux_server_name(),
        "socket_path": runtime::socket_path(),
        "tmux_server_alive": tmux::is_running(),
        "control_client_alive": tmux_control_client_alive(state.current_control_pid()),
    })
}

fn wait_for<F>(timeout_ms: u64, mut predicate: F, description: &str) -> Result<(), String>
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() <= deadline {
        if predicate() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(format!("timed out waiting for {description}"))
}

fn request_timeout_ms(request: &TestDriverRequest) -> u64 {
    match request {
        TestDriverRequest::WaitForIdle { timeout_ms, .. }
        | TestDriverRequest::WaitForReady { timeout_ms }
        | TestDriverRequest::WaitForBootstrap { timeout_ms } => timeout_ms.unwrap_or(10_000),
        _ => 10_000,
    }
}

fn forward_test_driver_request(
    state: &AppState,
    app: &AppHandle,
    request: TestDriverRequest,
) -> SocketResponse {
    if !state.test_driver_frontend_ready() {
        return SocketResponse::error("frontend test driver is not ready".into());
    }

    let request_id = state.next_test_driver_request_id();
    let (sender, receiver) = mpsc::channel();
    if let Err(error) = state.register_test_driver_request(&request_id, sender) {
        return SocketResponse::error(error);
    }

    let emit_result = app.emit("test-driver-request", serde_json::json!({
        "request_id": request_id,
        "request": request,
    }));
    if let Err(error) = emit_result {
        state.cancel_test_driver_request(&request_id);
        return SocketResponse::error(format!("emit test-driver-request failed: {error}"));
    }

    match receiver.recv_timeout(Duration::from_millis(request_timeout_ms(&request))) {
        Ok(Ok(data)) => SocketResponse::success(Some(data)),
        Ok(Err(error)) => SocketResponse::error(error),
        Err(_) => {
            state.cancel_test_driver_request(&request_id);
            SocketResponse::error("timed out waiting for test-driver response".into())
        }
    }
}

fn handle_test_dom_query(js: String, app: &AppHandle) -> SocketResponse {
    if let Some(webview) = app.webview_windows().values().next() {
        let result_file = runtime::dom_result_path().to_string();
        let _ = std::fs::remove_file(&result_file);

        let wrapped = format!(
            r#"(function() {{
                try {{
                    const __r = (function(){{ {js} }})();
                    const __s = JSON.stringify(__r === undefined ? null : __r);
                    window.__TAURI_INTERNALS__.invoke('__write_dom_result', {{ result: __s }});
                }} catch(e) {{
                    window.__TAURI_INTERNALS__.invoke('__write_dom_result', {{ result: JSON.stringify("ERR:" + e.message) }});
                }}
            }})()"#
        );
        if let Err(error) = webview.eval(&wrapped) {
            return SocketResponse::error(format!("eval failed: {error}"));
        }

        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(data) = std::fs::read_to_string(&result_file) {
                let _ = std::fs::remove_file(&result_file);
                match serde_json::from_str::<serde_json::Value>(&data) {
                    Ok(value) => return SocketResponse::success(Some(value)),
                    Err(_) => return SocketResponse::success(Some(serde_json::json!(data))),
                }
            }
        }

        SocketResponse::success(Some(serde_json::json!(null)))
    } else {
        SocketResponse::error("No webview found".into())
    }
}

fn handle_test_dom_keys(keys: String, app: &AppHandle) -> SocketResponse {
    if let Some(webview) = app.webview_windows().values().next() {
        let js = format!(
            r#"(function() {{
                const keys = {keys_json};
                for (const k of keys.split(' ')) {{
                    let key = k, shiftKey = false, ctrlKey = false;
                    if (k.includes('+')) {{
                        const parts = k.split('+');
                        key = parts[parts.length - 1];
                        shiftKey = parts.includes('Shift');
                        ctrlKey = parts.includes('Ctrl');
                    }}
                    const ev = new KeyboardEvent('keydown', {{
                        key: key, code: 'Key' + key.toUpperCase(),
                        shiftKey, ctrlKey, bubbles: true, cancelable: true
                    }});
                    window.dispatchEvent(ev);
                }}
            }})()"#,
            keys_json = serde_json::to_string(&keys).unwrap_or_default(),
        );
        let _ = webview.eval(&js);
        std::thread::sleep(Duration::from_millis(200));
        SocketResponse::success(None)
    } else {
        SocketResponse::error("No webview found".into())
    }
}

fn resolve_agent_snapshot_metadata(
    state: &AppState,
    pane_id: &str,
    title_override: Option<String>,
) -> Result<(String, String, String), String> {
    let snapshot = crate::tmux_state::snapshot(state)?;
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.id == pane_id)
        .cloned()
        .ok_or_else(|| format!("no tmux pane found for {pane_id}"))?;
    let window = snapshot
        .windows
        .iter()
        .find(|window| window.id == pane.window_id)
        .cloned()
        .ok_or_else(|| format!("no tmux window found for {}", pane.window_id))?;
    let title = title_override
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if !window.name.trim().is_empty() {
                window.name.clone()
            } else if !pane.title.trim().is_empty() {
                pane.title.clone()
            } else {
                "Agent".to_string()
            }
        });
    Ok((window.id, pane.session_id, title))
}

async fn agent_ping_loop(state: AppState, app: AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let cycle = match state.prepare_agent_ping_cycle(AGENT_PING_INTERVAL, AGENT_PING_TIMEOUT) {
            Ok(cycle) => cycle,
            Err(error) => {
                log::warn!("agent ping cycle failed: {error}");
                continue;
            }
        };

        for agent_id in cycle.expired {
            let _ = mark_agent_dead(&state, &app, &agent_id);
        }

        for (agent_id, ping_id) in cycle.to_ping {
            let event = AgentChannelEvent {
                kind: AgentChannelEventKind::Ping,
                from_agent_id: None,
                from_display_name: "HERD".to_string(),
                to_agent_id: Some(agent_id.clone()),
                to_display_name: None,
                message: "PING".to_string(),
                topics: Vec::new(),
                mentions: Vec::new(),
                replay: false,
                ping_id: Some(ping_id),
                timestamp_ms: now_ms(),
            };
            if state.send_event_to_agent(&agent_id, event).is_err() {
                let _ = mark_agent_dead(&state, &app, &agent_id);
            }
        }
    }
}

async fn handle_agent_event_subscription(
    agent_id: String,
    mut lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    mut writer: tokio::net::unix::OwnedWriteHalf,
    state: AppState,
    app: AppHandle,
    logger: SharedLogger,
) {
    let (sender, mut receiver) = tokio_mpsc::unbounded_channel();
    let subscription = match state.subscribe_agent_events(&agent_id, sender) {
        Ok(subscription) => subscription,
        Err(error) => {
            let mut resp_json = serde_json::to_string(&SocketResponse::error(error)).unwrap_or_default();
            resp_json.push('\n');
            let _ = writer.write_all(resp_json.as_bytes()).await;
            return;
        }
    };
    let replay_entries = state
        .public_chatter_since_in_session(&subscription.info.session_id, now_ms() - AGENT_REPLAY_WINDOW_MS)
        .unwrap_or_default();

    let response = SocketResponse::success(Some(serde_json::json!({
        "agent": subscription.info,
    })));
    let mut resp_json = serde_json::to_string(&response).unwrap_or_default();
    if let Ok(mut guard) = logger.lock() {
        if let Some(ref mut l) = *guard {
            l.log("<<<", &resp_json);
        }
    }
    resp_json.push('\n');
    if writer.write_all(resp_json.as_bytes()).await.is_err() {
        if let Ok(Some(info)) = state.unsubscribe_agent_events(&agent_id, subscription.subscriber_id) {
            let _ = process_dead_agent(&state, &app, info);
        }
        return;
    }

    if subscription.signed_on {
        let entry = build_sign_on_entry(&subscription.info.session_id, &subscription.info.display_name);
        let _ = append_chatter_entry(&state, &app, entry.clone());
        broadcast_public_event(&state, &app, &entry);
        emit_agent_state(&app, &state);
    }

    if subscription.bootstrap {
        let welcome = AgentChannelEvent {
            kind: AgentChannelEventKind::Direct,
            from_agent_id: None,
            from_display_name: "HERD".to_string(),
            to_agent_id: Some(agent_id.clone()),
            to_display_name: Some(subscription.info.display_name.clone()),
            message: match subscription.info.agent_role {
                AgentRole::Root => HERD_ROOT_WELCOME_MESSAGE,
                AgentRole::Worker => HERD_WORKER_WELCOME_MESSAGE,
            }
            .to_string(),
            topics: Vec::new(),
            mentions: Vec::new(),
            replay: false,
            ping_id: None,
            timestamp_ms: now_ms(),
        };
        let _ = state.send_event_to_agent(&agent_id, welcome);
        for entry in replay_entries {
            let _ = state.send_event_to_agent(&agent_id, channel_event_from_entry(&entry, true));
        }
    }

    loop {
        tokio::select! {
            maybe_event = receiver.recv() => {
                let Some(event) = maybe_event else {
                    break;
                };
                let mut event_json = serde_json::to_string(&event).unwrap_or_default();
                if let Ok(mut guard) = logger.lock() {
                    if let Some(ref mut l) = *guard {
                        l.log("<<<", &event_json);
                    }
                }
                event_json.push('\n');
                if writer.write_all(event_json.as_bytes()).await.is_err() {
                    break;
                }
            }
            maybe_line = lines.next_line() => {
                match maybe_line {
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    if let Ok(Some(info)) = state.unsubscribe_agent_events(&agent_id, subscription.subscriber_id) {
        let _ = process_dead_agent(&state, &app, info);
    }
}

pub async fn start(state: AppState, app_handle: AppHandle) {
    let path = Path::new(runtime::socket_path());
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }

    let listener = match UnixListener::bind(runtime::socket_path()) {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind Unix socket at {}: {e}", runtime::socket_path());
            return;
        }
    };

    let logger: SharedLogger = Arc::new(Mutex::new(SocketLogger::open()));
    log::info!("Socket server listening on {}", runtime::socket_path());
    tokio::spawn(agent_ping_loop(state.clone(), app_handle.clone()));

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = state.clone();
                let app = app_handle.clone();
                let logger = logger.clone();
                tokio::spawn(async move {
                    handle_connection(stream, state, app, logger).await;
                });
            }
            Err(e) => {
                log::error!("Socket accept error: {e}");
            }
        }
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: AppState,
    app: AppHandle,
    logger: SharedLogger,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(mut guard) = logger.lock() {
            if let Some(ref mut l) = *guard {
                l.log(">>>", &line);
            }
        }

        let response = match serde_json::from_str::<SocketCommand>(&line) {
            Ok(SocketCommand::AgentEventsSubscribe { agent_id }) => {
                handle_agent_event_subscription(agent_id, lines, writer, state, app, logger).await;
                return;
            }
            Ok(cmd) => handle_command(cmd, &state, &app),
            Err(e) => SocketResponse::error(format!("Parse error: {e}")),
        };

        let mut resp_json = serde_json::to_string(&response).unwrap_or_default();

        if let Ok(mut guard) = logger.lock() {
            if let Some(ref mut l) = *guard {
                l.log("<<<", &resp_json);
            }
        }

        resp_json.push('\n');
        if writer.write_all(resp_json.as_bytes()).await.is_err() {
            break;
        }
    }
}

fn handle_command(
    cmd: SocketCommand,
    state: &AppState,
    app: &AppHandle,
) -> SocketResponse {
    match cmd {
        SocketCommand::ShellCreate { x: _, y: _, width: _, height: _, parent_session_id, parent_pane_id } => {
            let before = match crate::tmux_state::snapshot(state) {
                Ok(snapshot) => snapshot,
                Err(e) => return SocketResponse::error(e),
            };
            let (target_session_id, parent_window_id) =
                resolve_create_target(&before, parent_session_id, parent_pane_id);

            match crate::commands::new_window_detached(app.clone(), target_session_id) {
                Ok(window_id) => {
                    if let Some(parent_window_id) = parent_window_id.clone() {
                        state.set_window_parent(&window_id, Some(parent_window_id));
                        let _ = crate::tmux_state::emit_snapshot(app);
                    }
                    let snapshot = crate::tmux_state::snapshot(state);
                    let pane_id = snapshot
                        .ok()
                        .and_then(|snapshot| {
                            snapshot
                                .windows
                                .iter()
                                .find(|window| window.id == window_id)
                                .and_then(|window| window.pane_ids.first().cloned())
                        })
                        .unwrap_or_else(|| window_id.clone());
                    SocketResponse::success(Some(created_tile_response(
                        pane_id,
                        window_id,
                        parent_window_id,
                    )))
                }
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellDestroy { session_id, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_destroy") {
                return SocketResponse::error(error);
            }
            match crate::tmux_state::kill_pane(&session_id) {
                Ok(()) => {
                    let _ = crate::tmux_state::emit_snapshot(app);
                    SocketResponse::success(None)
                }
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellList { sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_list") {
                return SocketResponse::error(error);
            }
            match crate::tmux_state::snapshot(state) {
                Ok(snapshot) => {
                    let list: Vec<serde_json::Value> = snapshot
                        .windows
                        .iter()
                        .filter_map(|window| {
                            let pane = snapshot.panes.iter().find(|pane| pane.window_id == window.id)?;
                            Some(serde_json::json!({
                                "id": pane.id,
                                "pane_id": pane.id,
                                "window_id": window.id,
                                "session_id": window.session_id,
                                "title": window.name,
                                "command": pane.command,
                            }))
                        })
                        .collect();
                    SocketResponse::success(Some(serde_json::json!(list)))
                }
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellInputSend { session_id, input, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_input_send") {
                return SocketResponse::error(error);
            }
            match state.with_control(|ctrl| ctrl.writer.send_input_by_id(&session_id, input.as_bytes())) {
                Ok(()) => SocketResponse::success(None),
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellExec { session_id, shell_command, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_exec") {
                return SocketResponse::error(error);
            }
            match crate::tmux_state::respawn_pane_shell_command(&session_id, &shell_command) {
                Ok(()) => {
                    let _ = crate::tmux_state::emit_snapshot(app);
                    SocketResponse::success(None)
                }
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellOutputRead { session_id, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_output_read") {
                return SocketResponse::error(error);
            }
            match state.with_control(|ctrl| ctrl.read_output(&session_id)) {
                Ok(output) => {
                    SocketResponse::success(Some(serde_json::json!({ "output": output })))
                }
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellTitleSet { session_id, title, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_title_set") {
                return SocketResponse::error(error);
            }
            match crate::commands::set_pane_title(app.clone(), session_id, title) {
                Ok(()) => SocketResponse::success(None),
                Err(e) => SocketResponse::error(e),
            }
        }

        SocketCommand::ShellReadOnlySet { session_id, read_only, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_read_only_set") {
                return SocketResponse::error(error);
            }
            let payload = serde_json::json!({
                "session_id": session_id,
                "read_only": read_only,
            });
            let _ = app.emit("shell-read-only", payload);
            SocketResponse::success(None)
        }

        SocketCommand::ShellRoleSet { session_id, role, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "shell_role_set") {
                return SocketResponse::error(error);
            }
            let payload = serde_json::json!({
                "session_id": session_id,
                "role": role,
            });
            let _ = app.emit("shell-role", payload);
            SocketResponse::success(None)
        }

        SocketCommand::BrowserCreate { parent_session_id, parent_pane_id } => {
            let before = match crate::tmux_state::snapshot(state) {
                Ok(snapshot) => snapshot,
                Err(error) => return SocketResponse::error(error),
            };
            let (target_session_id, parent_window_id) =
                resolve_create_target(&before, parent_session_id, parent_pane_id);

            match crate::commands::spawn_browser_window_with_pane(app.clone(), target_session_id) {
                Ok(created) => {
                    if let Some(parent_window_id) = parent_window_id.clone() {
                        state.set_window_parent(&created.window_id, Some(parent_window_id));
                        let _ = crate::tmux_state::emit_snapshot(app);
                    }
                    SocketResponse::success(Some(created_tile_response(
                        created.pane_id,
                        created.window_id,
                        parent_window_id,
                    )))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::BrowserDestroy { pane_id, sender_agent_id, sender_pane_id } => {
            if let Err(error) = ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "browser_destroy") {
                return SocketResponse::error(error);
            }
            if let Err(error) = ensure_browser_pane(state, &pane_id) {
                return SocketResponse::error(error);
            }
            match crate::commands::kill_pane(app.clone(), pane_id) {
                Ok(()) => SocketResponse::success(None),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::BrowserNavigate { pane_id, url, sender_agent_id, sender_pane_id } => {
            if let Err(error) =
                ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "browser_navigate")
            {
                return SocketResponse::error(error);
            }
            if let Err(error) = ensure_browser_pane(state, &pane_id) {
                return SocketResponse::error(error);
            }
            match crate::browser::navigate_browser_webview(app, &pane_id, &url) {
                Ok(browser_state) => match serde_json::to_value(browser_state) {
                    Ok(data) => SocketResponse::success(Some(data)),
                    Err(error) => SocketResponse::error(format!("failed to serialize browser state: {error}")),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::BrowserLoad { pane_id, path, sender_agent_id, sender_pane_id } => {
            if let Err(error) =
                ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "browser_load")
            {
                return SocketResponse::error(error);
            }
            if let Err(error) = ensure_browser_pane(state, &pane_id) {
                return SocketResponse::error(error);
            }
            match crate::browser::load_browser_webview(app, &pane_id, &path) {
                Ok(browser_state) => match serde_json::to_value(browser_state) {
                    Ok(data) => SocketResponse::success(Some(data)),
                    Err(error) => SocketResponse::error(format!("failed to serialize browser state: {error}")),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::AgentCreate { parent_session_id, parent_pane_id } => {
            let before = match crate::tmux_state::snapshot(state) {
                Ok(snapshot) => snapshot,
                Err(error) => return SocketResponse::error(error),
            };
            let (target_session_id, parent_window_id) =
                resolve_create_target(&before, parent_session_id, parent_pane_id);

            match crate::commands::spawn_agent_window(app.clone(), target_session_id) {
                Ok(created) => {
                    if let Some(parent_window_id) = parent_window_id {
                        if let Some(window_id) = created.get("window_id").and_then(|value| value.as_str()) {
                            state.set_window_parent(window_id, Some(parent_window_id));
                            let _ = crate::tmux_state::emit_snapshot(app);
                        }
                    }
                    SocketResponse::success(Some(created))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::AgentRegister { agent_id, agent_type, agent_role, pane_id, agent_pid, title } => {
            match resolve_agent_snapshot_metadata(state, &pane_id, title) {
                Ok((window_id, session_id, resolved_title)) => {
                    let agent_type = match parse_agent_type(agent_type.as_deref()) {
                        Ok(agent_type) => agent_type,
                        Err(error) => return SocketResponse::error(error),
                    };
                    let agent_role = match parse_agent_role(agent_role.as_deref()) {
                        Ok(agent_role) => agent_role,
                        Err(error) => return SocketResponse::error(error),
                    };
                    match state.upsert_agent(agent_id, pane_id, window_id, session_id, resolved_title, agent_type, agent_role, agent_pid) {
                        Ok(info) => {
                            emit_agent_state(app, state);
                            SocketResponse::success(Some(serde_json::json!({ "agent": info })))
                        }
                        Err(error) => SocketResponse::error(error),
                    }
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::AgentUnregister { agent_id } => {
            match state.unregister_agent(&agent_id) {
                Ok(Some(info)) => {
                    emit_agent_state(app, state);
                    maybe_respawn_root_agent(state, app, &info);
                    SocketResponse::success(None)
                }
                Ok(None) => SocketResponse::success(None),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::AgentPingAck { agent_id } => match state.ack_agent_ping(&agent_id) {
            Ok(info) => SocketResponse::success(Some(serde_json::json!({ "agent": info }))),
            Err(error) => SocketResponse::error(error),
        },

        SocketCommand::ListAgents { sender_agent_id, sender_pane_id } => {
            match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "agent_list") {
                Ok(sender) => match state.list_agents_in_session(&sender.session_id) {
                    Ok(agents) => SocketResponse::success(Some(serde_json::json!(agents))),
                    Err(error) => SocketResponse::error(error),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::ListTopics { sender_agent_id, sender_pane_id } => {
            match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "topics_list") {
                Ok(sender) => match state.list_topics_in_session(&sender.session_id) {
                    Ok(topics) => SocketResponse::success(Some(serde_json::json!(topics))),
                    Err(error) => SocketResponse::error(error),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::ListNetwork { sender_agent_id, sender_pane_id, tile_type } => {
            match resolve_sender_context(state, sender_agent_id, sender_pane_id) {
                Ok(sender) => match component_for_sender(app, state, &sender) {
                    Ok(component) => SocketResponse::success(Some(serde_json::json!(
                        network::filter_component(component, tile_type)
                    ))),
                    Err(error) => SocketResponse::error(error),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::SessionList { sender_agent_id, sender_pane_id, tile_type } => {
            match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "session_list") {
                Ok(sender) => match session_component(app, state, &sender.session_id) {
                    Ok(component) => SocketResponse::success(Some(serde_json::json!(
                        network::filter_component(component, tile_type)
                    ))),
                    Err(error) => SocketResponse::error(error),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::TileList { sender_agent_id, sender_pane_id, tile_type } => {
            match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "tile_list") {
                Ok(sender) => match session_tile_list(app, state, &sender.session_id, tile_type) {
                    Ok(tiles) => SocketResponse::success(Some(serde_json::json!(tiles))),
                    Err(error) => SocketResponse::error(error),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::TileGet { tile_id, sender_agent_id, sender_pane_id } => {
            match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "tile_get") {
                Ok(sender) => match session_tile_by_id(app, state, &sender.session_id, &tile_id) {
                    Ok(tile) => SocketResponse::success(Some(serde_json::json!(tile))),
                    Err(error) => SocketResponse::error(error),
                },
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::TileMove {
            tile_id,
            x,
            y,
            sender_agent_id,
            sender_pane_id,
        } => {
            let sender = match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "tile_move") {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            let tile = match session_tile_by_id(app, state, &sender.session_id, &tile_id) {
                Ok(tile) => tile,
                Err(error) => return SocketResponse::error(error),
            };
            let current = tile_state_from_info(&tile);
            match update_tile_layout(
                app,
                state,
                &tile,
                TileState {
                    x,
                    y,
                    width: current.width,
                    height: current.height,
                },
                false,
            ) {
                Ok(updated) => SocketResponse::success(Some(serde_json::json!(updated))),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::TileResize {
            tile_id,
            width,
            height,
            sender_agent_id,
            sender_pane_id,
        } => {
            if width <= 0.0 || height <= 0.0 {
                return SocketResponse::error("tile dimensions must be greater than zero".to_string());
            }
            let sender = match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "tile_resize") {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            let tile = match session_tile_by_id(app, state, &sender.session_id, &tile_id) {
                Ok(tile) => tile,
                Err(error) => return SocketResponse::error(error),
            };
            let current = tile_state_from_info(&tile);
            match update_tile_layout(
                app,
                state,
                &tile,
                TileState {
                    x: current.x,
                    y: current.y,
                    width,
                    height,
                },
                tile.pane_id.is_some(),
            ) {
                Ok(updated) => SocketResponse::success(Some(serde_json::json!(updated))),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::NetworkConnect {
            from_tile_id,
            from_port,
            to_tile_id,
            to_port,
            sender_agent_id,
            sender_pane_id,
        } => {
            let sender = match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "network_connect") {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            let from_descriptor = match resolve_network_tile_descriptor(state, &sender.session_id, &from_tile_id) {
                Ok(descriptor) => descriptor,
                Err(error) => return SocketResponse::error(error),
            };
            let to_descriptor = match resolve_network_tile_descriptor(state, &sender.session_id, &to_tile_id) {
                Ok(descriptor) => descriptor,
                Err(error) => return SocketResponse::error(error),
            };
            let from_port = match network::parse_port(&from_port) {
                Ok(port) => port,
                Err(_) => return SocketResponse::error("invalid from_port".to_string()),
            };
            let to_port = match network::parse_port(&to_port) {
                Ok(port) => port,
                Err(_) => return SocketResponse::error("invalid to_port".to_string()),
            };
            match network::connect_at(
                Path::new(runtime::database_path()),
                &from_descriptor,
                from_port,
                &to_descriptor,
                to_port,
            ) {
                Ok(connection) => {
                    notify_agents_about_connection_change(state, app, &connection, true);
                    for work_id in work_ids_touched_by_connections(std::slice::from_ref(&connection)) {
                        if let Ok(item) = work::get_work_item_at(Path::new(runtime::database_path()), &work_id) {
                            emit_work_updated(app, &item);
                        }
                    }
                    emit_agent_state(app, state);
                    SocketResponse::success(Some(serde_json::json!(connection)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::NetworkDisconnect { tile_id, port, sender_agent_id, sender_pane_id } => {
            let sender = match ensure_root_for_sender(state, sender_agent_id, sender_pane_id, "network_disconnect") {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            let descriptor = match resolve_network_tile_descriptor(state, &sender.session_id, &tile_id) {
                Ok(descriptor) => descriptor,
                Err(error) => return SocketResponse::error(error),
            };
            let port = match network::parse_port(&port) {
                Ok(port) => port,
                Err(_) => return SocketResponse::error("invalid port".to_string()),
            };
            match network::disconnect_at(
                Path::new(runtime::database_path()),
                &descriptor.session_id,
                &descriptor.tile_id,
                port,
            ) {
                Ok(removed) => {
                    if let Some(connection) = removed.as_ref() {
                        notify_agents_about_connection_change(state, app, connection, false);
                        for work_id in work_ids_touched_by_connections(std::slice::from_ref(connection)) {
                            if let Ok(item) = work::get_work_item_at(Path::new(runtime::database_path()), &work_id) {
                                emit_work_updated(app, &item);
                            }
                        }
                    }
                    emit_agent_state(app, state);
                    SocketResponse::success(Some(serde_json::json!(removed)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::MessageDirect {
            to_agent_id,
            message,
            sender_agent_id,
            sender_pane_id,
        } => {
            let sender = match resolve_sender_context(state, sender_agent_id, sender_pane_id) {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            match send_direct_message_from_sender(state, app, sender, to_agent_id, message) {
                Ok(()) => SocketResponse::success(None),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::MessagePublic {
            message,
            topics,
            mentions,
            sender_agent_id,
            sender_pane_id,
        } => {
            let sender = match resolve_sender_context(state, sender_agent_id, sender_pane_id) {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            match send_public_message_from_sender(state, app, sender, message, topics, mentions) {
                Ok(()) => SocketResponse::success(None),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::MessageNetwork {
            message,
            sender_agent_id,
            sender_pane_id,
        } => {
            let sender = match resolve_sender_context(state, sender_agent_id, sender_pane_id) {
                Ok(sender) => sender,
                Err(error) => return SocketResponse::error(error),
            };
            let Some(from_agent_id) = sender.sender_agent_id.clone() else {
                return SocketResponse::error("message_network requires an agent sender".into());
            };
            let component = match component_for_sender(app, state, &sender) {
                Ok(component) => component,
                Err(error) => return SocketResponse::error(error),
            };
            let recipient_tile_ids = component
                .tiles
                .into_iter()
                .map(|tile| tile.tile_id)
                .collect::<BTreeSet<_>>();
            let recipients = match state.list_agents_in_session(&sender.session_id) {
                Ok(agents) => agents,
                Err(error) => return SocketResponse::error(error),
            };
            for recipient in recipients
                .into_iter()
                .filter(|agent| agent.alive)
                .filter(|agent| agent.agent_id != from_agent_id)
                .filter(|agent| recipient_tile_ids.contains(&agent.tile_id))
            {
                let event = AgentChannelEvent {
                    kind: AgentChannelEventKind::Direct,
                    from_agent_id: Some(from_agent_id.clone()),
                    from_display_name: sender.display_name.clone(),
                    to_agent_id: Some(recipient.agent_id.clone()),
                    to_display_name: Some(recipient.display_name.clone()),
                    message: message.clone(),
                    topics: Vec::new(),
                    mentions: Vec::new(),
                    replay: false,
                    ping_id: None,
                    timestamp_ms: now_ms(),
                };
                if let Err(error) = state.send_event_to_agent(&recipient.agent_id, event) {
                    let _ = mark_agent_dead(state, app, &recipient.agent_id);
                    return SocketResponse::error(error);
                }
            }
            let entry = build_network_entry(
                sender.session_id,
                sender.sender_agent_id,
                sender.display_name,
                message,
            );
            if let Err(error) = append_chatter_entry(state, app, entry) {
                return SocketResponse::error(error);
            }
            SocketResponse::success(None)
        }

        SocketCommand::MessageRoot {
            message,
            sender_agent_id,
            sender_pane_id,
        } => {
            if let Err(error) = send_root_message(state, app, message, sender_agent_id, sender_pane_id) {
                return SocketResponse::error(error);
            }
            SocketResponse::success(None)
        }

        SocketCommand::TopicSubscribe { topic, agent_id } => {
            let Some(topic) = crate::agent::normalize_topic(&topic) else {
                return SocketResponse::error("invalid topic".into());
            };
            let Some(agent_id) = agent_id else {
                return SocketResponse::error("agent_id is required".into());
            };
            if let Err(error) = live_agent_info(state, &agent_id) {
                return SocketResponse::error(error);
            }
            match state.topic_subscribe(&agent_id, &topic) {
                Ok(info) => {
                    emit_agent_state(app, state);
                    SocketResponse::success(Some(serde_json::json!(info)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::TopicUnsubscribe { topic, agent_id } => {
            let Some(topic) = crate::agent::normalize_topic(&topic) else {
                return SocketResponse::error("invalid topic".into());
            };
            let Some(agent_id) = agent_id else {
                return SocketResponse::error("agent_id is required".into());
            };
            if let Err(error) = live_agent_info(state, &agent_id) {
                return SocketResponse::error(error);
            }
            match state.topic_unsubscribe(&agent_id, &topic) {
                Ok(info) => {
                    emit_agent_state(app, state);
                    SocketResponse::success(Some(serde_json::json!(info)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkList {
            scope,
            session_id,
            agent_id,
            sender_pane_id,
        } => {
            if let Some(agent_id) = agent_id.as_deref() {
                if let Err(error) = ensure_root_agent_by_id(state, agent_id, "work_list") {
                    return SocketResponse::error(error);
                }
            }
            if matches!(scope.as_deref(), Some("all")) {
                return SocketResponse::error("work_list is private to the caller session".into());
            }
            let scope = match resolve_work_session_id(state, session_id, agent_id, sender_pane_id) {
                Ok(session_id) => work::WorkListScope::CurrentSession(session_id),
                Err(error) => return SocketResponse::error(error),
            };
            match work::list_work_at(Path::new(runtime::database_path()), scope) {
                Ok(items) => SocketResponse::success(Some(serde_json::json!(items))),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkGet { work_id, session_id, agent_id, sender_pane_id } => {
            if let Some(agent_id) = agent_id.as_deref() {
                if let Err(error) = ensure_root_agent_by_id(state, agent_id, "work_get") {
                    return SocketResponse::error(error);
                }
            }
            let expected_session_id = match resolve_work_session_id(state, session_id, agent_id, sender_pane_id) {
                Ok(session_id) => session_id,
                Err(error) => return SocketResponse::error(error),
            };
            match work::get_work_item_at(Path::new(runtime::database_path()), &work_id) {
                Ok(item) if item.session_id == expected_session_id => {
                    SocketResponse::success(Some(serde_json::json!(item)))
                }
                Ok(_) => SocketResponse::error(format!(
                    "work item {work_id} is not available from session {expected_session_id}",
                )),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkCreate {
            title,
            session_id,
            sender_agent_id,
            sender_pane_id,
        } => {
            if sender_agent_id.is_some() {
                if let Err(error) = ensure_root_for_sender(state, sender_agent_id.clone(), sender_pane_id.clone(), "work_create") {
                    return SocketResponse::error(error);
                }
            }
            let session_id = match resolve_work_session_id(
                state,
                session_id,
                sender_agent_id,
                sender_pane_id,
            ) {
                Ok(session_id) => session_id,
                Err(error) => return SocketResponse::error(error),
            };
            match work::create_work_item_at(
                Path::new(runtime::database_path()),
                &runtime::project_root_dir(),
                &session_id,
                &title,
            ) {
                Ok(item) => {
                    if let Err(error) = state.touch_topics_in_session(&item.session_id, std::slice::from_ref(&item.topic)) {
                        log::warn!("Failed to register work topic {}: {error}", item.topic);
                    } else {
                        emit_agent_state(app, state);
                    }
                    emit_work_updated(app, &item);
                    SocketResponse::success(Some(serde_json::json!(item)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkStageStart { work_id, agent_id } => {
            if let Err(error) = live_agent_info(state, &agent_id) {
                return SocketResponse::error(error);
            }
            match work::start_work_stage_at(Path::new(runtime::database_path()), &work_id, &agent_id) {
                Ok(item) => {
                    emit_work_updated(app, &item);
                    SocketResponse::success(Some(serde_json::json!(item)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkStageComplete { work_id, agent_id } => {
            if let Err(error) = live_agent_info(state, &agent_id) {
                return SocketResponse::error(error);
            }
            match work::complete_work_stage_at(Path::new(runtime::database_path()), &work_id, &agent_id) {
                Ok(item) => {
                    emit_work_updated(app, &item);
                    SocketResponse::success(Some(serde_json::json!(item)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkReviewApprove { work_id } => {
            match work::approve_work_stage_at(Path::new(runtime::database_path()), &work_id) {
                Ok(item) => {
                    emit_work_updated(app, &item);
                    SocketResponse::success(Some(serde_json::json!(item)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::WorkReviewImprove { work_id, comment } => {
            match work::improve_work_stage_at(Path::new(runtime::database_path()), &work_id, &comment) {
                Ok(item) => {
                    emit_work_updated(app, &item);
                    SocketResponse::success(Some(serde_json::json!(item)))
                }
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::AgentEventsSubscribe { .. } => {
            SocketResponse::error("agent event subscriptions require a dedicated streaming connection".into())
        }

        SocketCommand::AgentLogAppend { agent_id, kind, text, timestamp_ms } => {
            let info = match live_agent_info(state, &agent_id) {
                Ok(info) => info,
                Err(error) => return SocketResponse::error(error),
            };
            let kind = match parse_agent_log_kind(&kind) {
                Ok(kind) => kind,
                Err(error) => return SocketResponse::error(error),
            };
            let entry = AgentLogEntry {
                session_id: info.session_id,
                agent_id: info.agent_id,
                tile_id: info.tile_id,
                kind,
                text,
                timestamp_ms: timestamp_ms.unwrap_or_else(now_ms),
            };
            match append_agent_log_entry(state, app, entry) {
                Ok(()) => SocketResponse::success(None),
                Err(error) => SocketResponse::error(error),
            }
        }

        SocketCommand::TestDriver { request } => {
            if !test_driver_enabled() {
                return SocketResponse::error("test driver is not enabled".into());
            }

            match request.clone() {
                TestDriverRequest::Ping => SocketResponse::success(Some(serde_json::json!({
                    "pong": true,
                    "status": test_driver_status(state),
                }))),
                TestDriverRequest::WaitForReady { timeout_ms } => {
                    match wait_for(
                        timeout_ms.unwrap_or(10_000),
                        || state.test_driver_frontend_ready(),
                        "frontend test driver readiness",
                    ) {
                        Ok(()) => SocketResponse::success(Some(test_driver_status(state))),
                        Err(error) => SocketResponse::error(error),
                    }
                }
                TestDriverRequest::WaitForBootstrap { timeout_ms } => {
                    match wait_for(
                        timeout_ms.unwrap_or(10_000),
                        || state.test_driver_bootstrap_complete(),
                        "frontend bootstrap completion",
                    ) {
                        Ok(()) => SocketResponse::success(Some(test_driver_status(state))),
                        Err(error) => SocketResponse::error(error),
                    }
                }
                TestDriverRequest::GetStatus => SocketResponse::success(Some(test_driver_status(state))),
                other => forward_test_driver_request(state, app, other),
            }
        }

        SocketCommand::TestDomQuery { js } => {
            if !test_driver_enabled() {
                return SocketResponse::error("test driver is not enabled".into());
            }
            handle_test_dom_query(js, app)
        }

        SocketCommand::TestDomKeys { keys } => {
            if !test_driver_enabled() {
                return SocketResponse::error("test driver is not enabled".into());
            }
            handle_test_dom_keys(keys, app)
        }
    }
}

pub fn cleanup() {
    let path = Path::new(runtime::socket_path());
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}
