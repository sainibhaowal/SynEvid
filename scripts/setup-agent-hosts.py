#!/usr/bin/env python3
"""setup-agent-hosts.py — Wire (or unwire) the SynEvid MCP server and hooks in every detected agent host.

Idempotent and non-destructive: existing entries are preserved, a timestamped
backup is written before any modification, and hosts that are absent are
skipped with a notice (never created from scratch except workspace files).

Supported hosts (global/user scope across Linux, macOS, and Windows):
  Claude Code ......... ~/.claude.json  ({"mcpServers": {...}})
  Claude Desktop ...... Linux: ~/.config/Claude/claude_desktop_config.json
                        macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
                        Win:   %APPDATA%/Claude/claude_desktop_config.json
  Cursor .............. ~/.cursor/mcp.json
  Windsurf ............ ~/.codeium/windsurf/mcp_config.json
  VS Code (Copilot) ... Linux: ~/.config/Code/User/mcp.json  ({"servers": {...}})
                        macOS: ~/Library/Application Support/Code/User/mcp.json
                        Win:   %APPDATA%/Code/User/mcp.json
  opencode ............ ~/.config/opencode/opencode.json ({"mcp": {...}})
  Gemini CLI .......... ~/.gemini/settings.json ({"mcpServers": {...}})

Usage (from a repository checkout or an installed copy):
  python3 setup-agent-hosts.py [--mcp-server JS] [--autopsy-bin BIN] [--check]
  python3 setup-agent-hosts.py --unwire [--check]
  python3 setup-agent-hosts.py --hooks [--check]

  --check  only reports what would change (dry run, works with every mode).
  --unwire removes the "synevid" entries (and the safe-edit + task-complete hooks
           when present); the inverse of the default wiring mode.
  --hooks  also installs the advisory PreToolUse safe-edit hook AND the Stop
           autopsy-task-complete forensic trajectory hook into Claude Code
           settings (opt-in, backup-first).

The MCP server is resolved automatically: an explicit --mcp-server wins, then
the installed layout next to this script (share/synevid/mcp-server/index.js),
then a repository checkout (apps/mcp-server/dist/index.js or the esbuild
bundle), then ~/.local/share/synevid/mcp-server/index.js.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import shlex
import shutil
import sys
from pathlib import Path

ENTRY_KEY = "synevid"

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent
INSTALLED_MCP = Path("~/.local/share/synevid/mcp-server/index.js").expanduser()


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


def _platform_claude_desktop_path() -> str:
    if sys.platform == "darwin":
        mac = Path("~/Library/Application Support/Claude/claude_desktop_config.json").expanduser()
        if mac.exists():
            return str(mac)
    elif sys.platform == "win32":
        appdata = os.environ.get("APPDATA")
        if appdata:
            win = Path(appdata) / "Claude" / "claude_desktop_config.json"
            if win.exists():
                return str(win)
    return "~/.config/Claude/claude_desktop_config.json"


def _platform_vscode_path() -> str:
    if sys.platform == "darwin":
        mac = Path("~/Library/Application Support/Code/User/mcp.json").expanduser()
        if mac.exists():
            return str(mac)
    elif sys.platform == "win32":
        appdata = os.environ.get("APPDATA")
        if appdata:
            win = Path(appdata) / "Code" / "User" / "mcp.json"
            if win.exists():
                return str(win)
    return "~/.config/Code/User/mcp.json"


def get_hosts() -> list[tuple[str, str, tuple[str, ...], object]]:
    return [
        ("Claude Code", "~/.claude.json", ("mcpServers",), build_command),
        ("Claude Desktop", _platform_claude_desktop_path(), ("mcpServers",), build_command),
        ("Cursor", "~/.cursor/mcp.json", ("mcpServers",), build_command),
        ("Windsurf", "~/.codeium/windsurf/mcp_config.json", ("mcpServers",), build_command),
        ("VS Code", _platform_vscode_path(), ("servers",), vscode_entry),
        ("opencode", "~/.config/opencode/opencode.json", ("mcp",), opencode_entry),
        ("Gemini CLI", "~/.gemini/settings.json", ("mcpServers",), build_command),
    ]


def load_json(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def backup(path: Path) -> None:
    """Copy `path` next to itself with a timestamp suffix (best effort)."""
    stamp = f"{datetime.datetime.now():%Y%m%d%H%M%S}"
    try:
        shutil.copy2(path, path.with_suffix(path.suffix + f".synevid-bak-{stamp}"))
    except OSError:
        pass


def resolve_mcp_server(explicit: Path | None) -> Path | None:
    if explicit is not None:
        return explicit if explicit.exists() else None
    candidates = [
        SCRIPT_DIR / "mcp-server" / "index.js",  # installed layout (share/synevid/)
        REPO_ROOT / "apps" / "mcp-server" / "dist" / "index.js",  # checkout, tsc build
        REPO_ROOT
        / "apps"
        / "mcp-server"
        / "dist"
        / "index.bundle.js",  # checkout, bundled
        INSTALLED_MCP,
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    return None


def resolve_hook_script(name: str = "safe-edit.sh") -> Path | None:
    for candidate in [
        SCRIPT_DIR / "hooks" / name,
        REPO_ROOT / "agents" / "hooks" / name,
    ]:
        if candidate.exists():
            return candidate
    return None


def detect_installed_binary() -> str | None:
    candidates = [
        Path("~/.local/bin/autopsy").expanduser(),
        Path("~/.local/bin/autopsy.exe").expanduser(),
        Path("~/.cargo/bin/autopsy").expanduser(),
        Path("~/.cargo/bin/autopsy.exe").expanduser(),
    ]
    for candidate in candidates:
        if candidate.exists():
            return str(candidate)
    return None


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
        backup(path)
        node[ENTRY_KEY] = entry
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"{action} {label}: {path}"


def unwind_host(
    label: str,
    path: Path,
    keys: tuple[str, ...],
    check: bool,
) -> str:
    if not path.exists():
        return f"SKIP {label}: {path} absent (host not installed?)"
    data = load_json(path)
    node = data
    for key in keys:
        child = node.get(key)
        if not isinstance(child, dict):
            return f"OK   {label}: nothing to remove"
        node = child
    if ENTRY_KEY not in node:
        return f"OK   {label}: nothing to remove"
    if check:
        return f"WOULD-REMOVE {label}: {ENTRY_KEY} entry from {path}"
    backup(path)
    del node[ENTRY_KEY]
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"REMOVED {label}: {ENTRY_KEY} entry from {path}"


def _drop_hook_entries(hook_list: list, needle: str) -> int:
    """Remove prior synevid hook entries matching `needle`; returns how many were dropped."""
    before = len(hook_list)
    hook_list[:] = [
        e
        for e in hook_list
        if not (
            isinstance(e, dict)
            and any(
                needle in str(h.get("command", ""))
                for h in e.get("hooks", [])
                if isinstance(h, dict)
            )
        )
    ]
    return before - len(hook_list)


def ensure_hooks(autopsy_bin: str | None, check: bool) -> str:
    """Install the advisory PreToolUse safe-edit hook AND the Stop autopsy-task-complete
    forensic trajectory hook into Claude Code settings.

    Opt-in, backup-first, replace-semantics: repeated runs (e.g. with a
    different AUTOPSY_BIN) never accumulate duplicate entries.
    Both hooks are fail-open by default (always exit 0); they never block
    an edit or task turn.
    """
    settings = Path("~/.claude.json").expanduser()
    if not settings.exists():
        return "SKIP hooks: ~/.claude.json absent (Claude Code not installed?)"
    safe_edit_script = resolve_hook_script("safe-edit.sh")
    if safe_edit_script is None:
        return "SKIP hooks: safe-edit.sh not found next to this script"
    task_complete_script = resolve_hook_script("autopsy-task-complete.sh")

    prefix = f"AUTOPSY_BIN={shlex.quote(autopsy_bin)} " if autopsy_bin else ""
    safe_edit_cmd = f"{prefix}{shlex.quote(str(safe_edit_script))}"

    data = load_json(settings)
    hooks = data.get("hooks")
    if not isinstance(hooks, dict):
        hooks = {}
        data["hooks"] = hooks

    pre = hooks.get("PreToolUse")
    if not isinstance(pre, list):
        pre = []
        hooks["PreToolUse"] = pre
    pre_entry = {
        "matcher": "Edit|Write",
        "hooks": [{"type": "command", "command": safe_edit_cmd, "timeout": 30}],
    }
    _drop_hook_entries(pre, "safe-edit.sh")
    changed = False
    if pre_entry not in pre:
        pre.append(pre_entry)
        changed = True

    if task_complete_script is not None:
        stop_list = hooks.get("Stop")
        if not isinstance(stop_list, list):
            stop_list = []
            hooks["Stop"] = stop_list
        task_cmd = f"{prefix}{shlex.quote(str(task_complete_script))}"
        stop_entry = {
            "matcher": "",
            "hooks": [{"type": "command", "command": task_cmd, "timeout": 60}],
        }
        _drop_hook_entries(stop_list, "autopsy-task-complete.sh")
        if stop_entry not in stop_list:
            stop_list.append(stop_entry)
            changed = True

    if not changed:
        return "OK   hooks: safe-edit and task-complete already registered"
    if check:
        return "WOULD-UPDATE hooks: Claude Code PreToolUse safe-edit + Stop task-complete"
    backup(settings)
    settings.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"UPDATED hooks: Claude Code PreToolUse safe-edit + Stop task-complete ({settings})"


def unwind_hooks(check: bool) -> str:
    settings = Path("~/.claude.json").expanduser()
    if not settings.exists():
        return "SKIP hooks: ~/.claude.json absent (Claude Code not installed?)"
    data = load_json(settings)
    hooks = data.get("hooks")
    if not isinstance(hooks, dict):
        return "OK   hooks: nothing to remove"
    dropped = 0
    pre = hooks.get("PreToolUse")
    if isinstance(pre, list):
        dropped += _drop_hook_entries(pre, "safe-edit.sh")
    stop_list = hooks.get("Stop")
    if isinstance(stop_list, list):
        dropped += _drop_hook_entries(stop_list, "autopsy-task-complete.sh")
    if dropped == 0:
        return "OK   hooks: nothing to remove"
    if check:
        return "WOULD-REMOVE hooks: Claude Code PreToolUse/Stop synevid hooks"
    backup(settings)
    settings.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"REMOVED hooks: Claude Code PreToolUse/Stop synevid hooks ({settings})"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Wire (or unwire) SynEvid MCP and hooks into agent hosts"
    )
    parser.add_argument("--mcp-server", type=Path, default=None)
    parser.add_argument("--autopsy-bin", default=None)
    parser.add_argument(
        "--check", action="store_true", help="Report what would change (dry run)"
    )
    parser.add_argument(
        "--unwire",
        action="store_true",
        help="Remove synevid entries instead of adding them (inverse of wiring)",
    )
    parser.add_argument(
        "--hooks",
        action="store_true",
        help="Also install the advisory PreToolUse safe-edit + Stop task-complete hooks into Claude Code settings (opt-in, backup-first)",
    )
    args = parser.parse_args()

    hosts = get_hosts()
    if args.unwire:
        for label, raw, keys, _factory in hosts:
            print(unwind_host(label, Path(raw).expanduser(), keys, args.check))
        print(unwind_hooks(args.check))
        return 0

    mcp_server = resolve_mcp_server(args.mcp_server)
    if mcp_server is None:
        print(
            "ERROR: MCP server not found. Looked for an explicit --mcp-server path, then:\n"
            f"  {SCRIPT_DIR / 'mcp-server' / 'index.js'}\n"
            f"  {REPO_ROOT / 'apps' / 'mcp-server' / 'dist' / 'index.js'}\n"
            f"  {INSTALLED_MCP}\n"
            "Run the installer, or build one with: npm ci && npm run build:bundle (in apps/mcp-server)",
            file=sys.stderr,
        )
        return 2

    autopsy_bin = args.autopsy_bin or detect_installed_binary()
    entry_cache: dict = {}
    for label, raw, keys, factory in hosts:
        path = Path(raw).expanduser()
        key = (label, str(path))
        if key not in entry_cache:
            entry_cache[key] = factory(mcp_server, autopsy_bin)
        print(ensure_host(label, path, keys, entry_cache[key], args.check))

    if args.hooks:
        print(ensure_hooks(autopsy_bin, args.check))
    return 0


if __name__ == "__main__":
    sys.exit(main())
