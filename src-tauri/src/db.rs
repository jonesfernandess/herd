use std::fs;
use rusqlite::{params, Connection};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent::{now_ms, AgentInfo};
use crate::runtime;

const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS tile_state (
  pane_id TEXT PRIMARY KEY,
  x REAL NOT NULL,
  y REAL NOT NULL,
  width REAL NOT NULL,
  height REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS chatter (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  entry_json TEXT NOT NULL,
  timestamp_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent_id TEXT NOT NULL,
  tile_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  entry_json TEXT NOT NULL,
  timestamp_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent (
  agent_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  tile_id TEXT NOT NULL,
  data_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS topic (
  name TEXT PRIMARY KEY,
  data_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS network_connection (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL,
  from_tile_id TEXT NOT NULL,
  from_port TEXT NOT NULL,
  to_tile_id TEXT NOT NULL,
  to_port TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS work_item (
  work_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  title TEXT NOT NULL,
  owner_agent_id TEXT,
  current_stage TEXT NOT NULL,
  data_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS work_stage (
  work_id TEXT NOT NULL,
  stage_name TEXT NOT NULL,
  status TEXT NOT NULL,
  file_path TEXT NOT NULL,
  PRIMARY KEY (work_id, stage_name)
);

CREATE TABLE IF NOT EXISTS work_review (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  work_id TEXT NOT NULL,
  stage_name TEXT NOT NULL,
  decision TEXT NOT NULL,
  comment TEXT,
  created_at INTEGER NOT NULL
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedTopicRecord {
    #[serde(default)]
    pub session_id: String,
    pub name: String,
    pub subscribers: Vec<String>,
    pub last_activity_at: Option<i64>,
}

pub fn open() -> Result<Connection, String> {
    open_at(Path::new(runtime::database_path()))
}

pub fn open_at(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let conn = Connection::open(path)
        .map_err(|error| format!("failed to open sqlite db {}: {error}", path.display()))?;
    conn.execute_batch(SCHEMA_SQL)
        .map_err(|error| format!("failed to initialize sqlite schema {}: {error}", path.display()))?;
    Ok(conn)
}

pub fn load_agents() -> Result<Vec<AgentInfo>, String> {
    load_agents_at(Path::new(runtime::database_path()))
}

pub fn load_agents_at(path: &Path) -> Result<Vec<AgentInfo>, String> {
    let conn = open_at(path)?;
    let mut stmt = conn
        .prepare("SELECT data_json FROM agent ORDER BY updated_at, agent_id")
        .map_err(|error| format!("failed to prepare agent query: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query agents: {error}"))?;
    let mut agents = Vec::new();
    for row in rows {
        let json = row.map_err(|error| format!("failed to decode agent row: {error}"))?;
        let agent = serde_json::from_str::<AgentInfo>(&json)
            .map_err(|error| format!("failed to parse agent json: {error}"))?;
        agents.push(agent);
    }
    Ok(agents)
}

pub fn replace_agents(agents: &[AgentInfo]) -> Result<(), String> {
    replace_agents_at(Path::new(runtime::database_path()), agents)
}

pub fn replace_agents_at(path: &Path, agents: &[AgentInfo]) -> Result<(), String> {
    let mut conn = open_at(path)?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("failed to begin agent transaction: {error}"))?;
    tx.execute("DELETE FROM agent", [])
        .map_err(|error| format!("failed to clear agent rows: {error}"))?;
    let updated_at = now_ms();
    for agent in agents {
        let data_json = serde_json::to_string(agent)
            .map_err(|error| format!("failed to serialize agent {}: {error}", agent.agent_id))?;
        tx.execute(
            "INSERT INTO agent (agent_id, session_id, tile_id, data_json, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![agent.agent_id, agent.session_id, agent.tile_id, data_json, updated_at],
        )
        .map_err(|error| format!("failed to insert agent {}: {error}", agent.agent_id))?;
    }
    tx.commit()
        .map_err(|error| format!("failed to commit agent transaction: {error}"))?;
    Ok(())
}

pub fn load_topics() -> Result<Vec<PersistedTopicRecord>, String> {
    load_topics_at(Path::new(runtime::database_path()))
}

pub fn load_topics_at(path: &Path) -> Result<Vec<PersistedTopicRecord>, String> {
    let conn = open_at(path)?;
    let mut stmt = conn
        .prepare("SELECT data_json FROM topic ORDER BY name")
        .map_err(|error| format!("failed to prepare topic query: {error}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("failed to query topics: {error}"))?;
    let mut topics = Vec::new();
    for row in rows {
        let json = row.map_err(|error| format!("failed to decode topic row: {error}"))?;
        let topic = serde_json::from_str::<PersistedTopicRecord>(&json)
            .map_err(|error| format!("failed to parse topic json: {error}"))?;
        topics.push(topic);
    }
    Ok(topics)
}

pub fn replace_topics(topics: &[PersistedTopicRecord]) -> Result<(), String> {
    replace_topics_at(Path::new(runtime::database_path()), topics)
}

pub fn replace_topics_at(path: &Path, topics: &[PersistedTopicRecord]) -> Result<(), String> {
    let mut conn = open_at(path)?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("failed to begin topic transaction: {error}"))?;
    tx.execute("DELETE FROM topic", [])
        .map_err(|error| format!("failed to clear topic rows: {error}"))?;
    let updated_at = now_ms();
    for topic in topics {
        let data_json = serde_json::to_string(topic)
            .map_err(|error| format!("failed to serialize topic {}: {error}", topic.name))?;
        let storage_key = format!("{}::{}", topic.session_id, topic.name);
        tx.execute(
            "INSERT INTO topic (name, data_json, updated_at) VALUES (?1, ?2, ?3)",
            params![storage_key, data_json, updated_at],
        )
        .map_err(|error| format!("failed to insert topic {}: {error}", topic.name))?;
    }
    tx.commit()
        .map_err(|error| format!("failed to commit topic transaction: {error}"))?;
    Ok(())
}

pub fn reset_runtime_presence_state() -> Result<(), String> {
    reset_runtime_presence_state_at(Path::new(runtime::database_path()))
}

pub fn reset_runtime_presence_state_at(path: &Path) -> Result<(), String> {
    let mut agents = load_agents_at(path)?;
    for agent in &mut agents {
        agent.alive = false;
    }
    replace_agents_at(path, &agents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        load_agents_at, load_topics_at, open_at, replace_agents_at, replace_topics_at,
        reset_runtime_presence_state_at, PersistedTopicRecord,
    };
    use crate::agent::{AgentInfo, AgentRole, AgentType};
    use rusqlite::params;
    use std::fs;
    use std::path::PathBuf;

    fn temp_db_path(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("herd-db-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root.join("herd.sqlite")
    }

    #[test]
    fn initializes_expected_schema_tables() {
        let path = temp_db_path("schema");
        let conn = open_at(&path).unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(names.contains(&"tile_state".to_string()));
        assert!(names.contains(&"chatter".to_string()));
        assert!(names.contains(&"agent_log".to_string()));
        assert!(names.contains(&"agent".to_string()));
        assert!(names.contains(&"topic".to_string()));
        assert!(names.contains(&"network_connection".to_string()));
        assert!(names.contains(&"work_item".to_string()));
        assert!(names.contains(&"work_stage".to_string()));
        assert!(names.contains(&"work_review".to_string()));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn agents_and_topics_round_trip_through_sqlite() {
        let path = temp_db_path("registry");
        let agents = vec![AgentInfo {
            agent_id: "agent-1".to_string(),
            agent_type: AgentType::Claude,
            agent_role: AgentRole::Worker,
            tile_id: "%1".to_string(),
            window_id: "@1".to_string(),
            session_id: "$1".to_string(),
            title: "Agent".to_string(),
            display_name: "Agent 1".to_string(),
            alive: true,
            chatter_subscribed: true,
            topics: vec!["#work-s1-001".to_string()],
            agent_pid: Some(42),
        }];
        let topics = vec![PersistedTopicRecord {
            session_id: "$1".to_string(),
            name: "#work-s1-001".to_string(),
            subscribers: vec!["agent-1".to_string()],
            last_activity_at: Some(123),
        }, PersistedTopicRecord {
            session_id: "$2".to_string(),
            name: "#work-s1-001".to_string(),
            subscribers: vec!["agent-2".to_string()],
            last_activity_at: Some(456),
        }];

        replace_agents_at(&path, &agents).unwrap();
        replace_topics_at(&path, &topics).unwrap();

        assert_eq!(load_agents_at(&path).unwrap(), agents);
        assert_eq!(load_topics_at(&path).unwrap(), topics);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_reset_clears_alive_agents() {
        let path = temp_db_path("reset");
        let agents = vec![AgentInfo {
            agent_id: "agent-1".to_string(),
            agent_type: AgentType::Claude,
            agent_role: AgentRole::Worker,
            tile_id: "%1".to_string(),
            window_id: "@1".to_string(),
            session_id: "$1".to_string(),
            title: "Agent".to_string(),
            display_name: "Agent 1".to_string(),
            alive: true,
            chatter_subscribed: true,
            topics: vec![],
            agent_pid: None,
        }];
        replace_agents_at(&path, &agents).unwrap();

        let conn = open_at(&path).unwrap();
        conn.execute(
            "INSERT INTO work_item (work_id, session_id, title, owner_agent_id, current_stage, data_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["work-s1-001", "$1", "Socket refactor", "agent-1", "plan", "{}", 1i64, 1i64],
        )
        .unwrap();

        reset_runtime_presence_state_at(&path).unwrap();

        let loaded_agents = load_agents_at(&path).unwrap();
        assert_eq!(loaded_agents.len(), 1);
        assert!(!loaded_agents[0].alive);

        let _ = fs::remove_file(path);
    }
}
