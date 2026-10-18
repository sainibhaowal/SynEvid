//! Hover intelligence (Tier 3.2): word extraction, symbol resolution, and the
//! impact-markdown the editor renders inline.
//!
//! Everything here is a pure function of (text, engine artifacts). The same inputs
//! produce the same hover bytes — no clocks, no RNG, no map iteration order. That is
//! the engine's determinism rule extended to the IDE surface: editor UIs are where
//! nondeterminism usually creeps into a toolchain, and the fix is the same one the
//! analyzer uses everywhere — canonical order and explicit uncertainty.
//!
//! Honesty rules:
//!
//! - **Ambiguity is stated, not guessed.** When a name matches several symbols, the
//!   hover lists up to five candidates in stable-id order and says how many matched.
//! - **An empty answer is null.** No symbol at the cursor returns `null`, which is
//!   how the spec says "nothing here"; there is no placeholder text pretending to be
//!   information.
//! - **Coverage travels with the claim.** The hover prints the coverage state of the
//!   edges it counted, so a `partial` graph is visible at the exact place the number
//!   is read.

use autopsy_domain::{CoverageState, Symbol};
use autopsy_graph::DependencyGraph;

/// A span of a line, in **char** indices (not bytes, not UTF-16 — the conversion
/// out of LSP's UTF-16 columns happens once, at the edge of this module).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharSpan {
    pub start: usize,
    pub end: usize,
}

/// The words under a cursor: the bare identifier, and the dotted/`::`-joined path
/// containing it (e.g. `svc.UserService.create` or `crate::billing::Ledger`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorWords {
    pub simple: CharSpan,
    /// The identifier text of `simple`.
    pub simple_text: String,
    pub full: CharSpan,
    /// The full path text with `::` normalized to `.` (candidate-ready).
    pub path: String,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Converts an LSP UTF-16 column to a char index, clamping to the line length.
pub fn utf16_to_char_index(chars: &[char], utf16_col: u32) -> usize {
    let target = utf16_col as usize;
    let mut acc = 0usize;
    for (i, c) in chars.iter().enumerate() {
        if acc >= target {
            return i;
        }
        acc += c.len_utf16();
    }
    chars.len()
}

fn extend_left(chars: &[char], mut start: usize) -> usize {
    loop {
        let mut k = start;
        let mut consumed_separator = false;
        if k >= 1 && chars[k - 1] == '.' {
            while k >= 1 && chars[k - 1] == '.' {
                k -= 1;
            }
            consumed_separator = true;
        } else if k >= 2 && chars[k - 1] == ':' && chars[k - 2] == ':' {
            while k >= 2 && chars[k - 1] == ':' && chars[k - 2] == ':' {
                k -= 2;
            }
            consumed_separator = true;
        }
        if !consumed_separator {
            return start;
        }
        if k > 0 && is_ident_char(chars[k - 1]) {
            let mut m = k;
            while m > 0 && is_ident_char(chars[m - 1]) {
                m -= 1;
            }
            start = m;
        } else {
            return start;
        }
    }
}

fn extend_right(chars: &[char], mut end: usize) -> usize {
    loop {
        let mut k = end;
        let mut consumed_separator = false;
        if k < chars.len() && chars[k] == '.' {
            while k < chars.len() && chars[k] == '.' {
                k += 1;
            }
            consumed_separator = true;
        } else if k + 1 < chars.len() && chars[k] == ':' && chars[k + 1] == ':' {
            while k + 1 < chars.len() && chars[k] == ':' && chars[k + 1] == ':' {
                k += 2;
            }
            consumed_separator = true;
        }
        if !consumed_separator {
            return end;
        }
        if k < chars.len() && is_ident_char(chars[k]) {
            let mut m = k;
            while m < chars.len() && is_ident_char(chars[m]) {
                m += 1;
            }
            end = m;
        } else {
            return end;
        }
    }
}

/// Extracts the words under a cursor position.
///
/// `character` is the LSP UTF-16 column. When the cursor sits on a non-identifier
/// character, the identifier immediately before it is used — the everyday editor
/// behavior of "hovering the end of a name".
pub fn cursor_words(line: &str, character: u32) -> Option<CursorWords> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let raw = utf16_to_char_index(&chars, character);
    let anchor = if raw < chars.len() && is_ident_char(chars[raw]) {
        raw
    } else if raw > 0 && is_ident_char(chars[raw - 1]) {
        raw - 1
    } else {
        return None;
    };

    let mut simple_start = anchor;
    while simple_start > 0 && is_ident_char(chars[simple_start - 1]) {
        simple_start -= 1;
    }
    let mut simple_end = anchor;
    while simple_end < chars.len() && is_ident_char(chars[simple_end]) {
        simple_end += 1;
    }

    let full_start = extend_left(&chars, simple_start);
    let full_end = extend_right(&chars, simple_end);
    let path: String = chars[full_start..full_end]
        .iter()
        .collect::<String>()
        .replace("::", ".");

    Some(CursorWords {
        simple: CharSpan {
            start: simple_start,
            end: simple_end,
        },
        simple_text: chars[simple_start..simple_end].iter().collect(),
        full: CharSpan {
            start: full_start,
            end: full_end,
        },
        path,
    })
}

/// Candidate names for resolution, most specific first: the full dotted path and
/// every suffix of it (`a.b.c` → `a.b.c`, `b.c`, `c`), deduplicated.
pub fn candidates(path: &str) -> Vec<String> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let mut out: Vec<String> = Vec::new();
    for start in 0..segments.len() {
        let joined = segments[start..].join(".");
        if !out.contains(&joined) {
            out.push(joined);
        }
    }
    out
}

/// The short (declared) name of a symbol.
fn short_name(symbol: &Symbol) -> &str {
    symbol
        .qualified_name
        .rsplit("::")
        .next()
        .unwrap_or(&symbol.qualified_name)
}

/// The scope-qualified suffix of a symbol: the qualified name minus its file prefix
/// (`src/a.ts::UserService::create` → `UserService.create`).
fn scope_suffix(symbol: &Symbol) -> String {
    let mut parts = symbol.qualified_name.split("::");
    parts.next(); // file path
    parts.collect::<Vec<_>>().join(".")
}

/// The name part of a stable id (`src/a.ts#method:UserService.create` →
/// `UserService.create`), used as one more matching surface.
fn id_name_suffix(symbol: &Symbol) -> &str {
    let id = symbol.stable_id.as_str();
    let after_hash = id.split_once('#').map(|(_, rest)| rest).unwrap_or(id);
    after_hash
        .split_once(':')
        .map(|(_, name)| name)
        .unwrap_or(after_hash)
}

/// Resolution outcome: one symbol, several equally named ones, or none.
#[derive(Debug)]
pub enum Resolution<'a> {
    Exact(&'a Symbol),
    Ambiguous(Vec<&'a Symbol>),
    None,
}

fn matches_candidate(symbol: &Symbol, candidate: &str) -> bool {
    let suffix = scope_suffix(symbol);
    let id_suffix = id_name_suffix(symbol);
    let dotted_suffix = format!(".{candidate}");
    suffix == candidate
        || suffix.ends_with(&dotted_suffix)
        || short_name(symbol) == candidate
        || id_suffix == candidate
        || id_suffix.ends_with(&dotted_suffix)
}

/// Resolves the cursor words against the analyzed symbol table.
///
/// Priority order, highest first:
///
/// 1. **Declaration at the cursor** — a symbol whose own source range covers the
///    position and whose declared name is the hovered word. That is the one case
///    where the answer is not a guess: the analyzer read this exact declaration.
/// 2. **Name match, filtered by the dotted path** — candidates from the full path
///    are tried most-specific-first; the first candidate with hits wins, and the
///    bare identifier is the last-resort candidate for left-of-cursor cases
///    (`a.b.c` with the cursor on `b` still finds `b`).
/// 3. **Same-file preference** among otherwise-equal candidates.
///
/// Anything still ambiguous is returned as `Ambiguous` with stable-id ordering.
pub fn resolve<'a>(
    artifacts: &'a autopsy_cli::AnalysisArtifacts,
    doc_path: Option<&str>,
    line_1based: u32,
    words: &CursorWords,
) -> Resolution<'a> {
    // 1. Declaration at the cursor.
    if let Some(doc) = doc_path {
        let mut declared: Vec<&Symbol> = artifacts
            .symbols
            .iter()
            .filter(|s| {
                s.range.as_ref().is_some_and(|r| {
                    r.path == doc
                        && r.start_line <= line_1based
                        && line_1based <= r.end_line
                        && words.simple_text == short_name(s)
                })
            })
            .collect();
        declared.sort_by(|a, b| {
            let ra = a
                .range
                .as_ref()
                .map(|r| r.end_line - r.start_line)
                .unwrap_or(u32::MAX);
            let rb = b
                .range
                .as_ref()
                .map(|r| r.end_line - r.start_line)
                .unwrap_or(u32::MAX);
            ra.cmp(&rb).then_with(|| a.stable_id.cmp(&b.stable_id))
        });
        if let Some(first) = declared.first() {
            return Resolution::Exact(first);
        }
    }

    // 2. Candidate-driven name matching; the bare identifier is the last candidate.
    let mut all_candidates = candidates(&words.path);
    if !all_candidates.contains(&words.simple_text) {
        all_candidates.push(words.simple_text.clone());
    }
    let mut chosen: Option<Vec<&Symbol>> = None;
    for candidate in &all_candidates {
        let hits: Vec<&Symbol> = artifacts
            .symbols
            .iter()
            .filter(|s| matches_candidate(s, candidate))
            .collect();
        if !hits.is_empty() {
            chosen = Some(hits);
            break;
        }
    }

    let Some(mut hits) = chosen else {
        return Resolution::None;
    };

    // 3. Same-file preference, then stable-id ordering.
    if let Some(doc) = doc_path {
        let same_file: Vec<&Symbol> = hits
            .iter()
            .copied()
            .filter(|s| s.range.as_ref().is_some_and(|r| r.path == doc))
            .collect();
        if !same_file.is_empty() {
            hits = same_file;
        }
    }
    hits.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    hits.dedup_by(|a, b| a.stable_id == b.stable_id);

    match hits.len() {
        0 => Resolution::None,
        1 => Resolution::Exact(hits[0]),
        _ => Resolution::Ambiguous(hits),
    }
}

/// Edge kinds the engine's `callers` profile follows (02 impact profiles): a caller
/// is an invocation or a reference, never mere structural containment.
fn is_call_like(kind: autopsy_domain::EdgeKind) -> bool {
    use autopsy_domain::EdgeKind;
    matches!(kind, EdgeKind::Calls | EdgeKind::References)
}

/// Edge kinds the engine's default impact profile follows. Kept in lockstep with
/// `autopsy_impact::ImpactProfile::default()` so the hover number and the CLI
/// `autopsy impact` number describe the same graph.
fn is_impact_kind(kind: autopsy_domain::EdgeKind) -> bool {
    use autopsy_domain::EdgeKind;
    matches!(
        kind,
        EdgeKind::Contains
            | EdgeKind::Imports
            | EdgeKind::References
            | EdgeKind::Calls
            | EdgeKind::Implements
            | EdgeKind::Inherits
    )
}

/// Counts and coverage of one symbol's direct neighborhood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighborhood {
    /// Distinct symbols that call or reference this one.
    pub callers: usize,
    /// Distinct symbols this one calls or references.
    pub callees: usize,
    /// Distinct symbols reachable in one hop over the default impact profile.
    pub impact_symbols: usize,
    /// Distinct files those reachable symbols live in.
    pub impact_files: usize,
    pub out_partial_edges: usize,
    pub out_unknown_edges: usize,
}

/// Computes the 1-hop neighborhood counts deterministically.
pub fn neighborhood(graph: &DependencyGraph, symbol: &Symbol) -> Neighborhood {
    let id = &symbol.stable_id;
    let outgoing = graph.outgoing_edges(id);
    let incoming = graph.incoming_edges(id);

    let mut caller_ids: Vec<&autopsy_domain::SymbolId> = incoming
        .iter()
        .filter(|e| is_call_like(e.kind))
        .map(|e| &e.source)
        .collect();
    caller_ids.sort();
    caller_ids.dedup();

    let mut callee_ids: Vec<&autopsy_domain::SymbolId> = outgoing
        .iter()
        .filter(|e| is_call_like(e.kind))
        .map(|e| &e.target)
        .collect();
    callee_ids.sort();
    callee_ids.dedup();

    let mut impact_ids: Vec<&autopsy_domain::SymbolId> = outgoing
        .iter()
        .filter(|e| is_impact_kind(e.kind))
        .map(|e| &e.target)
        .collect();
    impact_ids.sort();
    impact_ids.dedup();

    let mut files: Vec<&str> = Vec::new();
    for target in &impact_ids {
        if let Some(node) = graph.get_node(target)
            && let Some(range) = &node.range
            && !files.contains(&range.path.as_str())
        {
            files.push(&range.path);
        }
    }

    let mut partial = 0usize;
    let mut unknown = 0usize;
    for edge in outgoing.iter().filter(|e| is_impact_kind(e.kind)) {
        match edge.coverage {
            CoverageState::Verified => {}
            CoverageState::Partial | CoverageState::Unsupported => partial += 1,
            CoverageState::Unknown => unknown += 1,
        }
    }

    Neighborhood {
        callers: caller_ids.len(),
        callees: callee_ids.len(),
        impact_symbols: impact_ids.len(),
        impact_files: files.len(),
        out_partial_edges: partial,
        out_unknown_edges: unknown,
    }
}

fn coverage_label(state: &CoverageState) -> &'static str {
    match state {
        CoverageState::Verified => "verified",
        CoverageState::Partial => "partial",
        CoverageState::Unsupported => "unsupported",
        CoverageState::Unknown => "unknown",
    }
}

/// Renders the hover markdown for one resolved symbol.
pub fn render_hover(
    symbol: &Symbol,
    graph: &DependencyGraph,
    engine_coverage: &CoverageState,
) -> String {
    let n = neighborhood(graph, symbol);
    let mut out = String::new();

    let signature = symbol
        .normalized_signature
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&symbol.qualified_name);
    out.push_str(&format!("**`{signature}`** — `{}`\n\n", symbol.kind));

    let scope = scope_suffix(symbol);
    if !scope.is_empty() && scope != signature {
        out.push_str(&format!("`{scope}`\n"));
    }

    match &symbol.range {
        Some(range) => out.push_str(&format!("Declared `{}:{}`\n", range.path, range.start_line)),
        None => out.push_str("Declared location: unknown\n"),
    }

    out.push_str(&format!(
        "Callers **{}** · Callees **{}** · 1-hop impact **{} symbols / {} files**\n",
        n.callers, n.callees, n.impact_symbols, n.impact_files
    ));

    out.push_str(&format!(
        "Engine coverage `{}`",
        coverage_label(engine_coverage)
    ));
    if n.out_partial_edges > 0 || n.out_unknown_edges > 0 {
        out.push_str(&format!(
            " · out edges here: {} partial, {} unknown",
            n.out_partial_edges, n.out_unknown_edges
        ));
    }
    out.push('\n');
    out
}

/// Renders the ambiguity hover: candidates in stable-id order, capped and stated.
pub fn render_ambiguous(name: &str, hits: &[&Symbol]) -> String {
    const CAP: usize = 5;
    let mut out = format!(
        "**Synevid** — `{name}` matches {} symbols; hovering is ambiguous.\n\n",
        hits.len()
    );
    for hit in hits.iter().take(CAP) {
        let location = hit
            .range
            .as_ref()
            .map(|r| format!("`{}:{}`", r.path, r.start_line))
            .unwrap_or_else(|| "location unknown".to_string());
        out.push_str(&format!(
            "- `{}` (`{}`) — {}\n",
            hit.stable_id, hit.kind, location
        ));
    }
    if hits.len() > CAP {
        out.push_str(&format!("- … and {} more\n", hits.len() - CAP));
    }
    out.push_str(&format!(
        "\nUse `autopsy find \"{name}\"` to enumerate all candidates.\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_domain::{Edge, EdgeKind, SourceLocation, SymbolId, Visibility};

    fn symbol(id: &str, qname: &str, kind: &str, path: &str, line: u32) -> Symbol {
        Symbol {
            stable_id: SymbolId::new(id),
            language_id: "typescript".to_string(),
            kind: kind.to_string(),
            qualified_name: qname.to_string(),
            range: Some(SourceLocation {
                path: path.to_string(),
                start_line: line,
                end_line: line,
                start_col: Some(1),
                end_col: Some(10),
                symbol_id: None,
            }),
            normalized_signature: None,
            visibility: Visibility::Public,
        }
    }

    #[test]
    fn extracts_simple_and_dotted_words() {
        let line = "  const x = svc.userService.create(payload);";
        let idx = line.find("create").unwrap() as u32;
        let words = cursor_words(line, idx + 2).unwrap();
        let chars: Vec<char> = line.chars().collect();
        assert_eq!(
            chars[words.simple.start..words.simple.end]
                .iter()
                .collect::<String>(),
            "create"
        );
        assert_eq!(words.simple_text, "create");
        assert_eq!(words.path, "svc.userService.create");
    }

    #[test]
    fn extracts_rust_paths_with_double_colon() {
        let line = "    crate::billing::ledger::Ledger::settle(&mut vault);";
        let idx = line.find("settle").unwrap() as u32;
        let words = cursor_words(line, idx + 1).unwrap();
        assert_eq!(words.path, "crate.billing.ledger.Ledger.settle");
        let c = candidates(&words.path);
        assert_eq!(c[0], "crate.billing.ledger.Ledger.settle");
        assert_eq!(c.last().unwrap(), "settle");
    }

    #[test]
    fn cursor_on_separator_uses_preceding_word() {
        let line = "svc.create()";
        // Cursor on the `.` (char 3): the identifier before it is `svc`.
        let words = cursor_words(line, 3).unwrap();
        assert_eq!(words.simple_text, "svc");
        assert_eq!(words.path, "svc.create");
    }

    #[test]
    fn cursor_left_of_path_falls_back_to_bare_name() {
        let line = "a.b.c()";
        // Cursor on `b` (index 2).
        let words = cursor_words(line, 2).unwrap();
        assert_eq!(words.simple_text, "b");
        assert_eq!(words.path, "a.b.c");
        let c = candidates(&words.path);
        assert!(c.contains(&"b.c".to_string()));
        assert!(
            !c.contains(&"b".to_string()),
            "the bare name is appended later"
        );
    }

    #[test]
    fn utf16_columns_account_for_astral_chars() {
        let line = "let s = \"🚀\"; foo(bar);";
        let char_idx = line.chars().position(|c| c == 'b').unwrap() as u32;
        let mut utf16 = 0u32;
        for (i, c) in line.chars().enumerate() {
            if i as u32 == char_idx {
                break;
            }
            utf16 += c.len_utf16() as u32;
        }
        let words = cursor_words(line, utf16 + 1).unwrap();
        assert_eq!(words.simple_text, "bar");
    }

    #[test]
    fn non_identifier_cursor_yields_none() {
        assert!(cursor_words("   +   ", 1).is_none());
        assert!(cursor_words("", 0).is_none());
    }

    #[test]
    fn resolution_matches_scope_and_id_suffixes() {
        let s = symbol(
            "src/a.ts#method:UserService.create",
            "src/a.ts::UserService::create",
            "method",
            "src/a.ts",
            10,
        );
        assert!(matches_candidate(&s, "UserService.create"));
        assert!(matches_candidate(&s, "create"));
        assert!(!matches_candidate(&s, "Other.create"));
    }

    #[test]
    fn neighborhood_counts_are_deduplicated_and_deterministic() {
        let caller = symbol(
            "src/c.ts#fn:caller",
            "src/c.ts::caller",
            "function",
            "src/c.ts",
            1,
        );
        let target = symbol(
            "src/a.ts#method:UserService.create",
            "src/a.ts::UserService::create",
            "method",
            "src/a.ts",
            10,
        );
        let other = symbol(
            "src/b.ts#fn:helper",
            "src/b.ts::helper",
            "function",
            "src/b.ts",
            3,
        );
        let edge = |src: &str, dst: &str| Edge {
            source: SymbolId::new(src.to_string()),
            target: SymbolId::new(dst.to_string()),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Partial,
            location: None,
            provenance: "test".to_string(),
        };
        let graph = DependencyGraph::from_symbols_and_edges(
            vec![caller.clone(), target.clone(), other.clone()],
            vec![
                edge("src/c.ts#fn:caller", "src/a.ts#method:UserService.create"),
                edge("src/c.ts#fn:caller", "src/a.ts#method:UserService.create"), // duplicate multiedge
                edge("src/a.ts#method:UserService.create", "src/b.ts#fn:helper"),
            ],
        );
        let n = neighborhood(&graph, &target);
        assert_eq!(n.callers, 1, "multiedges from one caller count once");
        assert_eq!(n.callees, 1);
        assert_eq!(n.impact_symbols, 1);
        assert_eq!(n.impact_files, 1);
        assert_eq!(n.out_partial_edges, 1, "the one out-edge is partial");
        assert_eq!(n.out_unknown_edges, 0);
    }

    #[test]
    fn containment_is_not_a_caller() {
        // A class `Contains` its method, and `run` calls the method. Only `run`
        // is a caller: structural containment must not inflate the number.
        let class = symbol(
            "src/a.ts#class:Ledger",
            "src/a.ts::Ledger",
            "class",
            "src/a.ts",
            1,
        );
        let method = symbol(
            "src/a.ts#method:Ledger.settle",
            "src/a.ts::Ledger::settle",
            "method",
            "src/a.ts",
            2,
        );
        let run = symbol(
            "src/b.ts#fn:run",
            "src/b.ts::run",
            "function",
            "src/b.ts",
            1,
        );
        let contains = Edge {
            source: class.stable_id.clone(),
            target: method.stable_id.clone(),
            kind: EdgeKind::Contains,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        };
        let calls = Edge {
            source: run.stable_id.clone(),
            target: method.stable_id.clone(),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        };
        let graph = DependencyGraph::from_symbols_and_edges(
            vec![class, method.clone(), run],
            vec![contains, calls],
        );
        let n = neighborhood(&graph, &method);
        assert_eq!(n.callers, 1, "the class contains, the function calls");
    }

    #[test]
    fn hover_rendering_is_byte_stable_and_marks_partial() {
        let target = symbol(
            "src/a.ts#method:UserService.create",
            "src/a.ts::UserService::create",
            "method",
            "src/a.ts",
            10,
        );
        let graph = DependencyGraph::from_symbols_and_edges(vec![target.clone()], vec![]);
        let first = render_hover(&target, &graph, &CoverageState::Partial);
        let second = render_hover(&target, &graph, &CoverageState::Partial);
        assert_eq!(first, second, "identical inputs, identical bytes");
        assert!(first.contains("`method`"));
        assert!(first.contains("Declared `src/a.ts:10`"));
        assert!(first.contains("Engine coverage `partial`"));
        assert!(first.contains("Callers **0**"));
    }

    #[test]
    fn ambiguous_rendering_caps_and_states_overflow() {
        let hits: Vec<Symbol> = (0..7)
            .map(|i| {
                symbol(
                    &format!("src/f{i}.ts#fn:run"),
                    &format!("src/f{i}.ts::run"),
                    "function",
                    &format!("src/f{i}.ts"),
                    i as u32 + 1,
                )
            })
            .collect();
        let refs: Vec<&Symbol> = hits.iter().collect();
        let text = render_ambiguous("run", &refs);
        assert!(text.contains("matches 7 symbols"));
        assert!(text.contains("… and 2 more"));
        assert!(text.contains("autopsy find \"run\""));
    }
}
