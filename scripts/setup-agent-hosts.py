#!/usr/bin/env python3
"""setup-agent-hosts.py — Wire the SynEvid MCP server into every detected agent host.

Idempotent and non-destructive: existing entries are preserved, a timestamped
backup is written before any modification, and hosts that are absent are
skipped with a notice (never created from scratch except workspace files).

Supported hosts (global/user scope):
  Claude Code ......... ~/.claude.json  ({"mcpServers": {...}})
  Claude Desktop ...... ~/.config/Claude/claude_desktop_config.json
  Cursor .............. ~/.cursor/mcp.json
  Windsurf ............ ~/.codeium/windsurf/mcp_config.json
  VS Code (Copilot) ... ~/.config/Code/User/mcp.json  ({"servers": {...}})
  opencode ............ ~/.config/opencode/opencode.json ({"mcp": {...}})
  Gemini CLI .......... ~/.gemini/settings.json ({"mcpServers": {...}})

Usage:
  python3 scripts/setup-agent-hosts.py [--mcp-server JS] [--autopsy-bin BIN] [--check]

  --check only reports what would change.
"""

from __future__ import annotations

import argparse
import datetime
import json
import shutil
import sys
from pathlib import Path

ENTRY_KEY = "synevid"


def build_command(mcp_server: Path, autopsy_bin: str | None) -> dict:
    entry: dict = {
        "command": "node",
        "args": [str(mcp_server)],
    }
    if autopsy_bin:
        entry["env"] = {"AUTOPSY_BIN": autopsy_bin}
    return entry


def vscode_entry(mcp_server: Path, autopsy_bin: str | None) -> dict:
    entry: dict = {
        "command": "node",
        "args": [str(mcp_server)],
        "type": "stdio",
    }
    if autopsy_bin:
        entry["env"] = {"AUTOPSY_BIN": autopsy_bin}
    return entry


def opencode_entry(mcp_server: Path, autopsy_bin: str | None) -> dict:
    entry: dict = {
        "type": "local",
        "command": ["node", str(mcp_server)],
        "enabled": True,
    }
    if autopsy_bin:
        entry["environment"] = {"AUTOPSY_BIN": autopsy_bin}
    return entry


HOSTS = [
    # (label, config path, container keys, entry factory)
    ("Claude Code", "~/.claude.json", ("mcpServers",), build_command),
    (
        "Claude Desktop",
        "~/.config/Claude/claude_desktop_config.json",
        ("mcpServers",),
        build_command,
    ),
    ("Cursor", "~/.cursor/mcp.json", ("mcpServers",), build_command),
    ("Windsurf", "~/.codeium/windsurf/mcp_config.json", ("mcpServers",), build_command),
    ("VS Code", "~/.config/Code/User/mcp.json", ("servers",), vscode_entry),
    ("opencode", "~/.config/opencode/opencode.json", ("mcp",), opencode_entry),
    ("Gemini CLI", "~/.gemini/settings.json", ("mcpServers",), build_command),
]


def load_json(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def ensure_host(
    label: str,
    path: Path,
    keys: tuple[str, ...],
    entry: dict,
    check: bool,
) -> str:
    if not path.exists():
        return f"SKIP {label}: {path} absent (host not installed?)"
    data = load_json(path)
    node = data
    for key in keys:
        child = node.get(key)
        if not isinstance(child, dict):
            child = {}
            node[key] = child
        node = child
    if node.get(ENTRY_KEY) == entry:
        return f"OK   {label}: entry already current"
    action = "WOULD-UPDATE" if check else "UPDATED"
    if not check:
        backup = path.with_suffix(
            path.suffix + f".synevid-bak-{datetime.datetime.now():%Y%m%d%H%M%S}"
        )
        try:
            shutil.copy2(path, backup)
        except OSError:
            pass
        node[ENTRY_KEY] = entry
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"{action} {label}: {path}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Wire SynEvid MCP into agent hosts")
    parser.add_argument("--mcp-server", type=Path, default=None)
    parser.add_argument("--autopsy-bin", default=None)
    parser.add_argument("--check", action="store_true")
    parser.add_argument(
        "--hooks",
        action="store_true",
        help="Also install the advisory PreToolUse safe-edit hook into Claude Code settings (opt-in, backup-first)",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    mcp_server = args.mcp_server or (root / "apps" / "mcp-server" / "dist" / "index.js")
    if not mcp_server.exists():
        print(
            f"ERROR: MCP bundle missing: {mcp_server} (run: npm run build in apps/mcp-server)",
            file=sys.stderr,
        )
        return 2

    entry_cache: dict = {}
    for label, raw, keys, factory in HOSTS:
        path = Path(raw).expanduser()
        key = (label, str(path))
        if key not in entry_cache:
            entry_cache[key] = factory(mcp_server, args.autopsy_bin)
        print(ensure_host(label, path, keys, entry_cache[key], args.check))

    if args.hooks:
        print(ensure_hooks(root, args.autopsy_bin, args.check))


def ensure_hooks(root: Path, autopsy_bin: str | None, check: bool) -> str:
    """Install the advisory safe-edit hook into Claude Code settings.

    Opt-in, backup-first, replace-semantics: repeated runs (e.g. with a
    different AUTOPSY_BIN) never accumulate duplicate entries.
    The hook is advisory and fail-open (always exits 0); it never blocks
    an edit.
    """
    settings = Path("~/.claude.json").expanduser()
    if not settings.exists():
        return "SKIP hooks: ~/.claude.json absent (Claude Code not installed?)"
    hook_script = root / "agents" / "hooks" / "safe-edit.sh"
    prefix = f"AUTOPSY_BIN={autopsy_bin} " if autopsy_bin else ""
    command = f"{prefix}{hook_script}"
    data = load_json(settings)
    hooks = data.get("hooks")
    if not isinstance(hooks, dict):
        hooks = {}
        data["hooks"] = hooks
    pre = hooks.get("PreToolUse")
    if not isinstance(pre, list):
        pre = []
        hooks["PreToolUse"] = pre
    entry = {
        "matcher": "Edit|Write",
        "hooks": [{"type": "command", "command": command, "timeout": 30}],
    }
    # Replace semantics: drop prior synevid safe-edit entries first.
    pre[:] = [
        e
        for e in pre
        if not (
            isinstance(e, dict)
            and any(
                "safe-edit.sh" in str(h.get("command", ""))
                for h in e.get("hooks", [])
                if isinstance(h, dict)
            )
        )
    ]
    if entry in pre:
        return "OK   hooks: safe-edit already registered"
    if check:
        return "WOULD-UPDATE hooks: Claude Code PreToolUse safe-edit"
    backup = settings.with_suffix(
        settings.suffix + f".synevid-bak-{datetime.datetime.now():%Y%m%d%H%M%S}"
    )
    try:
        shutil.copy2(settings, backup)
    except OSError:
        pass
    pre.append(entry)
    settings.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"UPDATED hooks: Claude Code PreToolUse safe-edit ({settings})"


if __name__ == "__main__":
    sys.exit(main())
