//! Observed coverage ingestion (Step 34).
//!
//! Ingests LCOV tracefiles — the common denominator emitted by Vitest, Jest,
//! c8, and nyc — and links observed line executions to concrete source
//! locations, so Step 35 can select tests from runtime evidence in addition to
//! static imports.
//!
//! # Uninstrumented noise
//! A tracefile records only files that were instrumented. A source file
//! absent from every tracefile is **not** evidence that it is untested; it is
//! evidence that nothing was recorded. The parser therefore distinguishes
//! "zero hits" from "no record", and the matrix exposes both, because Step 35
//! must treat missing coverage as `Unknown` and fall back rather than assume a
//! narrow selection is safe.

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LcovError {
    #[error("line {line}: malformed record `{text}`")]
    Malformed { line: usize, text: String },
    #[error("line {line}: `DA` record requires `<line>,<hits>`")]
    BadDaRecord { line: usize },
    #[error("tracefile has no `SF` record before `end_of_record`")]
    RecordWithoutSource,
}

/// One observed line execution.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LineHit {
    /// Repository-relative POSIX path as written in the `SF` record.
    pub path: String,
    /// 1-based source line.
    pub line: u32,
    /// Execution count. Zero means "instrumented but never executed".
    pub hits: u64,
}

/// Observed coverage for one test suite.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SuiteCoverage {
    /// Sorted hit records.
    pub hits: BTreeSet<LineHit>,
    /// Files present in the tracefile, whether or not any line executed.
    pub instrumented_files: BTreeSet<String>,
}

impl SuiteCoverage {
    /// Files with at least one executed line.
    pub fn executed_files(&self) -> BTreeSet<&str> {
        self.hits
            .iter()
            .filter(|h| h.hits > 0)
            .map(|h| h.path.as_str())
            .filter(|p| !p.is_empty())
            .collect()
    }

    /// True when the suite executed at least one line in `path`.
    pub fn executed_in(&self, path: &str) -> bool {
        self.hits.iter().any(|h| h.hits > 0 && h.path == path)
    }
}

/// Observed coverage across all ingested suites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageMatrix {
    /// Suite id -> observed coverage.
    pub suites: BTreeMap<String, SuiteCoverage>,
    /// Aggregate honesty of the ingestion.
    pub coverage: CoverageState,
    /// Machine-readable reasons the aggregate is not `Verified`.
    pub notes: Vec<String>,
}

impl Default for CoverageMatrix {
    /// An empty matrix is `Unknown`, never `Verified`: no data is not the same
    /// as good data (`FR-014`, `FR-030`).
    fn default() -> Self {
        Self {
            suites: BTreeMap::new(),
            coverage: CoverageState::Unknown,
            notes: Vec::new(),
        }
    }
}

impl CoverageMatrix {
    /// True when no coverage was ingested at all.
    pub fn is_empty(&self) -> bool {
        self.suites.is_empty()
    }
    /// Suites that executed at least one line in the given file.
    pub fn suites_touching(&self, path: &str) -> Vec<&str> {
        self.suites
            .iter()
            .filter(|(_, c)| c.executed_in(path))
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// All files observed as instrumented anywhere.
    pub fn instrumented_files(&self) -> BTreeSet<&str> {
        self.suites
            .values()
            .flat_map(|c| c.instrumented_files.iter().map(String::as_str))
            .collect()
    }

    /// BLAKE3 over the canonical matrix body.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for (suite, cov) in &self.suites {
            hasher.update(suite.as_bytes());
            hasher.update(b"\x00");
            for hit in &cov.hits {
                hasher.update(hit.path.as_bytes());
                hasher.update(b":");
                hasher.update(hit.line.to_string().as_bytes());
                hasher.update(b":");
                hasher.update(hit.hits.to_string().as_bytes());
                hasher.update(b"\n");
            }
            hasher.update(b"\x01");
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// Parses one LCOV tracefile for a given suite.
///
/// Recognized records: `SF`, `DA`, `end_of_record`. Every other record type
/// (`FN`, `FNDA`, `BRDA`, `LH`, `LF`, `TN`, ...) is ignored rather than
/// guessed at — function and branch records are not needed for line-level
/// selection, and ignoring them cannot produce a false negative because
/// Step 35 unions static imports with observed coverage.
pub fn parse_lcov(suite_id: &str, content: &str) -> Result<CoverageMatrix, LcovError> {
    let mut hits: BTreeSet<LineHit> = BTreeSet::new();
    let mut instrumented: BTreeSet<String> = BTreeSet::new();
    let mut current_source: Option<String> = None;
    let mut notes: Vec<String> = Vec::new();

    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(path) = line.strip_prefix("SF:") {
            let path = path.trim();
            if path.is_empty() {
                return Err(LcovError::Malformed {
                    line: line_no,
                    text: raw.to_string(),
                });
            }
            current_source = Some(path.to_string());
            instrumented.insert(path.to_string());
        } else if let Some(rest) = line.strip_prefix("DA:") {
            let source = current_source
                .clone()
                .ok_or(LcovError::RecordWithoutSource)?;
            let (line_str, hits_str) = rest
                .split_once(',')
                .ok_or(LcovError::BadDaRecord { line: line_no })?;
            // A checksum may follow a second comma; it is not needed.
            let hits_str = hits_str.split(',').next().unwrap_or(hits_str);
            let src_line: u32 = line_str
                .trim()
                .parse()
                .map_err(|_| LcovError::BadDaRecord { line: line_no })?;
            let count: u64 = hits_str
                .trim()
                .parse()
                .map_err(|_| LcovError::BadDaRecord { line: line_no })?;
            hits.insert(LineHit {
                path: source,
                line: src_line,
                hits: count,
            });
        } else if line == "end_of_record" {
            current_source = None;
        }
    }

    // `malformed` is deliberately absent: every record that cannot be
    // interpreted returns an error instead of being skipped. Silently dropping
    // a record would mean reporting `Verified` over data the parser did not
    // actually read.
    let coverage = if instrumented.is_empty() {
        notes.push(format!(
            "tracefile for suite `{suite_id}` contained no SF records"
        ));
        CoverageState::Unknown
    } else if hits.is_empty() {
        notes.push(format!(
            "tracefile for suite `{suite_id}` instrumented {} file(s) but recorded no DA lines",
            instrumented.len()
        ));
        CoverageState::Partial
    } else {
        CoverageState::Verified
    };

    let mut suites = BTreeMap::new();
    suites.insert(
        suite_id.to_string(),
        SuiteCoverage {
            hits,
            instrumented_files: instrumented,
        },
    );

    Ok(CoverageMatrix {
        suites,
        coverage,
        notes,
    })
}

/// Merges several per-suite tracefiles into one matrix.
///
/// Aggregate coverage is the worst case across inputs: one unparseable or
/// empty tracefile is enough to make the whole matrix non-`Verified`.
pub fn merge_matrices(matrices: Vec<CoverageMatrix>) -> CoverageMatrix {
    let mut suites: BTreeMap<String, SuiteCoverage> = BTreeMap::new();
    let mut notes: Vec<String> = Vec::new();
    let mut worst = CoverageState::Verified;
    let mut count = 0usize;

    for matrix in matrices {
        count += 1;
        notes.extend(matrix.notes);
        worst = worse_of(worst, matrix.coverage);
        for (suite, cov) in matrix.suites {
            let entry = suites.entry(suite).or_default();
            entry.hits.extend(cov.hits);
            entry.instrumented_files.extend(cov.instrumented_files);
        }
    }

    if count == 0 {
        notes.push("no coverage tracefiles were ingested".to_string());
        worst = CoverageState::Unknown;
    }

    notes.sort();
    notes.dedup();

    CoverageMatrix {
        suites,
        coverage: worst,
        notes,
    }
}

/// Returns the less trustworthy of two coverage states.
fn worse_of(a: CoverageState, b: CoverageState) -> CoverageState {
    use CoverageState::*;
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Partial, _) | (_, Partial) => Partial,
        _ => Verified,
    }
}
