//! Test inventory discovery and manifest mapping (Step 33).
//!
//! Discovers Vitest, Jest, and Mocha test suites from a repository file set,
//! extracts their declared test cases, and maps each suite to the **source
//! files it declares imports from**.
//!
//! # Why a lexical scan, and what it costs
//! This crate sits below the language adapters in the dependency DAG
//! (`TestImpact -> Domain, Graph, Impact`), so it does not import the
//! TypeScript parser. Test files are scanned lexically for `import` forms and
//! `describe`/`it`/`test` declarations.
//!
//! The consequence is stated plainly rather than hidden behind a confidence
//! score: constructs the scanner cannot interpret — dynamic `require()`,
//! template-literal specifiers, computed test names, conditional imports —
//! degrade the suite's [`CoverageState`] to `Partial` or `Unknown`. Step 35
//! turns any non-`Verified` mapping into an explicit run-everything fallback,
//! so an honest `Unknown` costs correctness nothing.
//!
//! # Determinism
//! Suites, test cases, frameworks, and imports are all ordered. Two scans of
//! the same file set produce byte-identical inventories.

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A single input file offered to discovery.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TestDiscovery {
    /// POSIX-normalized repository-relative path.
    pub path: String,
    /// Raw UTF-8 content. Non-UTF-8 files must be excluded by the caller.
    pub content: String,
}

/// Recognized test frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestFramework {
    Vitest,
    Jest,
    Mocha,
}

impl TestFramework {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vitest => "vitest",
            Self::Jest => "jest",
            Self::Mocha => "mocha",
        }
    }
}

/// Kind of declaration found in a test file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestCaseKind {
    Describe,
    Test,
    Hook,
}

/// A declared test case or suite block.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TestCase {
    /// Literal name, or `None` when computed dynamically.
    pub name: Option<String>,
    pub kind: TestCaseKind,
    /// 1-based line of the declaration.
    pub line: u32,
}

impl TestCase {
    /// Stable identity for a test case within its file.
    ///
    /// Falls back to a positional id when the name is computed, so dynamic
    /// cases are still addressable without inventing a name for them.
    pub fn case_id(&self, suite_path: &str) -> String {
        match &self.name {
            Some(n) => format!("{suite_path}::{}::{n}", self.kind_str()),
            None => format!("{suite_path}::{}::@line{}", self.kind_str(), self.line),
        }
    }

    fn kind_str(&self) -> &'static str {
        match self.kind {
            TestCaseKind::Describe => "describe",
            TestCaseKind::Test => "test",
            TestCaseKind::Hook => "hook",
        }
    }
}

/// An import declaration found in a test file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImportSpec {
    /// The literal specifier as written, e.g. `./util` or `vitest`.
    pub specifier: String,
    /// Whether the specifier is relative (and therefore mappable to a repo path).
    pub is_relative: bool,
    /// 1-based line of the import.
    pub line: u32,
}

/// One discovered test suite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite identity: the file path.
    pub suite_id: String,
    pub path: String,
    /// Every framework the file's imports indicate, sorted.
    pub frameworks: Vec<TestFramework>,
    /// Declared test cases, in file order.
    pub test_cases: Vec<TestCase>,
    /// Resolved in-repository source paths this suite imports, sorted.
    pub mapped_sources: Vec<String>,
    /// Relative specifiers that could **not** be resolved to a repo file.
    /// These are the reason a suite degrades below `Verified`.
    pub unresolved_specifiers: Vec<String>,
    /// Honest coverage of the mapping.
    pub coverage: CoverageState,
    pub provenance: String,
}

impl TestSuite {
    /// BLAKE3 over the suite's canonical content.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.suite_id.as_bytes());
        hasher.update(b"\n");
        for f in &self.frameworks {
            hasher.update(f.as_str().as_bytes());
            hasher.update(b",");
        }
        hasher.update(b"\n");
        for s in &self.mapped_sources {
            hasher.update(s.as_bytes());
            hasher.update(b"\n");
        }
        for s in &self.unresolved_specifiers {
            hasher.update(b"UNRESOLVED:");
            hasher.update(s.as_bytes());
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// The full discovered inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestInventory {
    /// Suite path -> suite.
    pub suites: BTreeMap<String, TestSuite>,
    /// Every repository file path known to the scan, used for import
    /// resolution. Sorted.
    pub known_files: BTreeSet<String>,
    /// Aggregate coverage across suites.
    pub coverage: CoverageState,
}

impl TestInventory {
    /// Number of discovered suites.
    pub fn len(&self) -> usize {
        self.suites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.suites.is_empty()
    }

    /// Total declared test cases across all suites.
    pub fn test_case_count(&self) -> usize {
        self.suites.values().map(|s| s.test_cases.len()).sum()
    }

    /// Suites that import a given source path.
    pub fn suites_importing(&self, source_path: &str) -> Vec<&TestSuite> {
        self.suites
            .values()
            .filter(|s| s.mapped_sources.iter().any(|m| m == source_path))
            .collect()
    }

    /// BLAKE3 over the whole inventory's canonical content.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for (path, suite) in &self.suites {
            hasher.update(path.as_bytes());
            hasher.update(b"\x00");
            hasher.update(suite.digest().as_bytes());
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// Source file extensions the resolver will try when a specifier has none.
const RESOLVE_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];

/// True when a path looks like a test file.
///
/// Recognizes `*.test.*`, `*.spec.*`, and files under a `__tests__`,
/// `test`, or `tests` directory. Matching is on path segments only, so
/// `src/latest.ts` is not mistaken for a test.
pub fn is_test_file(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let Some(file_name) = segments.last() else {
        return false;
    };

    // Directory-based detection.
    if segments[..segments.len() - 1]
        .iter()
        .any(|s| matches!(*s, "__tests__" | "test" | "tests" | "__test__"))
    {
        return true;
    }

    // Filename-based detection: requires a stem before the marker.
    for marker in [".test.", ".spec."] {
        if let Some(idx) = file_name.find(marker)
            && idx > 0
            && file_name[idx + marker.len()..]
                .split('.')
                .next()
                .is_some_and(|ext| matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"))
        {
            return true;
        }
    }

    false
}

/// Builds a full inventory from a repository file set.
///
/// `files` must contain **every** file in the repository, not only tests:
/// import resolution needs the complete path set to tell a resolvable
/// specifier from an unresolvable one.
pub fn discover_inventory(files: &[TestDiscovery]) -> TestInventory {
    let known_files: BTreeSet<String> = files.iter().map(|f| f.path.clone()).collect();

    let mut suites = BTreeMap::new();
    for file in files {
        if !is_test_file(&file.path) {
            continue;
        }
        let suite = analyze_suite(&file.path, &file.content, &known_files);
        suites.insert(file.path.clone(), suite);
    }

    let coverage = aggregate_coverage(suites.values().map(|s| s.coverage));
    TestInventory {
        suites,
        known_files,
        coverage,
    }
}

/// Worst-case reduction over coverage states.
///
/// Never upgrades: a single `Unknown` suite makes the inventory `Unknown`.
fn aggregate_coverage(states: impl Iterator<Item = CoverageState>) -> CoverageState {
    let mut worst = CoverageState::Verified;
    let mut any = false;
    for s in states {
        any = true;
        worst = match (worst, s) {
            (CoverageState::Unknown, _) | (_, CoverageState::Unknown) => CoverageState::Unknown,
            (CoverageState::Unsupported, _) | (_, CoverageState::Unsupported) => {
                CoverageState::Unsupported
            }
            (CoverageState::Partial, _) | (_, CoverageState::Partial) => CoverageState::Partial,
            _ => CoverageState::Verified,
        };
    }
    if any { worst } else { CoverageState::Verified }
}

/// Analyzes one test file into a suite record.
fn analyze_suite(path: &str, content: &str, known_files: &BTreeSet<String>) -> TestSuite {
    let imports = scan_imports(content);
    let test_cases = scan_test_cases(content);

    let mut frameworks: BTreeSet<TestFramework> = BTreeSet::new();
    for imp in &imports {
        if let Some(fw) = framework_for_specifier(&imp.specifier) {
            frameworks.insert(fw);
        }
    }

    // Resolve relative specifiers to repository paths.
    let mut mapped: BTreeSet<String> = BTreeSet::new();
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    for imp in &imports {
        if !imp.is_relative {
            continue;
        }
        match resolve_relative(path, &imp.specifier, known_files) {
            Some(resolved) => {
                mapped.insert(resolved);
            }
            None => {
                unresolved.insert(imp.specifier.clone());
            }
        }
    }

    // Framework attribution when nothing was imported but test declarations
    // exist: the file uses ambient globals. Which framework provides them is
    // genuinely undecidable from this file alone.
    let mut framework_unknown = false;
    if frameworks.is_empty() && !test_cases.is_empty() {
        framework_unknown = true;
    }

    let coverage = if framework_unknown || !unresolved.is_empty() {
        CoverageState::Partial
    } else if frameworks.is_empty() && test_cases.is_empty() {
        // A test file with neither a framework import nor any recognizable
        // declaration: we cannot claim to have understood it.
        CoverageState::Unknown
    } else {
        CoverageState::Verified
    };

    TestSuite {
        suite_id: path.to_string(),
        path: path.to_string(),
        frameworks: frameworks.into_iter().collect(),
        test_cases,
        mapped_sources: mapped.into_iter().collect(),
        unresolved_specifiers: unresolved.into_iter().collect(),
        coverage,
        provenance: "lexical:ts-js-test-scan".to_string(),
    }
}

/// Maps a bare specifier to a known framework import.
fn framework_for_specifier(specifier: &str) -> Option<TestFramework> {
    let root = specifier.split('/').next().unwrap_or(specifier);
    match root {
        "vitest" => Some(TestFramework::Vitest),
        "jest" | "@jest" => Some(TestFramework::Jest),
        "mocha" => Some(TestFramework::Mocha),
        _ => None,
    }
}

/// Resolves a relative specifier against the repository file set.
///
/// Tries the literal path, each known extension, and `index.<ext>` under a
/// directory. Returns `None` when nothing matches, which the caller records as
/// an unresolved specifier rather than silently ignoring.
fn resolve_relative(
    from_path: &str,
    specifier: &str,
    known_files: &BTreeSet<String>,
) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }

    // Normalize `from_path`'s directory with the specifier, rejecting escapes.
    let mut segments: Vec<&str> = Vec::new();
    if let Some((dir, _file)) = from_path.rsplit_once('/') {
        for seg in dir.split('/') {
            if !seg.is_empty() {
                segments.push(seg);
            }
        }
    }

    for seg in specifier.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                // Escaping the repository root is never resolvable.
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }

    if segments.is_empty() {
        return None;
    }
    let base = segments.join("/");

    // Exact match, then extension probing, then directory index.
    if known_files.contains(&base) {
        return Some(base);
    }
    for ext in RESOLVE_EXTENSIONS {
        let candidate = format!("{base}{ext}");
        if known_files.contains(&candidate) {
            return Some(candidate);
        }
    }
    for ext in RESOLVE_EXTENSIONS {
        let candidate = format!("{base}/index{ext}");
        if known_files.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Extracts import specifiers from TypeScript/JavaScript source lexically.
///
/// Recognizes:
/// * `import ... from 'spec'`
/// * `import 'spec'`
/// * `export ... from 'spec'`
///
/// Dynamic `import(...)`, `require(...)`, and template-literal specifiers are
/// deliberately **not** guessed at; they leave the suite `Partial` through the
/// caller's handling of unresolved imports.
fn scan_imports(content: &str) -> Vec<ImportSpec> {
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        let is_import = trimmed.starts_with("import ") || trimmed.starts_with("import{");
        let is_export_from = trimmed.starts_with("export ") && trimmed.contains(" from ");
        if !is_import && !is_export_from {
            continue;
        }
        // The specifier is the last quoted literal on the line.
        if let Some(spec) = last_quoted_literal(line) {
            out.push(ImportSpec {
                is_relative: spec.starts_with('.'),
                specifier: spec,
                line: (idx + 1) as u32,
            });
        }
    }
    out
}

/// Returns the last single- or double-quoted literal on a line.
fn last_quoted_literal(line: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let quote = bytes[i];
        if quote == b'\'' || quote == b'"' || quote == b'`' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != quote {
                j += 1;
            }
            if j < bytes.len() {
                let literal = &line[start..j];
                // A backtick literal containing `${` is a computed specifier.
                let computed = quote == b'`' && literal.contains("${");
                if !computed && !literal.is_empty() {
                    best = Some((i, literal.to_string()));
                }
                i = j + 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    best.map(|(_, s)| s)
}

/// Declaration keywords scanned for test cases, in match-precedence order.
const TEST_DECLARATIONS: &[(&str, TestCaseKind)] = &[
    ("describe", TestCaseKind::Describe),
    ("it", TestCaseKind::Test),
    ("test", TestCaseKind::Test),
    ("beforeAll", TestCaseKind::Hook),
    ("afterAll", TestCaseKind::Hook),
    ("beforeEach", TestCaseKind::Hook),
    ("afterEach", TestCaseKind::Hook),
];

/// Extracts test declarations lexically, preserving file order.
fn scan_test_cases(content: &str) -> Vec<TestCase> {
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        // Skip comment lines so `// describe(...)` is not counted.
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        for (kw, kind) in TEST_DECLARATIONS {
            if let Some(rest) = call_after_keyword(trimmed, kw) {
                out.push(TestCase {
                    name: first_argument_literal(rest),
                    kind: *kind,
                    line: (idx + 1) as u32,
                });
                break;
            }
        }
    }
    out
}

/// Matches `<keyword>(` at the start of a statement and returns the arguments.
fn call_after_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    // Must be an actual call, and not a longer identifier such as `item(`.
    let rest = rest.strip_prefix('(').or_else(|| {
        let r = rest.trim_start();
        r.strip_prefix('(')
    })?;
    Some(rest)
}

/// Reads the first argument when it is a plain quoted literal.
///
/// Returns `None` for computed names, which keeps the case addressable by
/// position instead of inventing a name.
fn first_argument_literal(args: &str) -> Option<String> {
    let args = args.trim_start();
    let quote = args.as_bytes().first().copied()?;
    if quote != b'\'' && quote != b'"' && quote != b'`' {
        return None;
    }
    let body = &args[1..];
    let end = body.find(quote as char)?;
    let literal = &body[..end];
    if literal.contains("${") {
        return None;
    }
    Some(literal.to_string())
}
