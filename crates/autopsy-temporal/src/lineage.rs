//! Symbol lineage across moves and renames (Step 31).
//!
//! The promotion gate for Pillar 1 is *"zero false-positive renames"*. The
//! design therefore accepts a rename only on **exact content equality with a
//! unique counterpart**, and downgrades everything else to an explicit
//! [`CoverageState::Partial`] with the candidate set recorded. An arbitrary
//! rename edge is never invented (`01_4.2_PILLARS_SYSTEM_DESIGN.md` §1.3).
//!
//! # Confidence ladder
//! | Condition                                    | Outcome                       |
//! |----------------------------------------------|-------------------------------|
//! | content digest equal, one source & one target | `Renamed` / `Moved`, `Verified` |
//! | content digest equal, several counterparts    | `Partial` + candidate list     |
//! | token similarity above threshold, unique best | `Partial` + candidate list     |
//! | otherwise                                     | `Added` / `Removed`            |

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Default token-similarity floor for proposing a *candidate* rename.
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.85;

/// Minimum margin by which the best candidate must beat the runner-up before
/// a similarity-based candidate is even reported as partial.
pub const DEFAULT_SIMILARITY_MARGIN: f64 = 0.05;

/// Kind of historical identity transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageEventKind {
    /// Path changed, symbol identity survives.
    Renamed,
    /// Symbol relocated between files.
    Moved,
    /// Same path, changed content.
    Modified,
    Added,
    Removed,
}

impl LineageEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Renamed => "renamed",
            Self::Moved => "moved",
            Self::Modified => "modified",
            Self::Added => "added",
            Self::Removed => "removed",
        }
    }
}

/// Why a lineage assertion could not be made with full confidence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ambiguity {
    /// Sorted candidate counterparts, excluding the chosen one when one exists.
    pub candidates: Vec<String>,
    /// Human-readable explanation, stable across runs.
    pub reason: String,
}

/// One historical identity transition.
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct LineageEvent {
    pub kind: LineageEventKind,
    /// Canonical path before the transition; `None` for additions.
    pub from: Option<String>,
    /// Canonical path after the transition; `None` for removals.
    pub to: Option<String>,
    /// Similarity in `0.0..=1.0`; `1.0` when content digests are equal.
    pub similarity: f64,
    pub coverage: CoverageState,
    /// Present whenever `coverage` is not `Verified`.
    pub ambiguity: Option<Ambiguity>,
}

impl LineageEvent {
    /// True when identity is asserted with full confidence.
    pub fn is_confident(&self) -> bool {
        self.coverage == CoverageState::Verified
    }

    /// A confident rename or move: the only case where identity is preserved.
    pub fn preserves_identity(&self) -> bool {
        self.is_confident()
            && matches!(
                self.kind,
                LineageEventKind::Renamed | LineageEventKind::Moved
            )
    }
}

// `PartialEq` on f64 forces a manual `Eq`; equality is only used for tests
// and set membership, where NaN never occurs (similarity is clamped).
impl Eq for LineageEvent {}

/// A file as seen at one revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// POSIX-normalized repository-relative path.
    pub path: String,
    /// Content digest (BLAKE3 hex) of the blob.
    pub content_digest: String,
    /// Normalized token signature used for similarity scoring.
    pub tokens: Vec<String>,
}

impl FileEntry {
    /// Builds an entry, deriving tokens from raw content.
    pub fn new(path: impl Into<String>, content: &[u8]) -> Self {
        let path = path.into();
        Self {
            content_digest: blake3::hash(content).to_hex().to_string(),
            tokens: tokenize(content),
            path,
        }
    }

    /// A set of this entry's tokens.
    fn token_set(&self) -> BTreeSet<&str> {
        self.tokens.iter().map(String::as_str).collect()
    }
}

/// Splits content into a deterministic, normalized token sequence.
///
/// Normalization keeps only ASCII-alphanumeric runs and underscores, folds to
/// lowercase, and **sorts then deduplicates**. Sorting makes the signature
/// insensitive to reordering, so moving a function within a file does not
/// change the signature. This is a syntactic proxy for AST similarity, not a
/// parse; its limits are why similarity matches never reach `Verified`.
pub fn tokenize(content: &[u8]) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();
    for &b in content {
        let c = b as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            current.push(c.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.insert(current);
    }
    tokens.into_iter().collect()
}

/// Jaccard similarity of two token sets, in `0.0..=1.0`.
///
/// Returns `0.0` when both sets are empty, so an empty/empty pair is never
/// mistaken for an exact interface match.
pub fn jaccard(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let sa: BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let sb: BTreeSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Tuning for lineage detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineageConfig {
    pub similarity_threshold: f64,
    pub similarity_margin: f64,
}

impl Default for LineageConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            similarity_margin: DEFAULT_SIMILARITY_MARGIN,
        }
    }
}

/// Detects identity transitions between two revisions of a file set.
///
/// Deterministic: identical inputs always produce an identically ordered
/// `Vec`, because every group and candidate list is sorted by path.
///
/// # Guarantees
/// * A `Renamed`/`Moved` event with `CoverageState::Verified` implies the two
///   paths had **byte-identical content digests** and were each other's only
///   counterpart.
/// * Any uncertainty yields `CoverageState::Partial` with a populated
///   [`Ambiguity`], never a silent guess.
pub fn detect_lineage(
    before: &[FileEntry],
    after: &[FileEntry],
    config: &LineageConfig,
) -> Vec<LineageEvent> {
    let before_map: BTreeMap<&str, &FileEntry> =
        before.iter().map(|e| (e.path.as_str(), e)).collect();
    let after_map: BTreeMap<&str, &FileEntry> =
        after.iter().map(|e| (e.path.as_str(), e)).collect();

    let mut events = Vec::new();

    // 1. Exact-digest matching, the only path to `Verified`.
    let mut digest_groups: BTreeMap<&str, (Vec<&str>, Vec<&str>)> = BTreeMap::new();
    for (path, entry) in &before_map {
        if !after_map.contains_key(path) {
            digest_groups
                .entry(entry.content_digest.as_str())
                .or_default()
                .0
                .push(path);
        }
    }
    for (path, entry) in &after_map {
        if !before_map.contains_key(path) {
            digest_groups
                .entry(entry.content_digest.as_str())
                .or_default()
                .1
                .push(path);
        }
    }

    let mut consumed_before: BTreeSet<&str> = BTreeSet::new();
    let mut consumed_after: BTreeSet<&str> = BTreeSet::new();

    for (digest, (froms, tos)) in &mut digest_groups {
        froms.sort_unstable();
        tos.sort_unstable();
        match (froms.len(), tos.len()) {
            (1, 1) => {
                let from = froms[0];
                let to = tos[0];
                events.push(LineageEvent {
                    kind: LineageEventKind::Renamed,
                    from: Some(from.to_string()),
                    to: Some(to.to_string()),
                    similarity: 1.0,
                    coverage: CoverageState::Verified,
                    ambiguity: None,
                });
                consumed_before.insert(from);
                consumed_after.insert(to);
            }
            (n, m) if n > 0 && m > 0 => {
                // Ambiguous: several identical files moved at once. Emitting
                // any single pairing would be an invented rename edge, so all
                // participants are reported as partial with the full candidate
                // set.
                for from in froms.iter() {
                    events.push(LineageEvent {
                        kind: LineageEventKind::Renamed,
                        from: Some((*from).to_string()),
                        to: None,
                        similarity: 1.0,
                        coverage: CoverageState::Partial,
                        ambiguity: Some(Ambiguity {
                            candidates: tos.iter().map(|s| (*s).to_string()).collect(),
                            reason: format!(
                                "{n} identical source(s) and {m} identical target(s) share content digest {digest}; \
                                 rename cannot be uniquely attributed"
                            ),
                        }),
                    });
                    consumed_before.insert(from);
                }
                for to in tos.iter() {
                    consumed_after.insert(to);
                }
            }
            _ => {}
        }
    }

    // 2. Similarity scoring for leftovers, reported as `Partial` only.
    let leftover_before: Vec<&FileEntry> = before
        .iter()
        .filter(|e| !consumed_before.contains(e.path.as_str()))
        .collect();
    let leftover_after: Vec<&FileEntry> = after
        .iter()
        .filter(|e| !consumed_after.contains(e.path.as_str()))
        .collect();

    for b in &leftover_before {
        // A file that still exists at the same path is a content modification
        // (handled in step 3), never a move. Checking this once here replaces
        // an item-invariant filter that could only ever match everything or
        // nothing.
        if after_map.contains_key(b.path.as_str()) {
            continue;
        }
        let bset = b.token_set();
        // Scored pairs, sorted by (score desc, path asc) for determinism.
        let mut scored: Vec<(f64, &str)> = leftover_after
            .iter()
            .filter(|a| !consumed_after.contains(a.path.as_str()))
            .map(|a| {
                let aset = a.token_set();
                let inter = bset.intersection(&aset).count() as f64;
                let union = bset.union(&aset).count() as f64;
                let score = if union == 0.0 { 0.0 } else { inter / union };
                (score, a.path.as_str())
            })
            .collect();
        scored.sort_by(|x, y| {
            y.0.partial_cmp(&x.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| x.1.cmp(y.1))
        });

        let Some((best_score, best_path)) = scored.first().copied() else {
            continue;
        };
        if best_score < config.similarity_threshold {
            continue;
        }

        let runner_up = scored.get(1).map(|(s, _)| *s).unwrap_or(0.0);
        let margin = best_score - runner_up;
        let others: Vec<String> = scored
            .iter()
            .skip(1)
            .filter(|(s, _)| *s >= config.similarity_threshold)
            .map(|(_, p)| (*p).to_string())
            .collect();

        let mut candidates = others;
        candidates.sort();
        let reason = if !candidates.is_empty() {
            format!(
                "{} candidate target(s) at or above threshold {:.2}; best {:.4} vs runner-up {:.4}",
                candidates.len() + 1,
                config.similarity_threshold,
                best_score,
                runner_up
            )
        } else if margin < config.similarity_margin {
            format!(
                "unique candidate but margin {margin:.4} below required {:.4}",
                config.similarity_margin
            )
        } else {
            "similarity-based match is never identity-preserving; exact digest required".to_string()
        };

        if candidates.is_empty() && margin >= config.similarity_margin {
            candidates = Vec::new();
        }

        events.push(LineageEvent {
            kind: LineageEventKind::Moved,
            from: Some(b.path.clone()),
            to: Some(best_path.to_string()),
            similarity: best_score,
            coverage: CoverageState::Partial,
            ambiguity: Some(Ambiguity { candidates, reason }),
        });
        consumed_before.insert(b.path.as_str());
        consumed_after.insert(best_path);
    }

    // 3. Same-path content changes.
    for (path, before_entry) in &before_map {
        if let Some(after_entry) = after_map.get(path)
            && before_entry.content_digest != after_entry.content_digest
        {
            events.push(LineageEvent {
                kind: LineageEventKind::Modified,
                from: Some((*path).to_string()),
                to: Some((*path).to_string()),
                similarity: jaccard(&before_entry.tokens, &after_entry.tokens),
                coverage: CoverageState::Verified,
                ambiguity: None,
            });
            consumed_before.insert(path);
            consumed_after.insert(path);
        }
    }

    // 4. Unmatched additions and removals. A path present on both sides with
    //    equal digests is unchanged and yields no event.
    for path in after_map.keys() {
        let unchanged = before_map
            .get(path)
            .is_some_and(|b| b.content_digest == after_map[path].content_digest);
        if unchanged {
            continue;
        }
        if !consumed_after.contains(path) && !before_map.contains_key(path) {
            events.push(LineageEvent {
                kind: LineageEventKind::Added,
                from: None,
                to: Some((*path).to_string()),
                similarity: 0.0,
                coverage: CoverageState::Verified,
                ambiguity: None,
            });
        }
    }
    for path in before_map.keys() {
        let unchanged = after_map
            .get(path)
            .is_some_and(|a| a.content_digest == before_map[path].content_digest);
        if unchanged {
            continue;
        }
        if !consumed_before.contains(path) && !after_map.contains_key(path) {
            events.push(LineageEvent {
                kind: LineageEventKind::Removed,
                from: Some((*path).to_string()),
                to: None,
                similarity: 0.0,
                coverage: CoverageState::Verified,
                ambiguity: None,
            });
        }
    }

    // Canonical ordering: by (from, to, kind) so repeated runs are identical.
    events.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, content: &str) -> FileEntry {
        FileEntry::new(path, content.as_bytes())
    }

    fn cfg() -> LineageConfig {
        LineageConfig::default()
    }

    #[test]
    fn test_exact_digest_rename_is_verified() {
        let before = vec![entry("src/one.ts", "export function fn() { return 1; }")];
        let after = vec![entry("src/two.ts", "export function fn() { return 1; }")];
        let events = detect_lineage(&before, &after, &cfg());

        let renames: Vec<_> = events
            .iter()
            .filter(|e| e.kind == LineageEventKind::Renamed)
            .collect();
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].from.as_deref(), Some("src/one.ts"));
        assert_eq!(renames[0].to.as_deref(), Some("src/two.ts"));
        assert_eq!(renames[0].coverage, CoverageState::Verified);
        assert_eq!(renames[0].similarity, 1.0);
        assert!(renames[0].preserves_identity());
        assert!(renames[0].ambiguity.is_none());
    }

    #[test]
    fn test_ambiguous_identical_files_never_assert_identity() {
        // Two identical files both renamed -> no unique attribution is possible.
        let content = "export const dup = 1;";
        let before = vec![entry("a.ts", content), entry("b.ts", content)];
        let after = vec![entry("c.ts", content), entry("d.ts", content)];
        let events = detect_lineage(&before, &after, &cfg());

        let renames: Vec<_> = events
            .iter()
            .filter(|e| e.kind == LineageEventKind::Renamed)
            .collect();
        assert!(
            !renames.is_empty(),
            "ambiguity must be surfaced, not dropped"
        );

        for r in &renames {
            assert_eq!(
                r.coverage,
                CoverageState::Partial,
                "ambiguous renames must never be Verified"
            );
            assert!(!r.preserves_identity());
            let amb = r.ambiguity.as_ref().expect("ambiguity must be recorded");
            assert!(
                !amb.candidates.is_empty(),
                "candidate set must be disclosed"
            );
        }
    }

    #[test]
    fn test_zero_false_positive_renames_on_unrelated_files() {
        // Completely unrelated content must not be reported as a rename.
        let before = vec![entry("src/alpha.ts", "const alpha = computeAlpha();")];
        let after = vec![entry("src/beta.ts", "class Beta { gamma() { return 9; } }")];
        let events = detect_lineage(&before, &after, &cfg());

        let confident_renames: Vec<_> = events
            .iter()
            .filter(|e| e.kind == LineageEventKind::Renamed && e.is_confident())
            .collect();
        assert!(
            confident_renames.is_empty(),
            "unrelated content must not produce a confident rename"
        );

        // The transition must still be visible as add + remove.
        assert!(events.iter().any(|e| e.kind == LineageEventKind::Added));
        assert!(events.iter().any(|e| e.kind == LineageEventKind::Removed));
    }

    #[test]
    fn test_similarity_match_is_partial_not_verified() {
        let before = vec![entry(
            "src/one.ts",
            "export function calculateTotal(a: number, b: number) { return a + b; }",
        )];
        let after = vec![entry(
            "src/moved/one.ts",
            "export function calculateTotal(a: number, b: number) { return a + b; } // note",
        )];
        let events = detect_lineage(&before, &after, &cfg());

        let moved: Vec<_> = events
            .iter()
            .filter(|e| e.kind == LineageEventKind::Moved)
            .collect();
        assert_eq!(moved.len(), 1);
        assert_eq!(
            moved[0].coverage,
            CoverageState::Partial,
            "similarity alone can never reach Verified"
        );
        assert!(moved[0].similarity > 0.5);
        assert!(moved[0].ambiguity.is_some());
    }

    #[test]
    fn test_same_path_different_content_is_modified_not_renamed() {
        let before = vec![entry("src/a.ts", "export const x = 1;")];
        let after = vec![entry("src/a.ts", "export const x = 2;")];
        let events = detect_lineage(&before, &after, &cfg());

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, LineageEventKind::Modified);
        assert_eq!(events[0].from.as_deref(), Some("src/a.ts"));
        assert_eq!(events[0].to.as_deref(), Some("src/a.ts"));
        assert!(events[0].is_confident());
    }

    #[test]
    fn test_unchanged_files_produce_no_events() {
        let files = vec![
            entry("src/a.ts", "const a = 1;"),
            entry("src/b.ts", "const b = 2;"),
        ];
        let events = detect_lineage(&files, &files, &cfg());
        assert!(events.is_empty(), "no-change must be an empty event set");
    }

    #[test]
    fn test_pure_addition_and_removal() {
        let before = vec![entry("src/a.ts", "const a = 1;")];
        let after = vec![
            entry("src/a.ts", "const a = 1;"),
            entry("src/new.ts", "const brand = 'new';"),
        ];
        let events = detect_lineage(&before, &after, &cfg());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, LineageEventKind::Added);
        assert_eq!(events[0].to.as_deref(), Some("src/new.ts"));
    }

    #[test]
    fn test_detection_is_deterministic_across_runs() {
        let before = vec![
            entry("src/a.ts", "const a = 1;"),
            entry("src/b.ts", "const b = 2;"),
            entry("src/c.ts", "const c = 3;"),
        ];
        let after = vec![
            entry("src/x.ts", "const a = 1;"),
            entry("src/y.ts", "const b = 2;"),
            entry("src/c.ts", "const c = 4;"),
        ];
        let first = detect_lineage(&before, &after, &cfg());
        for _ in 0..100 {
            assert_eq!(detect_lineage(&before, &after, &cfg()), first);
        }
    }

    #[test]
    fn test_tokenize_is_order_insensitive_and_lowercased() {
        let a = tokenize(b"const Foo = 1; const Bar = 2;");
        let b = tokenize(b"const Bar = 2; const Foo = 1;");
        assert_eq!(a, b, "token signature must ignore ordering");
        assert!(a.contains(&"foo".to_string()));
        assert!(!a.contains(&"Foo".to_string()));
    }

    #[test]
    fn test_jaccard_bounds() {
        assert_eq!(jaccard(&[], &[]), 0.0);
        assert_eq!(jaccard(&["a".into()], &["a".into()]), 1.0);
        assert_eq!(jaccard(&["a".into()], &["b".into()]), 0.0);
        let v = jaccard(&["a".into(), "b".into()], &["b".into(), "c".into()]);
        assert!((v - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_empty_files_are_not_treated_as_identical_by_similarity() {
        let before = vec![FileEntry::new("a.ts", b"")];
        let after = vec![FileEntry::new("b.ts", b"")];
        let events = detect_lineage(&before, &after, &cfg());
        // Empty content is byte-identical, so this is a Verified rename:
        // the digest genuinely matches. Similarity must not be the reason.
        assert_eq!(events.len(), 1);
        let r = &events[0];
        assert_eq!(r.kind, LineageEventKind::Renamed);
        assert_eq!(r.similarity, 1.0);
        assert_eq!(jaccard(&[], &[]), 0.0, "empty similarity is not 1.0");
    }

    #[test]
    fn test_events_are_canonically_sorted() {
        let before = vec![entry("z.ts", "z"), entry("a.ts", "a")];
        let after = vec![entry("m.ts", "m"), entry("b.ts", "b")];
        let events = detect_lineage(&before, &after, &cfg());
        let keys: Vec<_> = events
            .iter()
            .map(|e| (e.from.clone(), e.to.clone(), e.kind))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
