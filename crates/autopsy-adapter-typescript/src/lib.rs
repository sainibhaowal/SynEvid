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
use autopsy_symbols::{SymbolKind, overload_id, qualified_name, short_name, stable_symbol_id};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

pub const ADAPTER_VERSION: &str = "0.0.1";
pub const LANGUAGE_ID: &str = "typescript";

/// Mutable scope cursor for the call-edge AST walk.
#[derive(Debug, Clone, Default)]
struct CallWalkContext {
    /// Innermost enclosing callable symbol (None => module scope).
    enclosing: Option<SymbolId>,
    /// Innermost enclosing class name (for `this.` resolution).
    enclosing_class: String,
}

/// Declared receiver-type maps: `(scope, binding) -> type name`.
/// Used for fields (`(class, field)`) and locals (`(function, name)`).
type ReceiverMap = BTreeMap<(String, String), String>;

fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut prev = '\0';

    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                out.push(ch);
            }
            continue;
        }
        if in_block_comment {
            if prev == '*' && ch == '/' {
                in_block_comment = false;
                prev = '\0';
            } else {
                prev = ch;
            }
            continue;
        }
        if in_string {
            out.push(ch);
            if ch == '"' && prev != '\\' {
                in_string = false;
            }
            prev = ch;
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            prev = ch;
        } else if ch == '/' && chars.peek() == Some(&'/') {
            in_line_comment = true;
            chars.next();
        } else if ch == '/' && chars.peek() == Some(&'*') {
            in_block_comment = true;
            chars.next();
            prev = '\0';
        } else {
            out.push(ch);
            prev = ch;
        }
    }
    out
}

fn parse_tsconfig_value(val: &serde_json::Value) -> (Option<String>, Vec<(String, Vec<String>)>) {
    let comp_opts = match val.get("compilerOptions") {
        Some(opts) => opts,
        None => return (None, Vec::new()),
    };

    let base_url = comp_opts
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut path_mappings = Vec::new();
    if let Some(paths_obj) = comp_opts.get("paths").and_then(|p| p.as_object()) {
        for (pattern, targets_val) in paths_obj {
            let mut targets = Vec::new();
            if let Some(arr) = targets_val.as_array() {
                for t in arr {
                    if let Some(s) = t.as_str() {
                        targets.push(s.to_string());
                    }
                }
            } else if let Some(s) = targets_val.as_str() {
                targets.push(s.to_string());
            }
            if !targets.is_empty() {
                path_mappings.push((pattern.clone(), targets));
            }
        }
    }
    path_mappings.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    (base_url, path_mappings)
}

fn load_tsconfig_recursive(
    target_path: &Path,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
) -> (Option<String>, Vec<(String, Vec<String>)>) {
    if depth > 4 {
        return (None, Vec::new());
    }
    let canonical = target_path
        .canonicalize()
        .unwrap_or_else(|_| target_path.to_path_buf());
    if !visited.insert(canonical) {
        return (None, Vec::new());
    }
    let Ok(raw) = std::fs::read_to_string(target_path) else {
        return (None, Vec::new());
    };
    let cleaned = strip_json_comments(&raw);
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&cleaned) else {
        return (None, Vec::new());
    };

    let (mut base_url, mut path_mappings) =
        if let Some(ext_val) = val.get("extends").and_then(|v| v.as_str()) {
            let parent_dir = target_path.parent().unwrap_or_else(|| Path::new("."));
            let mut ext_path = parent_dir.join(ext_val);
            if ext_path.extension().is_none() {
                ext_path.set_extension("json");
            }
            load_tsconfig_recursive(&ext_path, depth + 1, visited)
        } else {
            (None, Vec::new())
        };

    let (child_base_url, child_mappings) = parse_tsconfig_value(&val);
    if child_base_url.is_some() {
        base_url = child_base_url;
    }
    for (pat, targets) in child_mappings {
        path_mappings.retain(|(p, _)| p != &pat);
        path_mappings.push((pat, targets));
    }
    path_mappings.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    (base_url, path_mappings)
}

fn load_tsconfig(root: &Path) -> (Option<String>, Vec<(String, Vec<String>)>) {
    let mut visited = BTreeSet::new();
    let tsconfig_path = root.join("tsconfig.json");
    load_tsconfig_recursive(&tsconfig_path, 0, &mut visited)
}

fn normalize_slash_path(p: &str) -> String {
    let mut parts = Vec::new();
    for comp in Path::new(p).components() {
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
    parts.join("/")
}

/// Compiler bridge for resolving module specifiers and symbol references.
#[derive(Debug, Clone, Default)]
pub struct TypeScriptCompilerBridge {
    pub root_dir: PathBuf,
    pub base_url: Option<String>,
    pub path_mappings: Vec<(String, Vec<String>)>,
}

impl TypeScriptCompilerBridge {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        let root = root_dir.into();
        let (base_url, path_mappings) = load_tsconfig(&root);
        Self {
            root_dir: root,
            base_url,
            path_mappings,
        }
    }

    /// Explicitly configure path mappings (e.g. for unit tests or overrides).
    pub fn with_path_mappings(
        mut self,
        base_url: Option<String>,
        mappings: Vec<(String, Vec<String>)>,
    ) -> Self {
        self.base_url = base_url;
        self.path_mappings = mappings;
        self
    }

    /// Parse and apply a tsconfig.json content string (e.g. from in-memory snapshot).
    pub fn with_tsconfig_str(mut self, content: &str) -> Self {
        let cleaned = strip_json_comments(content);
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&cleaned) {
            let (base_url, mappings) = parse_tsconfig_value(&val);
            if base_url.is_some() {
                self.base_url = base_url;
            }
            if !mappings.is_empty() {
                self.path_mappings = mappings;
            }
        }
        self
    }

    fn resolve_candidate_extension(&self, clean: &str) -> String {
        if clean.ends_with(".ts")
            || clean.ends_with(".tsx")
            || clean.ends_with(".js")
            || clean.ends_with(".jsx")
        {
            return clean.to_string();
        }
        for ext in &[
            ".ts",
            ".tsx",
            ".js",
            ".jsx",
            "/index.ts",
            "/index.tsx",
            "/index.js",
        ] {
            let candidate = format!("{}{}", clean, ext);
            if self.root_dir.join(&candidate).exists() {
                return candidate;
            }
        }
        format!("{}.ts", clean)
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
            self.resolve_candidate_extension(&clean)
        } else {
            // Check tsconfig path mappings
            for (pattern, targets) in &self.path_mappings {
                if let Some(prefix) = pattern.strip_suffix('*') {
                    if let Some(suffix) = specifier.strip_prefix(prefix)
                        && let Some(target) = targets.first()
                    {
                        let target_clean = if let Some(target_prefix) = target.strip_suffix('*') {
                            format!("{}{}", target_prefix, suffix)
                        } else {
                            target.clone()
                        };
                        let base_prefix = self.base_url.as_deref().unwrap_or("");
                        let full = if base_prefix.is_empty() || base_prefix == "." {
                            target_clean
                        } else {
                            format!(
                                "{}/{}",
                                base_prefix.trim_matches('/'),
                                target_clean.trim_matches('/')
                            )
                        };
                        let norm = normalize_slash_path(&full);
                        return self.resolve_candidate_extension(&norm);
                    }
                } else if specifier == pattern
                    && let Some(target) = targets.first()
                {
                    let base_prefix = self.base_url.as_deref().unwrap_or("");
                    let full = if base_prefix.is_empty() || base_prefix == "." {
                        target.clone()
                    } else {
                        format!(
                            "{}/{}",
                            base_prefix.trim_matches('/'),
                            target.trim_matches('/')
                        )
                    };
                    let norm = normalize_slash_path(&full);
                    return self.resolve_candidate_extension(&norm);
                }
            }

            // Check baseUrl if configured
            if let Some(base) = &self.base_url
                && !base.is_empty()
                && base != "."
            {
                let full = format!("{}/{}", base.trim_matches('/'), specifier);
                let norm = normalize_slash_path(&full);
                for ext in &[
                    ".ts",
                    ".tsx",
                    ".js",
                    ".jsx",
                    "/index.ts",
                    "/index.tsx",
                    "/index.js",
                ] {
                    let candidate = format!("{}{}", norm, ext);
                    if self.root_dir.join(&candidate).exists() {
                        return candidate;
                    }
                }
            }

            // External or aliased package
            format!("external::{}", specifier)
        }
    }
}

fn extract_quoted_specifier(line: &str) -> Option<String> {
    if let Some(from_idx) = line.find("from") {
        let rest = &line[from_idx + 4..];
        let mut in_quote = false;
        let mut quote_char = '\0';
        let mut spec = String::new();
        for ch in rest.chars() {
            if !in_quote {
                if ch == '\'' || ch == '"' {
                    in_quote = true;
                    quote_char = ch;
                }
            } else if ch == quote_char {
                return Some(spec);
            } else {
                spec.push(ch);
            }
        }
    }
    None
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

        self.collect_symbols(root, &content, &parsed.file.path, "", 0, &mut symbols);
        self.disambiguate_overloads(&mut symbols);

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

        // 4. Call edges (bare calls, this-method calls, constructor calls).
        //    These power transitive blast-radius traversal (FR-008).
        //    The global symbol index enables cross-file import resolution.
        self.collect_call_edges(
            root,
            &content,
            &parsed.file.path,
            &file_symbol_id,
            &extracted_symbols,
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

        self.collect_contracts(root, &content, &parsed.file.path, "", 0, &mut contracts);

        // Overloaded declarations share one owner identity; keep the last
        // in traversal order (TS places the implementation signature
        // after overload signatures). Deterministic by construction.
        let mut by_owner: BTreeMap<SymbolId, Contract> = BTreeMap::new();
        for contract in contracts {
            by_owner.insert(contract.owner.clone(), contract);
        }
        Ok(by_owner.into_values().collect())
    }
}

impl TypeScriptAdapter {
    /// Builds the stable identity for a symbol via the `autopsy-symbols`
    /// authority (FR-005). Identity carries file + kind + scope + name
    /// only; signature material is stored on the record, never embedded.
    fn symbol_identity(
        &self,
        file_path: &str,
        kind: SymbolKind,
        parent_scope: &str,
        name: &str,
    ) -> SymbolId {
        stable_symbol_id(file_path, kind, parent_scope, name)
    }

    fn collect_symbols(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        parent_scope: &str,
        function_depth: u32,
        symbols: &mut Vec<Symbol>,
    ) {
        let kind = node.kind();

        // Children of any function-like node are one level deeper; only
        // module-level (`depth == 0`, no class scope) declarations become
        // variable/enum symbols.
        let child_depth = match kind {
            "function_declaration"
            | "function_signature"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression" => function_depth + 1,
            _ => function_depth,
        };

        match kind {
            // `function_signature` = bodiless overload declaration shape.
            "function_declaration" | "function_signature" | "generator_function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = qualified_name(file_path, parent_scope, name);
                    let sig = self.compute_signature(node, content);
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Function,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Function.kind_str().to_string(),
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
                    let qname = qualified_name(file_path, parent_scope, name);
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Class,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Class.kind_str().to_string(),
                        qualified_name: qname.clone(),
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("class {}", name)),
                        visibility,
                    });

                    // Visit class body for methods and fields
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut cursor = body.walk();
                        for child in body.children(&mut cursor) {
                            self.collect_symbols(
                                child,
                                content,
                                file_path,
                                name,
                                child_depth,
                                symbols,
                            );
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
                    let visibility = self.detect_member_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Method,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Method.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(sig),
                        visibility,
                    });
                }
            }
            // Interface / ambient member signatures: tracked as method
            // symbols so member edits surface in semantic diff even
            // though bodiless members carry no callable contract yet.
            "method_signature" | "abstract_method_signature" => {
                if !parent_scope.is_empty()
                    && let Some(name_node) = node.child_by_field_name("name")
                {
                    let name = &content[name_node.byte_range()];
                    let qname = format!("{}::{}::{}", file_path, parent_scope, name);
                    let sig = self.compute_signature(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Method,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Method.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(sig),
                        visibility: Visibility::Public,
                    });
                }
            }
            "interface_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = qualified_name(file_path, parent_scope, name);
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Interface,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Interface.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("interface {}", name)),
                        visibility,
                    });

                    // Visit interface body for member signatures
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut cursor = body.walk();
                        for child in body.children(&mut cursor) {
                            self.collect_symbols(
                                child,
                                content,
                                file_path,
                                name,
                                child_depth,
                                symbols,
                            );
                        }
                    }
                    return;
                }
            }
            "type_alias_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = qualified_name(file_path, parent_scope, name);
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Type,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Type.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("type {}", name)),
                        visibility,
                    });
                }
            }
            "enum_declaration" => {
                // Module-level enums only (FR-005 stable scope).
                if function_depth == 0
                    && parent_scope.is_empty()
                    && let Some(name_node) = node.child_by_field_name("name")
                {
                    let name = &content[name_node.byte_range()];
                    let qname = qualified_name(file_path, "", name);
                    let visibility = self.detect_visibility(node, content);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(file_path, SymbolKind::Enum, "", name),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Enum.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("enum {}", name)),
                        visibility,
                    });
                }
            }
            "variable_declarator" => {
                // Module-level named bindings only. Arrow/function values
                // become callable symbols; plain values become variables.
                if function_depth == 0
                    && parent_scope.is_empty()
                    && let Some(name_node) = node.child_by_field_name("name")
                    && name_node.kind() == "identifier"
                {
                    let name = &content[name_node.byte_range()];
                    let qname = qualified_name(file_path, "", name);
                    let visibility = self.detect_declarator_visibility(node, content);
                    let value_kind = node
                        .child_by_field_name("value")
                        .map(|v| v.kind().to_string());

                    match value_kind.as_deref() {
                        Some("arrow_function") => {
                            if let Some(value) = node.child_by_field_name("value") {
                                let sig = self.compute_arrow_signature(name, value, content);
                                symbols.push(Symbol {
                                    stable_id: self.symbol_identity(
                                        file_path,
                                        SymbolKind::Function,
                                        "",
                                        name,
                                    ),
                                    language_id: LANGUAGE_ID.to_string(),
                                    kind: SymbolKind::Function.kind_str().to_string(),
                                    qualified_name: qname,
                                    range: Some(self.node_location(node, file_path)),
                                    normalized_signature: Some(sig),
                                    visibility,
                                });
                            }
                        }
                        Some("function_expression") => {
                            if let Some(value) = node.child_by_field_name("value") {
                                let sig = self.compute_signature(value, content);
                                let named_sig = if let Some(rest) = sig.strip_prefix("anonymous") {
                                    format!("{}{}", name, rest)
                                } else {
                                    sig
                                };
                                symbols.push(Symbol {
                                    stable_id: self.symbol_identity(
                                        file_path,
                                        SymbolKind::Function,
                                        "",
                                        name,
                                    ),
                                    language_id: LANGUAGE_ID.to_string(),
                                    kind: SymbolKind::Function.kind_str().to_string(),
                                    qualified_name: qname,
                                    range: Some(self.node_location(node, file_path)),
                                    normalized_signature: Some(named_sig),
                                    visibility,
                                });
                            }
                        }
                        _ => {
                            let declarator_text = content[node.byte_range()].trim().to_string();
                            symbols.push(Symbol {
                                stable_id: self.symbol_identity(
                                    file_path,
                                    SymbolKind::Variable,
                                    "",
                                    name,
                                ),
                                language_id: LANGUAGE_ID.to_string(),
                                kind: SymbolKind::Variable.kind_str().to_string(),
                                qualified_name: qname,
                                range: Some(self.node_location(node, file_path)),
                                normalized_signature: Some(declarator_text),
                                visibility,
                            });
                        }
                    }
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_symbols(
                child,
                content,
                file_path,
                parent_scope,
                child_depth,
                symbols,
            );
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
            // Identity MUST match the emitted class symbol exactly
            // (identity/shape split: no signature hash), otherwise
            // hierarchy edges dangle off placeholder nodes.
            let class_symbol = self.symbol_identity(file_path, SymbolKind::Class, "", name);

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

    /// Resolves deterministic `:overload:N` suffixes for symbols that
    /// share one base identity (e.g. TS function overloads sharing a
    /// name). Groups are ordered by normalized-signature bytes so
    /// numbering is stable across runs and platforms.
    fn disambiguate_overloads(&self, symbols: &mut [Symbol]) {
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
        // Keep emission order deterministic after renames.
        symbols.sort_by(|a, b| {
            a.stable_id
                .cmp(&b.stable_id)
                .then_with(|| a.qualified_name.cmp(&b.qualified_name))
        });
    }

    /// Collects local import bindings: local name -> resolved module path.
    fn collect_imported_names(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
    ) -> BTreeMap<String, String> {
        let mut bindings = BTreeMap::new();
        self.collect_imported_names_inner(node, content, file_path, &mut bindings);
        bindings
    }

    fn collect_imported_names_inner(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        bindings: &mut BTreeMap<String, String>,
    ) {
        if node.kind() == "import_statement" {
            let source_node = node.child_by_field_name("source").or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor).find(|c| c.kind() == "string")
            });
            let resolved = source_node.map(|s| {
                let spec = content[s.byte_range()].trim_matches('\'').trim_matches('"');
                self.compiler_bridge
                    .resolve_module_specifier(file_path, spec)
            });
            if let Some(target) = resolved {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "import_clause" {
                        let mut c_cursor = child.walk();
                        for clause_child in child.children(&mut c_cursor) {
                            match clause_child.kind() {
                                "identifier" => {
                                    bindings.insert(
                                        content[clause_child.byte_range()].to_string(),
                                        target.clone(),
                                    );
                                }
                                "named_imports" => {
                                    let mut n_cursor = clause_child.walk();
                                    for spec_node in clause_child.children(&mut n_cursor) {
                                        if spec_node.kind() == "import_specifier" {
                                            let local = spec_node
                                                .child_by_field_name("alias")
                                                .or_else(|| spec_node.child_by_field_name("name"));
                                            if let Some(local) = local {
                                                bindings.insert(
                                                    content[local.byte_range()].to_string(),
                                                    target.clone(),
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_imported_names_inner(child, content, file_path, bindings);
        }
    }

    /// Collects `Calls` edges from call/new expressions.
    ///
    /// Resolution tiers (honest, syntactic):
    /// - bare `f()` resolving to exactly one same-file function/variable
    ///   symbol -> `Calls`, inherits file coverage, `call_expression:local`.
    /// - `this.m()` inside class `C` with a matching emitted method ->
    ///   `Calls`, `call_expression:this-method`.
    /// - `new C()` matching a same-file class -> `Calls`,
    ///   `new_expression:local-class`.
    /// - callee bound via a static import resolving to exactly one symbol
    ///   in the target module (via the global symbol index) -> `Calls`
    ///   to the real symbol id with `call_expression:imported-resolved`
    ///   provenance and inherited file coverage.
    /// - callee bound via a static import but ambiguous or absent in the
    ///   target module -> `Calls` to `symbol::{name}` with `Partial`
    ///   coverage and `call_expression:imported-name` provenance
    ///   (name-level only; binding is NOT resolved).
    /// - member calls on opaque receivers (`obj.m()`), builtins and
    ///   ambiguous bare names -> no edge (never invent targets).
    #[allow(clippy::too_many_arguments)]
    fn collect_call_edges(
        &self,
        root: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        file_symbols: &[Symbol],
        symbol_index: &BTreeMap<String, SymbolId>,
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        // Short-name -> symbol id for callable same-file symbols
        // (functions + variables; methods need `this.`, classes need `new`).
        let mut bare_callables: BTreeMap<String, Vec<SymbolId>> = BTreeMap::new();
        let mut class_names: BTreeMap<String, SymbolId> = BTreeMap::new();
        for sym in file_symbols {
            let short = short_name(&sym.qualified_name).to_string();
            match sym.kind.as_str() {
                "function" | "variable" => {
                    bare_callables
                        .entry(short)
                        .or_default()
                        .push(sym.stable_id.clone());
                }
                "class" => {
                    class_names.insert(short, sym.stable_id.clone());
                }
                _ => {}
            }
        }
        let imported = self.collect_imported_names(root, content, file_path);
        let (field_types, local_types) =
            self.collect_receiver_types(root, content, &imported, symbol_index);
        let mut ctx = CallWalkContext {
            enclosing: None,
            enclosing_class: String::new(),
        };
        self.collect_call_edges_inner(
            root,
            content,
            file_path,
            file_symbol,
            file_symbols,
            &bare_callables,
            &class_names,
            &imported,
            symbol_index,
            &field_types,
            &local_types,
            coverage,
            &mut ctx,
            edges,
        );
        // Deterministic emission order.
        edges.sort_by(|a, b| (&a.kind, &a.source, &a.target).cmp(&(&b.kind, &b.source, &b.target)));
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_call_edges_inner(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        file_symbols: &[Symbol],
        bare_callables: &BTreeMap<String, Vec<SymbolId>>,
        class_names: &BTreeMap<String, SymbolId>,
        imported: &BTreeMap<String, String>,
        symbol_index: &BTreeMap<String, SymbolId>,
        field_types: &BTreeMap<(String, String), String>,
        local_types: &BTreeMap<(String, String), String>,
        coverage: CoverageState,
        ctx: &mut CallWalkContext,
        edges: &mut Vec<Edge>,
    ) {
        let kind = node.kind();

        // Track enclosing callable scope + enclosing class for `this.`.
        if matches!(
            kind,
            "function_declaration" | "generator_function_declaration" | "method_definition"
        ) && let Some(name_node) = node.child_by_field_name("name")
        {
            let name = &content[name_node.byte_range()];
            let row = node.start_position().row + 1;
            if let Some(sym) = file_symbols.iter().find(|s| {
                short_name(&s.qualified_name) == name
                    && s.range.as_ref().map(|r| r.start_line) == Some(row as u32)
            }) {
                ctx.enclosing = Some(sym.stable_id.clone());
            }
        }
        if kind == "class_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            ctx.enclosing_class = content[name_node.byte_range()].to_string();
        }
        if kind == "variable_declarator"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Some(value) = node.child_by_field_name("value")
            && matches!(value.kind(), "arrow_function" | "function_expression")
        {
            let name = &content[name_node.byte_range()];
            let row = node.start_position().row + 1;
            if let Some(sym) = file_symbols.iter().find(|s| {
                short_name(&s.qualified_name) == name
                    && s.range.as_ref().map(|r| r.start_line) == Some(row as u32)
            }) {
                ctx.enclosing = Some(sym.stable_id.clone());
            }
        }

        if kind == "call_expression"
            && let Some(func) = node.child_by_field_name("function")
        {
            // Skip `import(...)` (dynamic construct, handled in parse).
            let func_text = &content[func.byte_range()];
            if func_text != "import" {
                self.emit_call_edge(
                    func,
                    false,
                    content,
                    file_path,
                    file_symbol,
                    file_symbols,
                    bare_callables,
                    class_names,
                    imported,
                    symbol_index,
                    field_types,
                    local_types,
                    coverage,
                    ctx,
                    node,
                    edges,
                );
            }
        }
        if kind == "new_expression" {
            let ctor = node.child_by_field_name("constructor").or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor).next()
            });
            if let Some(ctor) = ctor {
                self.emit_call_edge(
                    ctor,
                    true,
                    content,
                    file_path,
                    file_symbol,
                    file_symbols,
                    bare_callables,
                    class_names,
                    imported,
                    symbol_index,
                    field_types,
                    local_types,
                    coverage,
                    ctx,
                    node,
                    edges,
                );
            }
        }

        // Recurse with scope save/restore so siblings don't leak scope.
        let saved_enclosing = ctx.enclosing.clone();
        let saved_class = ctx.enclosing_class.clone();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_call_edges_inner(
                child,
                content,
                file_path,
                file_symbol,
                file_symbols,
                bare_callables,
                class_names,
                imported,
                symbol_index,
                field_types,
                local_types,
                coverage,
                ctx,
                edges,
            );
        }
        // Function-like nodes own their scope; restore on exit.
        if matches!(
            kind,
            "function_declaration"
                | "generator_function_declaration"
                | "method_definition"
                | "arrow_function"
                | "function_expression"
                | "class_declaration"
        ) {
            ctx.enclosing = saved_enclosing;
            ctx.enclosing_class = saved_class;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_call_edge(
        &self,
        func: Node,
        is_ctor: bool,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        file_symbols: &[Symbol],
        bare_callables: &BTreeMap<String, Vec<SymbolId>>,
        class_names: &BTreeMap<String, SymbolId>,
        imported: &BTreeMap<String, String>,
        symbol_index: &BTreeMap<String, SymbolId>,
        field_types: &BTreeMap<(String, String), String>,
        local_types: &BTreeMap<(String, String), String>,
        coverage: CoverageState,
        ctx: &CallWalkContext,
        site: Node,
        edges: &mut Vec<Edge>,
    ) {
        let source = ctx.enclosing.clone().unwrap_or_else(|| file_symbol.clone());

        // Cross-file import resolution against the global symbol index:
        // exactly one same-named symbol in the resolved target module
        // links to the real id; follows barrel re-export chains when present.
        let resolve_imported = |name: &str, module: &str| -> Option<SymbolId> {
            let prefix = format!("{}::", module);
            let mut candidates: Vec<&SymbolId> = symbol_index
                .iter()
                .filter(|(qname, _)| qname.starts_with(&prefix) && short_name(qname) == name)
                .map(|(_, id)| id)
                .collect();
            candidates.sort();
            candidates.dedup();
            if candidates.len() == 1 {
                return Some(candidates[0].clone());
            }

            // Cross-module barrel file / re-export chain traversal (up to 8 hops)
            let mut visited_modules = BTreeSet::new();
            visited_modules.insert(module.to_string());
            let mut pending_modules = vec![module.to_string()];

            while let Some(current_mod) = pending_modules.pop() {
                if visited_modules.len() > 8 {
                    break;
                }
                let file_candidates = [
                    self.compiler_bridge.root_dir.join(&current_mod),
                    PathBuf::from(&current_mod),
                ];
                let mut content_opt = None;
                for p in &file_candidates {
                    if let Ok(c) = std::fs::read_to_string(p) {
                        content_opt = Some(c);
                        break;
                    }
                }

                if let Some(mod_content) = content_opt {
                    for line in mod_content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("export")
                            && trimmed.contains("from")
                            && let Some(spec) = extract_quoted_specifier(trimmed)
                        {
                            let re_resolved = self
                                .compiler_bridge
                                .resolve_module_specifier(&current_mod, &spec);
                            if !re_resolved.starts_with("external::")
                                && visited_modules.insert(re_resolved.clone())
                            {
                                let target_prefix = format!("{}::", re_resolved);
                                let mut mod_candidates: Vec<&SymbolId> = symbol_index
                                    .iter()
                                    .filter(|(qname, _)| {
                                        qname.starts_with(&target_prefix)
                                            && short_name(qname) == name
                                    })
                                    .map(|(_, id)| id)
                                    .collect();
                                mod_candidates.sort();
                                mod_candidates.dedup();
                                if mod_candidates.len() == 1 {
                                    return Some(mod_candidates[0].clone());
                                }
                                pending_modules.push(re_resolved);
                            }
                        }
                    }
                }
            }

            // Virtual directory fallback for in-memory or synthetic barrel indexes
            let dir_prefix = if let Some(stripped) = module.strip_suffix("/index.ts") {
                stripped
            } else if let Some(stripped) = module.strip_suffix("/index.tsx") {
                stripped
            } else if let Some(stripped) = module.strip_suffix("/index.js") {
                stripped
            } else if let Some(stripped) = module.strip_suffix(".ts") {
                stripped
            } else if let Some(stripped) = module.strip_suffix(".tsx") {
                stripped
            } else {
                module.strip_suffix(".js").unwrap_or_default()
            };
            if !dir_prefix.is_empty() {
                let prefix = format!("{}/", dir_prefix);
                let mut dir_candidates: Vec<&SymbolId> = symbol_index
                    .iter()
                    .filter(|(qname, _)| qname.starts_with(&prefix) && short_name(qname) == name)
                    .map(|(_, id)| id)
                    .collect();
                dir_candidates.sort();
                dir_candidates.dedup();
                if dir_candidates.len() == 1 {
                    return Some(dir_candidates[0].clone());
                }
            }

            None
        };

        // Resolve target (tiered, honest).
        let (target, target_coverage, provenance): (Option<SymbolId>, CoverageState, String) =
            match func.kind() {
                "identifier" => {
                    let name = &content[func.byte_range()];
                    if is_ctor {
                        if let Some(class_id) = class_names.get(name) {
                            (
                                Some(class_id.clone()),
                                coverage,
                                "new_expression:local-class".to_string(),
                            )
                        } else if let Some(module) = imported.get(name) {
                            match resolve_imported(name, module) {
                                Some(real) => (
                                    Some(real),
                                    coverage,
                                    format!("new_expression:imported-resolved:{}", module),
                                ),
                                None => (
                                    Some(SymbolId::new(format!("symbol::{}", name))),
                                    CoverageState::Partial,
                                    format!("new_expression:imported-name:{}", module),
                                ),
                            }
                        } else {
                            (None, coverage, String::new())
                        }
                    } else if let Some(cands) = bare_callables.get(name)
                        && cands.len() == 1
                    {
                        (
                            Some(cands[0].clone()),
                            coverage,
                            "call_expression:local".to_string(),
                        )
                    } else if let Some(module) = imported.get(name) {
                        match resolve_imported(name, module) {
                            Some(real) => (
                                Some(real),
                                coverage,
                                format!("call_expression:imported-resolved:{}", module),
                            ),
                            None => (
                                Some(SymbolId::new(format!("symbol::{}", name))),
                                CoverageState::Partial,
                                format!("call_expression:imported-name:{}", module),
                            ),
                        }
                    } else {
                        (None, coverage, String::new())
                    }
                }
                "member_expression" => {
                    let object = func.child_by_field_name("object");
                    let property = func.child_by_field_name("property");
                    match (object, property) {
                        (Some(obj), Some(prop))
                            if obj.kind() == "this"
                                && prop.kind() != "private_property_identifier"
                                && !ctx.enclosing_class.is_empty() =>
                        {
                            // `this.method()` within a class.
                            let method = &content[prop.byte_range()];
                            let method_id = stable_symbol_id(
                                file_path,
                                SymbolKind::Method,
                                &ctx.enclosing_class,
                                method,
                            );
                            // Only link when the method was actually emitted
                            // (overload suffixes included); never invent ids.
                            let known = edges
                                .iter()
                                .any(|e| e.target == method_id && e.kind == EdgeKind::Contains);
                            if known {
                                (
                                    Some(method_id),
                                    coverage,
                                    "call_expression:this-method".to_string(),
                                )
                            } else {
                                (None, coverage, String::new())
                            }
                        }
                        (Some(obj), Some(prop)) => {
                            // Typed receivers: `this.field.method()` via the
                            // field's declared class, or `local.method()`
                            // via an annotated / `new`-bound local.
                            let obj_text = &content[obj.byte_range()];
                            let method = &content[prop.byte_range()];
                            if prop.kind() == "private_property_identifier"
                                || method.is_empty()
                                || method.contains(|c: char| {
                                    !c.is_alphanumeric() && c != '_' && c != '$'
                                })
                            {
                                (None, coverage, String::new())
                            } else {
                                let receiver_type = if let Some(field) =
                                    obj_text.strip_prefix("this.").and_then(|f| {
                                        // `this.a.b` chains stay unresolved.
                                        if f.contains('.') {
                                            None
                                        } else {
                                            field_types
                                                .get(&(ctx.enclosing_class.clone(), f.to_string()))
                                        }
                                    }) {
                                    Some(field.clone())
                                } else if !obj_text.contains('.') {
                                    let fn_key = Self::enclosing_fn_key(&ctx.enclosing);
                                    local_types.get(&(fn_key, obj_text.to_string())).cloned()
                                } else {
                                    None
                                };
                                match receiver_type.and_then(|ty| {
                                    self.resolve_class_symbol(
                                        &ty,
                                        file_path,
                                        file_symbols,
                                        symbol_index,
                                        imported,
                                    )
                                }) {
                                    Some((_, class_file, class_name)) => {
                                        let method_id = stable_symbol_id(
                                            &class_file,
                                            SymbolKind::Method,
                                            &class_name,
                                            method,
                                        );
                                        let known =
                                            symbol_index.values().any(|id| id == &method_id)
                                                || file_symbols
                                                    .iter()
                                                    .any(|s| s.stable_id == method_id);
                                        if known {
                                            (
                                                Some(method_id),
                                                coverage,
                                                "call_expression:typed-receiver".to_string(),
                                            )
                                        } else {
                                            (None, coverage, String::new())
                                        }
                                    }
                                    None => (None, coverage, String::new()),
                                }
                            }
                        }
                        _ => (None, coverage, String::new()),
                    }
                }
                _ => (None, coverage, String::new()),
            };

        if let Some(target) = target
            && target != source
        {
            edges.push(Edge {
                source,
                target,
                kind: EdgeKind::Calls,
                coverage: target_coverage,
                location: Some(self.node_location(site, file_path)),
                provenance,
            });
        }
    }

    /// Collects declared receiver types for member-call resolution.
    ///
    /// Returns `(field_types, local_types)` where `field_types` maps
    /// `(class, field) -> type name` (property declarations + constructor
    /// parameter properties) and `local_types` maps
    /// `(enclosing_function_name, binding) -> type name` (annotated
    /// parameters, annotated declarators, `new C(...)` initializers, and
    /// one-hop declared function returns `const x = getService();`).
    /// Only plain-identifier annotations are honored; generics, unions
    /// and computed shapes stay unresolved (honest syntactic scope).
    fn collect_receiver_types(
        &self,
        root: Node,
        content: &str,
        imported: &BTreeMap<String, String>,
        symbol_index: &BTreeMap<String, SymbolId>,
    ) -> (ReceiverMap, ReceiverMap) {
        let declared_returns = self.collect_file_declared_returns(root, content);
        let mut fields = BTreeMap::new();
        let mut locals = BTreeMap::new();
        self.collect_receiver_types_inner(
            root,
            content,
            "",
            "",
            &declared_returns,
            imported,
            symbol_index,
            &mut fields,
            &mut locals,
        );
        (fields, locals)
    }

    fn collect_file_declared_returns(&self, root: Node, content: &str) -> BTreeMap<String, String> {
        let mut returns = BTreeMap::new();
        self.collect_declared_returns_inner(root, content, &mut returns);
        returns
    }

    fn collect_declared_returns_inner(
        &self,
        node: Node,
        content: &str,
        returns: &mut BTreeMap<String, String>,
    ) {
        let kind = node.kind();
        if matches!(
            kind,
            "function_declaration"
                | "generator_function_declaration"
                | "function_signature"
                | "method_definition"
        ) {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = content[name_node.byte_range()].trim().to_string();
                if let Some(ty) = self.plain_type_name(node, content) {
                    returns.insert(name, ty);
                }
            }
        } else if kind == "variable_declarator"
            && let Some(name_node) = node.child_by_field_name("name")
            && name_node.kind() == "identifier"
        {
            let name = content[name_node.byte_range()].trim().to_string();
            if let Some(val) = node.child_by_field_name("value")
                && matches!(val.kind(), "arrow_function" | "function_expression")
                && let Some(ty) = self.plain_type_name(val, content)
            {
                returns.insert(name, ty);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_declared_returns_inner(child, content, returns);
        }
    }

    fn read_file_content(&self, rel_path: &str) -> Option<String> {
        let clean = rel_path.trim_end_matches('/');
        let exts = if clean.ends_with(".ts")
            || clean.ends_with(".tsx")
            || clean.ends_with(".js")
            || clean.ends_with(".jsx")
        {
            vec![""]
        } else {
            vec![
                "",
                ".ts",
                ".tsx",
                ".js",
                ".jsx",
                "/index.ts",
                "/index.tsx",
                "/index.js",
            ]
        };
        for ext in exts {
            let candidate = format!("{}{}", clean, ext);
            let paths = [
                self.compiler_bridge.root_dir.join(&candidate),
                PathBuf::from(&candidate),
            ];
            for p in &paths {
                if let Ok(c) = std::fs::read_to_string(p) {
                    return Some(c);
                }
            }
        }
        None
    }

    fn resolve_imported_function_return_type(
        &self,
        fn_name: &str,
        target_mod: &str,
        symbol_index: &BTreeMap<String, SymbolId>,
    ) -> Option<String> {
        let target_prefix = format!("{}::", target_mod);
        let cand = symbol_index
            .iter()
            .find(|(qname, _)| qname.starts_with(&target_prefix) && short_name(qname) == fn_name)
            .map(|(_, id)| id)
            .or_else(|| {
                let matches: Vec<&SymbolId> = symbol_index
                    .iter()
                    .filter(|(qname, _)| short_name(qname) == fn_name)
                    .map(|(_, id)| id)
                    .collect();
                if matches.len() == 1 {
                    Some(matches[0])
                } else {
                    None
                }
            });

        let declaring_file = cand
            .map(|c| c.as_str().split('#').next().unwrap_or(target_mod))
            .unwrap_or(target_mod);

        let content = self.read_file_content(declaring_file)?;
        let tree = self.parse_tree(declaring_file, &content).ok()?;
        let returns = self.collect_file_declared_returns(tree.root_node(), &content);
        returns.get(fn_name).cloned()
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_receiver_types_inner(
        &self,
        node: Node,
        content: &str,
        enclosing_class: &str,
        enclosing_fn: &str,
        declared_returns: &BTreeMap<String, String>,
        imported: &BTreeMap<String, String>,
        symbol_index: &BTreeMap<String, SymbolId>,
        fields: &mut BTreeMap<(String, String), String>,
        locals: &mut BTreeMap<(String, String), String>,
    ) {
        let kind = node.kind();
        let mut next_class = enclosing_class.to_string();
        let mut next_fn = enclosing_fn.to_string();

        if kind == "class_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            next_class = content[name_node.byte_range()].to_string();
        }
        if matches!(
            kind,
            "function_declaration"
                | "generator_function_declaration"
                | "function_signature"
                | "method_definition"
        ) && let Some(name_node) = node.child_by_field_name("name")
        {
            next_fn = content[name_node.byte_range()].to_string();
            // Annotated parameters: `controller: UserController`.
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for param in params.children(&mut cursor) {
                    if !matches!(param.kind(), "required_parameter" | "optional_parameter") {
                        continue;
                    }
                    // Constructor parameter properties declare fields.
                    let is_field = {
                        let mut pc = param.walk();
                        param
                            .children(&mut pc)
                            .any(|c| c.kind() == "accessibility_modifier")
                    };
                    let Some(pat) = param
                        .child_by_field_name("pattern")
                        .or_else(|| param.child_by_field_name("name"))
                    else {
                        continue;
                    };
                    let binding = content[pat.byte_range()].trim().to_string();
                    if binding.is_empty()
                        || binding.contains(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                    {
                        continue;
                    }
                    if let Some(ty) = self.plain_type_name(param, content) {
                        if is_field && !next_class.is_empty() {
                            fields.insert((next_class.clone(), binding.clone()), ty.clone());
                        }
                        locals.insert((next_fn.clone(), binding), ty);
                    }
                }
            }
        }
        // Class field declarations: `private userService: UserService;`
        if kind == "property_definition"
            && !enclosing_class.is_empty()
            && let Some(name_node) = node.child_by_field_name("name")
            && let Some(ty) = self.plain_type_name(node, content)
        {
            let field = content[name_node.byte_range()].to_string();
            fields.insert((enclosing_class.to_string(), field), ty);
        }
        // `const x: C = ...`, `const x = new C(...)`, and `const x = getService()` bindings.
        if kind == "variable_declarator"
            && let Some(name_node) = node.child_by_field_name("name")
            && name_node.kind() == "identifier"
        {
            let binding = content[name_node.byte_range()].to_string();
            let mut ty = self.plain_type_name(node, content);
            if ty.is_none()
                && let Some(value) = node.child_by_field_name("value")
            {
                if value.kind() == "new_expression"
                    && let Some(ctor) = value.child_by_field_name("constructor").or_else(|| {
                        let mut vc = value.walk();
                        value.children(&mut vc).next()
                    })
                    && ctor.kind() == "identifier"
                {
                    ty = Some(content[ctor.byte_range()].to_string());
                } else if value.kind() == "call_expression"
                    && let Some(func) = value.child_by_field_name("function").or_else(|| {
                        let mut vc = value.walk();
                        value.children(&mut vc).next()
                    })
                    && func.kind() == "identifier"
                {
                    let callee = &content[func.byte_range()];
                    if let Some(ret) = declared_returns.get(callee) {
                        ty = Some(ret.clone());
                    } else if let Some(target_mod) = imported.get(callee) {
                        ty = self.resolve_imported_function_return_type(
                            callee,
                            target_mod,
                            symbol_index,
                        );
                    }
                }
            }
            if let Some(ty) = ty {
                locals.insert((next_fn.clone(), binding), ty);
            }
        }
        // Arrow-function parameters: `(controller: UserController) => ...`
        if kind == "arrow_function" {
            // Name arrows by their declarator below; parameters handled
            // when the declarator scope resolves. Best effort: record
            // under the enclosing function scope.
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for param in params.children(&mut cursor) {
                    if !matches!(param.kind(), "required_parameter" | "optional_parameter") {
                        continue;
                    }
                    let Some(pat) = param
                        .child_by_field_name("pattern")
                        .or_else(|| param.child_by_field_name("name"))
                    else {
                        continue;
                    };
                    let binding = content[pat.byte_range()].trim().to_string();
                    if let Some(ty) = self.plain_type_name(param, content) {
                        locals.insert((next_fn.clone(), binding), ty);
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_receiver_types_inner(
                child,
                content,
                &next_class,
                &next_fn,
                declared_returns,
                imported,
                symbol_index,
                fields,
                locals,
            );
        }
    }

    /// Extracts a plain identifier type name from a `type_annotation`
    /// child (`: UserService`). Returns None for anything composite.
    fn plain_type_name(&self, node: Node, content: &str) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_annotation" {
                let mut tc = child.walk();
                for ty in child.children(&mut tc) {
                    if ty.kind() == "type_identifier" {
                        return Some(content[ty.byte_range()].to_string());
                    }
                }
                return None;
            }
        }
        None
    }

    /// Resolves a class name to its emitted class symbol: same-file
    /// first, then the import-bound module, then a unique global match.
    /// Returns None on ambiguity (never guess across files).
    fn resolve_class_symbol(
        &self,
        class_name: &str,
        file_path: &str,
        file_symbols: &[Symbol],
        symbol_index: &BTreeMap<String, SymbolId>,
        imported: &BTreeMap<String, String>,
    ) -> Option<(SymbolId, String, String)> {
        // 1. Same-file class.
        let same_file = stable_symbol_id(file_path, SymbolKind::Class, "", class_name);
        if file_symbols.iter().any(|s| s.stable_id == same_file) {
            return Some((same_file, file_path.to_string(), class_name.to_string()));
        }
        // 2. Import-bound module: the local binding names the class.
        if let Some(module) = imported.get(class_name) {
            let qname = format!("{}::{}", module, class_name);
            if let Some(id) = symbol_index.get(&qname) {
                return Some((id.clone(), module.clone(), class_name.to_string()));
            }
        }
        // 3. Unique global class match (file recovered from the
        // qualified name, never by splitting the identity string).
        let mut matches: Vec<(String, SymbolId)> = symbol_index
            .iter()
            .filter(|(qname, id)| {
                id.as_str().contains(&format!("#class:{}", class_name))
                    && short_name(qname) == class_name
            })
            .map(|(qname, id)| (qname.clone(), id.clone()))
            .collect();
        matches.sort();
        matches.dedup();
        if matches.len() == 1 {
            let (qname, id) = matches[0].clone();
            // Qualified names are `file::Name`; file paths never
            // contain `::`, so the file is everything but the tail.
            let mut parts: Vec<&str> = qname.split("::").collect();
            if parts.len() >= 2 {
                parts.pop();
                return Some((id, parts.join("::"), class_name.to_string()));
            }
        }
        None
    }

    /// Derives the receiver-type lookup key from an enclosing symbol id
    /// (`path#fn:handleCreate` -> `handleCreate`,
    /// `path#method:C.run` -> `run`).
    fn enclosing_fn_key(enclosing: &Option<SymbolId>) -> String {
        if let Some(id) = enclosing {
            let s = id.as_str();
            let after_hash = s.split('#').nth(1).unwrap_or(s);
            let after_colon = after_hash.split(':').nth(1).unwrap_or(after_hash);
            let base = after_colon.rsplit('.').next().unwrap_or(after_colon);
            base.split(':').next().unwrap_or(base).to_string()
        } else {
            String::new()
        }
    }
    fn compute_arrow_signature(&self, name: &str, arrow: Node, content: &str) -> String {
        let params = arrow
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].trim().to_string())
            .unwrap_or_else(|| "()".to_string());
        let params = if params.starts_with('(') {
            params
        } else {
            format!("({})", params)
        };
        let return_type = arrow
            .child_by_field_name("return_type")
            .map(|r| {
                format!(
                    ": {}",
                    content[r.byte_range()]
                        .trim()
                        .trim_start_matches(':')
                        .trim()
                )
            })
            .unwrap_or_default();
        format!("{}{}{}", name, params, return_type)
    }

    /// Visibility for module-level declarators: exported -> Public.
    fn detect_declarator_visibility(&self, node: Node, _content: &str) -> Visibility {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == "export_statement" {
                return Visibility::Public;
            }
            // Stop at statement boundaries; only direct export wrappers count.
            if matches!(
                parent.kind(),
                "program" | "lexical_declaration" | "variable_statement" | "export_statement"
            ) {
                current = parent.parent();
            } else {
                break;
            }
        }
        Visibility::Internal
    }

    fn collect_contracts(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        parent_scope: &str,
        function_depth: u32,
        contracts: &mut Vec<Contract>,
    ) {
        let kind = node.kind();
        let child_depth = match kind {
            "function_declaration"
            | "function_signature"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression" => function_depth + 1,
            _ => function_depth,
        };

        if kind == "function_declaration"
            || kind == "function_signature"
            || kind == "generator_function_declaration"
            || kind == "method_definition"
        {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &content[name_node.byte_range()];

                let owner = if parent_scope.is_empty() {
                    self.symbol_identity(file_path, SymbolKind::Function, "", name)
                } else {
                    self.symbol_identity(file_path, SymbolKind::Method, parent_scope, name)
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
        } else if kind == "variable_declarator"
            && function_depth == 0
            && parent_scope.is_empty()
            && let Some(name_node) = node.child_by_field_name("name")
            && name_node.kind() == "identifier"
            && let Some(value) = node.child_by_field_name("value")
            && matches!(value.kind(), "arrow_function" | "function_expression")
        {
            // Callable bindings carry contracts under the same identity
            // as their emitted function symbol so compat joins align.
            let name = &content[name_node.byte_range()];
            let owner = self.symbol_identity(file_path, SymbolKind::Function, "", name);
            let visibility = self.detect_declarator_visibility(node, content);
            let (inputs, output) = self.extract_parameter_and_return_types(value, content);
            contracts.push(Contract {
                owner,
                visibility,
                inputs,
                output,
                effects: Vec::new(),
            });
        } else if kind == "class_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            let name = &content[name_node.byte_range()];
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    self.collect_contracts(child, content, file_path, name, child_depth, contracts);
                }
            }
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_contracts(
                child,
                content,
                file_path,
                parent_scope,
                child_depth,
                contracts,
            );
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
            .map(|r| {
                format!(
                    ": {}",
                    content[r.byte_range()]
                        .trim()
                        .trim_start_matches(':')
                        .trim()
                )
            })
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

    fn test_file_unit(path: &str) -> FileUnit {
        FileUnit {
            path: path.to_string(),
            language: "typescript".to_string(),
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
    fn test_identity_stable_across_signature_edit() {
        // Forensic core: adding a required parameter (the breaking-change
        // event) must preserve identity so diff/compat paths fire.
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/svc.ts");

        let before = "export function create(name: string): string { return name; }";
        let after =
            "export function create(name: string, tenantId: string): string { return name; }";

        let parsed_b = adapter.parse(&ctx, &file, before).unwrap();
        let syms_b = adapter.extract_symbols(&parsed_b).unwrap();
        let parsed_a = adapter.parse(&ctx, &file, after).unwrap();
        let syms_a = adapter.extract_symbols(&parsed_a).unwrap();

        assert_eq!(syms_b.len(), 1);
        assert_eq!(syms_a.len(), 1);
        assert_eq!(syms_b[0].stable_id, syms_a[0].stable_id);
        assert_eq!(syms_b[0].stable_id.as_str(), "src/svc.ts#fn:create");
        assert_ne!(
            syms_b[0].normalized_signature,
            syms_a[0].normalized_signature
        );

        let contracts_b = adapter.extract_contracts(&parsed_b).unwrap();
        let contracts_a = adapter.extract_contracts(&parsed_a).unwrap();
        assert_eq!(contracts_b.len(), 1);
        assert_eq!(contracts_a.len(), 1);
        assert_eq!(contracts_b[0].owner, contracts_a[0].owner);
        assert_ne!(contracts_b[0].inputs, contracts_a[0].inputs);
    }

    #[test]
    fn test_call_edges_local_this_and_new() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/svc.ts");

        let source = r#"
export function hashPassword(raw: string): string { return raw; }
export function createUser(name: string): string { return hashPassword(name); }
export class Service {
    public run(): string { return this.helper(); }
    private helper(): string { return "ok"; }
}
export function boot(): Service { return new Service(); }
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let index = symbol_index_of(&symbols);
        let edges = adapter.extract_edges(&parsed, &index).unwrap();
        let calls: Vec<&Edge> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();

        let has = |src: &str, dst: &str| {
            calls.iter().any(|e| {
                e.source.as_str() == src
                    && e.target.as_str() == dst
                    && e.coverage == CoverageState::Verified
            })
        };

        // Bare local call: createUser -> hashPassword
        assert!(
            has("src/svc.ts#fn:createUser", "src/svc.ts#fn:hashPassword"),
            "missing local call edge: {:?}",
            calls
        );
        // this.method call inside class
        assert!(
            has(
                "src/svc.ts#method:Service.run",
                "src/svc.ts#method:Service.helper"
            ),
            "missing this-method call edge: {:?}",
            calls
        );
        // Constructor call
        assert!(
            has("src/svc.ts#fn:boot", "src/svc.ts#class:Service"),
            "missing constructor call edge: {:?}",
            calls
        );
        // Provenance honesty preserved
        assert!(calls.iter().all(|e| !e.provenance.is_empty()));
    }

    #[test]
    fn test_call_edges_imported_name_is_partial() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/controller.ts");

        let source = r#"
import { createUser } from "./svc";
export function handleCreate(): string { return createUser("a"); }
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let index = symbol_index_of(&symbols);
        let edges = adapter.extract_edges(&parsed, &index).unwrap();

        let imported_call = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str() == "src/controller.ts#fn:handleCreate"
                    && e.target.as_str() == "symbol::createUser"
            })
            .expect("imported-name call edge must exist");
        // Cross-file binding is NOT resolved: name-level only, Partial.
        assert_eq!(imported_call.coverage, CoverageState::Partial);
        assert!(
            imported_call
                .provenance
                .starts_with("call_expression:imported-name:"),
            "provenance must record imported module: {}",
            imported_call.provenance
        );
    }

    #[test]
    fn test_call_edges_imported_resolved_via_global_index() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        // Target module symbols, as the CLI two-phase pipeline would index them.
        let svc_file = test_file_unit("src/svc.ts");
        let svc_source = r#"
export function createUser(name: string): string { return name; }
"#;
        let parsed_svc = adapter.parse(&ctx, &svc_file, svc_source).unwrap();
        let svc_symbols = adapter.extract_symbols(&parsed_svc).unwrap();

        let ctl_file = test_file_unit("src/controller.ts");
        let ctl_source = r#"
import { createUser } from "./svc";
export function handleCreate(): string { return createUser("a"); }
"#;
        let parsed_ctl = adapter.parse(&ctx, &ctl_file, ctl_source).unwrap();
        let ctl_symbols = adapter.extract_symbols(&parsed_ctl).unwrap();

        let mut index = symbol_index_of(&svc_symbols);
        for s in &ctl_symbols {
            index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }

        let edges = adapter.extract_edges(&parsed_ctl, &index).unwrap();
        let resolved = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str() == "src/controller.ts#fn:handleCreate"
            })
            .expect("cross-file call edge must exist");
        // Real target identity, inherited file coverage, honest provenance.
        assert_eq!(resolved.target.as_str(), "src/svc.ts#fn:createUser");
        assert_eq!(resolved.coverage, CoverageState::Verified);
        assert!(
            resolved
                .provenance
                .starts_with("call_expression:imported-resolved:"),
            "provenance must record resolution: {}",
            resolved.provenance
        );
    }

    #[test]
    fn test_call_edges_typed_receivers_di_field_and_param() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        // Service module.
        let svc_file = test_file_unit("src/services/userService.ts");
        let svc_source = r#"
export class UserService {
    public createUser(name: string): string { return name; }
}
"#;
        let parsed_svc = adapter.parse(&ctx, &svc_file, svc_source).unwrap();
        let svc_symbols = adapter.extract_symbols(&parsed_svc).unwrap();

        // Controller with constructor-injected typed field.
        let ctl_file = test_file_unit("src/controllers/userController.ts");
        let ctl_source = r#"
import { UserService } from '../services/userService';
export class UserController {
    constructor(private userService: UserService) {}
    public handleCreate(): string {
        return this.userService.createUser("a");
    }
}
"#;
        let parsed_ctl = adapter.parse(&ctx, &ctl_file, ctl_source).unwrap();

        // Route handler with annotated parameter receiver.
        let route_file = test_file_unit("src/routes/userRoutes.ts");
        let route_source = r#"
import { UserController } from '../controllers/userController';
export function routeRequest(controller: UserController): string {
    return controller.handleCreate();
}
"#;
        let parsed_route = adapter.parse(&ctx, &route_file, route_source).unwrap();

        let mut index = symbol_index_of(&svc_symbols);
        for s in adapter.extract_symbols(&parsed_ctl).unwrap() {
            index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }
        for s in adapter.extract_symbols(&parsed_route).unwrap() {
            index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }

        let ctl_edges = adapter.extract_edges(&parsed_ctl, &index).unwrap();
        let di_edge = ctl_edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str()
                        == "src/controllers/userController.ts#method:UserController.handleCreate"
            })
            .expect("DI field call edge must exist");
        assert_eq!(
            di_edge.target.as_str(),
            "src/services/userService.ts#method:UserService.createUser"
        );
        assert_eq!(di_edge.provenance, "call_expression:typed-receiver");

        let route_edges = adapter.extract_edges(&parsed_route, &index).unwrap();
        let param_edge = route_edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str() == "src/routes/userRoutes.ts#fn:routeRequest"
            })
            .expect("annotated-param call edge must exist");
        assert_eq!(
            param_edge.target.as_str(),
            "src/controllers/userController.ts#method:UserController.handleCreate"
        );
        assert_eq!(param_edge.provenance, "call_expression:typed-receiver");
    }

    #[test]
    fn test_call_edges_untyped_receiver_stays_silent() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/loose.ts");

        // `any`-flavored and untyped receivers must not invent targets.
        let source = r#"
export function run(store: any): void {
    store.save();
}
export function run2(helper): void {
    helper.work();
}
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let edges = adapter.extract_edges(&parsed, &BTreeMap::new()).unwrap();
        assert!(
            edges.iter().all(|e| e.kind != EdgeKind::Calls),
            "untyped receivers must yield no call edges: {:?}",
            edges
        );
    }

    #[test]
    fn test_call_edges_ambiguous_import_stays_partial() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");

        // Two same-named symbols in the target module: overloads share one
        // qualified name but the index keeps a single entry; an unrelated
        // same-name collision must not resolve silently either.
        let mut index = BTreeMap::new();
        index.insert(
            "src/svc.ts::createUser".to_string(),
            SymbolId::new("src/svc.ts#fn:createUser"),
        );
        index.insert(
            "src/svc.ts::helper::createUser".to_string(),
            SymbolId::new("src/svc.ts#fn:helper:createUser"),
        );

        let ctl_file = test_file_unit("src/controller.ts");
        let ctl_source = r#"
import { createUser } from "./svc";
export function handleCreate(): string { return createUser("a"); }
"#;
        let parsed_ctl = adapter.parse(&ctx, &ctl_file, ctl_source).unwrap();
        let edges = adapter.extract_edges(&parsed_ctl, &index).unwrap();
        let edge = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str() == "src/controller.ts#fn:handleCreate"
            })
            .expect("call edge must exist");
        assert_eq!(edge.target.as_str(), "symbol::createUser");
        assert_eq!(edge.coverage, CoverageState::Partial);
    }

    #[test]
    fn test_variable_enum_arrow_symbols_module_scope_only() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/defs.ts");

        let source = r#"
export const MAX_RETRIES = 3;
export const hash = (raw: string): string => raw;
export enum Role { Admin, User }
export function outer(): number {
    const innerLocal = 1;
    return innerLocal;
}
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let ids: Vec<&str> = symbols.iter().map(|s| s.stable_id.as_str()).collect();

        assert!(ids.contains(&"src/defs.ts#variable:MAX_RETRIES"));
        assert!(ids.contains(&"src/defs.ts#fn:hash"));
        assert!(ids.contains(&"src/defs.ts#enum:Role"));
        assert!(ids.contains(&"src/defs.ts#fn:outer"));
        // Function-body locals must never leak into module scope.
        assert!(
            !ids.iter().any(|id| id.contains("innerLocal")),
            "locals leaked: {:?}",
            ids
        );
        // Arrow binding carries a callable contract under the same identity.
        let contracts = adapter.extract_contracts(&parsed).unwrap();
        let arrow_contract = contracts
            .iter()
            .find(|c| c.owner.as_str() == "src/defs.ts#fn:hash")
            .expect("arrow binding must carry a contract");
        assert_eq!(arrow_contract.inputs, vec!["raw: string".to_string()]);
    }

    #[test]
    fn test_overload_disambiguation_deterministic() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/over.ts");

        let source = r#"
export function get(id: string): string;
export function get(id: number): string;
export function get(id: unknown): string { return String(id); }
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let first = adapter.extract_symbols(&parsed).unwrap();
        let second = adapter.extract_symbols(&parsed).unwrap();
        assert_eq!(first, second, "overload numbering must be deterministic");

        let mut ids: Vec<&str> = first.iter().map(|s| s.stable_id.as_str()).collect();
        ids.sort();
        assert_eq!(
            ids,
            vec![
                "src/over.ts#fn:get",
                "src/over.ts#fn:get:overload:2",
                "src/over.ts#fn:get:overload:3",
            ]
        );
    }

    #[test]
    fn test_hierarchy_source_joins_emitted_class_symbol() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/dog.ts");

        let source = r#"
export class Dog extends Animal {
    public bark(): string { return "woof"; }
}
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let class_id = symbols
            .iter()
            .find(|s| s.kind == "class")
            .expect("class symbol must exist")
            .stable_id
            .clone();
        assert_eq!(class_id.as_str(), "src/dog.ts#class:Dog");

        let index = symbol_index_of(&symbols);
        let edges = adapter.extract_edges(&parsed, &index).unwrap();
        assert!(
            edges.iter().any(|e| e.kind == EdgeKind::Inherits
                && e.source == class_id
                && e.target.as_str() == "symbol::Animal"),
            "hierarchy edge must originate at the real class symbol"
        );
    }

    #[test]
    fn test_tsconfig_paths_resolution() {
        let bridge = TypeScriptCompilerBridge::new(".").with_path_mappings(
            Some("src".to_string()),
            vec![
                (
                    "@components/*".to_string(),
                    vec!["components/*".to_string()],
                ),
                ("@/*".to_string(), vec!["*".to_string()]),
                ("utils".to_string(), vec!["utils/index.ts".to_string()]),
            ],
        );

        assert_eq!(
            bridge.resolve_module_specifier("src/main.ts", "@/services/auth"),
            "src/services/auth.ts"
        );
        assert_eq!(
            bridge.resolve_module_specifier("src/main.ts", "@components/Button"),
            "src/components/Button.ts"
        );
        assert_eq!(
            bridge.resolve_module_specifier("src/main.ts", "utils"),
            "src/utils/index.ts"
        );
    }

    #[test]
    fn test_barrel_reexport_call_resolution() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let consumer_file = test_file_unit("src/app.ts");
        let consumer_code = r#"
import { helper } from './utils';

export function run() {
    helper();
}
"#;
        let parsed = adapter.parse(&ctx, &consumer_file, consumer_code).unwrap();

        // In symbol_index, helper actually lives in src/utils/helper.ts
        let mut index = BTreeMap::new();
        let helper_id = SymbolId::new("src/utils/helper.ts#fn:helper");
        index.insert("src/utils/helper.ts::helper".to_string(), helper_id.clone());

        let edges = adapter.extract_edges(&parsed, &index).unwrap();
        let call_edge = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Calls)
            .expect("call edge must exist");

        // Must resolve to the real symbol rather than falling back to symbol::helper
        assert_eq!(call_edge.target, helper_id);
        assert_eq!(call_edge.coverage, CoverageState::Verified);
        assert!(call_edge.provenance.contains("imported-resolved"));
    }

    #[test]
    fn test_declared_return_type_propagation() {
        let adapter = TypeScriptAdapter::new();
        let ctx = ParseContext::new(".");
        let consumer_file = test_file_unit("src/app.ts");
        let consumer_code = r#"
class UserService {
    login() {}
}

function getService(): UserService {
    return new UserService();
}

export function run() {
    const s = getService();
    s.login();
}
"#;
        let parsed = adapter.parse(&ctx, &consumer_file, consumer_code).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let index = symbol_index_of(&symbols);
        let edges = adapter.extract_edges(&parsed, &index).unwrap();

        let login_id = SymbolId::new("src/app.ts#method:UserService.login");
        let typed_call = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Calls && e.target == login_id)
            .expect("one-hop return-type propagation must resolve s.login()");

        assert_eq!(typed_call.coverage, CoverageState::Verified);
        assert_eq!(typed_call.provenance, "call_expression:typed-receiver");
    }

    #[test]
    fn test_tsconfig_with_string_configuration() {
        let tsconfig_json = r#"{
            "compilerOptions": {
                "baseUrl": "src",
                "paths": {
                    "@models/*": ["models/*"]
                }
            }
        }"#;
        let bridge = TypeScriptCompilerBridge::new(".").with_tsconfig_str(tsconfig_json);
        assert_eq!(bridge.base_url.as_deref(), Some("src"));
        assert_eq!(
            bridge.resolve_module_specifier("src/main.ts", "@models/user"),
            "src/models/user.ts"
        );
    }
}
