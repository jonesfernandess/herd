#!/usr/bin/env python3
import json
import re
import sys
import time


AGENT_ID_RE = re.compile(r"\bagentId:\s*([A-Za-z0-9_-]+)")
OUTPUT_FILE_RE = re.compile(r"\boutput(?:_|)file:\s*(\S+)", re.IGNORECASE)


def first_non_empty(*values: object) -> str | None:
    for value in values:
        if isinstance(value, str) and value:
            return value
    return None


def extract_text(content: object) -> str | None:
    if isinstance(content, dict):
        text = content.get("text")
        if isinstance(text, str) and text:
            return text
        for value in content.values():
            found = extract_text(value)
            if found:
                return found
        return None

    if isinstance(content, list):
        for item in content:
            found = extract_text(item)
            if found:
                return found
        return None

    if isinstance(content, str) and content:
        return content

    return None


def extract_from_content(content: object, pattern: re.Pattern[str]) -> str | None:
    if isinstance(content, list):
        for item in content:
            found = extract_from_content(item, pattern)
            if found:
                return found
        return None

    if isinstance(content, dict):
        text = content.get("text")
        if isinstance(text, str):
            match = pattern.search(text)
            if match:
                return match.group(1)
        for value in content.values():
            found = extract_from_content(value, pattern)
            if found:
                return found
        return None

    if isinstance(content, str):
        match = pattern.search(content)
        if match:
            return match.group(1)

    return None


def extract_fields(obj: dict, tool_use_id: str) -> tuple[str | None, str | None]:
    agent_id = None
    output_file = None

    if obj.get("parentToolUseID") == tool_use_id:
        data = obj.get("data")
        if isinstance(data, dict):
            if data.get("type") == "agent_progress":
                agent_id = first_non_empty(data.get("agentId"), data.get("agent_id"))
            output_file = first_non_empty(data.get("outputFile"), data.get("output_file"))

        result = obj.get("toolUseResult")
        if isinstance(result, dict):
            agent_id = first_non_empty(
                agent_id,
                result.get("agentId"),
                result.get("agent_id"),
            )
            output_file = first_non_empty(
                output_file,
                result.get("outputFile"),
                result.get("output_file"),
            )

    message = obj.get("message")
    if not isinstance(message, dict):
        return agent_id, output_file

    content = message.get("content")
    if not isinstance(content, list):
        return agent_id, output_file

    for item in content:
        if not isinstance(item, dict):
            continue
        if item.get("type") != "tool_result":
            continue
        if item.get("tool_use_id") != tool_use_id:
            continue

        root_tool_use_result = obj.get("toolUseResult")
        if isinstance(root_tool_use_result, dict):
            agent_id = first_non_empty(
                agent_id,
                root_tool_use_result.get("agentId"),
                root_tool_use_result.get("agent_id"),
            )
            output_file = first_non_empty(
                output_file,
                root_tool_use_result.get("outputFile"),
                root_tool_use_result.get("output_file"),
            )

        item_content = item.get("content")
        agent_id = first_non_empty(
            agent_id,
            extract_from_content(item_content, AGENT_ID_RE),
        )
        output_file = first_non_empty(
            output_file,
            item.get("outputFile"),
            item.get("output_file"),
            extract_from_content(item_content, OUTPUT_FILE_RE),
        )

        tool_use_result = item.get("toolUseResult")
        if isinstance(tool_use_result, dict):
            agent_id = first_non_empty(
                agent_id,
                tool_use_result.get("agentId"),
                tool_use_result.get("agent_id"),
            )
            output_file = first_non_empty(
                output_file,
                tool_use_result.get("outputFile"),
                tool_use_result.get("output_file"),
            )

        text = extract_text(item_content)
        if text:
            agent_id = first_non_empty(agent_id, extract_from_content(text, AGENT_ID_RE))
            output_file = first_non_empty(output_file, extract_from_content(text, OUTPUT_FILE_RE))

    return agent_id, output_file


def main() -> int:
    if len(sys.argv) != 4:
        return 1

    transcript = sys.argv[1]
    tool_use_id = sys.argv[2]
    field = sys.argv[3]
    deadline = time.time() + 90

    while time.time() <= deadline:
        try:
            with open(transcript, "r", encoding="utf-8") as handle:
                for raw_line in handle:
                    try:
                        obj = json.loads(raw_line)
                    except Exception:
                        continue

                    agent_id, output_file = extract_fields(obj, tool_use_id)
                    if field == "agent_id" and agent_id:
                        print(agent_id)
                        return 0
                    if field == "output_file" and output_file:
                        print(output_file)
                        return 0
        except FileNotFoundError:
            pass

        time.sleep(0.2)

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
