#!/usr/bin/env python3
"""render-package-manifests.py — Stamp release SHA256SUMS.txt into Homebrew, Scoop, and Winget manifests."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

PLACEHOLDERS = {
    "autopsy-aarch64-apple-darwin.tar.gz": "REPLACE_WITH_SHA256_AARCH64_APPLE_DARWIN",
    "autopsy-x86_64-apple-darwin.tar.gz": "REPLACE_WITH_SHA256_X86_64_APPLE_DARWIN",
    "autopsy-aarch64-unknown-linux-gnu.tar.gz": "REPLACE_WITH_SHA256_AARCH64_UNKNOWN_LINUX_GNU",
    "autopsy-x86_64-unknown-linux-gnu.tar.gz": "REPLACE_WITH_SHA256_X86_64_UNKNOWN_LINUX_GNU",
    "autopsy-x86_64-pc-windows-msvc.zip": "REPLACE_WITH_SHA256_X86_64_PC_WINDOWS_MSVC",
}


def parse_sums(sums_file: Path) -> dict[str, str]:
    mapping: dict[str, str] = {}
    for line in sums_file.read_text(encoding="utf-8").splitlines():
        parts = line.strip().split()
        if len(parts) >= 2:
            digest = parts[0]
            fname = parts[1].lstrip("*")
            mapping[fname] = digest
    return mapping


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sums", type=Path, required=True, help="Path to SHA256SUMS.txt")
    parser.add_argument("--version", default="0.1.0", help="Release version without v prefix")
    parser.add_argument("--out-dir", type=Path, default=None, help="Optional directory to write rendered manifests")
    args = parser.parse_args()

    sums = parse_sums(args.sums)
    manifests = [
        ROOT / "packaging" / "homebrew" / "synevid.rb",
        ROOT / "packaging" / "scoop" / "synevid.json",
        ROOT / "packaging" / "winget" / "sainibhaowal.SynEvid.yaml",
    ]

    for manifest in manifests:
        text = manifest.read_text(encoding="utf-8")
        text = text.replace("0.1.0", args.version)
        for asset, placeholder in PLACEHOLDERS.items():
            if asset in sums:
                text = text.replace(placeholder, sums[asset])
        target = (args.out_dir / manifest.name) if args.out_dir else manifest
        if args.out_dir:
            args.out_dir.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
        print(f"Rendered {target}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
