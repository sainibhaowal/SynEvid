#!/usr/bin/env python3
"""
Fabricated Output Scanner for SynEvid CI

Scans source code for hardcoded/fabricated operational metrics that should
come from the daemon or engine, not be hardcoded in presentation layers.

Exit codes:
  0 - No fabricated output found
  1 - Fabricated output detected
  2 - Error during scan
"""

import os
import re
import sys
import argparse
from pathlib import Path
from typing import List, Tuple, Set

# Patterns that indicate fabricated/hardcoded output
FORBIDDEN_PATTERNS = [
    # Specific hardcoded numbers that should come from engine
    (r'\b14 affected callers?\b', 'Hardcoded "14 affected callers" metric'),
    (r'\b\d+ affected callers?\b', 'Hardcoded "N affected callers" metric'),
    (r'\b\d+ newly dead symbols?\b', 'Hardcoded "N newly dead symbols" metric'),
    (r'\b\d+ broken imports?\b', 'Hardcoded "N broken imports" metric'),
    (r'\b\d+ syntax errors?\b', 'Hardcoded "N syntax errors" metric'),
    
    # Fake/mock indicators
    (r'\bfake\b', 'Contains "fake" - likely test data in production code'),
    (r'\bmock\b', 'Contains "mock" - likely test data in production code'),
    (r'\bstub\b', 'Contains "stub" - likely test data in production code'),
    (r'\bplaceholder\b', 'Contains "placeholder" - incomplete implementation'),
    (r'\btodo\b', 'Contains "TODO" - incomplete implementation'),
    (r'\bfixme\b', 'Contains "FIXME" - incomplete implementation'),
    (r'\bhack\b', 'Contains "hack" - workaround in production'),
    
    # Hardcoded verdicts that should come from engine
    (r'verdict.*["\']PASS["\']', 'Hardcoded PASS verdict'),
    (r'verdict.*["\']FAIL["\']', 'Hardcoded FAIL verdict'),
    (r'verdict.*["\']WARN["\']', 'Hardcoded WARN verdict'),
    
    # Hardcoded coverage states
    (r'coverage.*["\']verified["\']', 'Hardcoded verified coverage'),
    (r'coverage.*["\']partial["\']', 'Hardcoded partial coverage'),
    (r'coverage.*["\']unknown["\']', 'Hardcoded unknown coverage'),
    
    # Hardcoded digests/IDs
    (r'blake3:[a-f0-9]{64}', 'Hardcoded BLAKE3 digest'),
    (r'operation_id.*["\'][a-f0-9]{16,}["\']', 'Hardcoded operation ID'),
    (r'repository_id.*["\'][a-f0-9]{64}["\']', 'Hardcoded repository ID'),
    (r'snapshot_id.*["\'][a-f0-9]{64}["\']', 'Hardcoded snapshot ID'),
    
    # Latency metrics that should be measured
    (r'latency_ms.*:\s*\d+', 'Hardcoded latency metric'),
    (r'computation_latency_ms.*:\s*\d+', 'Hardcoded computation latency'),
]

# Files/directories to skip
SKIP_PATHS = {
    'target', 'node_modules', '.git', 'dist', 'build',
    '__pycache__', '.pytest_cache', '.coverage',
    'test', 'tests', 'spec', 'specs', 'fixtures',
    '.d.ts', '.map', '.min.js', '.min.css',
}

# File extensions to scan
SCAN_EXTENSIONS = {
    '.rs', '.ts', '.tsx', '.js', '.jsx', '.py', 
    '.json', '.toml', '.md', '.txt', '.sh', '.ps1'
}


def should_skip(path: Path) -> bool:
    """Check if path should be skipped."""
    for part in path.parts:
        if part in SKIP_PATHS:
            return True
        if part.startswith('.'):
            return True
    return False


def should_scan(path: Path) -> bool:
    """Check if file should be scanned."""
    if should_skip(path):
        return False
    return path.suffix in SCAN_EXTENSIONS


def scan_file(path: Path, patterns: List[Tuple[re.Pattern, str]]) -> List[Tuple[int, str, str]]:
    """Scan a file for forbidden patterns."""
    violations = []
    
    try:
        content = path.read_text(encoding='utf-8')
    except (UnicodeDecodeError, PermissionError):
        return violations
    
    lines = content.split('\n')
    for i, line in enumerate(lines, 1):
        for pattern, description in patterns:
            if pattern.search(line):
                violations.append((i, description, line.strip()[:120]))
    
    return violations


def main():
    parser = argparse.ArgumentParser(description='Scan for fabricated output')
    parser.add_argument('--root', default='.', help='Root directory to scan')
    parser.add_argument('--fail-on-warning', action='store_true', 
                        help='Treat warnings as errors')
    parser.add_argument('--exclude', action='append', default=[],
                        help='Additional paths to exclude')
    parser.add_argument('--verbose', '-v', action='store_true')
    args = parser.parse_args()
    
    root = Path(args.root).resolve()
    exclude_paths = {Path(e).resolve() for e in args.exclude}
    
    # Compile patterns
    patterns = [(re.compile(p, re.IGNORECASE), d) for p, d in FORBIDDEN_PATTERNS]
    
    all_violations = []
    files_scanned = 0
    
    for file_path in root.rglob('*'):
        if not file_path.is_file():
            continue
        if any(file_path.is_relative_to(e) for e in exclude_paths):
            continue
        if not should_scan(file_path):
            continue
            
        files_scanned += 1
        violations = scan_file(file_path, patterns)
        
        if violations:
            for line_num, desc, line in violations:
                rel_path = file_path.relative_to(root)
                all_violations.append((str(rel_path), line_num, desc, line))
    
    # Report results
    if all_violations:
        print(f"\n{'='*60}")
        print(f"FABRICATED OUTPUT DETECTED")
        print(f"{'='*60}")
        print(f"Files scanned: {files_scanned}")
        print(f"Violations found: {len(all_violations)}")
        print()
        
        # Group by file
        by_file = {}
        for file_path, line_num, desc, line in all_violations:
            if file_path not in by_file:
                by_file[file_path] = []
            by_file[file_path].append((line_num, desc, line))
        
        for file_path, violations in by_file.items():
            print(f"\n{file_path}:")
            for line_num, desc, line in violations:
                print(f"  Line {line_num}: {desc}")
                print(f"    {line}")
        
        print(f"\n{'='*60}")
        return 1
    else:
        print(f"✓ No fabricated output detected ({files_scanned} files scanned)")
        return 0


if __name__ == '__main__':
    sys.exit(main())