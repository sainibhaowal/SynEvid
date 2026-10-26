"""SynEvid (Code Autopsy) Python & uvx launcher.

Allows running the deterministic engine via:
    pip install synevid
    uvx synevid --help
    uvx --from synevid autopsy verify
"""

import os
import platform
import shutil
import stat
import subprocess
import sys
import urllib.request
import tarfile
import zipfile
from pathlib import Path

VERSION = "0.1.0"
GITHUB_REPO = "sainibhaowal/SynEvid"

PLATFORM_MAP = {
    ("Darwin", "arm64"): f"autopsy-aarch64-apple-darwin.tar.gz",
    ("Darwin", "x86_64"): f"autopsy-x86_64-apple-darwin.tar.gz",
    ("Linux", "x86_64"): f"autopsy-x86_64-unknown-linux-gnu.tar.gz",
    ("Linux", "aarch64"): f"autopsy-aarch64-unknown-linux-gnu.tar.gz",
    ("Windows", "AMD64"): f"autopsy-x86_64-pc-windows-msvc.zip",
    ("Windows", "x86_64"): f"autopsy-x86_64-pc-windows-msvc.zip",
}


def find_existing_binary(binary_name: str) -> str | None:
    """Finds already installed binary in PATH or standard user directories."""
    # 1. Explicit environment variable override
    env_bin = os.environ.get("AUTOPSY_BIN")
    if env_bin and os.path.isfile(env_bin):
        return env_bin

    # 2. Check system PATH
    found = shutil.which(binary_name)
    if found:
        return found

    # 3. Check common installation directories
    home = Path.home()
    is_win = platform.system() == "Windows"
    exe_name = f"{binary_name}.exe" if is_win else binary_name

    candidates = [
        home / ".local" / "bin" / exe_name,
        home / ".cargo" / "bin" / exe_name,
        home / ".synevid" / "bin" / exe_name,
        home / ".gemini" / "antigravity" / "bin" / exe_name,
        home / "AppData" / "Roaming" / "Antigravity" / "bin" / exe_name,
        Path("/usr/local/bin") / exe_name,
    ]

    for cand in candidates:
        if cand.is_file():
            return str(cand)

    return None


def ensure_binary() -> str:
    """Ensures the autopsy binary is present, downloading if necessary."""
    is_win = platform.system() == "Windows"
    binary_name = "autopsy.exe" if is_win else "autopsy"

    existing = find_existing_binary(binary_name)
    if existing:
        return existing

    # Download from official release
    sys_name = platform.system()
    machine = platform.machine()
    target_archive = PLATFORM_MAP.get((sys_name, machine))
    if not target_archive:
        sys.stderr.write(
            f"Error: Unsupported platform {sys_name} {machine} for automatic download.\n"
            f"Please build from source: cargo install autopsy-cli\n"
        )
        sys.exit(3)

    cache_dir = Path.home() / ".synevid" / "bin"
    cache_dir.mkdir(parents=True, exist_ok=True)
    bin_path = cache_dir / binary_name
    if bin_path.is_file():
        return str(bin_path)

    url = f"https://github.com/{GITHUB_REPO}/releases/download/v{VERSION}/{target_archive}"
    sys.stderr.write(f"Downloading SynEvid engine v{VERSION} for {sys_name}/{machine}...\n")

    archive_path = cache_dir / target_archive
    try:
        urllib.request.urlretrieve(url, archive_path)
        if target_archive.endswith(".zip"):
            with zipfile.ZipFile(archive_path, "r") as zf:
                zf.extractall(cache_dir)
        else:
            with tarfile.open(archive_path, "r:gz") as tf:
                tf.extractall(cache_dir)
        archive_path.unlink(missing_ok=True)
    except Exception as exc:
        sys.stderr.write(f"Failed to download prebuilt binary: {exc}\n")
        sys.exit(3)

    if not is_win and bin_path.is_file():
        bin_path.chmod(bin_path.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

    return str(bin_path)


def main():
    """Main CLI entrypoint for synevid and autopsy commands."""
    binary = ensure_binary()
    args = [binary] + sys.argv[1:]

    if platform.system() == "Windows":
        proc = subprocess.run(args)
        sys.exit(proc.returncode)
    else:
        os.execv(binary, args)


def ui_main():
    """Main CLI entrypoint for synevid-ui command."""
    binary = ensure_binary()
    args = [binary, "ui"] + sys.argv[1:]

    if platform.system() == "Windows":
        proc = subprocess.run(args)
        sys.exit(proc.returncode)
    else:
        os.execv(binary, args)


if __name__ == "__main__":
    main()
