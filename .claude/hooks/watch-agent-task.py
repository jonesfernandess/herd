#!/usr/bin/env python3
import json
import re
import socket
import sys
import time
from pathlib import Path


TOOL_USE_ID_RE = re.compile(r"<tool-use-id>(.*?)</tool-use-id>", re.DOTALL)
STATUS_RE = re.compile(r"<status>(.*?)</status>", re.DOTALL)
FINAL_STATUSES = {"completed", "killed", "failed", "error", "errored", "stopped", "cancelled"}


def parse_notification(content: str) -> tuple[str | None, str | None]:
    tool_use_id_match = TOOL_USE_ID_RE.search(content)
    status_match = STATUS_RE.search(content)
    tool_use_id = tool_use_id_match.group(1).strip() if tool_use_id_match else None
    status = status_match.group(1).strip().lower() if status_match else None
    return tool_use_id, status


def extract_status(obj: object) -> tuple[str | None, str | None]:
    if not isinstance(obj, dict):
        return None, None

    if obj.get("type") == "queue-operation":
        content = obj.get("content")
        if isinstance(content, str) and "<task-notification>" in content:
            return parse_notification(content)

    message = obj.get("message")
    if isinstance(message, dict):
        content = message.get("content")
        if isinstance(content, str) and "<task-notification>" in content:
            return parse_notification(content)

    return None, None


def destroy_shell(sock_path: str, pane_id: str) -> None:
    payload = json.dumps({
        "command": "shell_destroy",
        "session_id": pane_id,
    })
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        client.connect(sock_path)
        client.sendall((payload + "\n").encode("utf-8"))
        client.recv(65536)
    finally:
        client.close()


def main() -> int:
    if len(sys.argv) != 5:
        return 1

    transcript_path = Path(sys.argv[1])
    tool_use_id = sys.argv[2]
    sock_path = sys.argv[3]
    pane_id = sys.argv[4]

    last_size = 0
    deadline = time.time() + 60 * 60 * 6

    while time.time() <= deadline:
        try:
            if not transcript_path.exists():
                time.sleep(0.2)
                continue

            size = transcript_path.stat().st_size
            if size < last_size:
                last_size = 0

            with transcript_path.open("r", encoding="utf-8", errors="replace") as handle:
                handle.seek(last_size)
                while True:
                    raw_line = handle.readline()
                    if not raw_line:
                        break
                    last_size = handle.tell()
                    try:
                        obj = json.loads(raw_line)
                    except Exception:
                        continue

                    notification_tool_use_id, status = extract_status(obj)
                    if notification_tool_use_id != tool_use_id:
                        continue
                    if status not in FINAL_STATUSES:
                        continue

                    destroy_shell(sock_path, pane_id)
                    return 0
        except Exception:
            pass

        time.sleep(0.2)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
