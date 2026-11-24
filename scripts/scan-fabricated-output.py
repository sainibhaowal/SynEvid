#!/usr/bin/env python3
"""
Scan for fabricated/hardcoded operational output in source code.
This script detects hardcoded metrics, fake counts, canned results, etc.
that would violate Gate G7 (no fabricated output).
"""

import os
import re
import sys
from pathlib import Path
from typing import List, Tuple

# Patterns that indicate fabricated/hardcoded operational output
FORBIDDEN_PATTERNS = [
    # Hardcoded counts/metrics
    (r'\b\d+\s+affected\s+callers?\b', "Hardcoded affected caller count"),
    (r'\b14\s+affected\s+callers?\b', "Specific hardcoded count '14 affected callers'"),
    (r'\b\d+\s+callers?\s+found\b', "Hardcoded caller count"),
    (r'\b\d+\s+symbols?\s+analyzed\b', "Hardcoded symbol count"),
    (r'\b\d+\s+files?\s+changed\b', "Hardcoded file change count"),
    (r'\b\d+\s+errors?\s+found\b', "Hardcoded error count"),
    (r'\b\d+\s+warnings?\b', "Hardcoded warning count"),
    
    # Fake latency/timing
    (r'\b\d+\.?\d*\s*ms\s+latency\b', "Hardcoded latency value"),
    (r'\b\d+\.?\d*\s*ms\s+p95\b', "Hardcoded p95 latency"),
    (r'\b\d+\.?\d*\s*ms\s+p99\b', "Hardcoded p99 latency"),
    
    # Fake percentages
    (r'\b100%\s+coverage\b', "Hardcoded 100% coverage"),
    (r'\b\d+%\s+cache\s+hit\b', "Hardcoded cache hit percentage"),
    
    # Canned success/failure messages
    (r'"success":\s*true\s*(?!.*[\[{])', "Hardcoded success true without dynamic data"),
    (r'"verdict":\s*"PASS"\s*(?!.*[\[{])', "Hardcoded PASS verdict without evidence"),
    (r'"verdict":\s*"FAIL"\s*(?!.*[\[{])', "Hardcoded FAIL verdict without evidence"),
    
    # Mock/fake indicators in source code (not tests)
    (r'\bfake\b', "Contains 'fake' in production code"),
    (r'\bmock\b', "Contains 'mock' in production code"),
    (r'\bstub\b', "Contains 'stub' in production code"),
    (r'\bhardcoded\b', "Contains 'hardcoded' in production code"),
    (r'\bTODO\b', "TODO in production code"),
    (r'\bFIXME\b', "FIXME in production code"),
    (r'\bXXX\b', "XXX marker in production code"),
    
    # Placeholder values
    (r'"placeholder"', "Placeholder string in production code"),
    (r'"example"', "Example value in production code"),
    (r'"test"', "Test value in production code"),
    (r'"dummy"', "Dummy value in production code"),
    
    # Suspicious static returns
    (r'return\s+(?:true|false|0|1|null|undefined|"")\s*;', "Static return without computation"),
    (r'return\s+\[\]\s*;', "Empty array return"),
    (r'return\s+\{\}\s*;', "Empty object return"),
]

# Files/directories to exclude from scanning
EXCLUDE_DIRS = {
    'target', 'node_modules', '.git', 'dist', 'build', 'dist-newstyle',
    '__pycache__', '.pytest_cache', '.mypy_cache', 'coverage', 'docs',
    'examples', 'fixtures', 'testdata', 'snapshots', 'playwright-report'
}

EXCLUDE_FILES = {
    'Cargo.lock', 'package-lock.json', 'pnpm-lock.yaml', 'yarn.lock',
    'CHANGELOG.md', 'README.md', 'LICENSE', 'CONTRIBUTING.md',
    'CODEOWNERS', 'SECURITY.md', 'go.sum', 'go.mod'
}

EXCLUDE_PATTERNS = [
    r'.*/tests/.*\.rs$',           # Test files
    r'.*/tests/.*\.ts$',           # Test files
    r'.*/tests/.*\.py$',           # Test files
    r'.*/tests/.*\.js$',           # Test files
    r'.*/test/.*\.rs$',            # Test files
    r'.*/test/.*\.ts$',            # Test files
    r'.*/test/.*\.py$',            # Test files
    r'.*/test/.*\.js$',            # Test files
    r'.*_test\.rs$',               # Rust test files
    r'.*_test\.py$',               # Python test files
    r'.*_test\.ts$',               # TypeScript test files
    r'.*_test\.js$',               # JS test files
    r'.*\.test\.rs$',              # Rust test files
    r'.*\.test\.ts$',              # TypeScript test files
    r'.*\.spec\.rs$',              # Spec files
    r'.*\.spec\.ts$',              # Spec files
    r'.*/fixtures/.*',             # Fixture files
    r'.*/testdata/.*',             # Test data files
    r'.*/snapshots/.*',            # Snapshot files
    r'.*\.snap$',                  # Snapshot files
    r'.*/examples/.*',             # Example files
    r'.*/scripts/.*',              # Script files
]

def should_exclude(path: Path) -> bool:
    """Check if a path should be excluded from scanning."""
    # Check directory exclusions
    for part in path.parts:
        if part in EXCLUDE_DIRS:
            return True
    
    # Check file exclusions
    if path.name in EXCLUDE_FILES:
        return True
    
    # Check pattern exclusions
    path_str = str(path)
    for pattern in EXCLUDE_PATTERNS:
        if re.search(pattern, path_str):
            return True
    
    return False

def scan_file(filepath: Path) -> List[Tuple[int, str, str]]:
    """Scan a single file for forbidden patterns."""
    violations = []
    try:
        content = filepath.read_text(encoding='utf-8', errors='ignore')
    except Exception:
        return violations
    
    lines = content.split('\n')
    for line_num, line in enumerate(lines, 1):
        for pattern, description in FORBIDDEN_PATTERNS:
            if re.search(pattern, line, re.IGNORECASE):
                violations.append((line_num, line.strip(), description))
    
    return violations

def main():
    repo_root = Path(__file__).parent.parent
    print(f"Scanning for fabricated output in: {repo_root}")
    
    all_violations = []
    files_scanned = 0
    
    # Scan Rust files
    for ext in ['*.rs', '*.ts', '*.tsx', '*.py', '*.js', '*.jsx', '*.go', '*.java', '*.cs', '*.cpp', '*.c', '*.h']:
        for filepath in repo_root.rglob(ext):
            if should_exclude(filepath):
                continue
            
            violations = scan_file(filepath)
            if violations:
                all_violations.append((filepath, violations))
            files_scanned += 1
    
    print(f"\nFiles scanned: {files_scanned}")
    print(f"Files with violations: {len(all_violations)}")
    
    if all_violations:
        print("\n=== VIOLATIONS FOUND ===")
        for filepath, violations in all_violations:
            rel_path = filepath.relative_to(repo_root)
            print(f"\n{rel_path}:")
            for line_num, line, description in violations:
                print(f"  Line {line_num}: {description}")
                print(f"    {line[:120]}")
        print(f"\nTotal violations: {sum(len(v) for _, v in all_violations)}")
        return 1
    else:
        print("\n✓ No fabricated output detected.")
        return 0

if __name__ == '__main__':
    sys.exit(main())