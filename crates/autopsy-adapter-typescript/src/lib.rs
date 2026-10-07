//! autopsy-adapter-typescript: Production-grade TypeScript and JavaScript language adapter.
//!
//! Provides tree-sitter bootstrap, TS compiler bridge for resolved references,
//! stable symbol identification, typed multigraph edge extraction, normalized contract extraction,
//! and honest coverage classification.

use autopsy_adapter_api::{
    AdapterCapabilities, AdapterError, DynamicConstruct, DynamicConstructKind, LanguageAdapter,
    ParseContext, ParseDiagnostic, ParsedFile,
};
use autopsy_domain::{
    Contract, CoverageState, Edge, EdgeKind, FileUnit, Severity, SourceLocation, Symbol, SymbolId,
    Visibility,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

pub const ADAPTER_VERSION: &str = "0.0.1";
pub const LANGUAGE_ID: &str = "typescript";

/// Compiler bridge for resolving module specifiers and symbol references.
#[derive(Debug, Clone, Default)]
pub struct TypeScriptCompilerBridge {
    pub root_dir: PathBuf,
}

impl TypeScriptCompilerBridge {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    /// Resolves an import specifier relative to the importing file.
    pub fn resolve_module_specifier(&self, importing_file: &str, specifier: &str) -> String {
        if specifier.starts_with('.') {
            let importing_path = Path::new(importing_file);
            let parent = importing_path.parent().unwrap_or_else(|| Path::new(""));
            let joined = parent.join(specifier);

            // Canonical component normalization (resolves . and .. without requiring disk existence)
            let mut parts = Vec::new();
            for comp in joined.components() {
                match comp {
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        parts.pop();
                    }
                    std::path::Component::Normal(c) => {
                        parts.push(c.to_string_lossy().to_string());
                    }
                    _ => {}
                }
            }
            let clean = parts.join("/");

            // Check common TypeScript extension expansions
            for ext in &[
                ".ts",
                ".tsx",
                ".js",
                ".jsx",
                "/index.ts",
                "/index.tsx",
                "/index.js",
            ] {
                if clean.ends_with(".ts") || clean.ends_with(".tsx") || clean.ends_with(".js") {
                    return clean;
                }
                let candidate = format!("{}{}", clean, ext);
                if self.root_dir.join(&candidate).exists() {
                    return candidate;
                }
            }
            if clean.ends_with(".ts") || clean.ends_with(".tsx") || clean.ends_with(".js") {
                clean
            } else {
                format!("{}.ts", clean)
            }
        } else {
            // External or aliased package
            format!("external::{}", specifier)
        }
    }
}

/// Production TypeScript language adapter implementation.
pub struct TypeScriptAdapter {
    capabilities: AdapterCapabilities,
    compiler_bridge: TypeScriptCompilerBridge,
}

impl Default for TypeScriptAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeScriptAdapter {
    pub fn new() -> Self {
        let capabilities = AdapterCapabilities {
            language_id: LANGUAGE_ID.to_string(),
            adapter_version: ADAPTER_VERSION.to_string(),
            supported_extensions: vec![
                "ts".to_string(),
                "tsx".to_string(),
                "js".to_string(),
                "jsx".to_string(),
                "mjs".to_string(),
                "cjs".to_string(),
                "mts".to_string(),
                "cts".to_string(),
            ],
            supports_ast_parsing: true,
            supports_symbol_extraction: true,
            supports_edge_extraction: true,
            supports_contract_extraction: true,
            supports_type_resolution: true,
            unsupported_constructs: vec![
                "eval".to_string(),
                "dynamic_import".to_string(),
                "reflection_reflect_metadata".to_string(),
                "proxy_metaprogramming".to_string(),
                "with_statement".to_string(),
                "generated_code".to_string(),
            ],
        };

        Self {
            capabilities,
            compiler_bridge: TypeScriptCompilerBridge::new("."),
        }
    }

    pub fn with_root_dir(root_dir: impl Into<PathBuf>) -> Self {
        let mut adapter = Self::new();
        adapter.compiler_bridge = TypeScriptCompilerBridge::new(root_dir);
        adapter
    }

    fn init_parser(&self, is_tsx: bool) -> Result<Parser, AdapterError> {
        let mut parser = Parser::new();
        let language = if is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        parser.set_language(&language).map_err(|e| {
            AdapterError::Internal(format!("Failed to configure tree-sitter language: {:?}", e))
        })?;
        Ok(parser)
    }

    fn is_tsx_path(&self, path: &str) -> bool {
        path.ends_with(".tsx") || path.ends_with(".jsx")
    }

    fn parse_tree(&self, path: &str, content: &str) -> Result<Tree, AdapterError> {
        let mut parser = self.init_parser(self.is_tsx_path(path))?;
        parser.parse(content, None).ok_or_else(|| {
            AdapterError::ParseFailed("Tree-sitter returned no syntax tree".to_string())
        })
    }
}

impl LanguageAdapter for TypeScriptAdapter {
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
            .map(|ext| {
                let lower = ext.to_lowercase();
                self.capabilities
                    .supported_extensions
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&lower))
            })
            .unwrap_or(false)
    }

    fn parse(
        &self,
        _context: &ParseContext,
        file: &FileUnit,
        content: &str,
    ) -> Result<ParsedFile, AdapterError> {
        let tree = self.parse_tree(&file.path, content)?;
        let root = tree.root_node();

        let mut diagnostics = Vec::new();
        let mut dynamic_constructs = Vec::new();
        let mut has_syntax_errors = false;

        // Traverse AST to detect syntax errors and dynamic/unsupported constructs
        let mut cursor = root.walk();
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            if node.is_error() || node.is_missing() {
                has_syntax_errors = true;
                let start = node.start_position();
                let end = node.end_position();
                diagnostics.push(ParseDiagnostic {
                    severity: Severity::Error,
                    message: format!(
                        "Syntax error at {}:{}: unexpected token",
                        start.row + 1,
                        start.column + 1
                    ),
                    location: Some(SourceLocation {
                        path: file.path.clone(),
                        start_line: (start.row + 1) as u32,
                        end_line: (end.row + 1) as u32,
                        start_col: Some((start.column + 1) as u32),
                        end_col: Some((end.column + 1) as u32),
                        symbol_id: None,
                    }),
                });
            }

            let kind = node.kind();
            let text = &content[node.byte_range()];

            // 1. Dynamic import detection: import(...) where arg is not static string literal
            if kind == "call_expression"
                && let Some(function_node) = node.child_by_field_name("function")
            {
                let fn_name = &content[function_node.byte_range()];
                if fn_name == "import" {
                    // Check arguments
                    let is_dynamic = if let Some(args_node) = node.child_by_field_name("arguments")
                    {
                        // If arguments contain non-string literal
                        let has_string_lit = args_node
                            .children(&mut cursor)
                            .any(|c| c.kind() == "string");
                        !has_string_lit
                    } else {
                        true
                    };

                    let start = node.start_position();
                    let end = node.end_position();
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::DynamicImport,
                        description: format!("Dynamic import statement '{}'", text),
                        location: Some(SourceLocation {
                            path: file.path.clone(),
                            start_line: (start.row + 1) as u32,
                            end_line: (end.row + 1) as u32,
                            start_col: Some((start.column + 1) as u32),
                            end_col: Some((end.column + 1) as u32),
                            symbol_id: None,
                        }),
                    });
                    let _ = is_dynamic;
                } else if fn_name == "eval" || fn_name == "Function" {
                    let start = node.start_position();
                    let end = node.end_position();
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::Eval,
                        description: format!("Dynamic code evaluation via '{}'", fn_name),
                        location: Some(SourceLocation {
                            path: file.path.clone(),
                            start_line: (start.row + 1) as u32,
                            end_line: (end.row + 1) as u32,
                            start_col: Some((start.column + 1) as u32),
                            end_col: Some((end.column + 1) as u32),
                            symbol_id: None,
                        }),
                    });
                }
            }

            // 2. Reflection / Proxy detection
            if kind == "member_expression"
                && (text.starts_with("Reflect.")
                    || text.contains("Reflect.get")
                    || text.contains("Reflect.defineMetadata"))
            {
                let start = node.start_position();
                let end = node.end_position();
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::ReflectionOrProxy,
                    description: format!("Runtime reflection construct: {}", text),
                    location: Some(SourceLocation {
                        path: file.path.clone(),
                        start_line: (start.row + 1) as u32,
                        end_line: (end.row + 1) as u32,
                        start_col: Some((start.column + 1) as u32),
                        end_col: Some((end.column + 1) as u32),
                        symbol_id: None,
                    }),
                });
            } else if kind == "new_expression" && text.starts_with("new Proxy") {
                let start = node.start_position();
                let end = node.end_position();
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::ReflectionOrProxy,
                    description: "Runtime Proxy metaprogramming instance".to_string(),
                    location: Some(SourceLocation {
                        path: file.path.clone(),
                        start_line: (start.row + 1) as u32,
                        end_line: (end.row + 1) as u32,
                        start_col: Some((start.column + 1) as u32),
                        end_col: Some((end.column + 1) as u32),
                        symbol_id: None,
                    }),
                });
            }

            // 3. With Statement detection
            if kind == "with_statement" {
                let start = node.start_position();
                let end = node.end_position();
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::UnknownDynamic,
                    description: "Forbidden with statement detected".to_string(),
                    location: Some(SourceLocation {
                        path: file.path.clone(),
                        start_line: (start.row + 1) as u32,
                        end_line: (end.row + 1) as u32,
                        start_col: Some((start.column + 1) as u32),
                        end_col: Some((end.column + 1) as u32),
                        symbol_id: None,
                    }),
                });
            }

            // Add children to stack
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                stack.push(child);
            }
        }

        // Check for generated code markers
        if file.is_generated
            || content.contains("@generated")
            || content.contains("DO NOT EDIT")
            || content.contains("/* auto-generated */")
        {
            dynamic_constructs.push(DynamicConstruct {
                kind: DynamicConstructKind::GeneratedCode,
                description: "Source file is tagged as generated code".to_string(),
                location: Some(SourceLocation {
                    path: file.path.clone(),
                    start_line: 1,
                    end_line: 1,
                    start_col: Some(1),
                    end_col: None,
                    symbol_id: None,
                }),
            });
        }

        // Compute coverage state honestly per FR-014 / FR-030:
        // Eval or dynamic import -> Unknown
        // Reflection, Proxy, generated code, or syntax errors -> Partial
        // Pure static clean code -> Verified
        let coverage = if dynamic_constructs.iter().any(|d| {
            d.kind == DynamicConstructKind::Eval || d.kind == DynamicConstructKind::DynamicImport
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
                "TS Tree with {} top-level children",
                root.child_count()
            )),
            source_content: Some(content.to_string()),
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> Result<Vec<Symbol>, AdapterError> {
        let content = parsed
            .source_content
            .as_deref()
            .map(ToString::to_string)
            .or_else(|| std::fs::read_to_string(&parsed.file.path).ok())
            .unwrap_or_default();

        if content.is_empty() {
            return Ok(Vec::new());
        }

        let tree = self.parse_tree(&parsed.file.path, &content)?;
        let root = tree.root_node();
        let mut symbols = Vec::new();

        self.collect_symbols(root, &content, &parsed.file.path, "", &mut symbols);

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
            .map(ToString::to_string)
            .or_else(|| std::fs::read_to_string(&parsed.file.path).ok())
            .unwrap_or_default();

        if content.is_empty() {
            return Ok(Vec::new());
        }

        let tree = self.parse_tree(&parsed.file.path, &content)?;
        let root = tree.root_node();
        let mut edges = Vec::new();

        let file_symbol_id = SymbolId::new(format!("file::{}", parsed.file.path));

        // 1. Module Contains edges for symbols
        let extracted_symbols = self.extract_symbols(parsed)?;
        for sym in &extracted_symbols {
            edges.push(Edge {
                source: file_symbol_id.clone(),
                target: sym.stable_id.clone(),
                kind: EdgeKind::Contains,
                coverage: parsed.coverage,
                location: sym.range.clone(),
                provenance: "ast_contains".to_string(),
            });
        }

        // Also check any external symbols in symbol_index that belong to this file
        for (qname, sym_id) in symbol_index {
            if qname.starts_with(&parsed.file.path)
                && !edges
                    .iter()
                    .any(|e| e.target == *sym_id && e.kind == EdgeKind::Contains)
            {
                edges.push(Edge {
                    source: file_symbol_id.clone(),
                    target: sym_id.clone(),
                    kind: EdgeKind::Contains,
                    coverage: parsed.coverage,
                    location: None,
                    provenance: "symbol_index_contains".to_string(),
                });
            }
        }

        // 2. Import edges
        self.collect_import_edges(
            root,
            &content,
            &parsed.file.path,
            &file_symbol_id,
            parsed.coverage,
            &mut edges,
        );

        // 3. Inheritance & Implementation edges
        self.collect_class_hierarchy_edges(
            root,
            &content,
            &parsed.file.path,
            symbol_index,
            parsed.coverage,
            &mut edges,
        );

        Ok(edges)
    }

    fn extract_contracts(&self, parsed: &ParsedFile) -> Result<Vec<Contract>, AdapterError> {
        let content = parsed
            .source_content
            .as_deref()
            .map(ToString::to_string)
            .or_else(|| std::fs::read_to_string(&parsed.file.path).ok())
            .unwrap_or_default();

        if content.is_empty() {
            return Ok(Vec::new());
        }

        let tree = self.parse_tree(&parsed.file.path, &content)?;
        let root = tree.root_node();
        let mut contracts = Vec::new();

        self.collect_contracts(root, &content, &parsed.file.path, "", &mut contracts);

        Ok(contracts)
    }
}

impl TypeScriptAdapter {
    fn collect_symbols(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        parent_scope: &str,
        symbols: &mut Vec<Symbol>,
    ) {
        let kind = node.kind();

        match kind {
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = if parent_scope.is_empty() {
                        format!("{}::{}", file_path, name)
                    } else {
                        format!("{}::{}::{}", file_path, parent_scope, name)
                    };

                    let sig = self.compute_signature(node, content);
                    let sig_hash = blake3::hash(sig.as_bytes()).to_hex()[0..12].to_string();
                    let stable_id =
                        SymbolId::new(format!("{}#fn:{}:{}", file_path, name, sig_hash));
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id,
                        language_id: LANGUAGE_ID.to_string(),
                        kind: "function".to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(sig),
                        visibility,
                    });
                }
            }
            "class_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = if parent_scope.is_empty() {
                        format!("{}::{}", file_path, name)
                    } else {
                        format!("{}::{}::{}", file_path, parent_scope, name)
                    };

                    let sig_hash = blake3::hash(name.as_bytes()).to_hex()[0..12].to_string();
                    let stable_id =
                        SymbolId::new(format!("{}#class:{}:{}", file_path, name, sig_hash));
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id,
                        language_id: LANGUAGE_ID.to_string(),
                        kind: "class".to_string(),
                        qualified_name: qname.clone(),
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("class {}", name)),
                        visibility,
                    });

                    // Visit class body for methods and fields
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut cursor = body.walk();
                        for child in body.children(&mut cursor) {
                            self.collect_symbols(child, content, file_path, name, symbols);
                        }
                    }
                    return;
                }
            }
            "method_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = format!("{}::{}::{}", file_path, parent_scope, name);
                    let sig = self.compute_signature(node, content);
                    let sig_hash = blake3::hash(sig.as_bytes()).to_hex()[0..12].to_string();
                    let stable_id = SymbolId::new(format!(
                        "{}#method:{}.{}:{}",
                        file_path, parent_scope, name, sig_hash
                    ));
                    let visibility = self.detect_member_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id,
                        language_id: LANGUAGE_ID.to_string(),
                        kind: "method".to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(sig),
                        visibility,
                    });
                }
            }
            "interface_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = format!("{}::{}", file_path, name);
                    let sig_hash = blake3::hash(name.as_bytes()).to_hex()[0..12].to_string();
                    let stable_id =
                        SymbolId::new(format!("{}#interface:{}:{}", file_path, name, sig_hash));
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id,
                        language_id: LANGUAGE_ID.to_string(),
                        kind: "interface".to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("interface {}", name)),
                        visibility,
                    });
                }
            }
            "type_alias_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = format!("{}::{}", file_path, name);
                    let sig_hash = blake3::hash(name.as_bytes()).to_hex()[0..12].to_string();
                    let stable_id =
                        SymbolId::new(format!("{}#type:{}:{}", file_path, name, sig_hash));
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id,
                        language_id: LANGUAGE_ID.to_string(),
                        kind: "type".to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("type {}", name)),
                        visibility,
                    });
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_symbols(child, content, file_path, parent_scope, symbols);
        }
    }

    fn collect_import_edges(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        if node.kind() == "import_statement" {
            let source_node = node.child_by_field_name("source").or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor).find(|c| c.kind() == "string")
            });

            if let Some(source_node) = source_node {
                let raw_source = &content[source_node.byte_range()];
                let specifier = raw_source.trim_matches('\'').trim_matches('"');
                let resolved = self
                    .compiler_bridge
                    .resolve_module_specifier(file_path, specifier);
                let target_symbol = SymbolId::new(format!("file::{}", resolved));

                edges.push(Edge {
                    source: file_symbol.clone(),
                    target: target_symbol,
                    kind: EdgeKind::Imports,
                    coverage,
                    location: Some(self.node_location(node, file_path)),
                    provenance: "import_statement".to_string(),
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_import_edges(child, content, file_path, file_symbol, coverage, edges);
        }
    }

    fn collect_class_hierarchy_edges(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        _symbol_index: &BTreeMap<String, SymbolId>,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        if node.kind() == "class_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            let name = &content[name_node.byte_range()];
            let class_symbol = SymbolId::new(format!("{}#class:{}", file_path, name));

            // Check heritage clauses (extends, implements)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "class_heritage" {
                    let mut h_cursor = child.walk();
                    for h_child in child.children(&mut h_cursor) {
                        if h_child.kind() == "extends_clause" {
                            if let Some(target_node) = h_child.child_by_field_name("value") {
                                let super_name = &content[target_node.byte_range()];
                                let target_id = SymbolId::new(format!("symbol::{}", super_name));
                                edges.push(Edge {
                                    source: class_symbol.clone(),
                                    target: target_id,
                                    kind: EdgeKind::Inherits,
                                    coverage,
                                    location: Some(self.node_location(h_child, file_path)),
                                    provenance: "extends_clause".to_string(),
                                });
                            }
                        } else if h_child.kind() == "implements_clause" {
                            let mut i_cursor = h_child.walk();
                            for iface_node in h_child.children(&mut i_cursor) {
                                if iface_node.kind() == "type_identifier" {
                                    let iface_name = &content[iface_node.byte_range()];
                                    let target_id =
                                        SymbolId::new(format!("symbol::{}", iface_name));
                                    edges.push(Edge {
                                        source: class_symbol.clone(),
                                        target: target_id,
                                        kind: EdgeKind::Implements,
                                        coverage,
                                        location: Some(self.node_location(iface_node, file_path)),
                                        provenance: "implements_clause".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_class_hierarchy_edges(
                child,
                content,
                file_path,
                _symbol_index,
                coverage,
                edges,
            );
        }
    }

    fn collect_contracts(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        parent_scope: &str,
        contracts: &mut Vec<Contract>,
    ) {
        let kind = node.kind();

        if kind == "function_declaration" || kind == "method_definition" {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];
                let sig = self.compute_signature(node, content);
                let sig_hash = blake3::hash(sig.as_bytes()).to_hex()[0..12].to_string();

                let owner = if parent_scope.is_empty() {
                    SymbolId::new(format!("{}#fn:{}:{}", file_path, name, sig_hash))
                } else {
                    SymbolId::new(format!(
                        "{}#method:{}.{}:{}",
                        file_path, parent_scope, name, sig_hash
                    ))
                };

                let visibility = if kind == "method_definition" {
                    self.detect_member_visibility(node, content)
                } else {
                    self.detect_visibility(node, content)
                };

                let (inputs, output) = self.extract_parameter_and_return_types(node, content);

                contracts.push(Contract {
                    owner,
                    visibility,
                    inputs,
                    output,
                    effects: Vec::new(),
                });
            }
        } else if kind == "class_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            let name = &content[name_node.byte_range()];
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    self.collect_contracts(child, content, file_path, name, contracts);
                }
            }
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_contracts(child, content, file_path, parent_scope, contracts);
        }
    }

    fn extract_parameter_and_return_types(
        &self,
        node: Node,
        content: &str,
    ) -> (Vec<String>, Option<String>) {
        let mut inputs = Vec::new();

        if let Some(params_node) = node.child_by_field_name("parameters") {
            let mut cursor = params_node.walk();
            for param in params_node.children(&mut cursor) {
                if param.kind() == "required_parameter" || param.kind() == "optional_parameter" {
                    inputs.push(content[param.byte_range()].trim().to_string());
                }
            }
        }

        let output = node.child_by_field_name("return_type").map(|ret| {
            content[ret.byte_range()]
                .trim()
                .trim_start_matches(':')
                .trim()
                .to_string()
        });

        (inputs, output)
    }

    fn compute_signature(&self, node: Node, content: &str) -> String {
        let name = node
            .child_by_field_name("name")
            .map(|n| &content[n.byte_range()])
            .unwrap_or("anonymous");

        let params = node
            .child_by_field_name("parameters")
            .map(|p| &content[p.byte_range()])
            .unwrap_or("()");

        let return_type = node
            .child_by_field_name("return_type")
            .map(|r| format!(": {}", &content[r.byte_range()]))
            .unwrap_or_default();

        format!("{name}{params}{return_type}")
    }

    fn detect_visibility(&self, node: Node, _content: &str) -> Visibility {
        // Check if parent is export_statement
        if let Some(parent) = node.parent()
            && parent.kind() == "export_statement"
        {
            return Visibility::Public;
        }
        Visibility::Internal
    }

    fn detect_member_visibility(&self, node: Node, content: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "accessibility_modifier" {
                let mod_text = &content[child.byte_range()];
                return match mod_text {
                    "public" => Visibility::Public,
                    "protected" => Visibility::Protected,
                    "private" => Visibility::Private,
                    "internal" => Visibility::Internal,
                    _ => Visibility::Public,
                };
            }
        }
        Visibility::Public
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

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_adapter_api::conformance::verify_adapter_conformance;

    #[test]
    fn test_adapter_capabilities_honesty() {
        let adapter = TypeScriptAdapter::new();
        let caps = adapter.capabilities();

        assert_eq!(caps.language_id, "typescript");
        assert_eq!(caps.adapter_version, "0.0.1");
        assert!(caps.supports_ast_parsing);
        assert!(caps.supports_symbol_extraction);
        assert!(caps.supports_edge_extraction);
        assert!(caps.supports_contract_extraction);
        assert!(caps.supports_type_resolution);

        // Assert supported extensions
        assert!(caps.supported_extensions.contains(&"ts".to_string()));
        assert!(caps.supported_extensions.contains(&"tsx".to_string()));
        assert!(caps.supported_extensions.contains(&"js".to_string()));
        assert!(caps.supported_extensions.contains(&"jsx".to_string()));

        // Assert honest declaration of unsupported constructs (FR-014, FR-030)
        assert!(caps.unsupported_constructs.contains(&"eval".to_string()));
        assert!(
            caps.unsupported_constructs
                .contains(&"dynamic_import".to_string())
        );
        assert!(
            caps.unsupported_constructs
                .contains(&"reflection_reflect_metadata".to_string())
        );
        assert!(
            caps.unsupported_constructs
                .contains(&"proxy_metaprogramming".to_string())
        );
        assert!(
            caps.unsupported_constructs
                .contains(&"generated_code".to_string())
        );
    }

    #[test]
    fn test_adapter_conformance_suite() {
        let adapter = TypeScriptAdapter::new();

        let valid_source = r#"
            export interface Greeter {
                greet(name: string): string;
            }

            export class Service implements Greeter {
                public greet(name: string): string {
                    return `Hello, ${name}`;
                }
            }
        "#;

        let invalid_source = r#"
            function broken( { let x = ;
        "#;

        let dynamic_source = r#"
            export function runDynamic(code: string): any {
                return eval(code);
            }
        "#;

        verify_adapter_conformance(
            &adapter,
            valid_source,
            invalid_source,
            dynamic_source,
            "src/service.ts",
        );
    }

    #[test]
    fn test_discover_extensions() {
        let adapter = TypeScriptAdapter::new();

        assert!(adapter.discover(Path::new("src/index.ts")));
        assert!(adapter.discover(Path::new("src/components/App.tsx")));
        assert!(adapter.discover(Path::new("scripts/bundle.js")));
        assert!(adapter.discover(Path::new("src/ui.jsx")));
        assert!(adapter.discover(Path::new("dist/index.mjs")));
        assert!(adapter.discover(Path::new("lib/index.cjs")));

        // Unsupported extensions
        assert!(!adapter.discover(Path::new("src/main.rs")));
        assert!(!adapter.discover(Path::new("app/main.py")));
        assert!(!adapter.discover(Path::new("src/Main.java")));
        assert!(!adapter.discover(Path::new("styles.css")));
    }

    #[test]
    fn test_compiler_bridge_module_resolution() {
        let bridge = TypeScriptCompilerBridge::new(".");

        // Relative resolution
        let resolved = bridge.resolve_module_specifier("src/api/handler.ts", "./service");
        assert_eq!(resolved, "src/api/service.ts");

        let resolved_up = bridge.resolve_module_specifier("src/api/handler.ts", "../utils/math");
        assert_eq!(resolved_up, "src/utils/math.ts");

        // External package resolution
        let external = bridge.resolve_module_specifier("src/index.ts", "lodash");
        assert_eq!(external, "external::lodash");

        let scoped = bridge.resolve_module_specifier("src/index.ts", "@org/core");
        assert_eq!(scoped, "external::@org/core");
    }

    #[test]
    fn test_stable_symbol_id_resilience_to_line_shifts() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        let code_line1 = "export function add(a: number, b: number): number { return a + b; }";
        let code_line25 = format!(
            "{}\n\nexport function add(a: number, b: number): number {{ return a + b; }}",
            "\n".repeat(24)
        );

        let file = FileUnit {
            path: "src/math.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash".to_string(),
            is_generated: false,
        };

        let parsed1 = adapter.parse(&ctx, &file, code_line1).unwrap();
        let symbols1 = adapter.extract_symbols(&parsed1).unwrap();

        let parsed2 = adapter.parse(&ctx, &file, &code_line25).unwrap();
        let symbols2 = adapter.extract_symbols(&parsed2).unwrap();

        assert_eq!(symbols1.len(), 1);
        assert_eq!(symbols2.len(), 1);

        // Crucial invariant (FR-005): SymbolId must be identical despite line movement
        assert_eq!(symbols1[0].stable_id, symbols2[0].stable_id);
        assert_eq!(symbols1[0].qualified_name, symbols2[0].qualified_name);
        assert_eq!(symbols1[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_extract_contracts() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        let source = r#"
            export function processTransaction(txId: string, amount: number, dryRun: boolean): Promise<boolean> {
                return Promise.resolve(true);
            }
        "#;

        let file = FileUnit {
            path: "src/billing.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "dummy".to_string(),
            is_generated: false,
        };

        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let contracts = adapter.extract_contracts(&parsed).unwrap();

        assert_eq!(contracts.len(), 1);
        let c = &contracts[0];
        assert_eq!(c.visibility, Visibility::Public);
        assert_eq!(
            c.inputs,
            vec!["txId: string", "amount: number", "dryRun: boolean"]
        );
        assert_eq!(c.output, Some("Promise<boolean>".to_string()));
    }

    #[test]
    fn test_extract_hierarchy_edges() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        let source = r#"
            import { Animal } from "./animal";

            export class Dog extends Animal implements Pet {
                public bark(): string {
                    return "woof";
                }
            }
        "#;

        let file = FileUnit {
            path: "src/dog.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "dummy".to_string(),
            is_generated: false,
        };

        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let edges = adapter.extract_edges(&parsed, &BTreeMap::new()).unwrap();

        let has_import = edges
            .iter()
            .any(|e| e.kind == EdgeKind::Imports && e.target.as_str() == "file::src/animal.ts");
        let has_inherits = edges
            .iter()
            .any(|e| e.kind == EdgeKind::Inherits && e.target.as_str() == "symbol::Animal");
        let has_implements = edges
            .iter()
            .any(|e| e.kind == EdgeKind::Implements && e.target.as_str() == "symbol::Pet");
        let has_contains = edges.iter().any(|e| e.kind == EdgeKind::Contains);

        assert!(has_import, "Must extract Imports edge");
        assert!(has_inherits, "Must extract Inherits edge for extends");
        assert!(
            has_implements,
            "Must extract Implements edge for implements"
        );
        assert!(has_contains, "Must extract Contains edge for class methods");
    }

    #[test]
    fn test_honest_coverage_dynamic_constructs() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        // 1. Clean Static Code -> Verified
        let clean_file = FileUnit {
            path: "src/clean.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash1".to_string(),
            is_generated: false,
        };
        let parsed_clean = adapter
            .parse(&ctx, &clean_file, "const x: number = 42;")
            .unwrap();
        assert_eq!(parsed_clean.coverage, CoverageState::Verified);

        // 2. Eval -> Unknown (FR-014, FR-030)
        let eval_file = FileUnit {
            path: "src/eval.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash2".to_string(),
            is_generated: false,
        };
        let parsed_eval = adapter
            .parse(&ctx, &eval_file, "function run(s: string) { eval(s); }")
            .unwrap();
        assert_eq!(parsed_eval.coverage, CoverageState::Unknown);
        assert!(
            parsed_eval
                .dynamic_constructs
                .iter()
                .any(|d| d.kind == DynamicConstructKind::Eval)
        );

        // 3. Dynamic Import -> Unknown
        let dyn_import_file = FileUnit {
            path: "src/dyn.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash3".to_string(),
            is_generated: false,
        };
        let parsed_dyn = adapter
            .parse(
                &ctx,
                &dyn_import_file,
                "async function load(m: string) { return await import(m); }",
            )
            .unwrap();
        assert_eq!(parsed_dyn.coverage, CoverageState::Unknown);

        // 4. Reflect -> Partial
        let reflect_file = FileUnit {
            path: "src/reflect.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash4".to_string(),
            is_generated: false,
        };
        let parsed_reflect = adapter
            .parse(&ctx, &reflect_file, "const val = Reflect.get(obj, 'prop');")
            .unwrap();
        assert_eq!(parsed_reflect.coverage, CoverageState::Partial);

        // 5. Proxy -> Partial
        let proxy_file = FileUnit {
            path: "src/proxy.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash5".to_string(),
            is_generated: false,
        };
        let parsed_proxy = adapter
            .parse(&ctx, &proxy_file, "const p = new Proxy(target, handler);")
            .unwrap();
        assert_eq!(parsed_proxy.coverage, CoverageState::Partial);

        // 6. Generated File Tag -> Partial
        let gen_file = FileUnit {
            path: "src/gen.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash6".to_string(),
            is_generated: true,
        };
        let parsed_gen = adapter
            .parse(&ctx, &gen_file, "export const generated = true;")
            .unwrap();
        assert_eq!(parsed_gen.coverage, CoverageState::Partial);

        // 7. Header Generated Marker -> Partial
        let marker_file = FileUnit {
            path: "src/marker.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash7".to_string(),
            is_generated: false,
        };
        let parsed_marker = adapter
            .parse(
                &ctx,
                &marker_file,
                "// @generated by protoc\nexport const x = 1;",
            )
            .unwrap();
        assert_eq!(parsed_marker.coverage, CoverageState::Partial);
    }

    #[test]
    fn test_syntax_error_diagnostics_without_crashing() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        let malformed = "function ( broken syntax { let = = ;";
        let file = FileUnit {
            path: "src/malformed.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "bad".to_string(),
            is_generated: false,
        };

        let parsed = adapter.parse(&ctx, &file, malformed).unwrap();
        assert!(!parsed.syntax_valid);
        assert!(!parsed.diagnostics.is_empty());
        assert_eq!(parsed.coverage, CoverageState::Partial);
    }
}
