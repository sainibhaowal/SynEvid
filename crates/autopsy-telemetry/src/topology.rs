//! Runtime-to-static correlation and topology diff (Step 41).
//!
//! Gate: *"Maps `code.function` and `http.route` spans to static `SymbolId`;
//! identifies dead code and undocumented endpoints."*
//!
//! # The static side is supplied, not parsed
//!
//! This module takes a [`DeclaredSurface`] — the routes a contract declares and the
//! symbols a symbol table contains — rather than reading an OpenAPI document or a
//! language AST itself. Three reasons, in order of importance:
//!
//! 1. **Two definitions in two places drift.** Canonicalising a route template is a
//!    decision with a rule behind it ([`canonical_route_template`]); doing it once in
//!    the contract engine and again here is how the two answers end up disagreeing
//!    about `/users/{id}`.
//! 2. **Protocol-agnostic.** A route is a method and a template. An OpenAPI operation,
//!    an AsyncAPI channel and a gRPC method all reduce to that, and coupling this crate
//!    to one protocol's parser would mean editing it for the next one.
//! 3. **The DAG stays a DAG.** `autopsy-telemetry` depends on `autopsy-domain` only.
//!    It reaches the symbol table through [`DeclaredSymbol`], whose `From<&Symbol>` impl
//!    is the one-line boundary.
//!
//! # Dead code is a claim, so it is the last thing the engine will make
//!
//! `"This function never ran"` is what a reader will hear, and it is **false** in at
//! least four situations the engine can see coming:
//!
//! | Situation | What the engine does |
//! |---|---|
//! | The export was **sampled**, so absence is expected | Finding moves to [`InsufficientEvidence`] — never asserted |
//! | Fewer than the required observation **windows** | Same: [`InsufficientEvidence`] |
//! | **Zero** windows (nothing was looked at) | No findings at all; coverage `Unknown` |
//! | The language has **dynamic dispatch**, so the call may not name the symbol | Finding is emitted, but its coverage is `Partial` and the report says why |
//!
//! So a finding is emitted only when the observation covered enough windows, and it
//! reaches `Verified` coverage only when the caller has asserted that the exports are
//! complete **and** that the dispatch is static. Both are caller assertions with
//! conservative defaults, because the engine cannot detect either from a payload.
//!
//! # Undocumented endpoints have the mirror-image trap
//!
//! `"This route has no contract"` is unanswerable if no contract was loaded.
//! [`DeclaredSurface::routes`] being empty is indistinguishable from a service that
//! genuinely declares no routes, so the engine reports `Unknown` and **emits no
//! undocumented findings at all** rather than reporting every observed route as a
//! violation of a document nobody read. The same rule as an empty Pact (Step 39) and an
//! unchanged-but-unmodelled document (Step 38).

use crate::span::{OccupancyKey, SpanRecord, TraceRecord};
use autopsy_domain::{CoverageState, Symbol, SymbolId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

/// Whether a call can reach a symbol without naming it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchModel {
    /// Every call names its callee, so an unobserved symbol was not called.
    Static,
    /// Reflection, dynamic dispatch, DI containers, callbacks, string-built method
    /// names — a symbol can run without anything naming it. The conservative
    /// assumption, and the default.
    Dynamic,
}

/// A route a contract declares.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeclaredRoute {
    /// Upper-cased. `None` means the contract does not distinguish methods, which
    /// makes the route a match for any observed method — and degrades coverage,
    /// because a `None` here cannot catch a wrong method.
    pub method: Option<String>,
    /// The template as the contract wrote it, placeholder names included.
    pub template: String,
    pub operation_id: Option<String>,
}

impl DeclaredRoute {
    pub fn new(method: Option<&str>, template: &str) -> Self {
        Self {
            method: method.map(|m| m.to_ascii_uppercase()),
            template: template.to_string(),
            operation_id: None,
        }
    }

    /// The placeholder-name-erased shape, which is what matching uses.
    pub fn shape(&self) -> String {
        canonical_route_template(&self.template)
    }
}

/// A symbol the static analysis knows about, in the form the fusion needs.
///
/// A projection of [`autopsy_domain::Symbol`], not a replacement: the name and the
/// file are exactly the two facts a span carries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeclaredSymbol {
    pub id: SymbolId,
    /// Matched against `code.function`.
    pub name: String,
    /// Matched against `code.filepath`. `None` means the symbol table did not record a
    /// file, which makes name-only matching necessary and ambiguity likely.
    pub path: Option<String>,
    pub kind: Option<String>,
}

impl DeclaredSymbol {
    pub fn new(id: &str, path: &str, name: &str) -> Self {
        Self {
            id: SymbolId::new(id),
            name: name.to_string(),
            path: Some(normalise_path(path)),
            kind: None,
        }
    }
}

impl From<&Symbol> for DeclaredSymbol {
    fn from(symbol: &Symbol) -> Self {
        Self {
            id: symbol.stable_id.clone(),
            // `qualified_name` is what adapters put in `code.function` — the bare
            // function name, not a path-qualified one. Files are matched separately,
            // which is why both halves travel together.
            name: symbol.qualified_name.clone(),
            path: symbol
                .range
                .as_ref()
                .map(|range| normalise_path(&range.path)),
            kind: Some(symbol.kind.clone()),
        }
    }
}

/// The static picture a fusion is checked against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredSurface {
    pub routes: Vec<DeclaredRoute>,
    pub symbols: Vec<DeclaredSymbol>,
}

/// Paths are compared POSIX-style on every platform (`INV-CROSS-001`), and a
/// leading `./` is stripped so an instrumented file and a symbol table agree.
fn normalise_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.strip_prefix("./").unwrap_or(&path).to_string()
}

/// Erases placeholder names: `/users/{id}` and `/users/{userId}` are one shape.
///
/// A placeholder must be a **whole segment**. `/files/{name}.json` is therefore
/// literal, which under-matches rather than over-matches — an unmatched observation is
/// visible, a wrong match is not. The same rule Step 39 uses for concrete paths.
pub fn canonical_route_template(template: &str) -> String {
    let trimmed = template.split(['?', '#']).next().unwrap_or(template);
    let mut out = String::with_capacity(trimmed.len());
    for (index, segment) in trimmed.split('/').enumerate() {
        if index > 0 {
            out.push('/');
        }
        if is_placeholder(segment) {
            out.push_str("{}");
        } else {
            out.push_str(segment);
        }
    }
    out
}

fn is_placeholder(segment: &str) -> bool {
    segment.len() >= 3
        && segment.starts_with('{')
        && segment.ends_with('}')
        && !segment[1..segment.len() - 1].contains(['{', '}'])
}

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// How many placeholders a template has, or `None` if it does not subsume `concrete`.
fn subsumes(concrete: &[&str], template: &[&str]) -> Option<usize> {
    if concrete.len() != template.len() {
        return None;
    }
    let mut placeholders = 0;
    for (actual, declared) in concrete.iter().zip(template) {
        if actual.is_empty() {
            return None;
        }
        if is_placeholder(declared) {
            placeholders += 1;
        } else if declared != actual {
            return None;
        }
    }
    Some(placeholders)
}

/// How a span was placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrelationBasis {
    /// `http.route` was present and its shape is declared.
    RouteTemplate,
    /// No `http.route`; the observed path was matched to a declared template.
    ObservedPathSubsumption,
    /// `code.function` (+ `code.filepath`) matched a declared symbol.
    CodeFunction,
    /// `code.function` named **more than one** declared symbol and nothing
    /// disambiguated them.
    ///
    /// Split from [`Self::Unmatched`] deliberately: the two read identically as
    /// "not placed", and they are fixed in different places — a missing binding versus
    /// two symbols the symbol table should have distinguished. Collapsing them would
    /// hide a `SymbolId` collision behind an ordinary gap.
    AmbiguousSymbol,
    /// Nothing matched.
    Unmatched,
}

impl CorrelationBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RouteTemplate => "route_template",
            Self::ObservedPathSubsumption => "observed_path_subsumption",
            Self::CodeFunction => "code_function",
            Self::AmbiguousSymbol => "ambiguous_symbol",
            Self::Unmatched => "unmatched",
        }
    }

    /// Whether the basis came from an inference rather than from a declared fact.
    pub fn is_inferred(self) -> bool {
        matches!(self, Self::ObservedPathSubsumption)
    }

    /// Whether the span was placed against the static surface at all.
    pub fn is_placed(self) -> bool {
        !matches!(self, Self::Unmatched | Self::AmbiguousSymbol)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpanCorrelation {
    pub span_id_hash: String,
    pub occupancy: OccupancyKey,
    pub symbol: Option<SymbolId>,
    pub route: Option<DeclaredRoute>,
    pub basis: CorrelationBasis,
    pub coverage: CoverageState,
}

/// A code path with zero observed invocations over enough windows to say so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadCodeFinding {
    pub symbol: SymbolId,
    pub path: Option<String>,
    /// Always zero — the finding exists because it is zero. Carried explicitly so a
    /// reader does not have to infer the claim from the absence of a count.
    pub observed_invocations: u32,
    pub windows_observed: u32,
    pub coverage: CoverageState,
}

/// A route the contract declares that nothing was observed calling.
///
/// The mirror of [`UndocumentedRoute`], and the reason this type had to exist: without
/// it, a declared route that nothing calls leaves **no trace in the report at all**, so
/// the digest does not cover the contract's surface and a reader cannot ask the obvious
/// question. (Found by a control assertion that a changed contract must move the
/// digest — it did not.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninvokedRoute {
    pub method: Option<String>,
    pub template: String,
    pub operation_id: Option<String>,
    /// Always zero, carried explicitly so the claim is not inferred from an absence.
    pub observed_invocations: u32,
    pub windows_observed: u32,
    pub coverage: CoverageState,
}

/// What an [`InsufficientEvidence`] entry is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSubject {
    Symbol(SymbolId),
    Route {
        method: Option<String>,
        template: String,
    },
}

/// Something with no observed activity that the evidence base **cannot** support a
/// finding for.
///
/// Deliberately a separate list rather than a `DeadCodeFinding` with a weak coverage:
/// merging them would let a reader counting findings see a code path called dead when
/// the engine has said only that it did not look long enough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsufficientEvidence {
    pub subject: EvidenceSubject,
    pub windows_observed: u32,
    pub required_windows: u32,
    pub reason: InsufficientReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsufficientReason {
    /// Fewer observation windows than the caller required.
    TooFewWindows,
    /// The caller has not asserted that the exports are unsampled, so absence of a
    /// span may mean only that the sampler dropped it.
    ObservationMayBeSampled,
}

/// An observed route no contract declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndocumentedRoute {
    pub method: Option<String>,
    /// The placeholder-name-erased shape.
    pub template: String,
    /// One concrete observed path, so a reader has something to grep for.
    pub example_path: String,
    pub services: BTreeSet<String>,
    pub invocations: usize,
    pub coverage: CoverageState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationSummary {
    /// One [`TraceRecord`] is one observation window.
    pub windows_observed: u32,
    pub required_windows: u32,
    pub span_count: usize,
    pub complete_observation: bool,
    pub dispatch: DispatchModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyReport {
    pub observation: ObservationSummary,
    pub correlations: Vec<SpanCorrelation>,
    pub dead_code: Vec<DeadCodeFinding>,
    pub uninvoked_routes: Vec<UninvokedRoute>,
    pub insufficient_evidence: Vec<InsufficientEvidence>,
    pub undocumented_routes: Vec<UndocumentedRoute>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

/// Caller assertions the engine cannot derive from a payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyOptions {
    required_windows: NonZeroU32,
    complete_observation: bool,
    dispatch: DispatchModel,
}

impl TopologyOptions {
    /// Both defaults are the conservative reading: the exports may be sampled, and
    /// dispatch may be dynamic. Relaxing either is a caller assertion, and each one
    /// that is relaxed is named in the report's notes.
    pub fn new(required_windows: NonZeroU32) -> Self {
        Self {
            required_windows,
            complete_observation: false,
            dispatch: DispatchModel::Dynamic,
        }
    }

    pub fn with_complete_observation(mut self, complete: bool) -> Self {
        self.complete_observation = complete;
        self
    }

    pub fn with_dispatch_model(mut self, dispatch: DispatchModel) -> Self {
        self.dispatch = dispatch;
        self
    }

    /// Expressed as a `u32` because a window budget is not a count of something.
    pub fn required_windows(&self) -> u32 {
        self.required_windows.get()
    }
}

fn digest_of<T: Serialize>(value: &T) -> String {
    let mut json = serde_json::to_value(value).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut json {
        // Prose and the field being computed. Everything else is a detected fact.
        map.remove("digest");
        map.remove("notes");
    }
    let text = serde_json::to_string(&json).unwrap_or_default();
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// Indexes the declared symbols so name-only lookups can report ambiguity instead of
/// picking.
struct SymbolIndex<'a> {
    by_path_and_name: BTreeMap<(String, &'a str), &'a DeclaredSymbol>,
    by_name: BTreeMap<&'a str, Vec<&'a DeclaredSymbol>>,
}

impl<'a> SymbolIndex<'a> {
    fn build(symbols: &'a [DeclaredSymbol]) -> Self {
        let mut by_path_and_name = BTreeMap::new();
        let mut by_name: BTreeMap<&str, Vec<&DeclaredSymbol>> = BTreeMap::new();
        for symbol in symbols {
            if let Some(path) = &symbol.path {
                by_path_and_name.insert((path.clone(), symbol.name.as_str()), symbol);
            }
            by_name
                .entry(symbol.name.as_str())
                .or_default()
                .push(symbol);
        }
        Self {
            by_path_and_name,
            by_name,
        }
    }

    /// `Ok(&symbol)`, `Err(None)` for no match, `Err(Some(ids))` for an ambiguity.
    fn lookup(
        &self,
        name: &str,
        path: Option<&str>,
    ) -> Result<&'a DeclaredSymbol, Option<Vec<&'a DeclaredSymbol>>> {
        if let Some(path) = path
            && let Some(symbol) = self.by_path_and_name.get(&(normalise_path(path), name))
        {
            return Ok(symbol);
        }
        match self.by_name.get(name) {
            Some(candidates) if candidates.len() == 1 => Ok(candidates[0]),
            Some(candidates) if !candidates.is_empty() => Err(Some(candidates.clone())),
            _ => Err(None),
        }
    }
}

/// Finds the declared route an observed `http.route` template names.
///
/// Matching is on the erased shape, so `/users/{id}` and `/users/{userId}` agree. An
/// observed template that is not declared is not an error here — it is an
/// undocumented route, which is the finding this step exists to produce.
fn match_declared_route<'a>(
    routes: &'a [DeclaredRoute],
    observed_template: &str,
    method: Option<&str>,
) -> Vec<&'a DeclaredRoute> {
    let observed_shape = canonical_route_template(observed_template);
    routes
        .iter()
        .filter(|route| route.shape() == observed_shape)
        .filter(|route| match (&route.method, method) {
            // A contract that names no method matches any observation.
            (None, _) => true,
            (Some(declared), Some(observed)) => declared.eq_ignore_ascii_case(observed),
            // The observation named no method, so the method cannot be checked.
            (Some(_), None) => true,
        })
        .collect()
}

/// The most specific declared routes that subsume a concrete observed path.
fn match_by_subsumption<'a>(
    routes: &'a [DeclaredRoute],
    path: &str,
    method: Option<&str>,
) -> (Option<&'a DeclaredRoute>, BTreeSet<String>) {
    let actual = segments(path);
    let mut candidates: Vec<(usize, &DeclaredRoute)> = Vec::new();
    for route in routes {
        if let (Some(declared), Some(observed)) = (&route.method, method)
            && !declared.eq_ignore_ascii_case(observed)
        {
            continue;
        }
        if let Some(placeholders) = subsumes(&actual, &segments(&route.template)) {
            candidates.push((placeholders, route));
        }
    }
    // Fewest placeholders first, then lexicographically by shape, so the choice never
    // depends on declaration order.
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.shape().cmp(&b.1.shape()))
            .then_with(|| a.1.template.cmp(&b.1.template))
    });
    let Some((best, chosen)) = candidates.first().copied() else {
        return (None, BTreeSet::new());
    };
    let tied: BTreeSet<String> = candidates
        .iter()
        .filter(|(specificity, route)| *specificity == best && route.shape() != chosen.shape())
        .map(|(_, route)| route.shape())
        .collect();
    (Some(chosen), tied)
}

/// `(shape, method)` pairs the contract declares more than once, in sorted order.
fn duplicated_route_shapes(routes: &[DeclaredRoute]) -> BTreeSet<String> {
    let mut seen: BTreeMap<(String, Option<String>), usize> = BTreeMap::new();
    for route in routes {
        *seen
            .entry((route.shape(), route.method.clone()))
            .or_insert(0) += 1;
    }
    seen.into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|((shape, method), _)| match method {
            Some(method) => format!("{method} {shape}"),
            None => shape,
        })
        .collect()
}

/// Correlates observations with the static surface and diffs the topology.
pub fn correlate_topology(
    records: &[TraceRecord],
    surface: &DeclaredSurface,
    options: &TopologyOptions,
) -> TopologyReport {
    let windows_observed = u32::try_from(records.len()).unwrap_or(u32::MAX);
    let required_windows = options.required_windows();
    let mut notes: Vec<String> = Vec::new();

    let mut coverage = if records.is_empty() {
        CoverageState::Unknown
    } else {
        CoverageState::Verified
    };
    for record in records {
        coverage = worse(coverage, record.coverage);
    }

    let observation = ObservationSummary {
        windows_observed,
        required_windows,
        span_count: records.iter().map(|record| record.span_count).sum(),
        complete_observation: options.complete_observation,
        dispatch: options.dispatch,
    };

    if records.is_empty() {
        // Nothing was observed. No route was seen, no symbol was seen, and no claim
        // about either can be made — a route is not undocumented if nobody looked, and
        // a function is not dead if nobody looked.
        notes.push(
            "no observation windows were supplied, so no runtime behaviour was observed and \
             nothing can be correlated or reported absent"
                .to_string(),
        );
        let mut report = TopologyReport {
            observation,
            correlations: Vec::new(),
            dead_code: Vec::new(),
            uninvoked_routes: Vec::new(),
            insufficient_evidence: Vec::new(),
            undocumented_routes: Vec::new(),
            coverage: CoverageState::Unknown,
            notes,
            digest: String::new(),
        };
        report.digest = digest_of(&report);
        return report;
    }

    let index = SymbolIndex::build(&surface.symbols);
    let duplicated = duplicated_route_shapes(&surface.routes);
    let routes_declared = !surface.routes.is_empty();
    if !routes_declared {
        coverage = worse(coverage, CoverageState::Unknown);
        notes.push(
            "no declared routes were supplied, so an observed route cannot be called \
             undocumented: an empty contract and an absent contract are indistinguishable"
                .to_string(),
        );
    }
    if surface.symbols.is_empty() {
        notes.push(
            "no declared symbols were supplied, so no span could be mapped to a symbol and \
             no dead-code finding can be made"
                .to_string(),
        );
    }

    // ---- correlate every span ------------------------------------------------
    let mut correlations: Vec<SpanCorrelation> = Vec::new();
    // Symbol ids observed at least once, so the dead-code pass is a set difference and
    // not a scan of the spans per symbol.
    let mut observed_symbols: BTreeSet<SymbolId> = BTreeSet::new();
    // observed shape -> (method, example path, services, invocations)
    let mut observed_routes: BTreeMap<RouteShape, (String, BTreeSet<String>, usize)> =
        BTreeMap::new();

    for record in records {
        for span in &record.spans {
            let (correlation, observed_shape) =
                correlate_span(span, surface, &index, routes_declared);
            if let Some(symbol) = &correlation.symbol {
                observed_symbols.insert(symbol.clone());
            }
            if let Some((key, example, services, count)) = observed_shape {
                let entry = observed_routes
                    .entry(key)
                    .or_insert_with(|| (example, BTreeSet::new(), 0));
                entry.1.extend(services);
                entry.2 += count;
            }
            coverage = worse(coverage, correlation.coverage);
            correlations.push(correlation);
        }
    }

    // ---- dead code: a set difference, never a claim about an unobserved universe --
    let mut dead_code: Vec<DeadCodeFinding> = Vec::new();
    let mut insufficient: Vec<InsufficientEvidence> = Vec::new();
    for symbol in &surface.symbols {
        if observed_symbols.contains(&symbol.id) {
            continue;
        }
        if windows_observed < required_windows {
            insufficient.push(InsufficientEvidence {
                subject: EvidenceSubject::Symbol(symbol.id.clone()),
                windows_observed,
                required_windows,
                reason: InsufficientReason::TooFewWindows,
            });
            continue;
        }
        if !options.complete_observation {
            // The export may be sampled, so absence is exactly what a sampled export
            // looks like for a function that ran. Calling that dead would be the
            // engine's own UNKNOWN-as-PASS, in the direction that hurts someone.
            insufficient.push(InsufficientEvidence {
                subject: EvidenceSubject::Symbol(symbol.id.clone()),
                windows_observed,
                required_windows,
                reason: InsufficientReason::ObservationMayBeSampled,
            });
            continue;
        }
        let finding_coverage = if options.dispatch == DispatchModel::Static {
            CoverageState::Verified
        } else {
            CoverageState::Partial
        };
        coverage = worse(coverage, finding_coverage);
        dead_code.push(DeadCodeFinding {
            symbol: symbol.id.clone(),
            path: symbol.path.clone(),
            observed_invocations: 0,
            windows_observed,
            coverage: finding_coverage,
        });
    }

    // ---- declared-but-uninvoked routes ---------------------------------------
    let mut uninvoked: Vec<UninvokedRoute> = Vec::new();
    if routes_declared {
        let matched: BTreeSet<DeclaredRoute> = correlations
            .iter()
            .filter_map(|correlation| correlation.route.clone())
            .collect();
        for route in &surface.routes {
            if matched.contains(route) {
                continue;
            }
            if windows_observed < required_windows {
                insufficient.push(InsufficientEvidence {
                    subject: EvidenceSubject::Route {
                        method: route.method.clone(),
                        template: route.shape(),
                    },
                    windows_observed,
                    required_windows,
                    reason: InsufficientReason::TooFewWindows,
                });
                continue;
            }
            if !options.complete_observation {
                // A sampled export is exactly what an uncalled route looks like, so the
                // same refusal applies here as for a symbol. Dynamic dispatch is not a
                // factor: a route is not reached by reflection.
                insufficient.push(InsufficientEvidence {
                    subject: EvidenceSubject::Route {
                        method: route.method.clone(),
                        template: route.shape(),
                    },
                    windows_observed,
                    required_windows,
                    reason: InsufficientReason::ObservationMayBeSampled,
                });
                continue;
            }
            uninvoked.push(UninvokedRoute {
                method: route.method.clone(),
                template: route.shape(),
                operation_id: route.operation_id.clone(),
                observed_invocations: 0,
                windows_observed,
                coverage: CoverageState::Verified,
            });
        }
    }

    // ---- undocumented routes -------------------------------------------------
    let mut undocumented: Vec<UndocumentedRoute> = Vec::new();
    if routes_declared {
        for ((shape, method), (example, services, invocations)) in observed_routes {
            let declared = surface.routes.iter().any(|route| {
                route.shape() == shape
                    && match (&route.method, &method) {
                        (None, _) => true,
                        (Some(declared), Some(observed)) => declared.eq_ignore_ascii_case(observed),
                        (Some(_), None) => true,
                    }
            });
            if declared {
                continue;
            }
            // Observed and not declared. `Partial`, never `Verified`: the engine knows
            // the route exists and knows no contract here names it, and it does not know
            // whether the contract it was handed covers this service.
            coverage = worse(coverage, CoverageState::Partial);
            undocumented.push(UndocumentedRoute {
                method,
                template: shape,
                example_path: example,
                services,
                invocations,
                coverage: CoverageState::Partial,
            });
        }
    }

    if !dead_code.is_empty() && options.dispatch == DispatchModel::Dynamic {
        notes.push(
            "dead-code findings are reported with `partial` coverage: this report asserts \
             only that no span named the symbol, and a dynamically dispatched call may reach \
             a function without naming it"
                .to_string(),
        );
    }
    if !duplicated.is_empty() {
        // Two contract operations on one shape+method cannot be told apart from an
        // observation, so an observed route may be attributed to the wrong operation.
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} route shape(s)/method pair(s) are declared more than once ({}), so an \
             observation of that shape cannot be attributed to a single operation",
            duplicated.len(),
            duplicated
                .iter()
                .cloned()
                .collect::<Vec<String>>()
                .join(", ")
        ));
    }
    // gRPC spans are identified (Step 40 puts them in the occupancy key) but not
    // correlated: the static surface for an RPC handler is a protobuf service, and no
    // adapter emits a `SymbolId` for one yet. Said out loud rather than left as an
    // unexplained absence from the correlation list.
    let rpc_spans = records
        .iter()
        .flat_map(|record| record.spans.iter())
        .filter(|span| span.rpc_service.is_some() || span.rpc_method.is_some())
        .count();
    if rpc_spans > 0 {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "{rpc_spans} span(s) carried `rpc.service`/`rpc.method`; no static surface is \
             modelled for RPC handlers, so they are not correlated and cannot mark a \
             symbol as invoked"
        ));
    }

    // A contract that names no method matches any observation, so a call with the wrong
    // method reads as documented. The engine will not silently treat that as a check.
    let method_agnostic = correlations
        .iter()
        .filter(|correlation| {
            correlation
                .route
                .as_ref()
                .is_some_and(|route| route.method.is_none())
        })
        .count();
    if method_agnostic > 0 {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "{method_agnostic} matched route(s) are declared without a method, so the \
             observed method was not checked against the contract"
        ));
    }

    let unplaced = correlations
        .iter()
        .filter(|correlation| !correlation.basis.is_placed())
        .count();
    if unplaced > 0 {
        notes.push(format!(
            "{unplaced} span(s) carried none of `http.route`, `code.function` or \
             `rpc.service`+`rpc.method`, so they could not be placed against the static \
             surface and cannot count as evidence that any symbol ran"
        ));
    }
    if !insufficient.is_empty() {
        let reasons: BTreeSet<&str> = insufficient
            .iter()
            .map(|entry| match entry.reason {
                InsufficientReason::TooFewWindows => "too few observation windows",
                InsufficientReason::ObservationMayBeSampled => "the exports may be sampled",
            })
            .collect();
        notes.push(format!(
            "{} symbol(s) had no observed invocation but no finding is made for them ({})",
            insufficient.len(),
            reasons.into_iter().collect::<Vec<_>>().join("; ")
        ));
    }

    // Sorted so the report does not depend on the order records were handed over.
    correlations.sort();
    dead_code.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    uninvoked.sort_by(|a, b| {
        a.template
            .cmp(&b.template)
            .then_with(|| a.method.cmp(&b.method))
    });
    insufficient.sort_by(|a, b| a.subject.cmp(&b.subject));
    undocumented.sort_by(|a, b| {
        a.template
            .cmp(&b.template)
            .then_with(|| a.method.cmp(&b.method))
    });
    notes.sort();
    notes.dedup();

    let mut report = TopologyReport {
        observation,
        correlations,
        dead_code,
        uninvoked_routes: uninvoked,
        insufficient_evidence: insufficient,
        undocumented_routes: undocumented,
        coverage,
        notes,
        digest: String::new(),
    };
    report.digest = digest_of(&report);
    report
}

/// The identity of a route as observed: its placeholder-erased shape and its method.
type RouteShape = (String, Option<String>);

/// A route observed at runtime: its shape, one concrete example path, the services that
/// served it, and how many times it was seen.
type RouteObservation = (RouteShape, String, BTreeSet<String>, usize);

/// Correlates one span, returning it and the route it contributed (if any).
fn correlate_span(
    span: &SpanRecord,
    surface: &DeclaredSurface,
    index: &SymbolIndex<'_>,
    routes_declared: bool,
) -> (SpanCorrelation, Option<RouteObservation>) {
    let mut coverage = span.coverage;
    let method = span.http_method.clone();
    let symbol_match = span
        .code_function
        .as_deref()
        .map(|name| index.lookup(name, span.code_filepath.as_deref()));
    let (symbol, symbol_basis) = match symbol_match {
        None => (None, None),
        Some(Ok(symbol)) => (
            Some(symbol.id.clone()),
            Some(CorrelationBasis::CodeFunction),
        ),
        Some(Err(Some(_candidates))) => {
            // Two symbols with the same name and no file to separate them. Picking one
            // would be a coin flip reported as a match, which a reader cannot
            // distinguish from a correct answer — and it would mark one code path alive
            // on no evidence at all.
            coverage = worse(coverage, CoverageState::Partial);
            (None, Some(CorrelationBasis::AmbiguousSymbol))
        }
        Some(Err(None)) => (None, None),
    };

    let mut basis = symbol_basis;
    let mut route = None;
    let mut route_basis = None;
    let mut observed_shape = None;

    if let Some(template) = &span.route {
        let shape = canonical_route_template(template);
        let services: BTreeSet<String> = span.service.iter().cloned().collect();
        observed_shape = Some((
            (shape.clone(), method.clone()),
            span.url_path.clone().unwrap_or_else(|| template.clone()),
            services,
            1,
        ));
        if routes_declared {
            let matched = match_declared_route(&surface.routes, template, method.as_deref());
            match matched.len() {
                1 => {
                    route = Some(matched[0].clone());
                    route_basis = Some(CorrelationBasis::RouteTemplate);
                }
                0 => {}
                _ => {
                    coverage = worse(coverage, CoverageState::Partial);
                }
            }
        }
    } else if let Some(path) = &span.url_path
        && routes_declared
    {
        // No route template, so the observed path is matched against declared
        // templates by subsumption. This is an *inference* — the concrete path may be a
        // special case of the template — so it degrades coverage even when it succeeds.
        let (matched, tied) = match_by_subsumption(&surface.routes, path, method.as_deref());
        if !tied.is_empty() {
            coverage = worse(coverage, CoverageState::Partial);
        }
        if let Some(matched) = matched {
            observed_shape = Some((
                (canonical_route_template(&matched.template), method.clone()),
                path.clone(),
                span.service.iter().cloned().collect(),
                1,
            ));
            route = Some(matched.clone());
            route_basis = Some(CorrelationBasis::ObservedPathSubsumption);
            coverage = worse(coverage, CoverageState::Partial);
        }
    }

    // A span that names both a symbol and a route is placed by both. The route is the
    // more specific fact for an HTTP span, so it becomes the reported basis while the
    // symbol binding is kept alongside it.
    if let Some(route_basis) = route_basis {
        basis = Some(route_basis);
    }
    if method.is_none() && route.is_some() {
        // The method could not be checked, so a declared route that does not serve this
        // method would still report a match.
        coverage = worse(coverage, CoverageState::Partial);
    }

    let basis = basis.unwrap_or(CorrelationBasis::Unmatched);
    if !basis.is_placed() {
        coverage = worse(coverage, CoverageState::Partial);
    }

    (
        SpanCorrelation {
            span_id_hash: span.span_id_hash.clone(),
            occupancy: span.occupancy_key(),
            symbol,
            route,
            basis,
            coverage,
        },
        observed_shape,
    )
}

fn worse(left: CoverageState, right: CoverageState) -> CoverageState {
    use CoverageState::{Partial, Unknown, Unsupported, Verified};
    match (left, right) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Partial, _) | (_, Partial) => Partial,
        (Verified, Verified) => Verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_names_are_erased_and_only_within_a_whole_segment() {
        assert_eq!(canonical_route_template("/users/{id}"), "/users/{}");
        assert_eq!(canonical_route_template("/users/{userId}"), "/users/{}");
        assert_eq!(canonical_route_template("/users/{}"), "/users/{}");
        // A placeholder sharing a segment with literal text is literal, so this
        // under-matches rather than over-matches.
        assert_eq!(
            canonical_route_template("/files/{name}.json"),
            "/files/{name}.json"
        );
        assert_eq!(canonical_route_template("/users/{a}/{b}"), "/users/{}/{}");
        assert_eq!(canonical_route_template("/users/"), "/users/");
        assert_eq!(canonical_route_template("/"), "/");
        // A query is already stripped by Step 40, but a template that somehow carries
        // one must not make two spellings of one route.
        assert_eq!(canonical_route_template("/users/{id}?x=1"), "/users/{}");
    }

    #[test]
    fn subsumption_requires_the_same_segment_count_and_reports_specificity() {
        let concrete = segments("/a/3");
        assert_eq!(subsumes(&concrete, &segments("/a/{id}")), Some(1));
        assert_eq!(subsumes(&concrete, &segments("/a/3")), Some(0));
        assert_eq!(subsumes(&concrete, &segments("/a/{id}/{x}")), None);
        assert_eq!(subsumes(&concrete, &segments("/b/{id}")), None);
    }

    #[test]
    fn a_name_only_lookup_reports_its_candidates_rather_than_choosing() {
        let single = DeclaredSymbol::new("s1", "src/a.ts", "charge");
        let index = SymbolIndex::build(std::slice::from_ref(&single));
        assert_eq!(index.lookup("charge", None).unwrap().id.as_str(), "s1");
        assert_eq!(
            index
                .lookup("charge", Some("src/a.ts"))
                .unwrap()
                .id
                .as_str(),
            "s1"
        );
        assert!(matches!(index.lookup("absent", None), Err(None)));

        let both = vec![
            DeclaredSymbol::new("s1", "src/a.ts", "charge"),
            DeclaredSymbol::new("s2", "src/b.ts", "charge"),
        ];
        let index = SymbolIndex::build(&both);
        // The file separates them.
        assert_eq!(
            index
                .lookup("charge", Some("src/b.ts"))
                .unwrap()
                .id
                .as_str(),
            "s2"
        );
        // Without a file, both are reported rather than one being chosen.
        match index.lookup("charge", None) {
            Err(Some(candidates)) => assert_eq!(candidates.len(), 2),
            other => panic!("expected two candidates, got {other:?}"),
        }
    }

    /// The shape in `routes` that is not `chosen` — used to state the tie expectation
    /// without hard-coding which of two equally specific templates the sort picks.
    fn deterministic_other_shape(chosen: &DeclaredRoute, routes: &[DeclaredRoute]) -> String {
        routes
            .iter()
            .map(DeclaredRoute::shape)
            .find(|shape| shape != &chosen.shape())
            .expect("the fixture declares two shapes")
    }

    #[test]
    fn a_declared_route_without_a_method_matches_any_observation() {
        let routes = vec![DeclaredRoute::new(None, "/users/{id}")];
        assert_eq!(
            match_declared_route(&routes, "/users/{ident}", Some("GET")).len(),
            1
        );
        let routes = vec![DeclaredRoute::new(Some("post"), "/users/{id}")];
        assert!(match_declared_route(&routes, "/users/{id}", Some("GET")).is_empty());
        assert_eq!(
            match_declared_route(&routes, "/users/{id}", Some("post")).len(),
            1
        );
    }

    #[test]
    fn subsumption_prefers_the_most_specific_template_and_reports_ties() {
        let routes = vec![
            DeclaredRoute::new(Some("GET"), "/a/{id}/{rest}"),
            DeclaredRoute::new(Some("GET"), "/a/{id}"),
        ];
        let (chosen, tied) = match_by_subsumption(&routes, "/a/3", Some("GET"));
        assert_eq!(chosen.map(|r| r.template.as_str()), Some("/a/{id}"));
        assert!(tied.is_empty());

        // `/a/{id}` and `/a/{name}` erase to one shape, so they are one route declared
        // twice rather than two candidates — and the choice is deterministic.
        let same_shape = vec![
            DeclaredRoute::new(Some("GET"), "/a/{name}"),
            DeclaredRoute::new(Some("GET"), "/a/{id}"),
        ];
        let (chosen, tied) = match_by_subsumption(&same_shape, "/a/3", Some("GET"));
        assert_eq!(chosen.map(|r| r.template.as_str()), Some("/a/{id}"));
        assert!(tied.is_empty(), "one shape is not an ambiguity");

        // A real tie: two *different* shapes, equally specific, both subsuming the path.
        let routes = vec![
            DeclaredRoute::new(Some("GET"), "/a/{id}"),
            DeclaredRoute::new(Some("GET"), "/{prefix}/3"),
        ];
        let (chosen, tied) = match_by_subsumption(&routes, "/a/3", Some("GET"));
        assert!(chosen.is_some());
        assert_eq!(
            tied.iter().cloned().collect::<Vec<String>>(),
            vec![deterministic_other_shape(chosen.unwrap(), &routes)],
            "the equally specific, structurally different template is named"
        );
    }
}
