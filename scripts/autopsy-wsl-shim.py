#!/usr/bin/env python3
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

REPO_ROOT = Path(__file__).resolve().parents[1]
EXE = os.environ.get(
    "SYNEVID_AUTOPSY_EXE",
    str(REPO_ROOT / "target" / "debug" / "autopsy.exe"),
)
PATH_FLAGS = {
    "-r",
    "--repo-root",
    "--db-path",
    "--cache-dir",
    "--invariants-file",
    "--before-doc",
    "--after-doc",
    "--trace-file",
    "--surface-file",
    "--lcov",
    "--contract",
    "--before",
    "--after",
}

def to_win(p: str, is_db_or_cache: bool = False) -> str:
    if is_db_or_cache and p.startswith("/tmp/"):
        rel = p[5:].replace("/", "\\")
        windows_temp = os.environ.get("TEMP") or os.environ.get("TMP")
        if windows_temp and (":" in windows_temp or "\\" in windows_temp):
            target = str(Path(windows_temp) / rel)
            os.makedirs(os.path.dirname(p) or tempfile.gettempdir(), exist_ok=True)
        else:
            target = to_win(tempfile.gettempdir()) + "\\" + rel
        return target
    if p.startswith("/"):
        try:
            out = subprocess.check_output(["wslpath", "-w", p], text=True).strip()
            if out:
                return out
        except Exception:
            pass
    return p

args = []
prev = ""
repo_root = os.getcwd()
has_db_path = False

for a in sys.argv[1:]:
    if prev in ("-r", "--repo-root"):
        repo_root = os.path.abspath(a)
    if a == "--db-path":
        has_db_path = True
    is_db = prev in ("--db-path", "--cache-dir")
    if prev in PATH_FLAGS and a.startswith("/"):
        args.append(to_win(a, is_db))
    elif a.startswith("/") and os.path.exists(a):
        args.append(to_win(a, is_db))
    else:
        args.append(a)
    prev = a

if not has_db_path:
    h = hashlib.sha256(repo_root.encode()).hexdigest()[:16]
    temp_root = os.environ.get("TEMP") or os.environ.get("TMP") or tempfile.gettempdir()
    win_db = str(Path(temp_root) / "synevid-wsl-dbs" / h / "storage.db")
    win_cache = str(Path(temp_root) / "synevid-wsl-dbs" / h / "cache")
    args = ["--db-path", win_db, "--cache-dir", win_cache, *args]

res = subprocess.run([EXE, *args])
sys.exit(res.returncode)
