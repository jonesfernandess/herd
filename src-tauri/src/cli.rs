use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug)]
struct CliContext {
    socket_path: String,
    agent_pid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SocketResponse {
    ok: bool,
    data: Option<Value>,
    error: Option<String>,
}

pub fn is_cli_invocation(args: &[String]) -> bool {
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--socket" | "--agent-pid" => index += 2,
            "-h" | "--help" | "help" | "-V" | "--version" | "version" => return true,
            value => {
                return matches!(
                    value,
                    "sudo"
                        | "network"
                        | "session"
                        | "tile"
                        | "browser"
                        | "message"
                        | "topic"
                        | "agent"
                        | "shell"
                        | "work"
                        | "raw"
                );
            }
        }
    }
    false
}

pub fn run(args: Vec<String>) -> Result<(), String> {
    let (ctx, index) = parse_global_flags(&args)?;
    if let Some(command) = args.get(index).map(String::as_str) {
        match command {
            "-h" | "--help" | "help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" | "version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }
    let payload = build_command_payload(&ctx, &args[index..])?;
    let output = send_command(&ctx.socket_path, &payload)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn parse_global_flags(args: &[String]) -> Result<(CliContext, usize), String> {
    let mut socket_path = env::var("HERD_SOCK").unwrap_or_else(|_| crate::runtime::socket_path().to_string());
    let mut agent_pid = None;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--socket" => {
                index += 1;
                let value = args.get(index).ok_or("--socket requires a value")?;
                socket_path = value.clone();
                index += 1;
            }
            "--agent-pid" => {
                index += 1;
                let value = args.get(index).ok_or("--agent-pid requires a value")?;
                agent_pid = Some(value.clone());
                index += 1;
            }
            _ => break,
        }
    }
    Ok((CliContext { socket_path, agent_pid }, index))
}

fn print_help() {
    println!(
        "\
Usage:
  herd [--socket <path>] [--agent-pid <pid>] sudo <message>
  herd [--socket <path>] [--agent-pid <pid>] agent create [--parent-session-id <id>] [--parent-pane-id <id>]
  herd [--socket <path>] [--agent-pid <pid>] agent list
  herd [--socket <path>] [--agent-pid <pid>] agent ack-ping [<agent_id>]
  herd [--socket <path>] [--agent-pid <pid>] network list [shell|agent|browser|work]
  herd [--socket <path>] [--agent-pid <pid>] network connect <from_tile> <from_port> <to_tile> <to_port>
  herd [--socket <path>] [--agent-pid <pid>] network disconnect <tile> <port>
  herd [--socket <path>] [--agent-pid <pid>] session list [shell|agent|browser|work]
  herd [--socket <path>] [--agent-pid <pid>] tile list [shell|agent|browser|work]
  herd [--socket <path>] [--agent-pid <pid>] tile get <tile_id>
  herd [--socket <path>] [--agent-pid <pid>] tile move <tile_id> <x> <y>
  herd [--socket <path>] [--agent-pid <pid>] tile resize <tile_id> <width> <height>
  herd [--socket <path>] [--agent-pid <pid>] topic list
  herd [--socket <path>] [--agent-pid <pid>] topic subscribe <topic>
  herd [--socket <path>] [--agent-pid <pid>] topic unsubscribe <topic>
  herd [--socket <path>] [--agent-pid <pid>] message direct <agent_id> <message>
  herd [--socket <path>] [--agent-pid <pid>] message public <message> [--topic <topic>...] [--mention <agent_id>...]
  herd [--socket <path>] [--agent-pid <pid>] message network <message>
  herd [--socket <path>] [--agent-pid <pid>] message root <message>
  herd [--socket <path>] [--agent-pid <pid>] message topic <topic> <message>
  herd [--socket <path>] [--agent-pid <pid>] shell list
  herd [--socket <path>] [--agent-pid <pid>] shell create [--x <n>] [--y <n>] [--width <n>] [--height <n>] [--parent-session-id <id>] [--parent-pane-id <id>]
  herd [--socket <path>] [--agent-pid <pid>] shell destroy <pane_id>
  herd [--socket <path>] [--agent-pid <pid>] shell send <pane_id> <input>
  herd [--socket <path>] [--agent-pid <pid>] shell exec <pane_id> <command>
  herd [--socket <path>] [--agent-pid <pid>] shell read <pane_id>
  herd [--socket <path>] [--agent-pid <pid>] shell title <pane_id> <title>
  herd [--socket <path>] [--agent-pid <pid>] shell read-only <pane_id> <true|false>
  herd [--socket <path>] [--agent-pid <pid>] shell role <pane_id> <regular|claude|output>
  herd [--socket <path>] [--agent-pid <pid>] browser create [--parent-session-id <id>] [--parent-pane-id <id>]
  herd [--socket <path>] [--agent-pid <pid>] browser destroy <pane_id>
  herd [--socket <path>] [--agent-pid <pid>] browser navigate <pane_id> <url>
  herd [--socket <path>] [--agent-pid <pid>] browser load <pane_id> <path>
  herd [--socket <path>] [--agent-pid <pid>] work list
  herd [--socket <path>] [--agent-pid <pid>] work show <work_id>
  herd [--socket <path>] [--agent-pid <pid>] work create <title>
  herd [--socket <path>] [--agent-pid <pid>] work stage start <work_id>
  herd [--socket <path>] [--agent-pid <pid>] work stage complete <work_id>
  herd [--socket <path>] [--agent-pid <pid>] raw <json>
  herd --help
  herd --version"
    );
}

fn send_command(socket_path: &str, payload: &Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("failed to connect to Herd socket at {socket_path}: {error}"))?;
    stream
        .write_all(format!("{}\n", payload).as_bytes())
        .map_err(|error| format!("failed to write socket payload: {error}"))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("failed to read socket response: {error}"))?;
    let response: SocketResponse =
        serde_json::from_str(&line).map_err(|error| format!("invalid socket response: {error}"))?;
    if response.ok {
        Ok(response.data.unwrap_or(Value::Null))
    } else {
        Err(response.error.unwrap_or_else(|| "socket request failed".to_string()))
    }
}

fn env_agent_id() -> Option<String> {
    env::var("HERD_AGENT_ID").ok().filter(|value| !value.trim().is_empty())
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn env_pane_id() -> Option<String> {
    if let Some(pane_id) = non_empty_env("HERD_PANE_ID") {
        return Some(pane_id);
    }

    if non_empty_env("HERD_SOCK").is_some() || non_empty_env("HERD_SESSION_ID").is_some() {
        return non_empty_env("TMUX_PANE");
    }

    None
}

fn require_env_agent_id() -> Result<String, String> {
    env_agent_id().ok_or("HERD_AGENT_ID is required for this command".to_string())
}

fn shell_create_payload(args: &[String]) -> Result<Value, String> {
    let mut x = None;
    let mut y = None;
    let mut width = None;
    let mut height = None;
    let mut parent_session_id = None;
    let mut parent_pane_id = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = args.get(index).ok_or_else(|| format!("{flag} requires a value"))?.clone();
        index += 1;
        match flag {
            "--x" => x = value.parse::<f64>().ok(),
            "--y" => y = value.parse::<f64>().ok(),
            "--width" => width = value.parse::<f64>().ok(),
            "--height" => height = value.parse::<f64>().ok(),
            "--parent-session-id" => parent_session_id = Some(value),
            "--parent-pane-id" => parent_pane_id = Some(value),
            _ => return Err(format!("unknown shell create flag: {flag}")),
        }
    }
    Ok(json!({
        "command": "shell_create",
        "x": x,
        "y": y,
        "width": width,
        "height": height,
        "parent_session_id": parent_session_id,
        "parent_pane_id": parent_pane_id,
    }))
}

fn browser_create_payload(args: &[String]) -> Result<Value, String> {
    let mut parent_session_id = None;
    let mut parent_pane_id = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = args.get(index).ok_or_else(|| format!("{flag} requires a value"))?.clone();
        index += 1;
        match flag {
            "--parent-session-id" => parent_session_id = Some(value),
            "--parent-pane-id" => parent_pane_id = Some(value),
            _ => return Err(format!("unknown browser create flag: {flag}")),
        }
    }
    Ok(json!({
        "command": "browser_create",
        "parent_session_id": parent_session_id,
        "parent_pane_id": parent_pane_id,
    }))
}

fn agent_create_payload(args: &[String]) -> Result<Value, String> {
    let mut parent_session_id = None;
    let mut parent_pane_id = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = args.get(index).ok_or_else(|| format!("{flag} requires a value"))?.clone();
        index += 1;
        match flag {
            "--parent-session-id" => parent_session_id = Some(value),
            "--parent-pane-id" => parent_pane_id = Some(value),
            _ => return Err(format!("unknown agent create flag: {flag}")),
        }
    }
    Ok(json!({
        "command": "agent_create",
        "parent_session_id": parent_session_id,
        "parent_pane_id": parent_pane_id,
    }))
}

fn parse_optional_tile_type(args: &[String], command_name: &str) -> Result<Option<String>, String> {
    let Some(tile_type) = args.first() else {
        return Ok(None);
    };
    if args.len() > 1 {
        return Err(format!("{command_name} accepts at most one optional tile type"));
    }
    match tile_type.as_str() {
        "shell" | "agent" | "browser" | "work" => Ok(Some(tile_type.clone())),
        other => Err(format!("unsupported tile type for {command_name}: {other}")),
    }
}

fn tile_list_payload(command: &str, tile_type: Option<String>) -> Value {
    let mut payload = json!({
        "command": command,
        "sender_agent_id": env_agent_id(),
        "sender_pane_id": env_pane_id(),
    });
    if let Some(tile_type) = tile_type {
        payload["tile_type"] = json!(tile_type);
    }
    payload
}

fn parse_number_arg(value: Option<&String>, error: &str) -> Result<f64, String> {
    value
        .ok_or_else(|| error.to_string())?
        .parse::<f64>()
        .map_err(|_| error.to_string())
}

fn build_command_payload(ctx: &CliContext, args: &[String]) -> Result<Value, String> {
    let Some(group) = args.first().map(String::as_str) else {
        return Err("missing command group".to_string());
    };

    match group {
        "sudo" => Ok(json!({
            "command": "message_root",
            "message": args.get(1..).ok_or("sudo requires a message")?.join(" "),
            "sender_agent_id": env_agent_id(),
            "sender_pane_id": env_pane_id(),
            "sender_agent_pid": ctx.agent_pid,
        })),
        "network" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing network target")?;
            match sub {
                "list" => Ok(tile_list_payload(
                    "network_list",
                    parse_optional_tile_type(&args[2..], "network list")?,
                )),
                "connect" => Ok(json!({
                    "command": "network_connect",
                    "from_tile_id": args.get(2).ok_or("network connect requires <from_tile> <from_port> <to_tile> <to_port>")?,
                    "from_port": args.get(3).ok_or("network connect requires <from_tile> <from_port> <to_tile> <to_port>")?,
                    "to_tile_id": args.get(4).ok_or("network connect requires <from_tile> <from_port> <to_tile> <to_port>")?,
                    "to_port": args.get(5).ok_or("network connect requires <from_tile> <from_port> <to_tile> <to_port>")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "disconnect" => Ok(json!({
                    "command": "network_disconnect",
                    "tile_id": args.get(2).ok_or("network disconnect requires <tile> <port>")?,
                    "port": args.get(3).ok_or("network disconnect requires <tile> <port>")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                _ => Err(format!("unknown network target: {sub}")),
            }
        }
        "session" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing session target")?;
            match sub {
                "list" => Ok(tile_list_payload(
                    "session_list",
                    parse_optional_tile_type(&args[2..], "session list")?,
                )),
                _ => Err(format!("unknown session target: {sub}")),
            }
        }
        "tile" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing tile target")?;
            match sub {
                "list" => Ok(tile_list_payload(
                    "tile_list",
                    parse_optional_tile_type(&args[2..], "tile list")?,
                )),
                "get" => Ok(json!({
                    "command": "tile_get",
                    "tile_id": args.get(2).ok_or("tile get requires a tile_id")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "move" => Ok(json!({
                    "command": "tile_move",
                    "tile_id": args.get(2).ok_or("tile move requires <tile_id> <x> <y>")?,
                    "x": parse_number_arg(args.get(3), "tile move requires <tile_id> <x> <y>")?,
                    "y": parse_number_arg(args.get(4), "tile move requires <tile_id> <x> <y>")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "resize" => Ok(json!({
                    "command": "tile_resize",
                    "tile_id": args.get(2).ok_or("tile resize requires <tile_id> <width> <height>")?,
                    "width": parse_number_arg(args.get(3), "tile resize requires <tile_id> <width> <height>")?,
                    "height": parse_number_arg(args.get(4), "tile resize requires <tile_id> <width> <height>")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                _ => Err(format!("unknown tile target: {sub}")),
            }
        }
        "topic" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing topic target")?;
            match sub {
                "list" => Ok(json!({
                    "command": "topics_list",
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "subscribe" => {
                    let topic = args.get(2).ok_or("topic subscribe requires a topic")?;
                    Ok(json!({
                        "command": "topic_subscribe",
                        "agent_id": require_env_agent_id()?,
                        "topic": topic,
                    }))
                }
                "unsubscribe" => {
                    let topic = args.get(2).ok_or("topic unsubscribe requires a topic")?;
                    Ok(json!({
                        "command": "topic_unsubscribe",
                        "agent_id": require_env_agent_id()?,
                        "topic": topic,
                    }))
                }
                _ => Err(format!("unknown topic target: {sub}")),
            }
        }
        "browser" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing browser target")?;
            match sub {
                "create" => browser_create_payload(&args[2..]),
                "destroy" => Ok(json!({
                    "command": "browser_destroy",
                    "pane_id": args.get(2).ok_or("browser destroy requires a pane_id")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "navigate" => Ok(json!({
                    "command": "browser_navigate",
                    "pane_id": args.get(2).ok_or("browser navigate requires <pane_id> <url>")?,
                    "url": args.get(3).ok_or("browser navigate requires a url")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "load" => Ok(json!({
                    "command": "browser_load",
                    "pane_id": args.get(2).ok_or("browser load requires <pane_id> <path>")?,
                    "path": args.get(3).ok_or("browser load requires a path")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                _ => Err(format!("unknown browser target: {sub}")),
            }
        }
        "message" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing message target")?;
            match sub {
                "direct" => {
                    let to_agent_id = args.get(2).ok_or("message direct requires <agent_id> <message>")?;
                    let message = args.get(3..).ok_or("message direct requires a message")?.join(" ");
                    Ok(json!({
                        "command": "message_direct",
                        "to_agent_id": to_agent_id,
                        "message": message,
                        "sender_agent_id": env_agent_id(),
                        "sender_pane_id": env_pane_id(),
                        "sender_agent_pid": ctx.agent_pid,
                    }))
                }
                "public" | "chatter" => {
                    let mut topics = Vec::new();
                    let mut mentions = Vec::new();
                    let mut message_parts = Vec::new();
                    let mut index = 2usize;
                    while index < args.len() {
                        match args[index].as_str() {
                            "--topic" => {
                                index += 1;
                                topics.push(args.get(index).ok_or("--topic requires a value")?.clone());
                            }
                            "--mention" => {
                                index += 1;
                                mentions.push(args.get(index).ok_or("--mention requires a value")?.clone());
                            }
                            value => message_parts.push(value.to_string()),
                        }
                        index += 1;
                    }
                    if message_parts.is_empty() {
                        return Err(format!("message {sub} requires a message"));
                    }
                    Ok(json!({
                        "command": "message_public",
                        "message": message_parts.join(" "),
                        "topics": topics,
                        "mentions": mentions,
                        "sender_agent_id": env_agent_id(),
                        "sender_pane_id": env_pane_id(),
                        "sender_agent_pid": ctx.agent_pid,
                    }))
                }
                "network" => Ok(json!({
                    "command": "message_network",
                    "message": args.get(2..).ok_or("message network requires a message")?.join(" "),
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                    "sender_agent_pid": ctx.agent_pid,
                })),
                "root" => Ok(json!({
                    "command": "message_root",
                    "message": args.get(2..).ok_or("message root requires a message")?.join(" "),
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                    "sender_agent_pid": ctx.agent_pid,
                })),
                "topic" => {
                    let topic = args.get(2).ok_or("message topic requires <topic> <message>")?;
                    let message = args.get(3..).ok_or("message topic requires a message")?.join(" ");
                    Ok(json!({
                        "command": "message_public",
                        "message": message,
                        "topics": [topic],
                        "mentions": [],
                        "sender_agent_id": env_agent_id(),
                        "sender_pane_id": env_pane_id(),
                        "sender_agent_pid": ctx.agent_pid,
                    }))
                }
                _ => Err(format!("unknown message target: {sub}")),
            }
        }
        "agent" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing agent target")?;
            match sub {
                "create" => agent_create_payload(&args[2..]),
                "list" => Ok(json!({
                    "command": "agent_list",
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "ack-ping" => {
                    let agent_id = args.get(2).cloned().or_else(env_agent_id).ok_or("agent ack-ping requires an agent id or HERD_AGENT_ID")?;
                    Ok(json!({
                        "command": "agent_ping_ack",
                        "agent_id": agent_id,
                    }))
                }
                _ => Err(format!("unknown agent target: {sub}")),
            }
        }
        "shell" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing shell target")?;
            match sub {
                "list" => Ok(json!({
                    "command": "shell_list",
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "create" => shell_create_payload(&args[2..]),
                "destroy" => Ok(json!({
                    "command": "shell_destroy",
                    "session_id": args.get(2).ok_or("shell destroy requires a pane_id")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "send" => Ok(json!({
                    "command": "shell_input_send",
                    "session_id": args.get(2).ok_or("shell send requires <pane_id> <input>")?,
                    "input": args.get(3..).ok_or("shell send requires input")?.join(" "),
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "exec" => Ok(json!({
                    "command": "shell_exec",
                    "session_id": args.get(2).ok_or("shell exec requires <pane_id> <command>")?,
                    "shell_command": args.get(3..).ok_or("shell exec requires a command")?.join(" "),
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "read" => Ok(json!({
                    "command": "shell_output_read",
                    "session_id": args.get(2).ok_or("shell read requires a pane_id")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "title" => Ok(json!({
                    "command": "shell_title_set",
                    "session_id": args.get(2).ok_or("shell title requires <pane_id> <title>")?,
                    "title": args.get(3..).ok_or("shell title requires a title")?.join(" "),
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "read-only" => Ok(json!({
                    "command": "shell_read_only_set",
                    "session_id": args.get(2).ok_or("shell read-only requires <pane_id> <true|false>")?,
                    "read_only": args.get(3).ok_or("shell read-only requires a boolean")?.parse::<bool>().map_err(|_| "invalid boolean for shell read-only".to_string())?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                "role" => Ok(json!({
                    "command": "shell_role_set",
                    "session_id": args.get(2).ok_or("shell role requires <pane_id> <role>")?,
                    "role": args.get(3).ok_or("shell role requires a role")?,
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                })),
                _ => Err(format!("unknown shell target: {sub}")),
            }
        }
        "work" => {
            let sub = args.get(1).map(String::as_str).ok_or("missing work target")?;
            match sub {
                "list" => {
                    if args.get(2).map(String::as_str) == Some("--all") {
                        return Err("work list is session-local; --all is no longer supported".to_string());
                    }
                    Ok(json!({
                        "command": "work_list",
                        "scope": "current_session",
                        "agent_id": env_agent_id(),
                        "session_id": Value::Null,
                        "sender_pane_id": env_pane_id(),
                    }))
                }
                "show" => Ok(json!({
                    "command": "work_get",
                    "work_id": args.get(2).ok_or("work show requires a work_id")?,
                    "agent_id": env_agent_id(),
                    "session_id": Value::Null,
                    "sender_pane_id": env_pane_id(),
                })),
                "create" => Ok(json!({
                    "command": "work_create",
                    "title": args.get(2..).ok_or("work create requires a title")?.join(" "),
                    "sender_agent_id": env_agent_id(),
                    "sender_pane_id": env_pane_id(),
                    "session_id": Value::Null,
                })),
                "stage" => {
                    let action = args.get(2).map(String::as_str).ok_or("missing work stage action")?;
                    let work_id = args.get(3).ok_or("work stage requires a work_id")?;
                    match action {
                        "start" => Ok(json!({
                            "command": "work_stage_start",
                            "work_id": work_id,
                            "agent_id": require_env_agent_id()?,
                        })),
                        "complete" => Ok(json!({
                            "command": "work_stage_complete",
                            "work_id": work_id,
                            "agent_id": require_env_agent_id()?,
                        })),
                        _ => Err(format!("unknown work stage action: {action}")),
                    }
                }
                _ => Err(format!("unknown work target: {sub}")),
            }
        }
        "raw" => {
            let raw = args.get(1..).ok_or("raw requires a JSON payload")?.join(" ");
            serde_json::from_str::<Value>(&raw).map_err(|error| format!("invalid raw JSON: {error}"))
        }
        _ => Err(format!("unknown command group: {group}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_command_payload, CliContext};
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};

    fn ctx() -> CliContext {
        CliContext {
            socket_path: "/tmp/herd-test.sock".to_string(),
            agent_pid: Some("4242".to_string()),
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_agent_env<R>(agent_id: &str, f: impl FnOnce() -> R) -> R {
        let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
        let previous = std::env::var("HERD_AGENT_ID").ok();
        std::env::set_var("HERD_AGENT_ID", agent_id);
        let result = f();
        match previous {
            Some(value) => std::env::set_var("HERD_AGENT_ID", value),
            None => std::env::remove_var("HERD_AGENT_ID"),
        }
        result
    }

    fn with_agent_and_pane_env<R>(agent_id: &str, pane_id: &str, f: impl FnOnce() -> R) -> R {
        let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
        let previous_agent = std::env::var("HERD_AGENT_ID").ok();
        let previous_pane = std::env::var("HERD_PANE_ID").ok();
        let previous_tmux_pane = std::env::var("TMUX_PANE").ok();
        std::env::set_var("HERD_AGENT_ID", agent_id);
        std::env::set_var("HERD_PANE_ID", pane_id);
        std::env::set_var("TMUX_PANE", pane_id);
        let result = f();
        match previous_agent {
            Some(value) => std::env::set_var("HERD_AGENT_ID", value),
            None => std::env::remove_var("HERD_AGENT_ID"),
        }
        match previous_pane {
            Some(value) => std::env::set_var("HERD_PANE_ID", value),
            None => std::env::remove_var("HERD_PANE_ID"),
        }
        match previous_tmux_pane {
            Some(value) => std::env::set_var("TMUX_PANE", value),
            None => std::env::remove_var("TMUX_PANE"),
        }
        result
    }

    fn with_cli_env<R>(
        herd_pane_id: Option<&str>,
        tmux_pane: Option<&str>,
        herd_sock: Option<&str>,
        herd_session_id: Option<&str>,
        f: impl FnOnce() -> R,
    ) -> R {
        let _guard = env_lock().lock().unwrap_or_else(|error| error.into_inner());
        let previous_herd_pane = std::env::var("HERD_PANE_ID").ok();
        let previous_tmux_pane = std::env::var("TMUX_PANE").ok();
        let previous_herd_sock = std::env::var("HERD_SOCK").ok();
        let previous_herd_session = std::env::var("HERD_SESSION_ID").ok();

        match herd_pane_id {
            Some(value) => std::env::set_var("HERD_PANE_ID", value),
            None => std::env::remove_var("HERD_PANE_ID"),
        }
        match tmux_pane {
            Some(value) => std::env::set_var("TMUX_PANE", value),
            None => std::env::remove_var("TMUX_PANE"),
        }
        match herd_sock {
            Some(value) => std::env::set_var("HERD_SOCK", value),
            None => std::env::remove_var("HERD_SOCK"),
        }
        match herd_session_id {
            Some(value) => std::env::set_var("HERD_SESSION_ID", value),
            None => std::env::remove_var("HERD_SESSION_ID"),
        }

        let result = f();

        match previous_herd_pane {
            Some(value) => std::env::set_var("HERD_PANE_ID", value),
            None => std::env::remove_var("HERD_PANE_ID"),
        }
        match previous_tmux_pane {
            Some(value) => std::env::set_var("TMUX_PANE", value),
            None => std::env::remove_var("TMUX_PANE"),
        }
        match previous_herd_sock {
            Some(value) => std::env::set_var("HERD_SOCK", value),
            None => std::env::remove_var("HERD_SOCK"),
        }
        match previous_herd_session {
            Some(value) => std::env::set_var("HERD_SESSION_ID", value),
            None => std::env::remove_var("HERD_SESSION_ID"),
        }

        result
    }

    #[test]
    fn serializes_work_list_payload() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(&ctx(), &["work".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "work_list",
                    "scope": "current_session",
                    "agent_id": "agent-7",
                    "session_id": null,
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn rejects_global_work_list_flag() {
        let error = build_command_payload(&ctx(), &["work".into(), "list".into(), "--all".into()]).unwrap_err();
        assert_eq!(error, "work list is session-local; --all is no longer supported");
    }

    #[test]
    fn serializes_list_agents_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(&ctx(), &["agent".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "agent_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_agent_create_payload() {
        let payload = build_command_payload(
            &ctx(),
            &[
                "agent".into(),
                "create".into(),
                "--parent-session-id".into(),
                "$7".into(),
                "--parent-pane-id".into(),
                "%7".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            payload,
            json!({
                "command": "agent_create",
                "parent_session_id": "$7",
                "parent_pane_id": "%7",
            })
        );
    }

    #[test]
    fn serializes_list_network_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(&ctx(), &["network".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "network_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_filtered_list_payloads_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let network_payload = build_command_payload(
                &ctx(),
                &["network".into(), "list".into(), "agent".into()],
            )
            .unwrap();
            assert_eq!(
                network_payload,
                json!({
                    "command": "network_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "tile_type": "agent",
                })
            );

            let session_payload = build_command_payload(
                &ctx(),
                &["session".into(), "list".into(), "work".into()],
            )
            .unwrap();
            assert_eq!(
                session_payload,
                json!({
                    "command": "session_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "tile_type": "work",
                })
            );

            let tile_payload = build_command_payload(
                &ctx(),
                &["tile".into(), "list".into(), "shell".into()],
            )
            .unwrap();
            assert_eq!(
                tile_payload,
                json!({
                    "command": "tile_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "tile_type": "shell",
                })
            );
        });
    }

    #[test]
    fn rejects_invalid_optional_tile_type() {
        let error = build_command_payload(
            &ctx(),
            &["network".into(), "list".into(), "invalid".into()],
        )
        .unwrap_err();
        assert!(error.contains("unsupported tile type"));
    }

    #[test]
    fn serializes_tile_get_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &["tile".into(), "get".into(), "%9".into()],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "tile_get",
                    "tile_id": "%9",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_tile_move_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &["tile".into(), "move".into(), "%9".into(), "420".into(), "160".into()],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "tile_move",
                    "tile_id": "%9",
                    "x": 420.0,
                    "y": 160.0,
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_tile_resize_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &["tile".into(), "resize".into(), "%9".into(), "720".into(), "480".into()],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "tile_resize",
                    "tile_id": "%9",
                    "width": 720.0,
                    "height": 480.0,
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_topic_list_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(&ctx(), &["topic".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "topics_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_shell_create_payload() {
        let payload = build_command_payload(
            &ctx(),
            &[
                "shell".into(),
                "create".into(),
                "--x".into(),
                "180".into(),
                "--y".into(),
                "140".into(),
                "--width".into(),
                "640".into(),
                "--height".into(),
                "400".into(),
                "--parent-pane-id".into(),
                "%1".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            payload,
            json!({
                "command": "shell_create",
                "x": 180.0,
                "y": 140.0,
                "width": 640.0,
                "height": 400.0,
                "parent_session_id": null,
                "parent_pane_id": "%1",
            })
        );
    }

    #[test]
    fn serializes_shell_list_payload_with_sender_context() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(&ctx(), &["shell".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "shell_list",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_browser_command_payloads() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let create = build_command_payload(
                &ctx(),
                &[
                    "browser".into(),
                    "create".into(),
                    "--parent-session-id".into(),
                    "$1".into(),
                ],
            )
            .unwrap();
            assert_eq!(
                create,
                json!({
                    "command": "browser_create",
                    "parent_session_id": "$1",
                    "parent_pane_id": null,
                })
            );

            let destroy = build_command_payload(
                &ctx(),
                &["browser".into(), "destroy".into(), "%9".into()],
            )
            .unwrap();
            assert_eq!(
                destroy,
                json!({
                    "command": "browser_destroy",
                    "pane_id": "%9",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );

            let navigate = build_command_payload(
                &ctx(),
                &["browser".into(), "navigate".into(), "%9".into(), "https://example.com".into()],
            )
            .unwrap();
            assert_eq!(
                navigate,
                json!({
                    "command": "browser_navigate",
                    "pane_id": "%9",
                    "url": "https://example.com",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );

            let load = build_command_payload(
                &ctx(),
                &["browser".into(), "load".into(), "%9".into(), "./fixtures/index.html".into()],
            )
            .unwrap();
            assert_eq!(
                load,
                json!({
                    "command": "browser_load",
                    "pane_id": "%9",
                    "path": "./fixtures/index.html",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_message_public_payload() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &[
                    "message".into(),
                    "public".into(),
                    "sync".into(),
                    "on".into(),
                    "#prd-7".into(),
                    "--topic".into(),
                    "#alpha".into(),
                    "--mention".into(),
                    "agent-2".into(),
                ],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "message_public",
                    "message": "sync on #prd-7",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "sender_agent_pid": "4242",
                    "topics": ["#alpha"],
                    "mentions": ["agent-2"],
                })
            );
        });
    }

    #[test]
    fn serializes_message_network_and_root_payloads() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let network = build_command_payload(
                &ctx(),
                &["message".into(), "network".into(), "hello".into(), "team".into()],
            )
            .unwrap();
            assert_eq!(
                network,
                json!({
                    "command": "message_network",
                    "message": "hello team",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "sender_agent_pid": "4242",
                })
            );

            let root = build_command_payload(
                &ctx(),
                &["message".into(), "root".into(), "need".into(), "help".into()],
            )
            .unwrap();
            assert_eq!(
                root,
                json!({
                    "command": "message_root",
                    "message": "need help",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "sender_agent_pid": "4242",
                })
            );
        });
    }

    #[test]
    fn serializes_sudo_payload_as_message_root() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &["sudo".into(), "please".into(), "take".into(), "over".into()],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "message_root",
                    "message": "please take over",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                    "sender_agent_pid": "4242",
                })
            );
        });
    }

    #[test]
    fn serializes_network_connect_and_disconnect_payloads() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let connect = build_command_payload(
                &ctx(),
                &[
                    "network".into(),
                    "connect".into(),
                    "%7".into(),
                    "left".into(),
                    "work:work-s4-001".into(),
                    "left".into(),
                ],
            )
            .unwrap();
            assert_eq!(
                connect,
                json!({
                    "command": "network_connect",
                    "from_tile_id": "%7",
                    "from_port": "left",
                    "to_tile_id": "work:work-s4-001",
                    "to_port": "left",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );

            let disconnect = build_command_payload(
                &ctx(),
                &["network".into(), "disconnect".into(), "%7".into(), "left".into()],
            )
            .unwrap();
            assert_eq!(
                disconnect,
                json!({
                    "command": "network_disconnect",
                    "tile_id": "%7",
                    "port": "left",
                    "sender_agent_id": "agent-7",
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_work_create_payload() {
        with_agent_and_pane_env("agent-1", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &["work".into(), "create".into(), "Socket".into(), "refactor".into()],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "work_create",
                    "title": "Socket refactor",
                    "sender_agent_id": "agent-1",
                    "sender_pane_id": "%7",
                    "session_id": null,
                })
            );
        });
    }

    #[test]
    fn serializes_work_show_payload() {
        with_agent_and_pane_env("agent-7", "%7", || {
            let payload = build_command_payload(
                &ctx(),
                &["work".into(), "show".into(), "work-s4-001".into()],
            )
            .unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "work_get",
                    "work_id": "work-s4-001",
                    "agent_id": "agent-7",
                    "session_id": null,
                    "sender_pane_id": "%7",
                })
            );
        });
    }

    #[test]
    fn serializes_work_stage_start_and_complete_payloads() {
        with_agent_env("owner-1", || {
            let start = build_command_payload(
                &ctx(),
                &["work".into(), "stage".into(), "start".into(), "work-s4-001".into()],
            )
            .unwrap();
            assert_eq!(
                start,
                json!({
                    "command": "work_stage_start",
                    "work_id": "work-s4-001",
                    "agent_id": "owner-1",
                })
            );

            let complete = build_command_payload(
                &ctx(),
                &["work".into(), "stage".into(), "complete".into(), "work-s4-001".into()],
            )
            .unwrap();
            assert_eq!(
                complete,
                json!({
                    "command": "work_stage_complete",
                    "work_id": "work-s4-001",
                    "agent_id": "owner-1",
                })
            );
        });
    }

    #[test]
    fn serializes_topic_subscribe_and_unsubscribe_payloads() {
        with_agent_env("owner-1", || {
            let subscribe = build_command_payload(
                &ctx(),
                &["topic".into(), "subscribe".into(), "#prd-7".into()],
            )
            .unwrap();
            assert_eq!(
                subscribe,
                json!({
                    "command": "topic_subscribe",
                    "agent_id": "owner-1",
                    "topic": "#prd-7",
                })
            );

            let unsubscribe = build_command_payload(
                &ctx(),
                &["topic".into(), "unsubscribe".into(), "#prd-7".into()],
            )
            .unwrap();
            assert_eq!(
                unsubscribe,
                json!({
                    "command": "topic_unsubscribe",
                    "agent_id": "owner-1",
                    "topic": "#prd-7",
                })
            );
        });
    }

    #[test]
    fn rejects_legacy_top_level_cli_groups() {
        let list_error = build_command_payload(&ctx(), &["list".into(), "agents".into()]).unwrap_err();
        assert_eq!(list_error, "unknown command group: list");

        let subscribe_error =
            build_command_payload(&ctx(), &["subscribe".into(), "topic".into(), "#prd-7".into()]).unwrap_err();
        assert_eq!(subscribe_error, "unknown command group: subscribe");

        let unsubscribe_error =
            build_command_payload(&ctx(), &["unsubscribe".into(), "topic".into(), "#prd-7".into()]).unwrap_err();
        assert_eq!(unsubscribe_error, "unknown command group: unsubscribe");
    }

    #[test]
    fn prefers_herd_pane_id_and_ignores_bare_tmux_pane() {
        with_cli_env(Some("%1"), Some("%99"), None, None, || {
            let payload = build_command_payload(&ctx(), &["shell".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "shell_list",
                    "sender_agent_id": null,
                    "sender_pane_id": "%1",
                })
            );
        });

        with_cli_env(None, Some("%99"), None, None, || {
            let payload = build_command_payload(&ctx(), &["shell".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "shell_list",
                    "sender_agent_id": null,
                    "sender_pane_id": null,
                })
            );
        });
    }

    #[test]
    fn uses_tmux_pane_when_running_inside_herd_environment() {
        with_cli_env(None, Some("%7"), Some("/tmp/herd.sock"), None, || {
            let payload = build_command_payload(&ctx(), &["shell".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "shell_list",
                    "sender_agent_id": null,
                    "sender_pane_id": "%7",
                })
            );
        });

        with_cli_env(None, Some("%8"), None, Some("$1"), || {
            let payload = build_command_payload(&ctx(), &["shell".into(), "list".into()]).unwrap();
            assert_eq!(
                payload,
                json!({
                    "command": "shell_list",
                    "sender_agent_id": null,
                    "sender_pane_id": "%8",
                })
            );
        });
    }
}
