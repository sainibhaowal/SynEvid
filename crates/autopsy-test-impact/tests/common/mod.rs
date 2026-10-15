//! Shared fixtures for the Pillar 2 test suite.
//!
//! Every fixture is built in memory — no tempdirs, no subprocesses, no
//! clock reads — so the same fixture produces the same bytes on every
//! platform (`NFR-001`).
#![allow(dead_code)]

use autopsy_test_impact::{Reachability, TestDiscovery};
use std::collections::BTreeSet;

/// Builds a `TestDiscovery` from `(path, content)` pairs, sorted by path.
pub fn files(entries: &[(&str, &str)]) -> Vec<TestDiscovery> {
    let mut out: Vec<TestDiscovery> = entries
        .iter()
        .map(|(path, content)| TestDiscovery {
            path: (*path).to_string(),
            content: (*content).to_string(),
        })
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Source files
// ---------------------------------------------------------------------------

pub const UTIL_TS: &str =
    "export function add(a: number, b: number): number {\n  return a + b;\n}\n";

pub const MID_TS: &str = "import { add } from './util';\n\nexport function compute(): number {\n  return add(1, 2);\n}\n";

pub const MAIN_TS: &str =
    "import { compute } from './mid';\n\nexport function run(): number {\n  return compute();\n}\n";

pub const OTHER_TS: &str = "export function standalone(): number {\n  return 42;\n}\n";

// ---------------------------------------------------------------------------
// Test files — one per framework
// ---------------------------------------------------------------------------

/// Vitest, mapping to `src/main.ts` (two call hops away from `src/util.ts`).
pub const MAIN_TEST_TS: &str = "import { describe, it, expect } from 'vitest';\nimport { run } from './main';\n\ndescribe('main', () => {\n  it('runs', () => {\n    expect(run()).toBe(3);\n  });\n});\n";

/// Jest, mapping to `src/mid.ts` (one call hop away from `src/util.ts`).
pub const MID_TEST_TS: &str = "import { describe, it, expect } from '@jest/globals';\nimport { compute } from './mid';\n\ndescribe('mid', () => {\n  it('computes', () => {\n    expect(compute()).toBe(3);\n  });\n});\n";

/// Vitest, mapping to `src/other.ts` — the unrelated suite that must be excluded.
pub const OTHER_TEST_TS: &str = "import { describe, it, expect } from 'vitest';\nimport { standalone } from './other';\n\ndescribe('other', () => {\n  it('stands alone', () => {\n    expect(standalone()).toBe(42);\n  });\n});\n";

/// Mocha, living under `test/`, importing across a directory boundary.
pub const LEGACY_SPEC_JS: &str = "import { describe, it } from 'mocha';\nimport { standalone } from '../src/other';\n\ndescribe('legacy', () => {\n  it('works', () => {});\n});\n";

/// Mocha with an import that cannot be resolved: forces `Partial`.
pub const BROKEN_SPEC_JS: &str = "import { describe, it } from 'mocha';\nimport { helper } from './missing-helper';\n\ndescribe('broken', () => {\n  it('works', () => {});\n});\n";

/// A `__tests__` file that uses ambient globals: framework is undecidable.
pub const AMBIENT_TEST_TS: &str =
    "import { run } from '../src/main';\n\ntest('ambient', () => {\n  run();\n});\n";

/// A test file with no framework import and no declarations at all.
pub const EMPTY_TEST_TS: &str = "export {};\n";

/// A test file using a computed test name and a dynamic import.
pub const DYNAMIC_TEST_TS: &str = "import { describe } from 'vitest';\nimport { run } from './main';\n\ndescribe('dynamic', () => {\n  ['a', 'b'].forEach((name) => {\n    test(name, () => {\n      run();\n    });\n  });\n});\n";

// ---------------------------------------------------------------------------
// Composed repositories
// ---------------------------------------------------------------------------

/// Every suite is `Verified`, so the only reason to select anything is a
/// positive mapping. This is the fixture for the narrowing assertions.
pub fn clean_repo() -> Vec<TestDiscovery> {
    files(&[
        ("src/main.ts", MAIN_TS),
        ("src/main.test.ts", MAIN_TEST_TS),
        ("src/mid.ts", MID_TS),
        ("src/mid.test.ts", MID_TEST_TS),
        ("src/other.ts", OTHER_TS),
        ("src/other.test.ts", OTHER_TEST_TS),
        ("src/util.ts", UTIL_TS),
    ])
}

/// `clean_repo` plus a suite that declares an unresolvable import.
pub fn repo_with_unresolvable_import() -> Vec<TestDiscovery> {
    let mut f = clean_repo();
    f.push(TestDiscovery {
        path: "test/broken.spec.js".to_string(),
        content: BROKEN_SPEC_JS.to_string(),
    });
    f.sort();
    f
}

/// `clean_repo` plus a suite whose framework cannot be determined.
pub fn repo_with_ambient_suite() -> Vec<TestDiscovery> {
    let mut f = clean_repo();
    f.push(TestDiscovery {
        path: "src/ambient.test.ts".to_string(),
        content: AMBIENT_TEST_TS.to_string(),
    });
    f.sort();
    f
}

// ---------------------------------------------------------------------------
// Graph oracle
// ---------------------------------------------------------------------------

/// The `clean_repo` **symbol-level** call graph: `run -> compute -> add`.
pub fn symbol_graph() -> SimpleGraph {
    let mut g = SimpleGraph::new();
    g.add_edge("src/main.ts::run", "src/mid.ts::compute");
    g.add_edge("src/mid.ts::compute", "src/util.ts::add");
    g
}

/// The same dependency expressed at **file** granularity.
///
/// Kept alongside the symbol graph so tests can assert that both granularities
/// yield the same affected set — a divergence would mean a change could hide
/// from selection depending on which layer reported it.
pub fn file_graph() -> SimpleGraph {
    let mut g = SimpleGraph::new();
    g.add_edge("src/mid.ts", "src/util.ts");
    g.add_edge("src/main.ts", "src/mid.ts");
    g
}

/// A change set for the regression in `add()`, carrying both granularities.
pub fn change_add() -> autopsy_test_impact::ChangeSetSummary {
    let mut cs =
        autopsy_test_impact::ChangeSetSummary::from_verified_files(vec!["src/util.ts".to_string()]);
    cs.changed_symbols.insert("src/util.ts::add".to_string());
    cs
}

/// A caller graph over entity names.
#[derive(Debug, Clone, Default)]
pub struct SimpleGraph {
    callers: std::collections::BTreeMap<String, BTreeSet<String>>,
}

impl SimpleGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_edge(&mut self, caller: &str, callee: &str) {
        self.callers
            .entry(callee.to_string())
            .or_default()
            .insert(caller.to_string());
    }
}

impl Reachability for SimpleGraph {
    fn reverse_reachable(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut seen = seeds.clone();
        let mut stack: Vec<String> = seeds.iter().cloned().collect();
        while let Some(n) = stack.pop() {
            if let Some(callers) = self.callers.get(&n) {
                for c in callers {
                    if seen.insert(c.clone()) {
                        stack.push(c.clone());
                    }
                }
            }
        }
        seen
    }
}

// ---------------------------------------------------------------------------
// LCOV fixtures
// ---------------------------------------------------------------------------

/// What `src/main.test.ts` observed: both `src/main.ts` and `src/mid.ts`.
pub const LCOV_MAIN_SUITE: &str = "SF:src/main.ts\nFN:4,run\nDA:1,1\nDA:4,1\nLF:2\nLH:2\nend_of_record\nSF:src/mid.ts\nDA:1,1\nend_of_record\n";

/// What `src/mid.test.ts` observed: only `src/mid.ts`.
pub const LCOV_MID_SUITE: &str = "SF:src/mid.ts\nDA:4,1\nend_of_record\n";

/// A tracefile whose every recorded line has zero hits.
pub const LCOV_ZERO_HITS: &str = "SF:src/mid.ts\nDA:1,0\nDA:4,0\nLF:2\nLH:0\nend_of_record\n";

/// Instrumented, but no line records at all.
pub const LCOV_NO_LINE_DATA: &str = "SF:src/mid.ts\nLF:0\nLH:0\nend_of_record\n";
