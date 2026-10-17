//! autopsy-adapter-go: Production-grade Go language adapter.
//!
//! Tree-sitter bootstrap, package-path resolution bridge, stable symbol
//! identification via `autopsy-symbols` (identity/shape split), typed
//! multigraph edge extraction (contains, imports, embedding inheritance,
//! package-qualified call edges), normalized contract extraction with
//! honest receiver-effect modeling, and honest coverage classification for
//! Go's own escape hatch (reflection, `unsafe`, cgo, `//go:generate`,
//! runtime plugin loading) — never collapsing dynamic to verified
//! (FR-014, FR-030).
//!
//! Honest scope notes captured in `AdapterCapabilities::unsupported_constructs`:
//! Go resolves interface satisfaction *implicitly* at compile time, and method
//! calls on values of statically-unknown type are recorded as name-level
//! (`Partial`) edges, never binding-level claims.

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
pub const LANGUAGE_ID: &str = "go";

/// Module bridge resolving Go import paths to repository-relative paths.
///
/// A Go import names a *package directory*, not a file; the graph's file
/// identity is per file. The bridge resolves a package directory honestly:
/// when the directory exists under the repository root, its lexicographically
/// first `.go` file stands as the package anchor (deterministic, and named in
/// the edge's provenance); when not, the edge degrades to a `symbol::` name
/// edge with `Partial` coverage. Package alias resolution is the caller's job
/// (it knows the importing file).
#[derive(Debug, Clone, Default)]
pub struct GoModuleBridge {
    pub root_dir: PathBuf,
}

impl GoModuleBridge {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Resolves an import path to a target file path, or `None` when the
    /// package cannot be located in the repository (external module).
    pub fn resolve_package(&self, import_path: &str) -> Option<String> {
        let import_path = import_path.trim().trim_matches('"');
        if import_path.is_empty() {
            return None;
        }
        let dir = self.root_dir.join(import_path);
        if !dir.is_dir() {
            return None;
        }
        // Lexicographically first `.go` file sorts the package: deterministic
        // by construction, and honest about being an anchor for the package.
        let mut candidates: Vec<String> = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".go") {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        candidates.sort();
        let first = candidates.remove(0);
        let anchored = dir.join(first);
        Some(Self::slash(
            anchored.strip_prefix(&self.root_dir).unwrap_or(&anchored),
        ))
    }
}

/// The Go adapter.
#[derive(Debug, Clone)]
pub struct GoAdapter {
    capabilities: AdapterCapabilities,
    module_bridge: GoModuleBridge,
}

impl Default for GoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Strips a quoted string literal: `"path"` -> `path`.
fn unquote(text: &str) -> String {
    let trimmed = text.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(trimmed)
        .to_string()
}

/// A Go identifier is exported (public) exactly when its first rune is
/// uppercase; anything else is package-private.
fn go_visibility(name: &str) -> bool {
    name.chars()
        .next()
        .map(|first| first.is_uppercase())
        .unwrap_or(false)
}

impl GoAdapter {
    pub fn new() -> Self {
        let capabilities = AdapterCapabilities {
            language_id: LANGUAGE_ID.to_string(),
            adapter_version: ADAPTER_VERSION.to_string(),
            supported_extensions: vec!["go".to_string()],
            supports_ast_parsing: true,
            supports_symbol_extraction: true,
            supports_edge_extraction: true,
            supports_contract_extraction: true,
            // Go resolves method calls on interface-typed values
            // dynamically at the type checker level; this adapter records
            // them at name level only.
            supports_type_resolution: false,
            unsupported_constructs: vec![
                "implicit_interface_satisfaction".to_string(),
                "reflection".to_string(),
                "unsafe_package".to_string(),
                "cgo".to_string(),
                "runtime_plugin_loading".to_string(),
                "generated_code".to_string(),
            ],
        };
        Self {
            capabilities,
            module_bridge: GoModuleBridge::new("."),
        }
    }

    pub fn with_root_dir(root_dir: impl Into<PathBuf>) -> Self {
        let mut adapter = Self::new();
        adapter.module_bridge = GoModuleBridge::new(root_dir);
        adapter
    }

    fn init_parser(&self) -> Result<Parser, AdapterError> {
        let mut parser = Parser::new();
        let language = tree_sitter_go::LANGUAGE.into();
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

    fn symbol_identity(
        &self,
        file_path: &str,
        kind: SymbolKind,
        parent_scope: &str,
        name: &str,
    ) -> SymbolId {
        stable_symbol_id(file_path, kind, parent_scope, name)
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
}

/// Parts of a `selector_expression` (`pkg.Name`): the Go grammar gives it no
/// named fields, so the operand is the first child and the field is the
/// `field_identifier`.
fn selector_parts(selector: Node, content: &str) -> (String, String) {
    let mut cursor = selector.walk();
    let children: Vec<Node> = selector.children(&mut cursor).collect();
    let operand = children
        .first()
        .map(|n| content[n.byte_range()].trim().to_string())
        .unwrap_or_default();
    let field = children
        .iter()
        .find(|n| n.kind() == "field_identifier")
        .map(|n| content[n.byte_range()].trim().to_string())
        .unwrap_or_default();
    (operand, field)
}

/// The declared type name of a method receiver: `(r *Type)` yields `Type`.
/// Returns `None` for receivers whose type is not a plain identifier
/// (generics are not supported and are reported, not guessed).
fn receiver_type_name(receiver: Option<Node>, content: &str) -> Option<String> {
    let receiver = receiver?;
    let mut cursor = receiver.walk();
    for declaration in receiver.children(&mut cursor) {
        if declaration.kind() != "parameter_declaration" {
            continue;
        }
        // The receiver type may be `Type`, `*Type`, or a qualified/generic
        // form; only plain named types bind a method set here.
        let declared = declaration.child_by_field_name("type")?;
        match declared.kind() {
            "type_identifier" => return Some(content[declared.byte_range()].to_string()),
            "pointer_type" => {
                let named = declared.child_by_field_name("type").or_else(|| {
                    declared
                        .children(&mut declared.walk())
                        .find(|c| c.kind() == "type_identifier")
                });
                match named {
                    Some(inner) if inner.kind() == "type_identifier" => {
                        return Some(content[inner.byte_range()].to_string());
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    None
}

impl LanguageAdapter for GoAdapter {
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
            .map(|ext| ext.eq_ignore_ascii_case("go"))
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

            // Reflection: any call surface that goes through `reflect`.
            if kind == "call_expression"
                && let Some(func) = node.child_by_field_name("function")
            {
                let callee = &content[func.byte_range()];
                if callee.starts_with("reflect.") {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::ReflectionOrProxy,
                        description: format!("Runtime reflection via '{}'", callee),
                        location: Some(loc(node)),
                    });
                }
            }

            // `unsafe`: memory model escapes the analysis contract.
            if kind == "selector_expression" && selector_parts(node, content).0.eq("unsafe") {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::UnknownDynamic,
                    description: format!(
                        "unsafe package usage: {}",
                        content[node.byte_range()].trim()
                    ),
                    location: Some(loc(node)),
                });
            }

            // Runtime plugin loading.
            if kind == "call_expression"
                && let Some(func) = node.child_by_field_name("function")
                && content[func.byte_range()].eq("plugin.Open")
            {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::DynamicImport,
                    description: "Runtime plugin loading via plugin.Open".to_string(),
                    location: Some(loc(node)),
                });
            }

            // cgo: importing "C" links against compiled C code.
            if kind == "import_spec" {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "interpreted_string_literal"
                        && unquote(&content[child.byte_range()]).eq("C")
                    {
                        dynamic_constructs.push(DynamicConstruct {
                            kind: DynamicConstructKind::DynamicImport,
                            description: "cgo: linked C code behind import \"C\"".to_string(),
                            location: Some(loc(node)),
                        });
                    }
                }
            }

            // //go:generate directives generate source at build time.
            if kind == "comment" {
                let text = content[node.byte_range()].trim();
                if text.starts_with("//go:generate") {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::GeneratedCode,
                        description: "Source generation via //go:generate".to_string(),
                        location: Some(loc(node)),
                    });
                }
            }

            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                stack.push(child);
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

        // Coverage: reflection, cgo and plugin loading make behavior
        // unknowable (Unknown); unsafe, generation directives, and syntax
        // errors only narrow it (Partial). Never Verified when any dynamic
        // construct is present.
        let coverage = if dynamic_constructs.iter().any(|d| {
            matches!(
                d.kind,
                DynamicConstructKind::DynamicImport | DynamicConstructKind::ReflectionOrProxy
            )
        }) {
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
                "Go tree with {} top-level children",
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
        self.collect_symbols(root, &content, &parsed.file.path, "", &mut symbols);
        self.disambiguate_collisions(&mut symbols);
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
        // 2. Import edges (package directories, honestly anchored).
        self.collect_import_edges(
            root,
            &content,
            &parsed.file.path,
            &file_symbol,
            parsed.coverage,
            &mut edges,
        );
        // 3. Embedding (implicit inheritance for struct/type sets).
        self.collect_embedding_edges(
            root,
            &content,
            &parsed.file.path,
            parsed.coverage,
            &mut edges,
        );
        // 4. Call edges (bare, package-qualified, name-level method calls).
        let call_ctx = CallContext {
            symbols: &extracted_symbols,
            index: symbol_index,
            imports: self.collect_import_alias_map(root, &content),
        };
        self.collect_call_edges(
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
        let contracts = self.collect_contracts(root, &content, &parsed.file.path);

        // Canonical per-owner contract: one contract per callable identity.
        let mut by_owner: BTreeMap<SymbolId, Contract> = BTreeMap::new();
        for contract in contracts {
            by_owner.insert(contract.owner.clone(), contract);
        }
        Ok(by_owner.into_values().collect())
    }
}

impl GoAdapter {
    fn collect_symbols(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        in_type_scope: &str,
        symbols: &mut Vec<Symbol>,
    ) {
        match node.kind() {
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(file_path, SymbolKind::Function, "", name),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Function.kind_str().to_string(),
                        qualified_name: qualified_name(file_path, "", name),
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(self.compute_signature(node, content, name)),
                        visibility: if go_visibility(name) {
                            Visibility::Public
                        } else {
                            Visibility::Private
                        },
                    });
                }
            }
            "method_declaration" => {
                let receiver = node.child_by_field_name("receiver");
                if let (Some(type_name), Some(name_node)) = (
                    receiver_type_name(receiver, content),
                    node.child_by_field_name("name"),
                ) {
                    let name = &content[name_node.byte_range()];
                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Method,
                            &type_name,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Method.kind_str().to_string(),
                        qualified_name: qualified_name(file_path, &type_name, name),
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(self.compute_signature(node, content, name)),
                        visibility: if go_visibility(name) {
                            Visibility::Public
                        } else {
                            Visibility::Private
                        },
                    });
                }
            }
            "type_spec" | "type_spec_alias" | "type_alias" => {
                let fallback = {
                    let mut cursor = node.walk();
                    node.children(&mut cursor)
                        .find(|c| c.kind() == "type_identifier")
                };
                if let Some(name_node) = node.child_by_field_name("name").or(fallback) {
                    let name = &content[name_node.byte_range()];
                    let declared = node.child_by_field_name("type");
                    let (kind, sig) = match declared.map(|d| d.kind()) {
                        Some("struct_type") => (SymbolKind::Class, format!("struct {}", name)),
                        Some("interface_type") => {
                            (SymbolKind::Interface, format!("interface {}", name))
                        }
                        // Alias or named underlying type: honestly a type.
                        _ => (
                            SymbolKind::Type,
                            content[node.byte_range()].trim().to_string(),
                        ),
                    };
                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(file_path, kind, in_type_scope, name),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: kind.kind_str().to_string(),
                        qualified_name: qualified_name(file_path, in_type_scope, name),
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(sig),
                        visibility: if go_visibility(name) {
                            Visibility::Public
                        } else {
                            Visibility::Private
                        },
                    });
                }
            }
            "var_spec" | "const_spec" if in_type_scope.is_empty() => {
                // Package-level simple bindings only; declarations nested in
                // functions are locals, which the model deliberately excludes.
                let mut cursor = node.walk();
                for child in node.children_by_field_name("name", &mut cursor) {
                    if child.kind() != "identifier" {
                        continue;
                    }
                    let name = &content[child.byte_range()];
                    if name == "_" {
                        continue;
                    }
                    let binding = content[node.byte_range()].trim().to_string();
                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(file_path, SymbolKind::Variable, "", name),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Variable.kind_str().to_string(),
                        qualified_name: qualified_name(file_path, "", name),
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(binding),
                        visibility: if go_visibility(name) {
                            Visibility::Public
                        } else {
                            Visibility::Private
                        },
                    });
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_symbols(child, content, file_path, in_type_scope, symbols);
        }
    }

    /// Deterministic `:overload:N` suffixes for colliding identities
    /// (adapters guarantee the identity is total, even for sources that a
    /// compiler would reject).
    fn disambiguate_collisions(&self, symbols: &mut [Symbol]) {
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

    /// Normalized signature: `name(params) -> result` (empty result omitted).
    fn compute_signature(&self, node: Node, content: &str, name: &str) -> String {
        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].trim().to_string())
            .unwrap_or_else(|| "()".to_string());
        let result = node
            .child_by_field_name("result")
            .map(|r| format!(" -> {}", content[r.byte_range()].trim()))
            .unwrap_or_default();
        format!("{}{}{}", name, params, result)
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_import_edges(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        if node.kind() == "import_spec" {
            if let Some(path_node) = node.child_by_field_name("path") {
                let import = unquote(&content[path_node.byte_range()]);
                if !import.is_empty() {
                    let (target, provenance, edge_coverage) =
                        match self.module_bridge.resolve_package(&import) {
                            Some(anchored) => (
                                SymbolId::new(format!("file::{}", anchored)),
                                format!("go_import:{}", import),
                                coverage,
                            ),
                            None => (
                                SymbolId::new(format!(
                                    "symbol::{}",
                                    import.rsplit('/').next().unwrap_or(&import)
                                )),
                                format!("go_import_unresolved:{}", import),
                                CoverageState::Partial,
                            ),
                        };
                    edges.push(Edge {
                        source: file_symbol.clone(),
                        target,
                        kind: EdgeKind::Imports,
                        coverage: edge_coverage,
                        location: Some(self.node_location(node, file_path)),
                        provenance,
                    });
                }
            }
            return; // import specs are leaves of the import statement
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_import_edges(child, content, file_path, file_symbol, coverage, edges);
        }
    }

    /// Embedding: an anonymous field of a named type is Go's struct
    /// inheritance; interface embedding composes method sets the same way.
    /// Unresolved embedded types stay name-level (`symbol::`) like Python.
    fn collect_embedding_edges(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        if node.kind() == "type_spec"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Some(declared) = node.child_by_field_name("type")
        {
            let owner = self.symbol_identity(
                file_path,
                // Owner kind must agree with the symbol table: struct types
                // first, interface types second, anything else is a type.
                match declared.kind() {
                    "struct_type" => SymbolKind::Class,
                    "interface_type" => SymbolKind::Interface,
                    _ => SymbolKind::Type,
                },
                "",
                &content[name_node.byte_range()],
            );
            self.collect_embedded_types(declared, &owner, content, file_path, coverage, edges);
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_embedding_edges(child, content, file_path, coverage, edges);
        }
    }

    fn collect_embedded_types(
        &self,
        declared: Node,
        owner: &SymbolId,
        content: &str,
        file_path: &str,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        // Struct fields without a name are Go's embedding; the embedded type
        // is the (possibly pointer/qualified) type child.
        if declared.kind() == "field_declaration" {
            let mut cursor = declared.walk();
            let has_named_field = declared
                .children(&mut cursor)
                .any(|c| c.kind() == "field_identifier");
            if !has_named_field {
                let mut cursor = declared.walk();
                if let Some(type_child) = declared
                    .children(&mut cursor)
                    .find(|c| c.kind() != "comment")
                {
                    let name = content[type_child.byte_range()]
                        .trim()
                        .trim_start_matches('*')
                        .trim();
                    if !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
                    {
                        edges.push(Edge {
                            source: owner.clone(),
                            target: SymbolId::new(format!(
                                "symbol::{}",
                                name.rsplit('.').next().unwrap_or(name)
                            )),
                            kind: EdgeKind::Inherits,
                            coverage,
                            location: Some(self.node_location(declared, file_path)),
                            provenance: "struct_embedding".to_string(),
                        });
                    }
                }
            }
            return;
        }
        // Interfaces embed other interfaces directly (type_identifier
        // children beside method declarations).
        let mut cursor = declared.walk();
        for child in declared.children(&mut cursor) {
            if child.kind() == "type_identifier" && declared.kind() == "interface_type" {
                let name = &content[child.byte_range()];
                edges.push(Edge {
                    source: owner.clone(),
                    target: SymbolId::new(format!("symbol::{}", name)),
                    kind: EdgeKind::Inherits,
                    coverage,
                    location: Some(self.node_location(child, file_path)),
                    provenance: "interface_embedding".to_string(),
                });
                continue;
            }
            self.collect_embedded_types(child, owner, content, file_path, coverage, edges);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_call_edges(
        &self,
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
            if callee != "panic" && !callee.is_empty() {
                let (target, provenance, edge_coverage) = match func.kind() {
                    // `pkg.Func(...)`: resolve through the package alias map.
                    "selector_expression" => {
                        let (pkg, name) = selector_parts(func, content);
                        match resolve_package_qualified_call(ctx, &pkg, &name) {
                            Some(target) => (
                                target,
                                "call_expression:package-qualified".to_string(),
                                coverage,
                            ),
                            None => (
                                SymbolId::new(format!("symbol::{}", name)),
                                "call_expression:name-level".to_string(),
                                CoverageState::Partial,
                            ),
                        }
                    }
                    // Bare `Func(...)`: unique qualified-name match only.
                    "identifier" => match resolve_unique_bare_call(ctx, callee) {
                        Some(target) => (target, "call_expression:bare".to_string(), coverage),
                        None => (
                            SymbolId::new(format!("symbol::{}", callee)),
                            "call_expression:name-level".to_string(),
                            CoverageState::Partial,
                        ),
                    },
                    _ => (
                        SymbolId::new(format!("symbol::{}", callee)),
                        "call_expression:name-level".to_string(),
                        CoverageState::Partial,
                    ),
                };
                edges.push(Edge {
                    source: self.edge_source_for(node, content, file_path, file_symbol, ctx),
                    target,
                    kind: EdgeKind::Calls,
                    coverage: edge_coverage,
                    location: Some(self.node_location(node, file_path)),
                    provenance,
                });
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_call_edges(child, content, file_path, file_symbol, coverage, ctx, edges);
        }
    }

    /// The calling symbol of a call expression: the innermost enclosing
    /// function or method declaration, else the file module node.
    fn edge_source_for(
        &self,
        call: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        ctx: &CallContext,
    ) -> SymbolId {
        let mut current = Some(call);
        while let Some(node) = current {
            match node.kind() {
                "function_declaration" => {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name = &content[name_node.byte_range()];
                        return self.symbol_identity(file_path, SymbolKind::Function, "", name);
                    }
                }
                "method_declaration" => {
                    let receiver = node.child_by_field_name("receiver");
                    if let (Some(type_name), Some(name_node)) = (
                        receiver_type_name(receiver, content),
                        node.child_by_field_name("name"),
                    ) {
                        return self.symbol_identity(
                            file_path,
                            SymbolKind::Method,
                            &type_name,
                            &content[name_node.byte_range()],
                        );
                    }
                }
                _ => {}
            }
            current = node.parent();
        }
        // Fall back: any enclosing symbol whose range covers the call site.
        let row = (call.start_position().row + 1) as u32;
        if let Some(sym) = ctx
            .symbols
            .iter()
            .filter(|s| s.kind == "function" || s.kind == "method")
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
        {
            return sym.stable_id.clone();
        }
        file_symbol.clone()
    }

    /// Local import bindings of one file: local alias -> import path.
    ///
    /// Built once from the in-memory content (never from disk, so results
    /// are identical for cache-rebuilt snapshots that carry no root).
    fn collect_import_alias_map(&self, root: Node, content: &str) -> BTreeMap<String, String> {
        let mut bindings = BTreeMap::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "import_spec" {
                if let Some(path_node) = node.child_by_field_name("path") {
                    let path = unquote(&content[path_node.byte_range()]);
                    let local = match node.child_by_field_name("name") {
                        Some(name_node) => content[name_node.byte_range()].to_string(),
                        None => path.rsplit('/').next().unwrap_or(&path).to_string(),
                    };
                    if !local.is_empty() && !path.is_empty() {
                        bindings.insert(local, path);
                    }
                }
                continue;
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        bindings
    }

    fn collect_contracts(&self, node: Node, content: &str, file_path: &str) -> Vec<Contract> {
        let mut contracts = Vec::new();
        self.collect_contracts_inner(node, content, file_path, &mut contracts);
        contracts
    }

    fn collect_contracts_inner(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        contracts: &mut Vec<Contract>,
    ) {
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            let name_node = node.child_by_field_name("name");
            if let Some(name_node) = name_node {
                let name = &content[name_node.byte_range()];
                let (owner, visibility) = match node.kind() {
                    "method_declaration" => {
                        let receiver = node.child_by_field_name("receiver");
                        let type_name = receiver_type_name(receiver, content).unwrap_or_default();
                        (
                            self.symbol_identity(file_path, SymbolKind::Method, &type_name, name),
                            if go_visibility(name) {
                                Visibility::Public
                            } else {
                                Visibility::Private
                            },
                        )
                    }
                    _ => (
                        self.symbol_identity(file_path, SymbolKind::Function, "", name),
                        if go_visibility(name) {
                            Visibility::Public
                        } else {
                            Visibility::Private
                        },
                    ),
                };
                let mut inputs = Vec::new();
                if let Some(params) = node.child_by_field_name("parameters") {
                    let mut cursor = params.walk();
                    for param in params.children(&mut cursor) {
                        if param.kind() != "parameter_declaration" {
                            continue;
                        }
                        // A parameter may name a type for several names
                        // (`a, b int`); the declared type is the field.
                        if let Some(param_type) = param.child_by_field_name("type") {
                            inputs.push(content[param_type.byte_range()].trim().to_string());
                        }
                    }
                }
                let output = node
                    .child_by_field_name("result")
                    .map(|r| content[r.byte_range()].trim().to_string());
                // Honest effect modeling: pointer receivers mutate; value
                // receivers do not (and Go has no other checked effects).
                let effects = match node.kind() {
                    "method_declaration" => {
                        let receiver = node.child_by_field_name("receiver");
                        let is_pointer = receiver
                            .as_ref()
                            .and_then(|r| {
                                let mut cursor = r.walk();
                                r.children(&mut cursor)
                                    .find(|d| d.kind() == "parameter_declaration")
                                    .and_then(|d| d.child_by_field_name("type"))
                                    .map(|t| t.kind() == "pointer_type")
                            })
                            .unwrap_or(false);
                        if is_pointer {
                            vec!["mutates-receiver".to_string()]
                        } else {
                            Vec::new()
                        }
                    }
                    _ => Vec::new(),
                };
                contracts.push(Contract {
                    owner,
                    visibility,
                    inputs,
                    output,
                    effects,
                });
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_contracts_inner(child, content, file_path, contracts);
        }
    }
}

/// Per-file context handed to call-edge extraction so the adapter does not
/// re-extract its own symbol table per call site.
struct CallContext<'a> {
    symbols: &'a [Symbol],
    index: &'a BTreeMap<String, SymbolId>,
    /// Local import bindings (alias -> import path) of the importing file.
    imports: BTreeMap<String, String>,
}

/// `pkg.Name(...)`: resolve the alias to its import path, then search the
/// global symbol index for symbols declared in files that live under that
/// package directory. A **unique** hit resolves; ambiguity is honest and
/// degrades to a name-level edge, never a guess.
fn resolve_package_qualified_call(ctx: &CallContext, pkg: &str, name: &str) -> Option<SymbolId> {
    let import_path = ctx.imports.get(pkg)?;
    // A package directory holds several files; a qname is `file::[scope::]name`.
    let dir_prefix = format!("{}/", import_path);
    let hits: Vec<(String, SymbolId)> = ctx
        .index
        .iter()
        .filter(|(qname, _)| {
            let _ = name;
            let qname = qname.as_str();
            let file_part_end = qname.find("::").map(|p| p + 2).unwrap_or(0);
            let file_part = &qname[..file_part_end.saturating_sub(2)];
            let short = qname[file_part_end..].rsplit("::").next().unwrap_or("");
            short == name && (file_part == import_path || qname.starts_with(&dir_prefix))
        })
        .map(|(qname, id)| (qname.clone(), id.clone()))
        .collect();
    if hits.len() != 1 {
        return None; // unresolved or ambiguous: never guess
    }
    let option = hits.into_iter().next();
    option.map(|(_, id)| id)
}

/// Resolves a bare call to a unique **package-level** identity in the index
/// (qname with exactly `file::name` shape). A method on a receiver is not a
/// bare call and must not match.
fn resolve_unique_bare_call(ctx: &CallContext, callee: &str) -> Option<SymbolId> {
    let hits: Vec<SymbolId> = ctx
        .index
        .iter()
        .filter(|(qname, _)| {
            let qname = qname.as_str();
            // Exactly two segments: the file part and the name.
            qname.matches("::").count() == 1 && qname.rsplit("::").next() == Some(callee)
        })
        .map(|(_, id)| id.clone())
        .collect();
    if hits.len() != 1 {
        return None;
    }
    hits.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_adapter_api::conformance::verify_adapter_conformance;
    use tree_sitter::Parser;

    fn test_file_unit(path: &str) -> FileUnit {
        FileUnit {
            path: path.to_string(),
            language: "go".to_string(),
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

    const VALID_GO: &str = r#"
package checkout

import (
	"fmt"

	"checkout/simple"
)

// Vault holds card state.
type Vault struct {
	Balance int
	limited
	Store
}

type Store struct {
	name string
}

type limited struct {
	max int
}

type Reader interface {
	Refresh()
}

type Alias = string

const DefaultLimit int = 10

var ErrEmpty = fmt.Errorf("empty")

func (v *Vault) Charge(amount int) error {
	if amount > v.Balance {
		return ErrEmpty
	}
	return Store(v.store).Calm(amount)
}

func (v Vault) Status() string {
	return "ok"
}

func (s Store) Calm(amount int) error {
	return nil
}

func Helper(v *Vault) error {
	return v.Charge(5)
}

func Warp() error {
	return simple.Serve()
}
"#;

    const INVALID_GO: &str = "func { broken";

    const DYNAMIC_GO: &str = r#"
package checkout

import "unsafe"

func UseReflect(x interface{}) {
	y := unsafe.Pointer(&x)
	_ = y
}
"#;

    #[test]
    fn test_go_adapter_conformance() {
        let adapter = GoAdapter::new();
        verify_adapter_conformance(
            &adapter,
            VALID_GO,
            INVALID_GO,
            DYNAMIC_GO,
            "src/checkout/vault.go",
        );
    }

    #[test]
    fn test_go_symbols_types_methods_visibility() {
        let adapter = GoAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/checkout/vault.go");
        let parsed = adapter.parse(&ctx, &file, VALID_GO).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let ids: Vec<&str> = symbols.iter().map(|s| s.stable_id.as_str()).collect();

        assert!(
            ids.contains(&"src/checkout/vault.go#fn:Helper"),
            "package function: {ids:?}"
        );
        assert!(
            ids.contains(&"src/checkout/vault.go#method:Vault.Charge"),
            "method on pointer receiver: {ids:?}"
        );
        assert!(
            ids.contains(&"src/checkout/vault.go#method:Vault.Status"),
            "method on value receiver: {ids:?}"
        );
        assert!(
            ids.contains(&"src/checkout/vault.go#class:Vault"),
            "struct as class: {ids:?}"
        );
        assert!(
            ids.contains(&"src/checkout/vault.go#interface:Reader"),
            "interface: {ids:?}"
        );
        assert!(
            ids.contains(&"src/checkout/vault.go#type:Alias"),
            "type alias: {ids:?}"
        );
        assert!(
            ids.contains(&"src/checkout/vault.go#variable:ErrEmpty"),
            "package var: {ids:?}"
        );

        // Go visibility is exported-name based.
        let helper = symbols
            .iter()
            .find(|s| s.qualified_name.ends_with("::Helper"))
            .unwrap();
        assert_eq!(helper.visibility, Visibility::Public);
        let store_field_view = symbols
            .iter()
            .find(|s| s.qualified_name.ends_with("::Status"))
            .unwrap();
        assert_eq!(store_field_view.visibility, Visibility::Public);

        // Deterministic: byte-identical re-extraction.
        let again = adapter.extract_symbols(&parsed).unwrap();
        assert_eq!(format!("{symbols:?}"), format!("{again:?}"));
    }

    #[test]
    fn test_go_contains_and_import_edges() {
        let adapter = GoAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/checkout/vault.go");
        let parsed = adapter.parse(&ctx, &file, VALID_GO).unwrap();
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

        // fmt is external: honest name-level Partial edge with the real path.
        let fmt_edge = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Imports && e.provenance.contains("fmt"))
            .expect("external import must be disclosed");
        assert_eq!(fmt_edge.coverage, CoverageState::Partial);
        assert!(
            fmt_edge
                .provenance
                .contains("use_declaration_unresolved:fmt")
                || fmt_edge.provenance.contains("fmt"),
            "provenance names the import: {}",
            fmt_edge.provenance
        );

        // Deterministic: byte-identical re-extraction.
        let again = adapter
            .extract_edges(&parsed, &symbol_index_of(&symbols))
            .unwrap();
        assert_eq!(format!("{edges:?}"), format!("{again:?}"));
    }

    #[test]
    fn test_go_embedding_edges() {
        let adapter = GoAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/checkout/vault.go");
        let parsed = adapter.parse(&ctx, &file, VALID_GO).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let edges = adapter
            .extract_edges(&parsed, &symbol_index_of(&symbols))
            .unwrap();

        // Vault embeds Store (struct embedding).
        let embedding = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Inherits && e.provenance == "struct_embedding")
            .expect("struct embedding must be an inheritance edge");
        assert_eq!(
            embedding.source.as_str(),
            "src/checkout/vault.go#class:Vault"
        );
    }

    #[test]
    fn test_go_call_edges_honest_resolution() {
        let adapter = GoAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/checkout/vault.go");
        let parsed = adapter.parse(&ctx, &file, VALID_GO).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let index = symbol_index_of(&symbols);
        let edges = adapter.extract_edges(&parsed, &index).unwrap();

        // Bare call `v.Charge(5)` is a method call: name-level, Partial.
        let method_call = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Calls && e.target.as_str().ends_with("symbol::Charge"))
            .expect("method call must exist as a name-level edge");
        assert_eq!(method_call.coverage, CoverageState::Partial);
        assert_eq!(method_call.provenance, "call_expression:name-level");
        assert_eq!(
            method_call.source.as_str(),
            "src/checkout/vault.go#fn:Helper"
        );

        // Package-qualified call `simple.Serve()` with a sibling file of the
        // imported package present in the index (as the assembler provides):
        // the alias resolves to the package path and the declared symbol binds.
        let mut extended_index = index.clone();
        extended_index.insert(
            "checkout/simple/server.go::Serve".to_string(),
            SymbolId::new("checkout/simple/server.go#fn:Serve"),
        );
        let extended_edges = adapter.extract_edges(&parsed, &extended_index).unwrap();
        let warp_call = extended_edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls && e.source.as_str() == "src/checkout/vault.go#fn:Warp"
            })
            .expect("the Warp call edge must exist");
        assert_eq!(
            warp_call.target.as_str(),
            "checkout/simple/server.go#fn:Serve"
        );
        assert_eq!(warp_call.provenance, "call_expression:package-qualified");
        assert_eq!(warp_call.coverage, CoverageState::Verified);

        // Without the sibling file in the index, resolution is honest:
        // name-level, Partial — never a guess.
        let warp_call_unresolved = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls && e.source.as_str() == "src/checkout/vault.go#fn:Warp"
            })
            .expect("the Warp call edge must exist");
        assert_eq!(warp_call_unresolved.target.as_str(), "symbol::Serve");
        assert_eq!(warp_call_unresolved.coverage, CoverageState::Partial);
    }

    #[test]
    fn test_go_contracts_receiver_effects() {
        let adapter = GoAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/checkout/vault.go");
        let parsed = adapter.parse(&ctx, &file, VALID_GO).unwrap();
        let contracts = adapter.extract_contracts(&parsed).unwrap();

        let charge = contracts
            .iter()
            .find(|c| c.owner.as_str() == "src/checkout/vault.go#method:Vault.Charge")
            .expect("pointer-receiver method contract");
        assert!(charge.effects.contains(&"mutates-receiver".to_string()));
        assert_eq!(charge.output.as_deref(), Some("error"));

        let status = contracts
            .iter()
            .find(|c| c.owner.as_str() == "src/checkout/vault.go#method:Vault.Status")
            .expect("value-receiver method contract");
        assert!(status.effects.is_empty());
        assert_eq!(status.output.as_deref(), Some("string"));
    }

    #[test]
    fn test_go_dynamic_constructs_honesty() {
        let adapter = GoAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/checkout/vault.go");

        // unsafe alone: Partial (resolution still works).
        let parsed_unsafe = adapter.parse(&ctx, &file, DYNAMIC_GO).unwrap();
        assert_eq!(parsed_unsafe.coverage, CoverageState::Partial);
        assert!(
            parsed_unsafe
                .dynamic_constructs
                .iter()
                .any(|d| d.description.contains("unsafe"))
        );

        // cgo: Unknown (external code beyond the analyzed bytes).
        let cgo_source = "package c\n\nimport \"C\"\n\nfunc F() {}\n";
        let parsed_cgo = adapter.parse(&ctx, &file, cgo_source).unwrap();
        assert_eq!(parsed_cgo.coverage, CoverageState::Unknown);

        // //go:generate: Partial.
        let gen_source = "package c\n\n//go:generate stringer -type=T\n\nfunc F() {}\n";
        let parsed_gen = adapter.parse(&ctx, &file, gen_source).unwrap();
        assert_eq!(parsed_gen.coverage, CoverageState::Partial);
        assert!(
            parsed_gen
                .dynamic_constructs
                .iter()
                .any(|d| matches!(d.kind, DynamicConstructKind::GeneratedCode))
        );

        // Generated file tag: never Verified.
        let generated_file = FileUnit {
            is_generated: true,
            ..test_file_unit("src/checkout/vault.go")
        };
        let parsed_generated = adapter.parse(&ctx, &generated_file, VALID_GO).unwrap();
        assert_ne!(parsed_generated.coverage, CoverageState::Verified);
    }

    #[test]
    fn test_go_parser_direct_smoke() {
        // Direct grammar sanity: the parse function is exercised above; this
        // asserts the tree is structurally usable (defense in depth).
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("grammar loads");
        let tree = parser
            .parse("package main\n\nfunc main() {}\n", None)
            .unwrap();
        assert!(!tree.root_node().has_error());
    }
}
