#!/usr/bin/env python3
import json
import re
import sys
import time


AGENT_ID_RE = re.compile(r"\bagentId:\s*([A-Za-z0-9_-]+)")


def extract_agent_id_from_tool_result_content(content: object) -> str | None:
    if isinstance(content, list):
        for item in content:
            found = extract_agent_id_from_tool_result_content(item)
            if found:
                return found
        return None

    if isinstance(content, dict):
        text = content.get("text")
        if isinstance(text, str):
            match = AGENT_ID_RE.search(text)
            if match:
                return match.group(1)
        for value in content.values():
            found = extract_agent_id_from_tool_result_content(value)
            if found:
                return found
        return None

    if isinstance(content, str):
        match = AGENT_ID_RE.search(content)
        if match:
            return match.group(1)

    return None


def extract_agent_id(obj: dict, tool_use_id: str) -> str | None:
    if obj.get("parentToolUseID") == tool_use_id:
        data = obj.get("data")
        if isinstance(data, dict) and data.get("type") == "agent_progress" and data.get("agentId"):
            return str(data["agentId"])

        result = obj.get("toolUseResult")
        if isinstance(result, dict) and result.get("agentId"):
            return str(result["agentId"])

    message = obj.get("message")
    if not isinstance(message, dict):
        return None

    content = message.get("content")
    if not isinstance(content, list):
        return None

    for item in content:
        if not isinstance(item, dict):
            continue
        if item.get("type") != "tool_result":
            continue
        if item.get("tool_use_id") != tool_use_id:
            continue
        found = extract_agent_id_from_tool_result_content(item.get("content"))
        if found:
            return found

    return None


def main() -> int:
    if len(sys.argv) != 3:
        return 1

    transcript = sys.argv[1]
    tool_use_id = sys.argv[2]
    deadline = time.time() + 90

    while time.time() <= deadline:
        try:
            with open(transcript, "r", encoding="utf-8") as handle:
                for raw_line in handle:
                    try:
                        obj = json.loads(raw_line)
                    except Exception:
                        continue

                    agent_id = extract_agent_id(obj, tool_use_id)
                    if agent_id:
                        print(agent_id)
                        return 0
        except FileNotFoundError:
            pass

        time.sleep(0.2)

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
