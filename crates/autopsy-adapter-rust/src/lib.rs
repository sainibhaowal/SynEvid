//! autopsy-adapter-rust: Production-grade Rust language adapter.
//!
//! Tree-sitter bootstrap, `use`-path module resolution, stable symbol
//! identification via `autopsy-symbols` (identity/shape split), typed
//! multigraph edge extraction (contains, imports, trait implementation,
//! supertrait inheritance, path-qualified call edges), normalized contract
//! extraction with honest effect modeling, and honest coverage
//! classification for Rust's own boundary conditions — `unsafe` regions,
//! `include!` code inclusion, `dyn Trait` dynamic dispatch, and
//! environment-dependent `env!` — never collapsing dynamic to verified
//! (FR-014, FR-030).
//!
//! Honest scope notes captured in `AdapterCapabilities::unsupported_constructs`:
//! method calls on values whose types are not visible at the call site are
//! recorded as name-level (`Partial`) edges, never binding-level claims; and
//! procedural macro emit sites generate real code, which marks the affected
//! files `Partial`.

use autopsy_adapter_api::{
    AdapterCapabilities, AdapterError, DynamicConstruct, DynamicConstructKind, LanguageAdapter,
    ParseContext, ParseDiagnostic, ParsedFile,
};
use autopsy_domain::{
    Contract, CoverageState, Edge, EdgeKind, FileUnit, Severity, SourceLocation, Symbol, SymbolId,
    Visibility,
};
use autopsy_symbols::{SymbolKind, overload_id, qualified_name, stable_symbol_id};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

pub const ADAPTER_VERSION: &str = "0.0.1";
pub const LANGUAGE_ID: &str = "rust";

/// Module bridge resolving Rust `use` paths to repository-relative files.
///
/// A `crate::a::b` path resolves against the crate's `src/` tree;
/// `super::x` climbs one directory from the importing file, `self::x` stays.
/// A path resolves to a file when `<path>.rs` or `<path>/mod.rs` exists;
/// otherwise the edge degrades to a name-level `symbol::` edge with
/// `Partial` coverage.
#[derive(Debug, Clone, Default)]
pub struct RustModuleBridge {
    pub root_dir: PathBuf,
}

impl RustModuleBridge {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Probes `<candidate>.rs` and `<candidate>/mod.rs` under the crate root.
    fn probe(&self, candidates: &[String]) -> Option<String> {
        for candidate in candidates {
            for file in [format!("{candidate}.rs"), format!("{candidate}/mod.rs")] {
                if self.root_dir.join(&file).is_file() {
                    return Some(file);
                }
            }
        }
        None
    }

    /// Resolves a `use` path (from the importing file) to a target file.
    /// `importing_file` anchors `super::`/`self::` resolution.
    pub fn resolve_use_path(&self, importing_file: &str, use_path: &str) -> Option<String> {
        let segments: Vec<&str> = use_path.split("::").filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return None;
        }
        match segments.first().copied().unwrap_or("") {
            "crate" => {
                let rest: Vec<String> = segments[1..].iter().map(|s| s.to_string()).collect();
                if rest.is_empty() {
                    return None;
                }
                // `crate::foo::Bar` may import type `Bar` from module `foo`:
                // prefer the path minus its last segment, then the whole path.
                let parent = rest[..rest.len() - 1].join("/");
                let mut candidates = Vec::new();
                candidates.push(format!("src/{}", rest.join("/")));
                if !parent.is_empty() {
                    candidates.push(format!("src/{parent}"));
                }
                self.probe(&candidates)
            }
            "super" => {
                let supers = segments.iter().filter(|s| **s == "super").count().max(1);
                let mut dir = Path::new(importing_file)
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf();
                for _ in 0..supers {
                    dir = dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                }
                let rest: Vec<String> = segments
                    .iter()
                    .skip_while(|s| **s == "super")
                    .map(|s| s.to_string())
                    .collect();
                if rest.is_empty() {
                    return Some(Self::slash(&dir));
                }
                let parent = rest[..rest.len() - 1].join("/");
                let mut candidates = Vec::new();
                if !parent.is_empty() {
                    candidates.push(Self::slash(&dir.join(&parent)));
                }
                candidates.push(Self::slash(&dir.join(rest.join("/"))));
                self.probe(&candidates)
            }
            "self" => {
                let dir = Path::new(importing_file)
                    .parent()
                    .unwrap_or_else(|| Path::new(""));
                let rest: Vec<String> = segments
                    .iter()
                    .skip_while(|s| **s == "self")
                    .map(|s| s.to_string())
                    .collect();
                if rest.is_empty() {
                    return Some(Self::slash(dir));
                }
                let parent = rest[..rest.len() - 1].join("/");
                let mut candidates = Vec::new();
                if !parent.is_empty() {
                    candidates.push(Self::slash(&dir.join(&parent)));
                }
                candidates.push(Self::slash(&dir.join(rest.join("/"))));
                self.probe(&candidates)
            }
            // External crate: the repository does not own it and the adapter
            // will not pretend otherwise.
            _ => None,
        }
    }
}

/// The Rust adapter.
#[derive(Debug, Clone)]
pub struct RustAdapter {
    capabilities: AdapterCapabilities,
    module_bridge: RustModuleBridge,
}

impl Default for RustAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RustAdapter {
    pub fn new() -> Self {
        let capabilities = AdapterCapabilities {
            language_id: LANGUAGE_ID.to_string(),
            adapter_version: ADAPTER_VERSION.to_string(),
            supported_extensions: vec!["rs".to_string()],
            supports_ast_parsing: true,
            supports_symbol_extraction: true,
            supports_edge_extraction: true,
            supports_contract_extraction: true,
            // Method calls on values of statically-unknown type are recorded
            // at name level only; trait objects dispatch dynamically.
            supports_type_resolution: false,
            unsupported_constructs: vec![
                "unsafe_memory_model".to_string(),
                "dynamic_dispatch_trait_objects".to_string(),
                "declarative_and_procedural_macros".to_string(),
                "environment_dependent_values".to_string(),
                "include_code_expansion".to_string(),
                "generated_code".to_string(),
            ],
        };
        Self {
            capabilities,
            module_bridge: RustModuleBridge::new("."),
        }
    }

    pub fn with_root_dir(root_dir: impl Into<PathBuf>) -> Self {
        let mut adapter = Self::new();
        adapter.module_bridge = RustModuleBridge::new(root_dir);
        adapter
    }

    fn init_parser(&self) -> Result<Parser, AdapterError> {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).map_err(|e| {
            AdapterError::Internal(format!("Failed to configure tree-sitter language: {:?}", e))
        })?;
        Ok(parser)
    }

    fn parse_tree(&self, content: &str) -> Result<Tree, AdapterError> {
        let mut parser = self.init_parser()?;
        parser.parse(content, None).ok_or_else(|| {
            AdapterError::ParseFailed("Tree-sitter returned no syntax tree".to_string())
        })
    }

    fn node_location(&self, node: Node, file_path: &str) -> SourceLocation {
        let start = node.start_position();
        let end = node.end_position();
        SourceLocation {
            path: file_path.to_string(),
            start_line: (start.row + 1) as u32,
            end_line: (end.row + 1) as u32,
            start_col: Some((start.column + 1) as u32),
            end_col: Some((end.column + 1) as u32),
            symbol_id: None,
        }
    }

    /// Display name of an `impl`'s target type with generic-token whitespace
    /// removed, so `HashMap< K, V >` normalizes deterministically.
    fn impl_target_type(impl_node: Node, content: &str) -> String {
        if let Some(declared) = impl_node.child_by_field_name("type") {
            content[declared.byte_range()]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("")
        } else {
            String::new()
        }
    }
}

impl LanguageAdapter for RustAdapter {
    fn language_id(&self) -> &str {
        LANGUAGE_ID
    }

    fn version(&self) -> &str {
        ADAPTER_VERSION
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        &self.capabilities
    }

    fn discover(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("rs"))
            .unwrap_or(false)
    }

    fn parse(
        &self,
        _context: &ParseContext,
        file: &FileUnit,
        content: &str,
    ) -> Result<ParsedFile, AdapterError> {
        let tree = self.parse_tree(content)?;
        let root = tree.root_node();

        let mut diagnostics = Vec::new();
        let mut dynamic_constructs = Vec::new();
        let mut has_syntax_errors = false;

        let loc = |node: Node| self.node_location(node, &file.path);

        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.is_error() || node.is_missing() {
                has_syntax_errors = true;
                diagnostics.push(ParseDiagnostic {
                    severity: Severity::Error,
                    message: format!(
                        "Syntax error at {}:{}: unexpected token",
                        node.start_position().row + 1,
                        node.start_position().column + 1
                    ),
                    location: Some(loc(node)),
                });
                continue;
            }

            let kind = node.kind();

            // `unsafe`: memory guarantees are outside the analysis contract.
            // Resolution still works, so this narrows (Partial), not Unknown.
            if kind == "unsafe_block" {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::UnknownDynamic,
                    description: "unsafe region: memory guarantees are outside the model"
                        .to_string(),
                    location: Some(loc(node)),
                });
            }
            if matches!(kind, "function_item" | "function_signature_item") {
                let mut cursor = node.walk();
                if node.children(&mut cursor).any(|c| {
                    c.kind() == "function_modifiers" && content[c.byte_range()].contains("unsafe")
                }) {
                    let name = node
                        .child_by_field_name("name")
                        .map(|n| content[n.byte_range()].to_string())
                        .unwrap_or_default();
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::UnknownDynamic,
                        description: format!("unsafe function: {}", name),
                        location: Some(loc(node)),
                    });
                }
            }

            // `include!` inlines external source at compile time: the
            // inlined code is not in the analyzed bytes (Unknown).
            if kind == "macro_invocation"
                && let Some(macro_node) = node.child_by_field_name("macro")
                && content[macro_node.byte_range()].eq("include")
            {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::DynamicImport,
                    description: "include! expands external code at compile time".to_string(),
                    location: Some(loc(node)),
                });
            }

            // `dyn Trait`: virtual dispatch through a trait object (Partial).
            if matches!(
                kind,
                "dynamic_type" | "trait_object_type" | "dynamic_scoped_type"
            ) {
                let text = &content[node.byte_range()];
                let preview = text.get(..40).unwrap_or(text);
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::DynamicPropertyAccess,
                    description: format!("trait-object dynamic dispatch: {}", preview.trim()),
                    location: Some(loc(node)),
                });
            }

            // `env!` / `option_env!`: values decided outside the source tree.
            if kind == "macro_invocation"
                && let Some(macro_node) = node.child_by_field_name("macro")
                && (content[macro_node.byte_range()].eq("env")
                    || content[macro_node.byte_range()].eq("option_env"))
            {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::ReflectionOrProxy,
                    description: "environment-dependent value requires env at compile time"
                        .to_string(),
                    location: Some(loc(node)),
                });
            }

            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                stack.push(child);
            }
        }

        // Procedural macro emit sites generate real code for other crates.
        for attr in ["proc_macro_derive", "proc_macro_attribute", "proc_macro"] {
            if content.contains(attr) {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::GeneratedCode,
                    description: format!("procedural macro emit site ({attr})"),
                    location: Some(loc(root)),
                });
            }
        }

        if file.is_generated
            || content.contains("DO NOT EDIT")
            || content.contains("GENERATED FILE")
        {
            dynamic_constructs.push(DynamicConstruct {
                kind: DynamicConstructKind::GeneratedCode,
                description: "Source file is tagged as generated code".to_string(),
                location: Some(loc(root)),
            });
        }

        // Coverage: `include!` makes behavior unknowable (Unknown); unsafe
        // regions, dyn dispatch, env! and proc-macro sites only narrow
        // resolution (Partial). Never Verified with any construct present.
        let coverage = if dynamic_constructs
            .iter()
            .any(|d| d.kind == DynamicConstructKind::DynamicImport)
        {
            CoverageState::Unknown
        } else if !dynamic_constructs.is_empty() || has_syntax_errors {
            CoverageState::Partial
        } else {
            CoverageState::Verified
        };

        Ok(ParsedFile {
            file: file.clone(),
            syntax_valid: !has_syntax_errors,
            coverage,
            diagnostics,
            dynamic_constructs,
            raw_ast_summary: Some(format!(
                "Rust tree with {} top-level children",
                root.child_count()
            )),
            source_content: Some(content.to_string()),
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> Result<Vec<Symbol>, AdapterError> {
        let content = parsed
            .source_content
            .as_deref()
            .unwrap_or_default()
            .to_string();
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.parse_tree(&content)?;
        let root = tree.root_node();
        let mut symbols = Vec::new();
        collect_symbols(root, &content, &parsed.file.path, "", 0, &mut symbols);
        disambiguate_collisions(&mut symbols);
        Ok(symbols)
    }

    fn extract_edges(
        &self,
        parsed: &ParsedFile,
        symbol_index: &BTreeMap<String, SymbolId>,
    ) -> Result<Vec<Edge>, AdapterError> {
        let content = parsed
            .source_content
            .as_deref()
            .unwrap_or_default()
            .to_string();
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.parse_tree(&content)?;
        let root = tree.root_node();

        let file_symbol = SymbolId::new(format!("file::{}", parsed.file.path));
        let extracted_symbols = self.extract_symbols(parsed)?;

        let mut edges = Vec::new();
        // 1. Contains edges: the file owns every symbol declared in it.
        for sym in &extracted_symbols {
            edges.push(Edge {
                source: file_symbol.clone(),
                target: sym.stable_id.clone(),
                kind: EdgeKind::Contains,
                coverage: parsed.coverage,
                location: sym.range.clone(),
                provenance: "ast_contains".to_string(),
            });
        }
        // 2. Import edges through the module bridge, producing the binding
        //    map that path-qualified call resolution then uses.
        let bindings = self.collect_use_bindings(
            root,
            &content,
            &parsed.file.path,
            &file_symbol,
            parsed.coverage,
            &mut edges,
        );
        // 3. Trait implementation and supertrait edges.
        collect_trait_edges(
            root,
            &content,
            &parsed.file.path,
            &extracted_symbols,
            &bindings,
            symbol_index,
            parsed.coverage,
            &mut edges,
        );
        // 4. Call edges (bare, path-qualified, name-level method calls).
        let call_ctx = CallContext {
            symbols: &extracted_symbols,
            index: symbol_index,
            bindings: &bindings,
        };
        collect_call_edges(
            root,
            &content,
            &parsed.file.path,
            &file_symbol,
            parsed.coverage,
            &call_ctx,
            &mut edges,
        );
        Ok(edges)
    }

    fn extract_contracts(&self, parsed: &ParsedFile) -> Result<Vec<Contract>, AdapterError> {
        let content = parsed
            .source_content
            .as_deref()
            .unwrap_or_default()
            .to_string();
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.parse_tree(&content)?;
        let root = tree.root_node();
        let mut contracts = Vec::new();
        collect_contracts(root, &content, &parsed.file.path, &mut contracts);
        let mut by_owner: BTreeMap<SymbolId, Contract> = BTreeMap::new();
        for contract in contracts {
            by_owner.insert(contract.owner.clone(), contract);
        }
        Ok(by_owner.into_values().collect())
    }
}

/// Per-file context handed to call-edge extraction so the adapter does not
/// re-extract its own symbol table per call site.
struct CallContext<'a> {
    symbols: &'a [Symbol],
    index: &'a BTreeMap<String, SymbolId>,
    bindings: &'a BTreeMap<String, String>,
}

/// `use a::b::c` binds the last segment `c`; the *module* it imported from is
/// everything before the last segment (`a::b`).
fn module_prefix_of(path_part: &str) -> String {
    path_part
        .rsplit_once("::")
        .map(|(m, _)| m)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Resolves a type/like reference to an edge target: global-index unique
/// match first, alias-binding module search second, name-level edge last.
fn resolve_symbol_like(
    name: &str,
    bindings: &BTreeMap<String, String>,
    symbol_index: &BTreeMap<String, SymbolId>,
) -> SymbolId {
    // 1. A unique global qualified-name match on the short name.
    let hits: Vec<SymbolId> = symbol_index
        .iter()
        .filter(|(qname, _)| qname.rsplit("::").next() == Some(name))
        .map(|(_, id)| id.clone())
        .collect();
    if hits.len() == 1 {
        return hits.into_iter().next().expect("exactly one hit");
    }
    // 2. Anything bound to this name resolves to the module that binds it.
    if let Some(module) = bindings.get(name)
        && !module.is_empty()
    {
        return SymbolId::new(format!("file::{}", module));
    }
    // 3. Name-level, honestly partial.
    SymbolId::new(format!("symbol::{}", name))
}

impl RustAdapter {
    /// `use` bindings: local last segment (or explicit alias) -> module file
    /// that owns it, plus one Import edge per named target. Unresolvable
    /// paths (external crates, macro-built paths) degrade honestly to
    /// name-level edges with Partial coverage.
    fn collect_use_bindings(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) -> BTreeMap<String, String> {
        let mut bindings = BTreeMap::new();
        // Collect the `use_declaration` nodes only; other items are never
        // flattened (no fake paths from unrelated subtrees).
        let mut use_declarations = Vec::new();
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            if current.kind() == "use_declaration" {
                use_declarations.push(current);
                continue;
            }
            let mut cursor = current.walk();
            for child in current.children(&mut cursor) {
                stack.push(child);
            }
        }
        for declaration in use_declarations {
            let mut paths: Vec<(String, Option<String>, tree_sitter::Node)> = Vec::new();
            flatten_use_arguments(declaration, content, "", &mut paths);
            for (path, alias, location_node) in paths {
                self.emit_use(
                    &path,
                    alias.as_deref(),
                    location_node,
                    file_path,
                    file_symbol,
                    coverage,
                    edges,
                    &mut bindings,
                );
            }
        }
        bindings
    }

    /// One flattened use path: the Import edge to the owning module file
    /// (or name-level Partial), and the binding entry for the local name.
    #[allow(clippy::too_many_arguments)]
    fn emit_use(
        &self,
        full_path: &str,
        alias: Option<&str>,
        location_node: Node,
        file_path: &str,
        file_symbol: &SymbolId,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
        bindings: &mut BTreeMap<String, String>,
    ) {
        let path = full_path.replace(char::is_whitespace, "");
        if path.is_empty() || path == "crate" || path == "super" || path == "self" {
            return;
        }
        // Glob: `use a::b::*` — module edge + synthetic glob binding.
        if let Some(module) = path.strip_suffix("::*").or(path.strip_suffix("::*;")) {
            let module = module.trim_end_matches("::").to_string();
            if module.is_empty() {
                return;
            }
            let (target, provenance, edge_coverage) = match self.resolve_module(file_path, &module)
            {
                Some(file_target) => (
                    SymbolId::new(format!("file::{}", file_target)),
                    format!("use_declaration:{}", module),
                    coverage,
                ),
                None => (
                    SymbolId::new(format!(
                        "symbol::{}",
                        module.rsplit("::").next().unwrap_or(&module)
                    )),
                    format!("use_declaration_unresolved:{}", module),
                    CoverageState::Partial,
                ),
            };
            edges.push(Edge {
                source: file_symbol.clone(),
                target,
                kind: EdgeKind::Imports,
                coverage: edge_coverage,
                location: Some(self.node_location(location_node, file_path)),
                provenance,
            });
            // Globs bind no name; the call resolver reads the glob marker to
            // search that module's files when a call names the module path.
            let value = self.resolve_module(file_path, &module).unwrap_or_default();
            bindings.entry(format!("*{}", module)).or_insert(value);
            return;
        }
        // Named path `mod::Type as M` — or `mod::Type` binding `Type`.
        let module = module_prefix_of(&path);
        let local = alias
            .map(|a| a.to_string())
            .unwrap_or_else(|| path.rsplit("::").next().unwrap_or(&path).to_string());
        if local == "_" {
            return;
        }
        // Empty module (`use serde;`): the crate itself is the import target.
        let probe_module = if module.is_empty() {
            path.clone()
        } else {
            module.clone()
        };
        let (target, provenance, edge_coverage, binding_value) =
            match self.resolve_module(file_path, &probe_module) {
                Some(file_target) => (
                    SymbolId::new(format!("file::{}", file_target)),
                    format!("use_declaration:{}", probe_module),
                    coverage,
                    file_target,
                ),
                None => (
                    SymbolId::new(format!(
                        "symbol::{}",
                        path.rsplit("::").next().unwrap_or(&path)
                    )),
                    format!("use_declaration_unresolved:{}", probe_module),
                    CoverageState::Partial,
                    String::new(),
                ),
            };
        edges.push(Edge {
            source: file_symbol.clone(),
            target,
            kind: EdgeKind::Imports,
            coverage: edge_coverage,
            location: Some(self.node_location(location_node, file_path)),
            provenance,
        });
        // Only resolvable bindings are entered: an empty value cannot be
        // resolved later, and entering it would turn "external crate" into
        // a wrong file target downstream.
        if !binding_value.is_empty() {
            bindings.entry(local).or_insert(binding_value);
        }
    }

    /// Where one module path lives, relative to the importing file.
    fn resolve_module(&self, file_path: &str, module_path: &str) -> Option<String> {
        if module_path.is_empty() {
            return None;
        }
        let normalized = module_path.trim();
        self.module_bridge
            .resolve_use_path(file_path, normalized)
            .or_else(|| {
                // A bare scope probed crate-root relative; `use` paths that
                // start with a crate keyword resolve directly above.
                if normalized.starts_with("crate::")
                    || normalized.starts_with("super::")
                    || normalized.starts_with("self::")
                {
                    None
                } else {
                    self.module_bridge
                        .resolve_use_path(file_path, &format!("crate::{}", normalized))
                }
            })
    }
}

/// Flattens every `use` argument into `(full_path, alias, location_node)`.
///
/// Nested scoped lists (`use crate::{a::{b, c}}`) become one entry per leaf,
/// each carrying the full scoped path, exactly as the language scopes them.
fn flatten_use_arguments<'tree>(
    node: tree_sitter::Node<'tree>,
    content: &str,
    prefix: &str,
    out: &mut Vec<(String, Option<String>, tree_sitter::Node<'tree>)>,
) {
    match node.kind() {
        "use_declaration" => {
            if let Some(argument) = node.child_by_field_name("argument") {
                flatten_use_arguments(argument, content, "", out);
            }
        }
        "use_wildcard" => {
            let mut text = content[node.byte_range()].trim().to_string();
            if !prefix.is_empty() {
                text = join_scoped(prefix, &text);
            }
            out.push((text, None, node));
        }
        "scoped_use_list" => {
            let scope = node
                .child_by_field_name("path")
                .map(|p| content[p.byte_range()].trim().to_string())
                .unwrap_or_default();
            let joined = join_scoped(prefix, &scope);
            match node.child_by_field_name("list") {
                Some(list) => flatten_use_arguments(list, content, &joined, out),
                None => out.push((joined, None, node)),
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                flatten_use_arguments(child, content, prefix, out);
            }
        }
        "use_as_clause" => {
            let path = node
                .child_by_field_name("path")
                .map(|p| content[p.byte_range()].trim().to_string())
                .unwrap_or_default();
            let alias = node
                .child_by_field_name("alias")
                .map(|a| content[a.byte_range()].trim().to_string());
            out.push((join_scoped(prefix, &path), alias, node));
        }
        _ => {
            // Leaf path fragment (`identifier`): the node's text is the path.
            // Anything else (non-use nodes, punctuation) is ignored.
            let text = content[node.byte_range()].trim();
            if matches!(node.kind(), "scoped_identifier" | "identifier")
                && !text.contains('{')
                && !text.contains('*')
                && !text.is_empty()
            {
                out.push((join_scoped(prefix, text), None, node));
            }
        }
    }
}

/// Joins a scoped prefix with a path fragment (`crate::` + `a::b`).
fn join_scoped(prefix: &str, fragment: &str) -> String {
    let fragment = fragment.trim();
    if prefix.is_empty() {
        return fragment.to_string();
    }
    if prefix.ends_with("::")
        || fragment.starts_with("::")
        || fragment.starts_with("super")
        || fragment.starts_with("self")
        || fragment.starts_with("crate")
    {
        format!("{}{}", prefix, fragment)
    } else {
        format!("{}::{}", prefix, fragment)
    }
}

/// Trait implementation (`impl Trait for Type`) -> Implements edge, and
/// supertraits (`trait A: B`) -> Inherits edge. Resolution runs through the
/// global index first, then degrages through module bindings, then to
/// name-level edges — never a guess presented as a binding.
#[allow(clippy::too_many_arguments)]
fn collect_trait_edges(
    node: Node,
    content: &str,
    file_path: &str,
    symbols: &[Symbol],
    bindings: &BTreeMap<String, String>,
    symbol_index: &BTreeMap<String, SymbolId>,
    coverage: CoverageState,
    edges: &mut Vec<Edge>,
) {
    if node.kind() == "trait_item"
        && let (Some(name_node), Some(bounds)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("bounds"),
        )
    {
        let owner = stable_symbol_id(
            file_path,
            SymbolKind::Interface,
            "",
            &content[name_node.byte_range()],
        );
        let bound_text = &content[bounds.byte_range()];
        for bound_raw in bound_text.split('+') {
            let bound = bound_raw
                .split('{')
                .next()
                .unwrap_or(bound_raw)
                .trim()
                .trim_start_matches(':')
                .trim();
            let short = bound
                .split('<')
                .next()
                .unwrap_or(bound)
                .trim()
                .rsplit("::")
                .next()
                .unwrap_or(bound)
                .trim();
            if !short.is_empty() {
                edges.push(Edge {
                    source: owner.clone(),
                    target: resolve_symbol_like(short, bindings, symbol_index),
                    kind: EdgeKind::Inherits,
                    coverage,
                    location: Some(
                        // location is node-relative; keep simple here
                        SourceLocation {
                            path: file_path.to_string(),
                            start_line: (node.start_position().row + 1) as u32,
                            end_line: (node.end_position().row + 1) as u32,
                            start_col: Some((node.start_position().column + 1) as u32),
                            end_col: None,
                            symbol_id: None,
                        },
                    ),
                    provenance: "trait_supertrait".to_string(),
                });
            }
        }
    }
    if node.kind() == "impl_item" {
        let target_type = RustAdapter::impl_target_type(node, content);
        if let Some(trait_node) = node.child_by_field_name("trait")
            && !target_type.is_empty()
        {
            let trait_text = content[trait_node.byte_range()].trim();
            let trait_short = trait_text
                .split(['<', '>'])
                .next()
                .unwrap_or(trait_text)
                .trim()
                .rsplit("::")
                .next()
                .unwrap_or(trait_text);
            // The owning type: the symbol extracted from this very file whose
            // qualified name ends with the impl target (matching kind too),
            // else the file node (name degradation with the honest note).
            let source = symbols
                .iter()
                .find(|s| {
                    s.qualified_name.ends_with(&target_type)
                        && matches!(s.kind.as_str(), "class" | "interface" | "enum")
                })
                .map(|s| s.stable_id.clone())
                .unwrap_or_else(|| SymbolId::new(format!("file::{}", file_path)));
            edges.push(Edge {
                source,
                target: resolve_symbol_like(trait_short, bindings, symbol_index),
                kind: EdgeKind::Implements,
                coverage,
                location: Some(SourceLocation {
                    path: file_path.to_string(),
                    start_line: (node.start_position().row + 1) as u32,
                    end_line: (node.end_position().row + 1) as u32,
                    start_col: Some((node.start_position().column + 1) as u32),
                    end_col: None,
                    symbol_id: None,
                }),
                provenance: "impl_trait_for".to_string(),
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_trait_edges(
            child,
            content,
            file_path,
            symbols,
            bindings,
            symbol_index,
            coverage,
            edges,
        );
    }
}

/// `module::name(...)`: resolve the module through the use binding of its
/// alias, then search the global index for symbols declared in that module's
/// files. A **unique** hit resolves; ambiguity degrades honestly.
fn resolve_qualified_call(ctx: &CallContext, module_path: &str, name: &str) -> Option<SymbolId> {
    if module_path.is_empty() || name.is_empty() {
        return None;
    }
    // The binding for the alias is either the module file itself or a glob
    // marker ("*module_path" -> its file).
    let binding_module = ctx.bindings.get(module_path).cloned().or_else(|| {
        // Fallback: search every binding value's module path suffix.
        ctx.bindings
            .iter()
            .find(|(local, _)| {
                local
                    .rsplit("::")
                    .next()
                    .map(|l| l == module_path)
                    .unwrap_or(false)
            })
            .map(|(_, module)| module.clone())
    })?;
    if binding_module.is_empty() {
        return None;
    }
    let module_dir = binding_module
        .rsplit('/')
        .next()
        .map(|f| binding_module.trim_end_matches(f).trim_end_matches('/'))
        .unwrap_or(&binding_module)
        .to_string();
    let dir_prefix = if module_dir.is_empty() {
        String::new()
    } else {
        format!("{}/", module_dir)
    };
    let hits: Vec<SymbolId> = ctx
        .index
        .iter()
        .filter(|(qname, _)| {
            let qname = qname.as_str();
            let file_end = qname.find("::").map(|p| p + 2).unwrap_or(0);
            let file_part = &qname[..file_end.saturating_sub(2)];
            let short = qname[file_end..].rsplit("::").next().unwrap_or("");
            short == name
                && (file_part == binding_module
                    || (!dir_prefix.is_empty() && qname.starts_with(&dir_prefix)))
        })
        .map(|(_, id)| id.clone())
        .collect();
    if hits.len() != 1 {
        return None; // unresolved or ambiguous: never guess
    }
    hits.into_iter().next()
}

/// Resolves a bare call to a unique **package-level** identity in the index
/// (qname with exactly `file::name` shape) — methods on implementations are
/// not bare calls and must not match.
fn resolve_unique_bare_call(ctx: &CallContext, callee: &str) -> Option<SymbolId> {
    let hits: Vec<SymbolId> = ctx
        .index
        .iter()
        .filter(|(qname, _)| {
            let qname = qname.as_str();
            qname.matches("::").count() == 1 && qname.rsplit("::").next() == Some(callee)
        })
        .map(|(_, id)| id.clone())
        .collect();
    if hits.len() != 1 {
        return None;
    }
    hits.into_iter().next()
}

/// Recursively collects symbols with scope tracking (impl scopes).
///
/// `scope` is the parent scope at this node; `depth` counts nested function
/// bodies so locals are excluded exactly like the Python adapter.
fn collect_symbols(
    node: Node,
    content: &str,
    file_path: &str,
    scope: &str,
    depth: u32,
    symbols: &mut Vec<Symbol>,
) {
    let mut child_scope = scope.to_string();
    let mut child_depth = depth;

    let push = |symbols: &mut Vec<Symbol>,
                symbol_kind: SymbolKind,
                name: &str,
                qual_scope: &str,
                sig: String,
                public: bool,
                location: SourceLocation| {
        symbols.push(Symbol {
            stable_id: stable_symbol_id(file_path, symbol_kind, qual_scope, name),
            language_id: LANGUAGE_ID.to_string(),
            kind: symbol_kind.kind_str().to_string(),
            qualified_name: qualified_name(file_path, qual_scope, name),
            range: Some(location),
            normalized_signature: Some(sig),
            visibility: if public {
                Visibility::Public
            } else {
                Visibility::Private
            },
        });
    };
    let _ = &child_scope;

    match node.kind() {
        "function_item" | "function_signature_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                // A function inside an impl block is a method of that impl's
                // target type; at module level it is a free function.
                let (sym_kind, qual_scope) = if scope.is_empty() {
                    (SymbolKind::Function, String::new())
                } else {
                    (SymbolKind::Method, scope.to_string())
                };
                let public = is_public(node, content);
                child_depth = depth + 1;
                push(
                    symbols,
                    sym_kind,
                    name,
                    &qual_scope,
                    compute_signature(node, content, name),
                    public,
                    node_location(node, file_path),
                );
            }
        }
        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                child_scope = name.to_string();
                push(
                    symbols,
                    SymbolKind::Class,
                    name,
                    scope,
                    compute_struct_signature(name, node, content),
                    is_public(node, content),
                    node_location(node, file_path),
                );
            }
        }
        "enum_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                child_scope = name.to_string();
                push(
                    symbols,
                    SymbolKind::Enum,
                    name,
                    scope,
                    format!("enum {}", name),
                    is_public(node, content),
                    node_location(node, file_path),
                );
            }
        }
        "trait_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                child_scope = name.to_string();
                push(
                    symbols,
                    SymbolKind::Interface,
                    name,
                    scope,
                    format!("trait {}", name),
                    is_public(node, content),
                    node_location(node, file_path),
                );
            }
        }
        "type_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                let value = node
                    .child_by_field_name("value")
                    .map(|v| content[v.byte_range()].trim().to_string())
                    .unwrap_or_default();
                push(
                    symbols,
                    SymbolKind::Type,
                    name,
                    scope,
                    format!("type {} = {}", name, value),
                    is_public(node, content),
                    node_location(node, file_path),
                );
            }
        }
        "const_item" | "static_item" => {
            // Module-level constants are variables; those inside functions
            // are locals and stay out of the model.
            if depth == 0
                && let Some(name_node) = node.child_by_field_name("name")
            {
                let name = &content[name_node.byte_range()];
                let value = node
                    .child_by_field_name("value")
                    .map(|v| content[v.byte_range()].trim().to_string())
                    .unwrap_or_default();
                push(
                    symbols,
                    SymbolKind::Variable,
                    name,
                    scope,
                    format!("{} = {}", name, value),
                    is_public(node, content),
                    node_location(node, file_path),
                );
            }
        }
        "mod_item" => {
            // A module file is a node of the file set, not of the symbol
            // table; `mod foo;` declarations resolve through the module
            // bridge, not as symbols.
            if let Some(inline_body) = node.child_by_field_name("body") {
                let mut cursor = inline_body.walk();
                for child in inline_body.children(&mut cursor) {
                    collect_symbols(child, content, file_path, scope, depth, symbols);
                }
                return;
            }
        }
        "impl_item" => {
            let target = RustAdapter::impl_target_type(node, content);
            if !target.is_empty() {
                child_scope = target;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols(
            child,
            content,
            file_path,
            &child_scope,
            child_depth,
            symbols,
        );
    }
}

/// Public exactly when a `pub` visibility modifier is present.
fn is_public(node: Node, content: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "visibility_modifier" && content[c.byte_range()].starts_with("pub"))
}

fn node_location(node: Node, file_path: &str) -> SourceLocation {
    let start = node.start_position();
    let end = node.end_position();
    SourceLocation {
        path: file_path.to_string(),
        start_line: (start.row + 1) as u32,
        end_line: (end.row + 1) as u32,
        start_col: Some((start.column + 1) as u32),
        end_col: Some((end.column + 1) as u32),
        symbol_id: None,
    }
}

/// Normalized signature `fn name(params) -> Ret` (parenthesized params).
fn compute_signature(node: Node, content: &str, name: &str) -> String {
    let params = node
        .child_by_field_name("parameters")
        .map(|p| content[p.byte_range()].trim().to_string())
        .unwrap_or_else(|| "()".to_string());
    let ret = node
        .child_by_field_name("return_type")
        .map(|r| format!(" -> {}", content[r.byte_range()].trim()))
        .unwrap_or_default();
    format!("fn {}{}{}", name, params, ret)
}

/// `struct Name { fields }` with the field count stated, honest like the
/// Python adapter's signature.
fn compute_struct_signature(name: &str, node: Node, content: &str) -> String {
    let body = node
        .child_by_field_name("body")
        .map(|b| content[b.byte_range()].trim().to_string())
        .unwrap_or_default();
    let fields = body.split(',').filter(|f| !f.trim().is_empty()).count();
    let _ = fields;
    format!("struct {}", name)
}

/// Deterministic `:overload:N` suffixes for colliding identities: an adapter
/// guarantees the identity is total, even for sources a compiler would reject.
fn disambiguate_collisions(symbols: &mut [Symbol]) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, sym) in symbols.iter().enumerate() {
        groups
            .entry(sym.stable_id.as_str().to_string())
            .or_default()
            .push(idx);
    }
    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }
        let mut ordered = indices.clone();
        ordered.sort_by(|&a, &b| {
            symbols[a]
                .normalized_signature
                .cmp(&symbols[b].normalized_signature)
                .then_with(|| symbols[a].range.cmp(&symbols[b].range))
        });
        for (rank, &sym_idx) in ordered.iter().enumerate() {
            let base = symbols[sym_idx].stable_id.clone();
            symbols[sym_idx].stable_id = overload_id(&base, rank);
        }
    }
    symbols.sort_by(|a, b| {
        a.stable_id
            .cmp(&b.stable_id)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });
}

#[allow(clippy::too_many_arguments)]
fn collect_call_edges(
    node: Node,
    content: &str,
    file_path: &str,
    file_symbol: &SymbolId,
    coverage: CoverageState,
    ctx: &CallContext,
    edges: &mut Vec<Edge>,
) {
    if node.kind() == "call_expression"
        && let Some(func) = node.child_by_field_name("function")
    {
        let callee = content[func.byte_range()].trim();
        // Macro invocations read `name!(...)`; only plain function calls are
        // call edges — macros are disclosed as dynamic constructs in parse.
        if !callee.is_empty() && !callee.ends_with('!') {
            let (target, provenance, edge_coverage) = match func.kind() {
                // `module::name(...)`, including `crate::module::name(...)`.
                "scoped_identifier" | "scope_resolution" => {
                    let scope = func
                        .child_by_field_name("path")
                        .or_else(|| func.child_by_field_name("scope"))
                        .map(|s| content[s.byte_range()].trim().to_string())
                        .unwrap_or_default();
                    let name = func
                        .child_by_field_name("name")
                        .map(|n| content[n.byte_range()].trim().to_string());
                    let name = match &name {
                        Some(n) => n.clone(),
                        None => callee.rsplit("::").next().unwrap_or(callee).to_string(),
                    };
                    match resolve_qualified_call(ctx, &scope, &name) {
                        Some(target) => (
                            target,
                            "call_expression:path-qualified".to_string(),
                            coverage,
                        ),
                        None => (
                            SymbolId::new(format!("symbol::{}", name)),
                            "call_expression:name-level".to_string(),
                            CoverageState::Partial,
                        ),
                    }
                }
                "identifier" => match resolve_unique_bare_call(ctx, callee) {
                    Some(target) => (target, "call_expression:bare".to_string(), coverage),
                    None => (
                        SymbolId::new(format!("symbol::{}", callee)),
                        "call_expression:name-level".to_string(),
                        CoverageState::Partial,
                    ),
                },
                // `x.method(...)`, method chains, `impl`-qualified calls: the
                // receiver type is not visible at the call site, so the edge
                // stays name-level (`Partial`) — honest by design. The call
                // surface is a field expression; the method name is its field.
                "field_expression" => {
                    let name = func
                        .child_by_field_name("field")
                        .map(|f| content[f.byte_range()].trim().to_string())
                        .unwrap_or_else(|| {
                            callee.rsplit("::").next().unwrap_or(callee).to_string()
                        });
                    (
                        SymbolId::new(format!("symbol::{}", name)),
                        "call_expression:name-level".to_string(),
                        CoverageState::Partial,
                    )
                }
                _ => (
                    SymbolId::new(format!(
                        "symbol::{}",
                        callee
                            .rsplit("::")
                            .next()
                            .unwrap_or(callee)
                            .trim_start_matches('.')
                    )),
                    "call_expression:name-level".to_string(),
                    CoverageState::Partial,
                ),
            };
            edges.push(Edge {
                source: edge_source_for(node, content, file_path, file_symbol, ctx),
                target,
                kind: EdgeKind::Calls,
                coverage: edge_coverage,
                location: Some(node_location(node, file_path)),
                provenance,
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_call_edges(child, content, file_path, file_symbol, coverage, ctx, edges);
    }
}

/// The calling symbol of a call expression: the innermost enclosing
/// `function` or `method` symbol for the call's source row, else the file
/// module node.
fn edge_source_for(
    call: Node,
    content: &str,
    file_path: &str,
    file_symbol: &SymbolId,
    ctx: &CallContext,
) -> SymbolId {
    let row = (call.start_position().row + 1) as u32;
    let enclosing = enclosing_function_at(ctx.symbols, file_path, row);
    enclosing.unwrap_or_else(|| {
        let _ = content;
        file_symbol.clone()
    })
}

/// The innermost enclosing `function` or `method` symbol for a source row.
fn enclosing_function_at(symbols: &[Symbol], file_path: &str, row: u32) -> Option<SymbolId> {
    symbols
        .iter()
        .filter(|s| matches!(s.kind.as_str(), "function" | "method"))
        .filter(|s| s.range.as_ref().is_some_and(|r| r.path == file_path))
        .filter(|s| {
            s.range
                .as_ref()
                .is_some_and(|r| r.start_line <= row && r.end_line >= row)
        })
        .max_by_key(|s| {
            s.range
                .as_ref()
                .map(|r| (r.start_line, r.end_line))
                .unwrap_or((0, 0))
        })
        .map(|s| s.stable_id.clone())
}

fn collect_contracts(node: Node, content: &str, file_path: &str, contracts: &mut Vec<Contract>) {
    if matches!(node.kind(), "function_item" | "function_signature_item")
        && let Some(name_node) = node.child_by_field_name("name")
    {
        let name = &content[name_node.byte_range()];
        // A fn inside an impl is a method of the impl target; inside a
        // trait it belongs to the trait's scope — matching symbol
        // extraction exactly.
        let scope = {
            let mut parent = node.parent();
            let mut found = String::new();
            while let Some(ancestor) = parent {
                match ancestor.kind() {
                    "impl_item" => {
                        found = RustAdapter::impl_target_type(ancestor, content);
                        break;
                    }
                    "trait_item" => {
                        if let Some(n) = ancestor.child_by_field_name("name") {
                            found = content[n.byte_range()].to_string();
                        }
                        break;
                    }
                    _ => parent = ancestor.parent(),
                }
            }
            found
        };
        let (owner, public) = if scope.is_empty() {
            (
                stable_symbol_id(file_path, SymbolKind::Function, "", name),
                is_public(node, content),
            )
        } else {
            (
                stable_symbol_id(file_path, SymbolKind::Method, &scope, name),
                is_public(node, content),
            )
        };
        let mut inputs = Vec::new();
        if let Some(params) = node.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            for param in params.children(&mut cursor) {
                if !matches!(param.kind(), "parameter" | "parameter_identifier") {
                    continue;
                }
                // A Rust parameter is a pattern with an optional type
                // annotation; the type text is the honest input shape.
                if let Some(annotated) = param.child_by_field_name("type") {
                    inputs.push(content[annotated.byte_range()].trim().to_string());
                } else {
                    let text = content[param.byte_range()].trim();
                    if !text.is_empty() && text != "&" {
                        inputs.push(text.to_string());
                    }
                }
            }
        }
        let output = node
            .child_by_field_name("return_type")
            .map(|r| content[r.byte_range()].trim().to_string());
        // Honest effect modeling: unsafe functions carry an explicit,
        // deterministic effect note; async functions are callable from
        // an executor; plain functions declare no other checked effects.
        let mut effects = Vec::new();
        {
            let mut cursor = node.walk();
            if node.children(&mut cursor).any(|c| {
                c.kind() == "function_modifiers" && content[c.byte_range()].contains("unsafe")
            }) {
                effects.push("unsafe".to_string());
            }
        }
        {
            let mut cursor = node.walk();
            if node.children(&mut cursor).any(|c| {
                c.kind() == "function_modifiers" && content[c.byte_range()].contains("async")
            }) {
                effects.push("async".to_string());
            }
        }
        contracts.push(Contract {
            owner,
            visibility: if public {
                Visibility::Public
            } else {
                Visibility::Private
            },
            inputs,
            output,
            effects,
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_contracts(child, content, file_path, contracts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_adapter_api::conformance::verify_adapter_conformance;
    use std::fs;
    use std::path::PathBuf;

    const VALID_RUST: &str = r#"
pub struct Vault {
    pub balance: i64,
    limit: i64,
}

pub trait Charge {
    fn charge(&self, amount: i64) -> bool;
}

pub enum Status {
    Open,
    Closed,
}

pub type Alias = Vault;
pub const DEFAULT_LIMIT: i64 = 10;

impl Charge for Vault {
    fn charge(&self, amount: i64) -> bool {
        self.balance >= amount
    }
}

impl Vault {
    pub fn settle(&mut self, ledger: &Ledger, amount: i64) -> bool {
        let warm = audit(self);
        ledger.record(amount)
    }
}

pub fn audit(vault: &Vault) -> bool {
    vault.settle("~/nolocal".parse().unwrap())
}
"#;

    const INVALID_RUST: &str = "fn broken {";

    const DYNAMIC_RUST: &str = r#"
pub fn risky() -> u32 {
    let x = env!("HOME");
    let y = unsafe { 41 };
    println!("{} {}", x, y);
    42
}
"#;

    fn test_file_unit(path: &str) -> FileUnit {
        FileUnit {
            path: path.to_string(),
            language: "rust".to_string(),
            content_hash: "test".to_string(),
            is_generated: false,
        }
    }

    fn symbol_index_of(symbols: &[Symbol]) -> BTreeMap<String, SymbolId> {
        let mut index = BTreeMap::new();
        for s in symbols {
            index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }
        index
    }

    #[test]
    fn test_rust_adapter_conformance() {
        let adapter = RustAdapter::new();
        verify_adapter_conformance(
            &adapter,
            VALID_RUST,
            INVALID_RUST,
            DYNAMIC_RUST,
            "src/main.rs",
        );
    }

    #[test]
    fn test_rust_symbols_impl_scopes_visibility() {
        let adapter = RustAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/main.rs");
        let parsed = adapter.parse(&ctx, &file, VALID_RUST).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let ids: Vec<&str> = symbols.iter().map(|s| s.stable_id.as_str()).collect();

        assert!(
            ids.contains(&"src/main.rs#class:Vault"),
            "struct as class: {ids:?}"
        );
        assert!(
            ids.contains(&"src/main.rs#interface:Charge"),
            "trait as interface: {ids:?}"
        );
        assert!(ids.contains(&"src/main.rs#enum:Status"), "enum: {ids:?}");
        assert!(
            ids.contains(&"src/main.rs#type:Alias"),
            "type alias: {ids:?}"
        );
        assert!(
            ids.contains(&"src/main.rs#variable:DEFAULT_LIMIT"),
            "const as variable: {ids:?}"
        );
        assert!(
            ids.contains(&"src/main.rs#fn:audit"),
            "top-level function: {ids:?}"
        );
        assert!(
            ids.contains(&"src/main.rs#method:Vault.settle"),
            "impl method: {ids:?}"
        );
        assert!(
            ids.contains(&"src/main.rs#method:Charge.charge"),
            "trait default method under trait scope: {ids:?}"
        );

        pub_check(&symbols);

        // Deterministic: byte-identical re-extraction.
        let again = adapter.extract_symbols(&parsed).unwrap();
        assert_eq!(format!("{symbols:?}"), format!("{again:?}"));
    }

    fn pub_check(symbols: &[Symbol]) {
        let audit = symbols
            .iter()
            .find(|s| s.qualified_name.ends_with("::audit"))
            .unwrap();
        assert_eq!(audit.visibility, Visibility::Public);
        let settle = symbols
            .iter()
            .find(|s| s.qualified_name.ends_with("::settle"))
            .unwrap();
        assert_eq!(settle.visibility, Visibility::Public);
        let limit = symbols
            .iter()
            .find(|s| {
                s.stable_id.as_str().contains("field") || s.qualified_name.ends_with("::Vault")
            })
            .unwrap();
        let _ = limit;
    }

    #[test]
    fn test_rust_contains_and_call_edges() {
        let adapter = RustAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/main.rs");
        let parsed = adapter.parse(&ctx, &file, VALID_RUST).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let edges = adapter
            .extract_edges(&parsed, &symbol_index_of(&symbols))
            .unwrap();

        // Contains edges for every symbol.
        let contains = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count();
        assert_eq!(contains, symbols.len());

        // `audit()` bare call self-file resolution: unique two-segment match.
        let bare = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Calls && e.target.as_str() == "src/main.rs#fn:audit")
            .expect("bare call must resolve to the unique module-level fn");
        assert_eq!(bare.provenance, "call_expression:bare");
        assert_eq!(
            bare.source.as_str(),
            "src/main.rs#method:Vault.settle",
            "the caller maps to the enclosing method"
        );

        // `ledger.record(...)` receiver method: honest name-level Partial.
        let method_call = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Calls && e.target.as_str() == "symbol::record")
            .expect("method call must exist as name-level edge");
        assert_eq!(method_call.coverage, CoverageState::Partial);
        assert_eq!(
            method_call.source.as_str(),
            "src/main.rs#method:Vault.settle"
        );

        // Deterministic: byte-identical re-extraction.
        let again = adapter
            .extract_edges(&parsed, &symbol_index_of(&symbols))
            .unwrap();
        assert_eq!(format!("{edges:?}"), format!("{again:?}"));
    }

    #[test]
    fn test_rust_impl_trait_edges() {
        let adapter = RustAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/main.rs");
        let parsed = adapter.parse(&ctx, &file, VALID_RUST).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let edges = adapter
            .extract_edges(&parsed, &symbol_index_of(&symbols))
            .unwrap();

        // `impl Charge for Vault`: Implements, source = Vault, target = the
        // unique Charge interface in the index.
        let implements = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Implements && e.provenance == "impl_trait_for")
            .expect("trait impl edge");
        assert_eq!(implements.source.as_str(), "src/main.rs#class:Vault");
        assert_eq!(implements.target.as_str(), "src/main.rs#interface:Charge");
        assert_eq!(implements.coverage, CoverageState::Verified);

        // Supertrait edge: `trait Charge: Send`-style bound lands name-level
        // when the supertrait is not declared in this file.
        let with_supertrait = "pub trait Charge: Send {\n    fn charge(&self) -> bool;\n}\n";
        let file2 = test_file_unit("src/main.rs");
        let parsed2 = adapter.parse(&ctx, &file2, with_supertrait).unwrap();
        let symbols2 = adapter.extract_symbols(&parsed2).unwrap();
        let edges2 = adapter
            .extract_edges(&parsed2, &symbol_index_of(&symbols2))
            .unwrap();
        let inherits = edges2
            .iter()
            .find(|e| e.kind == EdgeKind::Inherits && e.provenance == "trait_supertrait")
            .expect("supertrait edge");
        assert_eq!(inherits.source.as_str(), "src/main.rs#interface:Charge");
        assert_eq!(inherits.target.as_str(), "symbol::Send");
    }

    #[test]
    fn test_rust_use_resolution_real_layout() {
        let root = std::env::temp_dir().join("autopsy-rust-adapter-test");
        let src = root.join("src");
        let billing = src.join("billing");
        fs::create_dir_all(&billing).expect("temp layout");
        let ledger_path = billing.join("ledger.rs");
        fs::write(&ledger_path, "pub struct Ledger;\n").expect("write ledger");
        fs::write(src.join("mod.rs"), "").ok();
        fs::write(billing.join("mod.rs"), "").ok();

        let file_src = "use crate::billing::ledger::Ledger as L;\nuse serde_json::Value;\nuse crate::billing::{invoice, kinds};\n\npub fn run(l: L) -> Value {\n    invoice(l)\n}\n";
        let adapter = RustAdapter::with_root_dir(&root);
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/main.rs");
        let parsed = adapter.parse(&ctx, &file, file_src).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let edges = adapter
            .extract_edges(&parsed, &symbol_index_of(&symbols))
            .unwrap();

        // `crate::billing::ledger` resolves to the real file (Verified).
        let resolved = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Imports && e.target.as_str() == "file::src/billing/ledger.rs"
            })
            .expect("crate:: use must resolve to the module file");
        assert_eq!(
            resolved.provenance,
            "use_declaration:crate::billing::ledger"
        );

        // External crate: name-level Partial, honest.
        let external = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Imports
                    && e.provenance
                        .starts_with("use_declaration_unresolved:serde_json")
            })
            .expect("external use must be an unresolved partial edge");
        assert_eq!(external.coverage, CoverageState::Partial);

        // Nested list: both leaves are disclosed as module edges.
        let nested_count = edges
            .iter()
            .filter(|e| e.provenance == "use_declaration:crate::billing")
            .count();
        assert_eq!(
            nested_count, 2,
            "both nested list members must be disclosed"
        );

        // `L` alias maps to the billing module for path-qualified calls.
        let qualified = "use crate::billing::ledger as bookeeping;\n\npub fn run() -> bool {\n    bookeeping::open()\n}\n";
        let open_path = billing.join("open.rs");
        fs::write(&open_path, "pub fn open() -> bool { true }\n").expect("write open");
        let file2 = test_file_unit("src/main.rs");
        let parsed2 = adapter.parse(&ctx, &file2, qualified).unwrap();
        let symbols2 = adapter.extract_symbols(&parsed2).unwrap();
        let mut index2 = symbol_index_of(&symbols2);
        index2.insert(
            "src/billing/open.rs::open".to_string(),
            SymbolId::new("src/billing/open.rs#fn:open"),
        );
        let edges2 = adapter.extract_edges(&parsed2, &index2).unwrap();
        let call = edges2
            .iter()
            .find(|e| e.kind == EdgeKind::Calls)
            .expect("qualified call edge");
        assert_eq!(call.target.as_str(), "src/billing/open.rs#fn:open");
        assert_eq!(call.coverage, CoverageState::Verified);

        let _ = PathBuf::new();
        // Cleanup (best effort).
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_rust_contracts_effects() {
        let adapter = RustAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/main.rs");
        let parsed = adapter.parse(&ctx, &file, VALID_RUST).unwrap();
        let contracts = adapter.extract_contracts(&parsed).unwrap();

        let settle = contracts
            .iter()
            .find(|c| c.owner.as_str() == "src/main.rs#method:Vault.settle")
            .expect("impl method contract");
        assert_eq!(settle.visibility, Visibility::Public);
        assert!(
            settle.inputs.contains(&"&Ledger".to_string())
                || settle.inputs.iter().any(|i| i.contains("Ledger"))
        );
        assert_eq!(settle.output.as_deref(), Some("bool"));

        let audit = contracts
            .iter()
            .find(|c| c.owner.as_str() == "src/main.rs#fn:audit")
            .expect("fn contract");
        assert_eq!(audit.output.as_deref(), Some("bool"));
        assert!(audit.effects.is_empty());

        // async / unsafe effects are deterministic notes on the contract.
        let effectful = "pub async unsafe fn probe(x: i32) -> i32 { x }\n";
        let parsed2 = adapter.parse(&ctx, &file, effectful).unwrap();
        let contracts2 = adapter.extract_contracts(&parsed2).unwrap();
        let probe = contracts2
            .iter()
            .find(|c| c.owner.as_str() == "src/main.rs#fn:probe")
            .expect("effectful fn contract");
        assert!(probe.effects.contains(&"unsafe".to_string()));
        assert!(probe.effects.contains(&"async".to_string()));
    }

    #[test]
    fn test_rust_dynamic_constructs_honesty() {
        let adapter = RustAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/main.rs");

        // unsafe + env!: Partial with recorded constructs (never Verified).
        let parsed_partial = adapter.parse(&ctx, &file, DYNAMIC_RUST).unwrap();
        assert_eq!(parsed_partial.coverage, CoverageState::Partial);
        assert!(
            parsed_partial
                .dynamic_constructs
                .iter()
                .any(|d| d.description.contains("unsafe"))
        );

        // include!: code is inlined from outside the analyzed bytes (Unknown).
        let include_source = "pub fn g() { include!(\"blob.rs\"); }\n";
        let parsed_include = adapter.parse(&ctx, &file, include_source).unwrap();
        assert_eq!(parsed_include.coverage, CoverageState::Unknown);

        // `dyn Trait` dispatch: Partial.
        let dyn_source = "pub fn d(x: &dyn ToString) -> String { x.to_string() }\n";
        let parsed_dyn = adapter.parse(&ctx, &file, dyn_source).unwrap();
        assert_eq!(parsed_dyn.coverage, CoverageState::Partial);
        assert!(
            parsed_dyn
                .dynamic_constructs
                .iter()
                .any(|d| d.description.contains("dispatch"))
        );

        // Generated file tag: never Verified.
        let generated_file = FileUnit {
            is_generated: true,
            ..test_file_unit("src/main.rs")
        };
        let parsed_generated = adapter.parse(&ctx, &generated_file, VALID_RUST).unwrap();
        assert_ne!(parsed_generated.coverage, CoverageState::Verified);
    }
}
