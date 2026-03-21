use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::{
    agent::{AgentInfo, AgentRole, AgentType},
    db,
    work::{WorkReviewEntry, WorkStage, WorkStageState},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TilePort {
    Left,
    Top,
    Right,
    Bottom,
}

impl TilePort {
    pub const ALL: [Self; 4] = [Self::Left, Self::Top, Self::Right, Self::Bottom];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortMode {
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTileKind {
    Agent,
    RootAgent,
    Shell,
    Output,
    Work,
    Browser,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkTileDescriptor {
    pub tile_id: String,
    pub session_id: String,
    pub kind: NetworkTileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConnection {
    pub session_id: String,
    pub from_tile_id: String,
    pub from_port: TilePort,
    pub to_tile_id: String,
    pub to_port: TilePort,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TileTypeFilter {
    Agent,
    Shell,
    Browser,
    Work,
}

impl TileTypeFilter {
    pub fn matches_kind(self, kind: NetworkTileKind) -> bool {
        match self {
            Self::Agent => matches!(kind, NetworkTileKind::Agent | NetworkTileKind::RootAgent),
            Self::Shell => matches!(kind, NetworkTileKind::Shell | NetworkTileKind::Output),
            Self::Browser => kind == NetworkTileKind::Browser,
            Self::Work => kind == NetworkTileKind::Work,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTileDetails {
    pub agent_id: String,
    pub agent_type: AgentType,
    pub agent_role: AgentRole,
    pub display_name: String,
    pub alive: bool,
    pub chatter_subscribed: bool,
    pub topics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneTileDetails {
    pub window_name: String,
    pub window_index: u32,
    pub pane_index: u32,
    pub cols: u32,
    pub rows: u32,
    pub active: bool,
    pub dead: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserTileDetails {
    pub window_name: String,
    pub window_index: u32,
    pub pane_index: u32,
    pub cols: u32,
    pub rows: u32,
    pub active: bool,
    pub dead: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkTileDetails {
    pub work_id: String,
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_agent_id: Option<String>,
    pub current_stage: WorkStage,
    pub stages: Vec<WorkStageState>,
    pub reviews: Vec<WorkReviewEntry>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TileDetails {
    Agent(AgentTileDetails),
    Shell(PaneTileDetails),
    Browser(BrowserTileDetails),
    Work(WorkTileDetails),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTileInfo {
    pub tile_id: String,
    pub session_id: String,
    pub kind: NetworkTileKind,
    pub title: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub details: TileDetails,
}

impl SessionTileInfo {
    pub fn placeholder(tile_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        let tile_id = tile_id.into();
        Self {
            title: tile_id.clone(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            pane_id: None,
            window_id: None,
            parent_window_id: None,
            command: None,
            details: TileDetails::Shell(PaneTileDetails {
                window_name: String::new(),
                window_index: 0,
                pane_index: 0,
                cols: 0,
                rows: 0,
                active: false,
                dead: false,
            }),
            tile_id,
            session_id: session_id.into(),
            kind: NetworkTileKind::Shell,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkComponent {
    pub session_id: String,
    pub tiles: Vec<SessionTileInfo>,
    pub connections: Vec<NetworkConnection>,
}

pub fn filter_tiles(mut tiles: Vec<SessionTileInfo>, tile_type: Option<TileTypeFilter>) -> Vec<SessionTileInfo> {
    if let Some(tile_type) = tile_type {
        tiles.retain(|tile| tile_type.matches_kind(tile.kind));
    }
    tiles
}

pub fn work_tile_id(work_id: &str) -> String {
    format!("work:{work_id}")
}

pub fn browser_controller_agent_id_at(
    db_path: &Path,
    session_id: &str,
    browser_tile_id: &str,
) -> Result<Option<String>, String> {
    let conn = db::open_at(db_path)?;
    controller_agent_id_with_conn(&conn, session_id, browser_tile_id)
}

pub fn derived_work_owner_agent_id_at(
    db_path: &Path,
    session_id: &str,
    work_id: &str,
) -> Result<Option<String>, String> {
    let conn = db::open_at(db_path)?;
    derived_work_owner_agent_id_with_conn(&conn, session_id, work_id)
}

pub fn derived_work_owner_agent_id_with_conn(
    conn: &Connection,
    session_id: &str,
    work_id: &str,
) -> Result<Option<String>, String> {
    controller_agent_id_with_conn(conn, session_id, &work_tile_id(work_id))
}

pub fn controller_agent_id_with_conn(
    conn: &Connection,
    session_id: &str,
    controlled_tile_id: &str,
) -> Result<Option<String>, String> {
    let connections = list_connections_with_conn(conn, session_id)?;
    let connected_tile_id = connections.iter().find_map(|connection| {
        if connection.from_tile_id == controlled_tile_id && connection.from_port == TilePort::Left {
            Some(connection.to_tile_id.clone())
        } else if connection.to_tile_id == controlled_tile_id && connection.to_port == TilePort::Left {
            Some(connection.from_tile_id.clone())
        } else {
            None
        }
    });

    let Some(agent_tile_id) = connected_tile_id else {
        return Ok(None);
    };

    let mut stmt = conn
        .prepare("SELECT data_json FROM agent ORDER BY updated_at ASC")
        .map_err(|error| format!("failed to prepare agent owner lookup: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query agents for owner lookup: {error}"))?;
    for row in rows {
        let json = row.map_err(|error| format!("failed to decode agent owner row: {error}"))?;
        let agent = serde_json::from_str::<AgentInfo>(&json)
            .map_err(|error| format!("failed to parse agent owner json: {error}"))?;
        if agent.session_id == session_id && agent.tile_id == agent_tile_id && agent.alive {
            return Ok(Some(agent.agent_id));
        }
    }
    Ok(None)
}

pub fn list_connections_at(db_path: &Path, session_id: &str) -> Result<Vec<NetworkConnection>, String> {
    let conn = db::open_at(db_path)?;
    list_connections_with_conn(&conn, session_id)
}

pub fn list_connections_with_conn(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<NetworkConnection>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT from_tile_id, from_port, to_tile_id, to_port
             FROM network_connection
             WHERE session_id = ?1
             ORDER BY from_tile_id ASC, from_port ASC, to_tile_id ASC, to_port ASC",
        )
        .map_err(|error| format!("failed to prepare network query: {error}"))?;
    let rows = stmt
        .query_map([session_id], |row| {
            Ok(NetworkConnection {
                session_id: session_id.to_string(),
                from_tile_id: row.get(0)?,
                from_port: parse_port(&row.get::<_, String>(1)?)?,
                to_tile_id: row.get(2)?,
                to_port: parse_port(&row.get::<_, String>(3)?)?,
            })
        })
        .map_err(|error| format!("failed to query network connections: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode network connection rows: {error}"))
}

pub fn connect_at(
    db_path: &Path,
    from: &NetworkTileDescriptor,
    from_port: TilePort,
    to: &NetworkTileDescriptor,
    to_port: TilePort,
) -> Result<NetworkConnection, String> {
    let mut conn = db::open_at(db_path)?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("failed to begin network connect transaction: {error}"))?;
    let connection = connect_with_conn(&tx, from, from_port, to, to_port)?;
    tx.commit()
        .map_err(|error| format!("failed to commit network connect transaction: {error}"))?;
    Ok(connection)
}

pub fn connect_with_conn(
    conn: &Connection,
    from: &NetworkTileDescriptor,
    from_port: TilePort,
    to: &NetworkTileDescriptor,
    to_port: TilePort,
) -> Result<NetworkConnection, String> {
    validate_connect(conn, from, from_port, to, to_port)?;
    let connection = canonical_connection(
        from.session_id.clone(),
        from.tile_id.clone(),
        from_port,
        to.tile_id.clone(),
        to_port,
    );
    conn.execute(
        "INSERT INTO network_connection (session_id, from_tile_id, from_port, to_tile_id, to_port)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            connection.session_id,
            connection.from_tile_id,
            connection.from_port.as_str(),
            connection.to_tile_id,
            connection.to_port.as_str()
        ],
    )
    .map_err(|error| format!("failed to insert network connection: {error}"))?;
    Ok(connection)
}

pub fn disconnect_at(
    db_path: &Path,
    session_id: &str,
    tile_id: &str,
    port: TilePort,
) -> Result<Option<NetworkConnection>, String> {
    let mut conn = db::open_at(db_path)?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("failed to begin network disconnect transaction: {error}"))?;
    let removed = disconnect_with_conn(&tx, session_id, tile_id, port)?;
    tx.commit()
        .map_err(|error| format!("failed to commit network disconnect transaction: {error}"))?;
    Ok(removed)
}

pub fn disconnect_with_conn(
    conn: &Connection,
    session_id: &str,
    tile_id: &str,
    port: TilePort,
) -> Result<Option<NetworkConnection>, String> {
    let existing = find_connection_for_port_with_conn(conn, session_id, tile_id, port)?;
    let Some(connection) = existing else {
        return Ok(None);
    };
    conn.execute(
        "DELETE FROM network_connection
         WHERE session_id = ?1
           AND from_tile_id = ?2
           AND from_port = ?3
           AND to_tile_id = ?4
           AND to_port = ?5",
        params![
            connection.session_id,
            connection.from_tile_id,
            connection.from_port.as_str(),
            connection.to_tile_id,
            connection.to_port.as_str()
        ],
    )
    .map_err(|error| format!("failed to delete network connection: {error}"))?;
    Ok(Some(connection))
}

pub fn disconnect_all_for_tile_at(
    db_path: &Path,
    session_id: &str,
    tile_id: &str,
) -> Result<Vec<NetworkConnection>, String> {
    let mut conn = db::open_at(db_path)?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("failed to begin network tile disconnect transaction: {error}"))?;
    let connections = list_connections_with_conn(&tx, session_id)?;
    let removed = connections
        .into_iter()
        .filter(|connection| connection.from_tile_id == tile_id || connection.to_tile_id == tile_id)
        .collect::<Vec<_>>();
    for connection in &removed {
        tx.execute(
            "DELETE FROM network_connection
             WHERE session_id = ?1
               AND from_tile_id = ?2
               AND from_port = ?3
               AND to_tile_id = ?4
               AND to_port = ?5",
            params![
                connection.session_id,
                connection.from_tile_id,
                connection.from_port.as_str(),
                connection.to_tile_id,
                connection.to_port.as_str()
            ],
        )
        .map_err(|error| format!("failed to delete network connection: {error}"))?;
    }
    tx.commit()
        .map_err(|error| format!("failed to commit network tile disconnect transaction: {error}"))?;
    Ok(removed)
}

pub fn component_for_tile(
    session_id: &str,
    start_tile_id: &str,
    session_tiles: &[SessionTileInfo],
    connections: &[NetworkConnection],
) -> NetworkComponent {
    let tile_by_id = session_tiles
        .iter()
        .cloned()
        .map(|tile| (tile.tile_id.clone(), tile))
        .collect::<HashMap<_, _>>();

    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for connection in connections.iter().filter(|connection| connection.session_id == session_id) {
        adjacency
            .entry(connection.from_tile_id.as_str())
            .or_default()
            .push(connection.to_tile_id.as_str());
        adjacency
            .entry(connection.to_tile_id.as_str())
            .or_default()
            .push(connection.from_tile_id.as_str());
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([start_tile_id.to_string()]);
    while let Some(tile_id) = queue.pop_front() {
        if !visited.insert(tile_id.clone()) {
            continue;
        }
        for neighbor in adjacency.get(tile_id.as_str()).into_iter().flatten() {
            if !visited.contains(*neighbor) {
                queue.push_back((*neighbor).to_string());
            }
        }
    }

    if visited.is_empty() {
        visited.insert(start_tile_id.to_string());
    }

    let mut tiles = visited
        .iter()
        .map(|tile_id| {
            tile_by_id
                .get(tile_id)
                .cloned()
                .unwrap_or(SessionTileInfo::placeholder(tile_id.clone(), session_id))
        })
        .collect::<Vec<_>>();
    tiles.sort_by(|left, right| left.tile_id.cmp(&right.tile_id));

    let mut tile_ids = visited;
    tile_ids.insert(start_tile_id.to_string());
    let mut component_connections = connections
        .iter()
        .filter(|connection| connection.session_id == session_id)
        .filter(|connection| {
            tile_ids.contains(&connection.from_tile_id) && tile_ids.contains(&connection.to_tile_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    component_connections.sort_by(|left, right| {
        left.from_tile_id
            .cmp(&right.from_tile_id)
            .then_with(|| left.from_port.cmp(&right.from_port))
            .then_with(|| left.to_tile_id.cmp(&right.to_tile_id))
            .then_with(|| left.to_port.cmp(&right.to_port))
    });

    NetworkComponent {
        session_id: session_id.to_string(),
        tiles,
        connections: component_connections,
    }
}

pub fn filter_component(mut component: NetworkComponent, tile_type: Option<TileTypeFilter>) -> NetworkComponent {
    component.tiles = filter_tiles(component.tiles, tile_type);
    let tile_ids = component
        .tiles
        .iter()
        .map(|tile| tile.tile_id.clone())
        .collect::<HashSet<_>>();
    component
        .connections
        .retain(|connection| tile_ids.contains(&connection.from_tile_id) && tile_ids.contains(&connection.to_tile_id));
    component
}

pub fn port_mode(kind: NetworkTileKind, port: TilePort) -> PortMode {
    match kind {
        NetworkTileKind::Work | NetworkTileKind::Browser => {
            if port == TilePort::Left {
                PortMode::ReadWrite
            } else {
                PortMode::Read
            }
        }
        NetworkTileKind::Agent
        | NetworkTileKind::RootAgent
        | NetworkTileKind::Shell
        | NetworkTileKind::Output => PortMode::ReadWrite,
    }
}

pub fn parse_port(value: &str) -> Result<TilePort, rusqlite::Error> {
    match value {
        "left" => Ok(TilePort::Left),
        "top" => Ok(TilePort::Top),
        "right" => Ok(TilePort::Right),
        "bottom" => Ok(TilePort::Bottom),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown port: {value}"),
            )),
        )),
    }
}

fn canonical_connection(
    session_id: String,
    from_tile_id: String,
    from_port: TilePort,
    to_tile_id: String,
    to_port: TilePort,
) -> NetworkConnection {
    let left_key = (from_tile_id.as_str(), from_port.as_str());
    let right_key = (to_tile_id.as_str(), to_port.as_str());
    if left_key <= right_key {
        NetworkConnection {
            session_id,
            from_tile_id,
            from_port,
            to_tile_id,
            to_port,
        }
    } else {
        NetworkConnection {
            session_id,
            from_tile_id: to_tile_id,
            from_port: to_port,
            to_tile_id: from_tile_id,
            to_port: from_port,
        }
    }
}

fn validate_connect(
    conn: &Connection,
    from: &NetworkTileDescriptor,
    from_port: TilePort,
    to: &NetworkTileDescriptor,
    to_port: TilePort,
) -> Result<(), String> {
    if from.session_id != to.session_id {
        return Err("cannot connect tiles across sessions".to_string());
    }
    if from.tile_id == to.tile_id {
        return Err("cannot connect a tile to itself".to_string());
    }
    let from_mode = port_mode(from.kind, from_port);
    let to_mode = port_mode(to.kind, to_port);
    if from_mode == PortMode::Read && to_mode == PortMode::Read {
        return Err("cannot connect a read-only port to another read-only port".to_string());
    }
    validate_controlled_port(from, from_port, to)?;
    validate_controlled_port(to, to_port, from)?;

    if find_connection_for_port_with_conn(conn, &from.session_id, &from.tile_id, from_port)?.is_some() {
        return Err(format!("port {} on {} is already connected", from_port.as_str(), from.tile_id));
    }
    if find_connection_for_port_with_conn(conn, &to.session_id, &to.tile_id, to_port)?.is_some() {
        return Err(format!("port {} on {} is already connected", to_port.as_str(), to.tile_id));
    }

    Ok(())
}

fn validate_controlled_port(
    controlled: &NetworkTileDescriptor,
    controlled_port: TilePort,
    other: &NetworkTileDescriptor,
) -> Result<(), String> {
    if matches!(controlled.kind, NetworkTileKind::Work | NetworkTileKind::Browser)
        && controlled_port == TilePort::Left
        && !is_agent_kind(other.kind)
    {
        return Err(format!(
            "{} left port only accepts agent tiles",
            match controlled.kind {
                NetworkTileKind::Work => "work",
                NetworkTileKind::Browser => "browser",
                _ => "controlled",
            }
        ));
    }
    Ok(())
}

fn is_agent_kind(kind: NetworkTileKind) -> bool {
    matches!(kind, NetworkTileKind::Agent | NetworkTileKind::RootAgent)
}

fn find_connection_for_port_with_conn(
    conn: &Connection,
    session_id: &str,
    tile_id: &str,
    port: TilePort,
) -> Result<Option<NetworkConnection>, String> {
    let connections = list_connections_with_conn(conn, session_id)?;
    Ok(connections.into_iter().find(|connection| {
        (connection.from_tile_id == tile_id && connection.from_port == port)
            || (connection.to_tile_id == tile_id && connection.to_port == port)
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        component_for_tile, connect_at, derived_work_owner_agent_id_at, disconnect_all_for_tile_at,
        filter_component, list_connections_at, port_mode, work_tile_id, NetworkTileDescriptor,
        NetworkTileKind, PaneTileDetails, PortMode, SessionTileInfo, TileDetails, TileTypeFilter,
        TilePort, WorkTileDetails,
    };
    use crate::agent::{AgentInfo, AgentRole, AgentType};
    use crate::db;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;

    fn temp_db_path(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("herd-network-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root.join("herd.sqlite")
    }

    fn replace_agents(path: &PathBuf, agents: Vec<AgentInfo>) {
        db::replace_agents_at(path, &agents).unwrap();
    }

    fn agent(tile_id: &str, session_id: &str, agent_id: &str) -> AgentInfo {
        AgentInfo {
            agent_id: agent_id.to_string(),
            agent_type: AgentType::Claude,
            agent_role: AgentRole::Worker,
            tile_id: tile_id.to_string(),
            window_id: format!("@{}", tile_id.trim_start_matches('%')),
            session_id: session_id.to_string(),
            title: "Agent".to_string(),
            display_name: agent_id.to_string(),
            alive: true,
            chatter_subscribed: true,
            topics: Vec::new(),
            agent_pid: None,
        }
    }

    fn session_tile(tile_id: &str, session_id: &str, kind: NetworkTileKind) -> SessionTileInfo {
        let title = match kind {
            NetworkTileKind::Work => "Work".to_string(),
            _ => format!("Tile {tile_id}"),
        };
        let command = (!matches!(kind, NetworkTileKind::Work)).then(|| "zsh".to_string());
        let details = match kind {
            NetworkTileKind::Agent | NetworkTileKind::RootAgent => TileDetails::Agent(super::AgentTileDetails {
                agent_id: format!("agent-{}", tile_id.trim_start_matches('%')),
                agent_type: AgentType::Claude,
                agent_role: if kind == NetworkTileKind::RootAgent {
                    AgentRole::Root
                } else {
                    AgentRole::Worker
                },
                display_name: "Agent".to_string(),
                alive: true,
                chatter_subscribed: true,
                topics: Vec::new(),
                agent_pid: None,
            }),
            NetworkTileKind::Browser => TileDetails::Browser(super::BrowserTileDetails {
                window_name: "Browser".to_string(),
                window_index: 0,
                pane_index: 0,
                cols: 80,
                rows: 24,
                active: false,
                dead: false,
                current_url: Some("https://example.com/".to_string()),
            }),
            NetworkTileKind::Work => TileDetails::Work(WorkTileDetails {
                work_id: tile_id.trim_start_matches("work:").to_string(),
                topic: "#work".to_string(),
                owner_agent_id: None,
                current_stage: crate::work::WorkStage::Plan,
                stages: Vec::new(),
                reviews: Vec::new(),
                created_at: 0,
                updated_at: 0,
            }),
            NetworkTileKind::Shell | NetworkTileKind::Output => TileDetails::Shell(PaneTileDetails {
                window_name: "Shell".to_string(),
                window_index: 0,
                pane_index: 0,
                cols: 80,
                rows: 24,
                active: false,
                dead: false,
            }),
        };
        SessionTileInfo {
            tile_id: tile_id.to_string(),
            session_id: session_id.to_string(),
            kind,
            title,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            pane_id: (!matches!(kind, NetworkTileKind::Work)).then(|| tile_id.to_string()),
            window_id: (!matches!(kind, NetworkTileKind::Work)).then(|| format!("@{}", tile_id.trim_start_matches('%'))),
            parent_window_id: None,
            command,
            details,
        }
    }

    #[test]
    fn resolves_port_modes_by_tile_kind() {
        assert_eq!(port_mode(NetworkTileKind::Agent, TilePort::Top), PortMode::ReadWrite);
        assert_eq!(port_mode(NetworkTileKind::Shell, TilePort::Bottom), PortMode::ReadWrite);
        assert_eq!(port_mode(NetworkTileKind::Work, TilePort::Left), PortMode::ReadWrite);
        assert_eq!(port_mode(NetworkTileKind::Work, TilePort::Top), PortMode::Read);
        assert_eq!(port_mode(NetworkTileKind::Browser, TilePort::Right), PortMode::Read);
    }

    #[test]
    fn rejects_invalid_connection_shapes_and_enforces_port_uniqueness() {
        let path = temp_db_path("validation");
        let shell_a = NetworkTileDescriptor {
            tile_id: "%1".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Shell,
        };
        let shell_b = NetworkTileDescriptor {
            tile_id: "%2".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Shell,
        };
        let work = NetworkTileDescriptor {
            tile_id: work_tile_id("work-s1-001"),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Work,
        };
        let browser = NetworkTileDescriptor {
            tile_id: "%4".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Browser,
        };
        let agent = NetworkTileDescriptor {
            tile_id: "%3".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Agent,
        };

        let error = connect_at(&path, &work, TilePort::Top, &work, TilePort::Right).unwrap_err();
        assert!(error.contains("cannot connect a tile to itself"));

        let error = connect_at(&path, &work, TilePort::Top, &browser, TilePort::Top).unwrap_err();
        assert!(error.contains("read-only"));

        let error = connect_at(&path, &work, TilePort::Left, &shell_a, TilePort::Top).unwrap_err();
        assert!(error.contains("only accepts agent"));

        connect_at(&path, &agent, TilePort::Left, &shell_a, TilePort::Right).unwrap();
        let error = connect_at(&path, &shell_b, TilePort::Left, &agent, TilePort::Left).unwrap_err();
        assert!(error.contains("already connected"));
    }

    #[test]
    fn derives_session_local_components_and_singletons() {
        let path = temp_db_path("components");
        let a = NetworkTileDescriptor {
            tile_id: "%1".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Agent,
        };
        let b = NetworkTileDescriptor {
            tile_id: "%2".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Shell,
        };
        let c = NetworkTileDescriptor {
            tile_id: "%3".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Agent,
        };
        let isolated = NetworkTileDescriptor {
            tile_id: "%4".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Shell,
        };
        let foreign = NetworkTileDescriptor {
            tile_id: "%5".to_string(),
            session_id: "$2".to_string(),
            kind: NetworkTileKind::Shell,
        };

        connect_at(&path, &a, TilePort::Right, &b, TilePort::Left).unwrap();
        connect_at(&path, &b, TilePort::Top, &c, TilePort::Bottom).unwrap();

        let session_tiles = vec![
            session_tile(&a.tile_id, "$1", a.kind),
            session_tile(&b.tile_id, "$1", b.kind),
            session_tile(&c.tile_id, "$1", c.kind),
            session_tile(&isolated.tile_id, "$1", isolated.kind),
        ];
        let component = component_for_tile("$1", &a.tile_id, &session_tiles, &list_connections_at(&path, "$1").unwrap());
        assert_eq!(
            component.tiles.iter().map(|tile| tile.tile_id.as_str()).collect::<BTreeSet<_>>(),
            BTreeSet::from(["%1", "%2", "%3"])
        );

        let singleton = component_for_tile("$1", &isolated.tile_id, &session_tiles, &list_connections_at(&path, "$1").unwrap());
        assert_eq!(singleton.tiles.len(), 1);
        assert_eq!(singleton.tiles[0].tile_id, isolated.tile_id);
        assert!(list_connections_at(&path, "$2").unwrap().is_empty());
        let _ = foreign;
    }

    #[test]
    fn filters_components_by_requested_tile_type() {
        let component = super::NetworkComponent {
            session_id: "$1".to_string(),
            tiles: vec![
                session_tile("%1", "$1", NetworkTileKind::Agent),
                session_tile("%2", "$1", NetworkTileKind::Shell),
                session_tile("work:work-s1-001", "$1", NetworkTileKind::Work),
            ],
            connections: vec![
                super::NetworkConnection {
                    session_id: "$1".to_string(),
                    from_tile_id: "%1".to_string(),
                    from_port: TilePort::Left,
                    to_tile_id: "%2".to_string(),
                    to_port: TilePort::Right,
                },
                super::NetworkConnection {
                    session_id: "$1".to_string(),
                    from_tile_id: "%1".to_string(),
                    from_port: TilePort::Top,
                    to_tile_id: "work:work-s1-001".to_string(),
                    to_port: TilePort::Left,
                },
            ],
        };

        let filtered = filter_component(component, Some(TileTypeFilter::Agent));
        assert_eq!(filtered.tiles.len(), 1);
        assert_eq!(filtered.tiles[0].tile_id, "%1");
        assert!(filtered.connections.is_empty());
    }

    #[test]
    fn derives_work_owner_from_live_agent_connection_and_clears_on_disconnect() {
        let path = temp_db_path("owner");
        replace_agents(&path, vec![agent("%1", "$1", "agent-1")]);
        let work = NetworkTileDescriptor {
            tile_id: work_tile_id("work-s1-001"),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Work,
        };
        let agent_tile = NetworkTileDescriptor {
            tile_id: "%1".to_string(),
            session_id: "$1".to_string(),
            kind: NetworkTileKind::Agent,
        };

        connect_at(&path, &agent_tile, TilePort::Left, &work, TilePort::Left).unwrap();
        assert_eq!(
            derived_work_owner_agent_id_at(&path, "$1", "work-s1-001").unwrap(),
            Some("agent-1".to_string())
        );

        let removed = disconnect_all_for_tile_at(&path, "$1", "%1").unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(derived_work_owner_agent_id_at(&path, "$1", "work-s1-001").unwrap(), None);
    }
}
