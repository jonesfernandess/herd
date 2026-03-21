use serde::{Deserialize, Serialize};

use crate::network::TileTypeFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDriverKey {
    pub key: String,
    #[serde(default)]
    pub shift_key: bool,
    #[serde(default)]
    pub ctrl_key: bool,
    #[serde(default)]
    pub alt_key: bool,
    #[serde(default)]
    pub meta_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestDriverRequest {
    Ping,
    WaitForReady {
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    WaitForBootstrap {
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    WaitForIdle {
        #[serde(default)]
        timeout_ms: Option<u64>,
        #[serde(default)]
        settle_ms: Option<u64>,
    },
    GetStateTree,
    GetProjection,
    GetStatus,
    PressKeys {
        keys: Vec<TestDriverKey>,
        #[serde(default)]
        viewport_width: Option<f64>,
        #[serde(default)]
        viewport_height: Option<f64>,
    },
    CommandBarOpen,
    CommandBarSetText {
        text: String,
    },
    CommandBarSubmit,
    CommandBarCancel,
    ToolbarSelectTab {
        session_id: String,
    },
    ToolbarAddTab {
        #[serde(default)]
        name: Option<String>,
    },
    ToolbarSpawnShell,
    ToolbarSpawnWork {
        title: String,
    },
    SidebarOpen,
    SidebarClose,
    SidebarSelectItem {
        index: usize,
    },
    SidebarMoveSelection {
        delta: i32,
    },
    SidebarBeginRename,
    TileSelect {
        pane_id: String,
    },
    TileClose {
        pane_id: String,
    },
    TileDrag {
        pane_id: String,
        dx: f64,
        dy: f64,
    },
    TileResize {
        pane_id: String,
        width: f64,
        height: f64,
    },
    TileTitleDoubleClick {
        pane_id: String,
        #[serde(default)]
        viewport_width: Option<f64>,
        #[serde(default)]
        viewport_height: Option<f64>,
    },
    CanvasPan {
        dx: f64,
        dy: f64,
    },
    CanvasContextMenu {
        client_x: f64,
        client_y: f64,
    },
    CanvasZoomAt {
        x: f64,
        y: f64,
        zoom_factor: f64,
    },
    CanvasWheel {
        delta_y: f64,
        client_x: f64,
        client_y: f64,
    },
    CanvasFitAll {
        #[serde(default)]
        viewport_width: Option<f64>,
        #[serde(default)]
        viewport_height: Option<f64>,
    },
    CanvasReset,
    TileContextMenu {
        pane_id: String,
        client_x: f64,
        client_y: f64,
    },
    ContextMenuSelect {
        item_id: String,
    },
    ContextMenuDismiss,
    ConfirmCloseTab,
    CancelCloseTab,
}

#[derive(Deserialize)]
#[serde(tag = "command")]
pub enum SocketCommand {
    #[serde(rename = "shell_create")]
    ShellCreate {
        #[serde(default = "default_coord")]
        x: f64,
        #[serde(default = "default_coord")]
        y: f64,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        parent_session_id: Option<String>,
        #[serde(default)]
        parent_pane_id: Option<String>,
    },
    #[serde(rename = "shell_destroy")]
    ShellDestroy {
        session_id: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_list")]
    ShellList {
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_input_send")]
    ShellInputSend {
        session_id: String,
        input: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_exec")]
    ShellExec {
        session_id: String,
        shell_command: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_output_read")]
    ShellOutputRead {
        session_id: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_title_set")]
    ShellTitleSet {
        session_id: String,
        title: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_read_only_set")]
    ShellReadOnlySet {
        session_id: String,
        read_only: bool,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "shell_role_set")]
    ShellRoleSet {
        session_id: String,
        role: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "browser_create")]
    BrowserCreate {
        #[serde(default)]
        parent_session_id: Option<String>,
        #[serde(default)]
        parent_pane_id: Option<String>,
    },
    #[serde(rename = "browser_destroy")]
    BrowserDestroy {
        pane_id: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "browser_navigate")]
    BrowserNavigate {
        pane_id: String,
        url: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "browser_load")]
    BrowserLoad {
        pane_id: String,
        path: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "agent_create")]
    AgentCreate {
        #[serde(default)]
        parent_session_id: Option<String>,
        #[serde(default)]
        parent_pane_id: Option<String>,
    },
    #[serde(rename = "agent_register")]
    AgentRegister {
        agent_id: String,
        #[serde(default)]
        agent_type: Option<String>,
        #[serde(default)]
        agent_role: Option<String>,
        pane_id: String,
        #[serde(default)]
        agent_pid: Option<u32>,
        #[serde(default)]
        title: Option<String>,
    },
    #[serde(rename = "agent_unregister")]
    AgentUnregister { agent_id: String },
    #[serde(rename = "agent_events_subscribe")]
    AgentEventsSubscribe { agent_id: String },
    #[serde(rename = "agent_ping_ack")]
    AgentPingAck { agent_id: String },
    #[serde(rename = "agent_log_append")]
    AgentLogAppend {
        agent_id: String,
        kind: String,
        text: String,
        #[serde(default)]
        timestamp_ms: Option<i64>,
    },
    #[serde(rename = "agent_list")]
    ListAgents {
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "topics_list")]
    ListTopics {
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "network_list")]
    ListNetwork {
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
        #[serde(default)]
        tile_type: Option<TileTypeFilter>,
    },
    #[serde(rename = "session_list")]
    SessionList {
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
        #[serde(default)]
        tile_type: Option<TileTypeFilter>,
    },
    #[serde(rename = "tile_list")]
    TileList {
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
        #[serde(default)]
        tile_type: Option<TileTypeFilter>,
    },
    #[serde(rename = "tile_get")]
    TileGet {
        tile_id: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "tile_move")]
    TileMove {
        tile_id: String,
        x: f64,
        y: f64,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "tile_resize")]
    TileResize {
        tile_id: String,
        width: f64,
        height: f64,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "network_connect")]
    NetworkConnect {
        from_tile_id: String,
        from_port: String,
        to_tile_id: String,
        to_port: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "network_disconnect")]
    NetworkDisconnect {
        tile_id: String,
        port: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "message_direct")]
    MessageDirect {
        to_agent_id: String,
        message: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "message_public")]
    MessagePublic {
        message: String,
        #[serde(default)]
        topics: Vec<String>,
        #[serde(default)]
        mentions: Vec<String>,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "message_network")]
    MessageNetwork {
        message: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "message_root")]
    MessageRoot {
        message: String,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "topic_subscribe")]
    TopicSubscribe {
        topic: String,
        #[serde(default)]
        agent_id: Option<String>,
    },
    #[serde(rename = "topic_unsubscribe")]
    TopicUnsubscribe {
        topic: String,
        #[serde(default)]
        agent_id: Option<String>,
    },
    #[serde(rename = "work_list")]
    WorkList {
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "work_get")]
    WorkGet {
        work_id: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "work_create")]
    WorkCreate {
        title: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        sender_agent_id: Option<String>,
        #[serde(default)]
        sender_pane_id: Option<String>,
    },
    #[serde(rename = "work_stage_start")]
    WorkStageStart { work_id: String, agent_id: String },
    #[serde(rename = "work_stage_complete")]
    WorkStageComplete { work_id: String, agent_id: String },
    #[serde(rename = "work_review_approve")]
    WorkReviewApprove { work_id: String },
    #[serde(rename = "work_review_improve")]
    WorkReviewImprove { work_id: String, comment: String },
    #[serde(rename = "test_driver")]
    TestDriver { request: TestDriverRequest },
    #[serde(rename = "test_dom_query")]
    TestDomQuery { js: String },
    #[serde(rename = "test_dom_keys")]
    TestDomKeys { keys: String },
}

fn default_coord() -> f64 {
    100.0
}

#[derive(Serialize)]
pub struct SocketResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SocketResponse {
    pub fn success(data: Option<serde_json::Value>) -> Self {
        Self {
            ok: true,
            data,
            error: None,
        }
    }

    pub fn error(msg: String) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg),
        }
    }
}
