#!/usr/bin/env python3
import hashlib
import os
import subprocess
import sys

EXE = "/mnt/c/Users/Ravin/AppData/Local/Temp/synevid-target/debug/autopsy.exe"
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
        target = f"C:\\Users\\Ravin\\AppData\\Local\\Temp\\{rel}"
        os.makedirs(os.path.dirname(f"/mnt/c/Users/Ravin/AppData/Local/Temp/{p[5:]}"), exist_ok=True)
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
    win_db = f"C:\\Users\\Ravin\\AppData\\Local\\Temp\\synevid-wsl-dbs\\{h}\\storage.db"
    win_cache = f"C:\\Users\\Ravin\\AppData\\Local\\Temp\\synevid-wsl-dbs\\{h}\\cache"
    args = ["--db-path", win_db, "--cache-dir", win_cache, *args]

res = subprocess.run([EXE, *args])
sys.exit(res.returncode)
