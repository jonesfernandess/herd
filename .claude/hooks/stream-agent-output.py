#!/usr/bin/env python3
import json
import sys
import time


def emit(text: str) -> None:
    text = text.rstrip()
    if not text:
        return
    print(text, flush=True)


def summarize_tool_use(name: str, tool_input: object) -> str | None:
    if not isinstance(tool_input, dict):
        return name

    if name == "Bash":
        command = tool_input.get("command")
        if isinstance(command, str) and command:
            return f"[Bash] {command}"
    if name == "Read":
        file_path = tool_input.get("file_path")
        if isinstance(file_path, str) and file_path:
            return f"[Read] {file_path}"
    if name in {"Write", "Edit", "MultiEdit"}:
        file_path = tool_input.get("file_path")
        if isinstance(file_path, str) and file_path:
            return f"[{name}] {file_path}"

    description = tool_input.get("description")
    if isinstance(description, str) and description:
        return f"[{name}] {description}"

    return f"[{name}]"


def extract_text(content: object) -> str | None:
    if isinstance(content, str):
        return content

    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            text = extract_text(item)
            if text:
                parts.append(text)
        if parts:
            return "\n".join(parts)
        return None

    if isinstance(content, dict):
        if content.get("type") == "text":
            text = content.get("text")
            if isinstance(text, str):
                return text
        text = content.get("text")
        if isinstance(text, str):
            return text

    return None


def handle_message(obj: dict, tool_uses: dict[str, tuple[str, object]]) -> None:
    message = obj.get("message")
    if not isinstance(message, dict):
        return

    role = message.get("role")
    content = message.get("content")

    if role == "assistant" and isinstance(content, list):
        for item in content:
            if not isinstance(item, dict):
                continue
            item_type = item.get("type")
            if item_type == "text":
                text = item.get("text")
                if isinstance(text, str):
                    emit(text)
                continue
            if item_type == "tool_use":
                tool_use_id = item.get("id")
                tool_name = item.get("name")
                tool_input = item.get("input")
                if isinstance(tool_use_id, str) and isinstance(tool_name, str):
                    tool_uses[tool_use_id] = (tool_name, tool_input)
                    summary = summarize_tool_use(tool_name, tool_input)
                    if summary:
                        emit(summary)
                continue
        return

    if role == "user":
        if isinstance(content, str):
            emit(f"Prompt: {content}")
            return

        if not isinstance(content, list):
            return

        for item in content:
            if not isinstance(item, dict):
                continue
            if item.get("type") != "tool_result":
                continue
            tool_use_id = item.get("tool_use_id")
            tool_name = None
            if isinstance(tool_use_id, str):
                stored = tool_uses.get(tool_use_id)
                if stored:
                    tool_name = stored[0]

            text = extract_text(item.get("content"))
            if not text:
                continue

            if tool_name == "Bash" or item.get("is_error") is True:
                emit(text)


def follow_output(path: str) -> int:
    while True:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                tool_uses: dict[str, tuple[str, object]] = {}
                while True:
                    line = handle.readline()
                    if not line:
                        time.sleep(0.1)
                        continue
                    try:
                        obj = json.loads(line)
                    except Exception:
                        continue
                    handle_message(obj, tool_uses)
        except FileNotFoundError:
            time.sleep(0.1)


def main() -> int:
    if len(sys.argv) != 2:
        return 1
    return follow_output(sys.argv[1])


if __name__ == "__main__":
    raise SystemExit(main())
