#!/usr/bin/env python3
"""setup-agent-hosts.py — Wire (or unwire) the SynEvid MCP server in every detected agent host.

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

Usage (from a repository checkout or an installed copy):
  python3 setup-agent-hosts.py [--mcp-server JS] [--autopsy-bin BIN] [--check]
  python3 setup-agent-hosts.py --unwire [--check]
  python3 setup-agent-hosts.py --hooks [--check]

  --check  only reports what would change (dry run, works with every mode).
  --unwire removes the "synevid" entries (and the safe-edit hook when present);
           the inverse of the default wiring mode.
  --hooks  also installs the advisory PreToolUse safe-edit hook into Claude
           Code settings (opt-in, backup-first).

The MCP server is resolved automatically: an explicit --mcp-server wins, then
the installed layout next to this script (share/synevid/mcp-server/index.js),
then a repository checkout (apps/mcp-server/dist/index.js or the esbuild
bundle), then ~/.local/share/synevid/mcp-server/index.js.
"""

from __future__ import annotations

import argparse
import datetime
import json
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


def resolve_hook_script() -> Path | None:
    for candidate in [
        SCRIPT_DIR / "hooks" / "safe-edit.sh",
        REPO_ROOT / "agents" / "hooks" / "safe-edit.sh",
    ]:
        if candidate.exists():
            return candidate
    return None


def detect_installed_binary() -> str | None:
    for candidate in [
        Path("~/.local/bin/autopsy").expanduser(),
        Path("~/.cargo/bin/autopsy").expanduser(),
    ]:
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


def _drop_safe_edit_entries(pre: list) -> int:
    """Remove prior synevid safe-edit entries; returns how many were dropped."""
    before = len(pre)
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
    return before - len(pre)


def ensure_hooks(autopsy_bin: str | None, check: bool) -> str:
    """Install the advisory safe-edit hook into Claude Code settings.

    Opt-in, backup-first, replace-semantics: repeated runs (e.g. with a
    different AUTOPSY_BIN) never accumulate duplicate entries.
    The hook is advisory and fail-open (always exits 0); it never blocks
    an edit.
    """
    settings = Path("~/.claude.json").expanduser()
    if not settings.exists():
        return "SKIP hooks: ~/.claude.json absent (Claude Code not installed?)"
    hook_script = resolve_hook_script()
    if hook_script is None:
        return "SKIP hooks: safe-edit.sh not found next to this script"
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
    _drop_safe_edit_entries(pre)
    if entry in pre:
        return "OK   hooks: safe-edit already registered"
    if check:
        return "WOULD-UPDATE hooks: Claude Code PreToolUse safe-edit"
    backup(settings)
    pre.append(entry)
    settings.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"UPDATED hooks: Claude Code PreToolUse safe-edit ({settings})"


def unwind_hooks(check: bool) -> str:
    settings = Path("~/.claude.json").expanduser()
    if not settings.exists():
        return "SKIP hooks: ~/.claude.json absent (Claude Code not installed?)"
    data = load_json(settings)
    hooks = data.get("hooks")
    pre = hooks.get("PreToolUse") if isinstance(hooks, dict) else None
    if not isinstance(pre, list):
        return "OK   hooks: nothing to remove"
    dropped = _drop_safe_edit_entries(pre)
    if dropped == 0:
        return "OK   hooks: nothing to remove"
    if check:
        return "WOULD-REMOVE hooks: Claude Code PreToolUse safe-edit"
    backup(settings)
    settings.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return f"REMOVED hooks: Claude Code PreToolUse safe-edit ({settings})"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Wire (or unwire) SynEvid MCP into agent hosts"
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
        help="Also install the advisory PreToolUse safe-edit hook into Claude Code settings (opt-in, backup-first)",
    )
    args = parser.parse_args()

    if args.unwire:
        for label, raw, keys, _factory in HOSTS:
            print(unwind_host(label, Path(raw).expanduser(), keys, args.check))
        if args.hooks:
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
    for label, raw, keys, factory in HOSTS:
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
