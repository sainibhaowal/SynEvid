//! autopsy-adapter-python: Production-grade Python language adapter.
//!
//! Tree-sitter bootstrap, module resolution bridge, stable symbol
//! identification via `autopsy-symbols` (identity/shape split), typed
//! multigraph edge extraction, normalized contract extraction, and
//! honest coverage classification (FR-014, FR-030).

use autopsy_adapter_api::{
    AdapterCapabilities, AdapterError, DynamicConstruct, DynamicConstructKind, LanguageAdapter,
    ParseContext, ParseDiagnostic, ParsedFile,
};
use autopsy_domain::{
    Contract, CoverageState, Edge, EdgeKind, FileUnit, Severity, SourceLocation, Symbol, SymbolId,
    Visibility,
};
use autopsy_symbols::{SymbolKind, overload_id, qualified_name, short_name, stable_symbol_id};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

pub const ADAPTER_VERSION: &str = "0.0.1";
pub const LANGUAGE_ID: &str = "python";

/// Module bridge resolving absolute and explicit-relative imports to
/// repository-relative file paths.
#[derive(Debug, Clone, Default)]
pub struct PythonModuleBridge {
    pub root_dir: PathBuf,
}

impl PythonModuleBridge {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    /// Resolves a dotted module path to a repository-relative file path.
    /// Prefers paths that exist on disk (`mod.py`, then `mod/__init__.py`);
    /// otherwise returns the conventional guess.
    fn dotted_to_file(&self, base_dir: &Path, dotted: &str) -> String {
        let rel = dotted.replace('.', "/");
        for candidate in [format!("{}.py", rel), format!("{}/__init__.py", rel)] {
            let joined = base_dir.join(&candidate);
            // `base_dir` may be empty (repo-relative already).
            let probe = if base_dir.as_os_str().is_empty() {
                PathBuf::from(&candidate)
            } else {
                joined
            };
            if self.root_dir.join(&probe).exists() {
                return Self::slash(&probe);
            }
        }
        if base_dir.as_os_str().is_empty() {
            format!("{}.py", rel)
        } else {
            Self::slash(&base_dir.join(format!("{}.py", rel)))
        }
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Resolves `import a.b [as c]` to the target module file.
    pub fn resolve_absolute(&self, dotted: &str) -> String {
        self.dotted_to_file(Path::new(""), dotted)
    }

    /// Resolves `from <dots><module> import ...` from the importing file.
    /// `level` = number of leading dots (0 = absolute).
    ///
    /// Level 0 prefers the importing file's sibling directory WITHOUT an
    /// existence check: the global symbol index is the arbiter (hit =
    /// resolved, miss = honest Partial). This keeps resolution
    /// deterministic and independent of process working directory, which
    /// matters for cache-rebuilt snapshots that carry no root context.
    pub fn resolve_from(&self, importing_file: &str, level: usize, module: &str) -> String {
        if level == 0 {
            let sibling_dir = Self::slash(
                Path::new(importing_file)
                    .parent()
                    .unwrap_or_else(|| Path::new("")),
            );
            if !sibling_dir.is_empty() {
                return self.dotted_to_file(Path::new(&sibling_dir), module);
            }
            return self.resolve_absolute(module);
        }
        let importing_path = Path::new(importing_file);
        let mut dir = importing_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        // Level 1 = current package: `from . import x` stays in dir.
        for _ in 1..level {
            dir = dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            // Climbing above repo root: unresolvable, keep honest guess.
            if dir.as_os_str().is_empty() {
                break;
            }
        }
        if module.is_empty() {
            // `from . import sibling`: module is the package itself; the
            // caller maps each imported name onto it separately.
            return Self::slash(&dir);
        }
        self.dotted_to_file(&dir, module)
    }

    /// Package directory of the importing file (for `from . import x`).
    pub fn package_dir(&self, importing_file: &str, level: usize) -> String {
        let importing_path = Path::new(importing_file);
        let mut dir = importing_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        for _ in 1..level {
            dir = dir.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            if dir.as_os_str().is_empty() {
                break;
            }
        }
        Self::slash(&dir)
    }
}

/// Scope cursor for the call-edge walk.
#[derive(Debug, Clone, Default)]
struct CallWalkContext {
    enclosing: Option<SymbolId>,
    enclosing_class: String,
}

/// Declared receiver-type maps: `(scope, binding) -> type name`.
type ReceiverMap = BTreeMap<(String, String), String>;

/// Production Python language adapter implementation.
pub struct PythonAdapter {
    capabilities: AdapterCapabilities,
    module_bridge: PythonModuleBridge,
}

impl Default for PythonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonAdapter {
    pub fn new() -> Self {
        let capabilities = AdapterCapabilities {
            language_id: LANGUAGE_ID.to_string(),
            adapter_version: ADAPTER_VERSION.to_string(),
            supported_extensions: vec!["py".to_string(), "pyi".to_string(), "pyw".to_string()],
            supports_ast_parsing: true,
            supports_symbol_extraction: true,
            supports_edge_extraction: true,
            supports_contract_extraction: true,
            supports_type_resolution: true,
            unsupported_constructs: vec![
                "eval".to_string(),
                "exec".to_string(),
                "dynamic_import".to_string(),
                "reflection_getattr".to_string(),
                "metaclass_metaprogramming".to_string(),
                "generated_code".to_string(),
            ],
        };

        Self {
            capabilities,
            module_bridge: PythonModuleBridge::new("."),
        }
    }

    pub fn with_root_dir(root_dir: impl Into<PathBuf>) -> Self {
        let mut adapter = Self::new();
        adapter.module_bridge = PythonModuleBridge::new(root_dir);
        adapter
    }

    fn init_parser(&self) -> Result<Parser, AdapterError> {
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE.into();
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

    /// Python visibility: leading underscore means private.
    fn detect_visibility(&self, name: &str) -> Visibility {
        if name.starts_with('_') {
            Visibility::Private
        } else {
            Visibility::Public
        }
    }
}

impl LanguageAdapter for PythonAdapter {
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
        let tree = self.parse_tree(content)?;
        let root = tree.root_node();

        let mut diagnostics = Vec::new();
        let mut dynamic_constructs = Vec::new();
        let mut has_syntax_errors = false;

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
            let loc = || {
                let start = node.start_position();
                let end = node.end_position();
                SourceLocation {
                    path: file.path.clone(),
                    start_line: (start.row + 1) as u32,
                    end_line: (end.row + 1) as u32,
                    start_col: Some((start.column + 1) as u32),
                    end_col: Some((end.column + 1) as u32),
                    symbol_id: None,
                }
            };

            // Dynamic code evaluation: eval / exec / compile.
            if kind == "call"
                && let Some(func) = node.child_by_field_name("function")
            {
                let callee = &content[func.byte_range()];
                let base = callee.rsplit('.').next().unwrap_or(callee);
                if base == "eval" || base == "exec" || base == "compile" {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::Eval,
                        description: format!("Dynamic code evaluation via '{}'", base),
                        location: Some(loc()),
                    });
                } else if base == "__import__" {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::DynamicImport,
                        description: "Dynamic import via __import__".to_string(),
                        location: Some(loc()),
                    });
                } else if base == "getattr"
                    || base == "setattr"
                    || base == "delattr"
                    || base == "globals"
                    || base == "locals"
                    || base == "vars"
                {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::ReflectionOrProxy,
                        description: format!("Runtime reflection via '{}'", base),
                        location: Some(loc()),
                    });
                }
            }

            // Dynamic import machinery: importlib with non-literal args,
            // __import__ handled above.
            if kind == "attribute"
                && (text.starts_with("importlib.import_module")
                    || text.starts_with("importlib.__import__"))
                && let Some(parent) = node.parent()
                && parent.kind() == "call"
            {
                dynamic_constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::DynamicImport,
                    description: "Dynamic module loading via importlib".to_string(),
                    location: Some(loc()),
                });
            }

            // Metaprogramming: metaclasses and attribute interception.
            if kind == "function_definition"
                && let Some(name_node) = node.child_by_field_name("name")
            {
                let name = &content[name_node.byte_range()];
                if name == "__getattr__" || name == "__getattribute__" || name == "__setattr__" {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::ReflectionOrProxy,
                        description: format!("Attribute interception via '{}'", name),
                        location: Some(loc()),
                    });
                }
            }
            if kind == "class_definition" {
                let mut cursor = node.walk();
                let has_metaclass = node
                    .child_by_field_name("superclasses")
                    .map(|sup| {
                        let stext = &content[sup.byte_range()];
                        stext.contains("metaclass=") || stext.contains("ABCMeta")
                    })
                    .unwrap_or(false);
                let _ = &mut cursor;
                if has_metaclass {
                    dynamic_constructs.push(DynamicConstruct {
                        kind: DynamicConstructKind::ReflectionOrProxy,
                        description: "Metaclass-driven class construction".to_string(),
                        location: Some(loc()),
                    });
                }
            }

            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                stack.push(child);
            }
        }

        if file.is_generated
            || content.contains("@generated")
            || content.contains("DO NOT EDIT")
            || content.contains("GENERATED FILE")
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
                "Python tree with {} top-level children",
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

        let tree = self.parse_tree(&content)?;
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

        let tree = self.parse_tree(&content)?;
        let root = tree.root_node();
        let mut edges = Vec::new();

        let file_symbol_id = SymbolId::new(format!("file::{}", parsed.file.path));

        // 1. Module Contains edges.
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

        // 2. Import edges.
        self.collect_import_edges(
            root,
            &content,
            &parsed.file.path,
            &file_symbol_id,
            parsed.coverage,
            &mut edges,
        );

        // 3. Inheritance edges.
        self.collect_class_hierarchy_edges(
            root,
            &content,
            &parsed.file.path,
            parsed.coverage,
            &mut edges,
        );

        // 4. Call edges (bare, self., typed receivers, constructors).
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

        let tree = self.parse_tree(&content)?;
        let root = tree.root_node();
        let mut contracts = Vec::new();

        self.collect_contracts(root, &content, &parsed.file.path, "", &mut contracts);

        let mut by_owner: BTreeMap<SymbolId, Contract> = BTreeMap::new();
        for contract in contracts {
            by_owner.insert(contract.owner.clone(), contract);
        }
        Ok(by_owner.into_values().collect())
    }
}

impl PythonAdapter {
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

        let child_depth = match kind {
            "function_definition" | "lambda" => function_depth + 1,
            _ => function_depth,
        };

        match kind {
            "decorated_definition" => {
                // Unwrap to the decorated function/class; decorators do not
                // create scopes of their own.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "decorator" {
                        continue;
                    }
                    self.collect_symbols(
                        child,
                        content,
                        file_path,
                        parent_scope,
                        function_depth,
                        symbols,
                    );
                }
                return;
            }
            "function_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let (sym_kind, scope) = if parent_scope.is_empty() {
                        (SymbolKind::Function, "")
                    } else {
                        (SymbolKind::Method, parent_scope)
                    };
                    let qname = qualified_name(file_path, scope, name);
                    let sig = self.compute_signature(node, content, name);
                    let visibility = self.detect_visibility(name);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(file_path, sym_kind, scope, name),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: sym_kind.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(sig),
                        visibility,
                    });
                }
            }
            "class_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = &content[name_node.byte_range()];
                    let qname = qualified_name(file_path, parent_scope, name);
                    let visibility = self.detect_visibility(name);

                    symbols.push(Symbol {
                        stable_id: self.symbol_identity(
                            file_path,
                            SymbolKind::Class,
                            parent_scope,
                            name,
                        ),
                        language_id: LANGUAGE_ID.to_string(),
                        kind: SymbolKind::Class.kind_str().to_string(),
                        qualified_name: qname,
                        range: Some(self.node_location(node, file_path)),
                        normalized_signature: Some(format!("class {}", name)),
                        visibility,
                    });

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
            "assignment" => {
                // Module-level simple bindings only.
                if function_depth == 0
                    && parent_scope.is_empty()
                    && let Some(left) = node.child_by_field_name("left")
                    && left.kind() == "identifier"
                {
                    let name = &content[left.byte_range()];
                    // `__all__` and dunder assignments are metadata, not variables.
                    if !(name.starts_with("__") && name.ends_with("__")) {
                        let qname = qualified_name(file_path, "", name);
                        let visibility = self.detect_visibility(name);
                        let binding_text = content[node.byte_range()].trim().to_string();
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
                            normalized_signature: Some(binding_text),
                            visibility,
                        });
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

    /// Deterministic `:overload:N` suffixes for colliding identities.
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
        symbols.sort_by(|a, b| {
            a.stable_id
                .cmp(&b.stable_id)
                .then_with(|| a.qualified_name.cmp(&b.qualified_name))
        });
    }

    /// Normalized signature: `name(params) -> return` (async prefixed).
    fn compute_signature(&self, node: Node, content: &str, name: &str) -> String {
        let params = node
            .child_by_field_name("parameters")
            .map(|p| content[p.byte_range()].trim().to_string())
            .unwrap_or_else(|| "()".to_string());
        let ret = node
            .child_by_field_name("return_type")
            .map(|r| {
                format!(
                    " -> {}",
                    content[r.byte_range()]
                        .trim()
                        .trim_start_matches("->")
                        .trim()
                )
            })
            .unwrap_or_default();
        let async_prefix = if self.is_async_def(node, content) {
            "async "
        } else {
            ""
        };
        format!("{}{}{}{}", async_prefix, name, params, ret)
    }

    /// Detects `async def` via the line prefix (decorators are siblings,
    /// so the def node starts on its own line in the common layout).
    fn is_async_def(&self, node: Node, content: &str) -> bool {
        let start = node.start_byte();
        let line_start = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = content[line_start..start].trim_end();
        prefix == "async" || prefix.ends_with(" async")
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
            let text = content[node.byte_range()].trim();
            let body = text.strip_prefix("import").unwrap_or(text).trim();
            for part in body.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                // `a.b [as c]`
                let dotted = part.split(" as ").next().unwrap_or(part).trim();
                if dotted.is_empty() {
                    continue;
                }
                let resolved = self.module_bridge.resolve_absolute(dotted);
                edges.push(Edge {
                    source: file_symbol.clone(),
                    target: SymbolId::new(format!("file::{}", resolved)),
                    kind: EdgeKind::Imports,
                    coverage,
                    location: Some(self.node_location(node, file_path)),
                    provenance: "import_statement".to_string(),
                });
            }
        } else if node.kind() == "import_from_statement" {
            let text = content[node.byte_range()].trim();
            if let Some(rest) = text.strip_prefix("from") {
                let rest = rest.trim();
                // Split `...mod import ...` at the `import` keyword.
                if let Some(pos) = rest.find(" import ") {
                    let from_part = rest[..pos].trim();
                    let dots = from_part.chars().take_while(|&c| c == '.').count();
                    let module = from_part[dots..].trim();
                    if !(dots == 0 && module.is_empty()) {
                        let resolved = self.module_bridge.resolve_from(file_path, dots, module);
                        edges.push(Edge {
                            source: file_symbol.clone(),
                            target: SymbolId::new(format!("file::{}", resolved)),
                            kind: EdgeKind::Imports,
                            coverage,
                            location: Some(self.node_location(node, file_path)),
                            provenance: "import_from_statement".to_string(),
                        });
                    }
                }
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
        coverage: CoverageState,
        edges: &mut Vec<Edge>,
    ) {
        if node.kind() == "class_definition"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            let name = &content[name_node.byte_range()];
            let class_symbol = self.symbol_identity(file_path, SymbolKind::Class, "", name);

            if let Some(sup) = node.child_by_field_name("superclasses") {
                let sup_text = content[sup.byte_range()].trim();
                let inner = sup_text
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .trim();
                for base in inner.split(',') {
                    let base = base.trim();
                    // Skip keyword bases (`metaclass=...`) and parameterized
                    // generics (`Generic[T]`); record plain names only.
                    let base_name = base.split('[').next().unwrap_or(base).trim();
                    if base_name.is_empty()
                        || base_name.contains('=')
                        || base_name.contains('(')
                        || base_name.contains(' ')
                    {
                        continue;
                    }
                    let short = base_name.rsplit('.').next().unwrap_or(base_name);
                    edges.push(Edge {
                        source: class_symbol.clone(),
                        target: SymbolId::new(format!("symbol::{}", short)),
                        kind: EdgeKind::Inherits,
                        coverage,
                        location: Some(self.node_location(node, file_path)),
                        provenance: "class_bases".to_string(),
                    });
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_class_hierarchy_edges(child, content, file_path, coverage, edges);
        }
    }
}

impl PythonAdapter {
    /// Collects local import bindings: local name -> resolved module file.
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
            let text = content[node.byte_range()].trim();
            let body = text.strip_prefix("import").unwrap_or(text).trim();
            for part in body.split(',') {
                let part = part.trim();
                let (dotted, alias) = match part.split_once(" as ") {
                    Some((d, a)) => (d.trim(), Some(a.trim().to_string())),
                    None => (part, None),
                };
                if dotted.is_empty() {
                    continue;
                }
                let local = alias
                    .unwrap_or_else(|| dotted.rsplit('.').next().unwrap_or(dotted).to_string());
                bindings.insert(local, self.module_bridge.resolve_absolute(dotted));
            }
        } else if node.kind() == "import_from_statement" {
            let text = content[node.byte_range()].trim();
            if let Some(rest) = text.strip_prefix("from") {
                let rest = rest.trim();
                if let Some(pos) = rest.find(" import ") {
                    let from_part = rest[..pos].trim();
                    let names_part = rest[pos + " import ".len()..].trim();
                    let dots = from_part.chars().take_while(|&c| c == '.').count();
                    let module = from_part[dots..].trim();
                    let target = if dots == 0 {
                        self.module_bridge.resolve_from(file_path, 0, module)
                    } else if module.is_empty() {
                        // `from . import sibling`: each name is its own module.
                        String::new()
                    } else {
                        self.module_bridge.resolve_from(file_path, dots, module)
                    };
                    let names = names_part
                        .trim()
                        .trim_start_matches('(')
                        .trim_end_matches(')');
                    for name_part in names.split(',') {
                        let name_part = name_part.trim().trim_end_matches(';');
                        if name_part.is_empty() || name_part == "*" {
                            continue;
                        }
                        let (orig, alias) = match name_part.split_once(" as ") {
                            Some((o, a)) => (o.trim(), a.trim().to_string()),
                            None => (name_part, name_part.to_string()),
                        };
                        if target.is_empty() {
                            // Sibling module import: resolve per name.
                            let pkg = self.module_bridge.package_dir(file_path, dots);
                            let sib = if pkg.is_empty() {
                                format!("{}.py", orig)
                            } else {
                                format!("{}/{}.py", pkg, orig)
                            };
                            bindings.insert(alias, sib);
                        } else {
                            bindings.insert(alias, target.clone());
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

    /// Collects declared receiver types: fields `(class, field) -> type`
    /// from `__init__` param assignments and annotations, plus locals
    /// `(function, binding) -> type` from annotated params, annotated
    /// declarators, and `Class(...)` construction calls.
    fn collect_receiver_types(&self, root: Node, content: &str) -> (ReceiverMap, ReceiverMap) {
        let mut fields = BTreeMap::new();
        let mut locals = BTreeMap::new();
        self.collect_receiver_types_inner(root, content, "", "", &mut fields, &mut locals);
        (fields, locals)
    }

    fn collect_receiver_types_inner(
        &self,
        node: Node,
        content: &str,
        enclosing_class: &str,
        enclosing_fn: &str,
        fields: &mut BTreeMap<(String, String), String>,
        locals: &mut BTreeMap<(String, String), String>,
    ) {
        let kind = node.kind();
        let mut next_class = enclosing_class.to_string();
        let mut next_fn = enclosing_fn.to_string();

        if kind == "class_definition"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            next_class = content[name_node.byte_range()].to_string();
        }
        if kind == "function_definition"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            next_fn = content[name_node.byte_range()].to_string();
            // Annotated parameters: `def f(svc: Service)`.
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for param in params.children(&mut cursor) {
                    if let Some((binding, ty)) = self.split_binding_type(param, content) {
                        // Constructor params declare instance fields.
                        if next_fn == "__init__" && !next_class.is_empty() {
                            fields.insert((next_class.clone(), binding.clone()), ty.clone());
                        }
                        locals.insert((next_fn.clone(), binding), ty);
                    }
                }
            }
        }
        // `self.field = value` / `self.field: Type = value`.
        if kind == "assignment"
            && let Some(left) = node.child_by_field_name("left")
            && left.kind() == "attribute"
            && let Some(obj) = left.child_by_field_name("object")
            && content[obj.byte_range()] == *"self"
            && !enclosing_class.is_empty()
            && let Some(attr) = left.child_by_field_name("attribute")
        {
            let field = content[attr.byte_range()].to_string();
            let mut ty: Option<String> = None;
            // Annotated assignment carries the type.
            if let Some(ann) = node.child_by_field_name("type") {
                let t = content[ann.byte_range()].trim().to_string();
                if Self::is_plain_type(&t) {
                    ty = Some(t);
                }
            }
            // `self.x = ClassName(...)` construction call.
            if ty.is_none()
                && let Some(value) = node.child_by_field_name("right")
                && value.kind() == "call"
                && let Some(func) = value.child_by_field_name("function")
                && func.kind() == "identifier"
            {
                ty = Some(content[func.byte_range()].to_string());
            }
            // `self.x = param` copies the parameter's declared type.
            if ty.is_none()
                && let Some(value) = node.child_by_field_name("right")
                && value.kind() == "identifier"
            {
                let pname = content[value.byte_range()].to_string();
                if let Some(pt) = locals.get(&(next_fn.clone(), pname)) {
                    ty = Some(pt.clone());
                }
            }
            if let Some(ty) = ty {
                fields.insert((enclosing_class.to_string(), field), ty);
            }
        }
        // Module/function locals: `x: C = ...` and `x = C(...)`.
        if kind == "assignment"
            && let Some(left) = node.child_by_field_name("left")
            && left.kind() == "identifier"
        {
            let binding = content[left.byte_range()].to_string();
            let mut ty: Option<String> = None;
            if let Some(ann) = node.child_by_field_name("type") {
                let t = content[ann.byte_range()].trim().to_string();
                if Self::is_plain_type(&t) {
                    ty = Some(t);
                }
            }
            if ty.is_none()
                && let Some(value) = node.child_by_field_name("right")
                && value.kind() == "call"
                && let Some(func) = value.child_by_field_name("function")
                && func.kind() == "identifier"
            {
                ty = Some(content[func.byte_range()].to_string());
            }
            if let Some(ty) = ty {
                locals.insert((next_fn.clone(), binding), ty);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_receiver_types_inner(
                child,
                content,
                &next_class,
                &next_fn,
                fields,
                locals,
            );
        }
    }

    /// Splits a parameter node into (binding, plain type name).
    /// Handles `name`, `name: Type`, `name: Type = default`,
    /// `*args: Type`, `**kwargs: Type`. Returns None for composites.
    fn split_binding_type(&self, param: Node, content: &str) -> Option<(String, String)> {
        let text = content[param.byte_range()].trim();
        let text = text
            .strip_prefix('*')
            .unwrap_or(text)
            .trim_start_matches('*')
            .trim();
        let (binding, rest) = match text.split_once(':') {
            Some((b, r)) => (b.trim(), r.trim()),
            None => return None,
        };
        if binding.is_empty()
            || binding.contains(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
        {
            return None;
        }
        // Strip `= default` tail.
        let ty = rest.split('=').next().unwrap_or(rest).trim();
        if !Self::is_plain_type(ty) {
            return None;
        }
        Some((binding.to_string(), ty.to_string()))
    }

    fn is_plain_type(ty: &str) -> bool {
        !ty.is_empty()
            && ty
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$')
            && !ty.contains("..")
    }

    /// Resolves a class name to its emitted class symbol.
    fn resolve_class_symbol(
        &self,
        class_name: &str,
        file_path: &str,
        file_symbols: &[Symbol],
        symbol_index: &BTreeMap<String, SymbolId>,
        imported: &BTreeMap<String, String>,
    ) -> Option<(SymbolId, String, String)> {
        let short = class_name.rsplit('.').next().unwrap_or(class_name);
        // 1. Same-file class.
        let same_file = stable_symbol_id(file_path, SymbolKind::Class, "", short);
        if file_symbols.iter().any(|s| s.stable_id == same_file) {
            return Some((same_file, file_path.to_string(), short.to_string()));
        }
        // 2. Import-bound module.
        if let Some(module) = imported.get(short) {
            let qname = format!("{}::{}", module, short);
            if let Some(id) = symbol_index.get(&qname) {
                return Some((id.clone(), module.clone(), short.to_string()));
            }
        }
        // 3. Unique global class match.
        let mut matches: Vec<(String, SymbolId)> = symbol_index
            .iter()
            .filter(|(qname, id)| {
                id.as_str().contains(&format!("#class:{}", short)) && short_name(qname) == short
            })
            .map(|(qname, id)| (qname.clone(), id.clone()))
            .collect();
        matches.sort();
        matches.dedup();
        if matches.len() == 1 {
            let (qname, id) = matches[0].clone();
            let mut parts: Vec<&str> = qname.split("::").collect();
            if parts.len() >= 2 {
                parts.pop();
                return Some((id, parts.join("::"), short.to_string()));
            }
        }
        None
    }

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
}

impl PythonAdapter {
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
        let (field_types, local_types) = self.collect_receiver_types(root, content);
        let mut ctx = CallWalkContext::default();
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
        field_types: &ReceiverMap,
        local_types: &ReceiverMap,
        coverage: CoverageState,
        ctx: &mut CallWalkContext,
        edges: &mut Vec<Edge>,
    ) {
        let kind = node.kind();

        // Track enclosing callable + class.
        if kind == "function_definition"
            && let Some(name_node) = node.child_by_field_name("name")
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
        if kind == "class_definition"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            ctx.enclosing_class = content[name_node.byte_range()].to_string();
        }
        // Arrow-less lambdas assigned to names: `f = lambda x: ...`.
        if kind == "assignment"
            && let Some(left) = node.child_by_field_name("left")
            && left.kind() == "identifier"
            && let Some(value) = node.child_by_field_name("right")
            && value.kind() == "lambda"
        {
            let name = &content[left.byte_range()];
            let row = node.start_position().row + 1;
            if let Some(sym) = file_symbols.iter().find(|s| {
                short_name(&s.qualified_name) == name
                    && s.range.as_ref().map(|r| r.start_line) == Some(row as u32)
            }) {
                ctx.enclosing = Some(sym.stable_id.clone());
            }
        }

        if kind == "call"
            && let Some(func) = node.child_by_field_name("function")
        {
            self.emit_call_edge(
                func,
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
        if matches!(kind, "function_definition" | "lambda" | "class_definition") {
            ctx.enclosing = saved_enclosing;
            ctx.enclosing_class = saved_class;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_call_edge(
        &self,
        func: Node,
        content: &str,
        file_path: &str,
        file_symbol: &SymbolId,
        file_symbols: &[Symbol],
        bare_callables: &BTreeMap<String, Vec<SymbolId>>,
        class_names: &BTreeMap<String, SymbolId>,
        imported: &BTreeMap<String, String>,
        symbol_index: &BTreeMap<String, SymbolId>,
        field_types: &ReceiverMap,
        local_types: &ReceiverMap,
        coverage: CoverageState,
        ctx: &CallWalkContext,
        site: Node,
        edges: &mut Vec<Edge>,
    ) {
        let source = ctx.enclosing.clone().unwrap_or_else(|| file_symbol.clone());

        // Cross-file import resolution: exactly one same-named symbol in
        // the resolved target module links to the real id.
        let resolve_imported = |name: &str, module: &str| -> Option<SymbolId> {
            // `from . import sibling` produced a package dir, not a file.
            if !module.ends_with(".py") {
                return None;
            }
            let prefix = format!("{}::", module);
            let mut candidates: Vec<&SymbolId> = symbol_index
                .iter()
                .filter(|(qname, _)| qname.starts_with(&prefix) && short_name(qname) == name)
                .map(|(_, id)| id)
                .collect();
            candidates.sort();
            candidates.dedup();
            if candidates.len() == 1 {
                Some(candidates[0].clone())
            } else {
                None
            }
        };

        // Resolve a method on a known class: verify the method was emitted.
        let method_target =
            |class_file: &str, class_name: &str, method: &str| -> Option<SymbolId> {
                let method_id =
                    stable_symbol_id(class_file, SymbolKind::Method, class_name, method);
                let known = symbol_index.values().any(|id| id == &method_id)
                    || file_symbols.iter().any(|s| s.stable_id == method_id);
                known.then_some(method_id)
            };

        let (target, target_coverage, provenance): (Option<SymbolId>, CoverageState, String) =
            match func.kind() {
                "identifier" => {
                    let name = &content[func.byte_range()];
                    // Constructor-style call to a known class first:
                    // `Service(...)` is a Calls edge to the class.
                    if let Some(class_id) = class_names.get(name) {
                        (
                            Some(class_id.clone()),
                            coverage,
                            "call_expression:local-class".to_string(),
                        )
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
                            None => {
                                // Imported class constructed cross-file.
                                match self.resolve_class_symbol(
                                    name,
                                    file_path,
                                    file_symbols,
                                    symbol_index,
                                    imported,
                                ) {
                                    Some((class_id, _, _)) => (
                                        Some(class_id),
                                        coverage,
                                        format!("call_expression:imported-class:{}", module),
                                    ),
                                    None => (
                                        Some(SymbolId::new(format!("symbol::{}", name))),
                                        CoverageState::Partial,
                                        format!("call_expression:imported-name:{}", module),
                                    ),
                                }
                            }
                        }
                    } else {
                        (None, coverage, String::new())
                    }
                }
                "attribute" => {
                    let object = func.child_by_field_name("object");
                    let property = func.child_by_field_name("attribute");
                    match (object, property) {
                        (Some(obj), Some(prop)) => {
                            let obj_text = &content[obj.byte_range()];
                            let method = &content[prop.byte_range()];
                            if method.is_empty()
                                || method.contains(|c: char| {
                                    !c.is_alphanumeric() && c != '_' && c != '$'
                                })
                            {
                                (None, coverage, String::new())
                            } else if obj_text == "self" && !ctx.enclosing_class.is_empty() {
                                match method_target(file_path, &ctx.enclosing_class.clone(), method)
                                {
                                    Some(id) => (
                                        Some(id),
                                        coverage,
                                        "call_expression:this-method".to_string(),
                                    ),
                                    None => (None, coverage, String::new()),
                                }
                            } else if obj_text == "super()" && !ctx.enclosing_class.is_empty() {
                                // `super().m()`: resolve against base classes
                                // carrying an emitted method of that name.
                                match self.resolve_super_method(
                                    file_path,
                                    &ctx.enclosing_class,
                                    method,
                                    file_symbols,
                                    symbol_index,
                                ) {
                                    Some(id) => (
                                        Some(id),
                                        coverage,
                                        "call_expression:super-method".to_string(),
                                    ),
                                    None => (None, coverage, String::new()),
                                }
                            } else if obj.kind() == "call"
                                && let Some(ctor_fn) = obj.child_by_field_name("function")
                                && ctor_fn.kind() == "identifier"
                            {
                                // `Class().m(...)`: an inline construction chain. The
                                // receiver's class is the constructed name, so the method
                                // resolves against it without any local binding.
                                let ctor = &content[ctor_fn.byte_range()];
                                let resolved = match class_names.get(ctor) {
                                    Some(class_id) => method_target(file_path, ctor, method)
                                        .or_else(|| Some(class_id.clone())),
                                    None => self
                                        .resolve_class_symbol(
                                            ctor,
                                            file_path,
                                            file_symbols,
                                            symbol_index,
                                            imported,
                                        )
                                        .and_then(|(_, class_file, class_name)| {
                                            method_target(&class_file, &class_name, method)
                                        }),
                                };
                                match resolved {
                                    Some(id) => (
                                        Some(id),
                                        coverage,
                                        "call_expression:chained-constructor".to_string(),
                                    ),
                                    None => (None, coverage, String::new()),
                                }
                            } else {
                                let receiver_type = if let Some(field) =
                                    obj_text.strip_prefix("self.").and_then(|f| {
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
                                        match method_target(&class_file, &class_name, method) {
                                            Some(id) => (
                                                Some(id),
                                                coverage,
                                                "call_expression:typed-receiver".to_string(),
                                            ),
                                            None => (None, coverage, String::new()),
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

    /// Resolves `super().method()` to an emitted base-class method.
    /// Same-file bases win; otherwise the method name must be unique
    /// across all other indexed modules (never guess between candidates).
    fn resolve_super_method(
        &self,
        file_path: &str,
        enclosing_class: &str,
        method: &str,
        file_symbols: &[Symbol],
        symbol_index: &BTreeMap<String, SymbolId>,
    ) -> Option<SymbolId> {
        // Same-file candidate bases: any other emitted class carrying it.
        let mut same_file: Vec<String> = file_symbols
            .iter()
            .filter(|s| s.kind == "class")
            .map(|s| short_name(&s.qualified_name).to_string())
            .filter(|n| n != enclosing_class)
            .collect();
        same_file.sort();
        same_file.dedup();
        for base in same_file {
            let method_id = stable_symbol_id(file_path, SymbolKind::Method, &base, method);
            if file_symbols.iter().any(|s| s.stable_id == method_id) {
                return Some(method_id);
            }
        }
        // Cross-module: method name must resolve to exactly one symbol
        // outside this file.
        let mut candidates: Vec<SymbolId> = symbol_index
            .iter()
            .filter(|(qname, id)| {
                id.as_str().contains("#method:")
                    && short_name(qname) == method
                    && !qname.starts_with(&format!("{}::", file_path))
            })
            .map(|(_, id)| id.clone())
            .collect();
        candidates.sort();
        candidates.dedup();
        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }
        None
    }
}

impl PythonAdapter {
    fn collect_contracts(
        &self,
        node: Node,
        content: &str,
        file_path: &str,
        parent_scope: &str,
        contracts: &mut Vec<Contract>,
    ) {
        let kind = node.kind();

        if kind == "decorated_definition" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "decorator" {
                    continue;
                }
                self.collect_contracts(child, content, file_path, parent_scope, contracts);
            }
            return;
        }

        if kind == "function_definition"
            && let Some(name_node) = node.child_by_field_name("name")
        {
            let name = &content[name_node.byte_range()];
            let owner = if parent_scope.is_empty() {
                self.symbol_identity(file_path, SymbolKind::Function, "", name)
            } else {
                self.symbol_identity(file_path, SymbolKind::Method, parent_scope, name)
            };
            let visibility = self.detect_visibility(name);
            let (inputs, output) = self.extract_parameter_and_return_types(node, content);
            contracts.push(Contract {
                owner,
                visibility,
                inputs,
                output,
                effects: Vec::new(),
            });
        } else if kind == "class_definition"
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
            // Nested closures carry module scope (parity with TS adapter:
            // shared qualified names dedupe deterministically downstream).
            self.collect_contracts(child, content, file_path, parent_scope, contracts);
        }
    }

    /// Splits a parameters node on top-level commas and normalizes each
    /// entry to `name`, `name: Type`, `*args`, or `name = default`.
    fn extract_parameter_and_return_types(
        &self,
        node: Node,
        content: &str,
    ) -> (Vec<String>, Option<String>) {
        let mut inputs = Vec::new();

        if let Some(params_node) = node.child_by_field_name("parameters") {
            let params_text = content[params_node.byte_range()].trim();
            let inner = params_text
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim();
            for raw in Self::split_top_level(inner) {
                let entry = raw.trim().trim_end_matches(',').trim();
                if entry.is_empty() || entry == "/" || entry == "*" {
                    continue;
                }
                // Drop `self`/`cls` receivers: not part of the call contract.
                let bare = entry.trim_start_matches('*').trim_start_matches('*').trim();
                let name = bare.split([':', '=']).next().unwrap_or(bare).trim();
                if name == "self" || name == "cls" {
                    continue;
                }
                inputs.push(Self::normalize_param(entry));
            }
        }

        let output = node.child_by_field_name("return_type").map(|ret| {
            let t = content[ret.byte_range()].trim();
            t.trim_start_matches("->").trim().to_string()
        });

        (inputs, output)
    }

    /// Splits on top-level commas, respecting bracket/paren depth and
    /// string literals.
    fn split_top_level(text: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut depth = 0usize;
        let mut current = String::new();
        let mut in_str: Option<char> = None;
        for c in text.chars() {
            if let Some(q) = in_str {
                current.push(c);
                if c == q {
                    in_str = None;
                }
                continue;
            }
            match c {
                '\'' | '"' => {
                    in_str = Some(c);
                    current.push(c);
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    current.push(c);
                }
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    current.push(c);
                }
                ',' if depth == 0 => {
                    parts.push(std::mem::take(&mut current));
                }
                _ => current.push(c),
            }
        }
        if !current.trim().is_empty() {
            parts.push(current);
        }
        parts
    }

    /// Normalizes one parameter entry: strips default values to a
    /// canonical `= default` marker while keeping name and annotation.
    fn normalize_param(entry: &str) -> String {
        let stars = if entry.starts_with("**") {
            "**"
        } else if entry.starts_with('*') {
            "*"
        } else {
            ""
        };
        let rest = entry.trim_start_matches('*').trim();
        let (name_type, has_default) = match rest.split_once('=') {
            Some((l, _)) => (l.trim(), true),
            None => (rest, false),
        };
        let normalized = if has_default {
            format!("{} = default", name_type.trim_end())
        } else {
            name_type.to_string()
        };
        format!("{}{}", stars, normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_adapter_api::conformance::verify_adapter_conformance;
    use std::collections::BTreeMap;

    fn test_file_unit(path: &str) -> FileUnit {
        FileUnit {
            path: path.to_string(),
            language: "python".to_string(),
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
    fn test_adapter_capabilities_honesty() {
        let adapter = PythonAdapter::new();
        let caps = adapter.capabilities();

        assert_eq!(caps.language_id, "python");
        assert_eq!(caps.adapter_version, "0.0.1");
        assert!(caps.supports_ast_parsing);
        assert!(caps.supports_symbol_extraction);
        assert!(caps.supports_edge_extraction);
        assert!(caps.supports_contract_extraction);
        assert!(caps.supported_extensions.contains(&"py".to_string()));
        assert!(caps.unsupported_constructs.contains(&"eval".to_string()));
        assert!(caps.unsupported_constructs.contains(&"exec".to_string()));
        assert!(
            caps.unsupported_constructs
                .contains(&"dynamic_import".to_string())
        );
        assert!(
            caps.unsupported_constructs
                .contains(&"reflection_getattr".to_string())
        );
        assert!(
            caps.unsupported_constructs
                .contains(&"generated_code".to_string())
        );
    }

    #[test]
    fn test_adapter_conformance_suite() {
        let adapter = PythonAdapter::new();

        let valid_source = r#"
class Service:
    def greet(self, name: str) -> str:
        return f"Hello, {name}"
"#;

        let invalid_source = r#"
def broken(:
"#;

        let dynamic_source = r#"
def run_dynamic(code: str):
    return eval(code)
"#;

        verify_adapter_conformance(
            &adapter,
            valid_source,
            invalid_source,
            dynamic_source,
            "src/service.py",
        );
    }

    #[test]
    fn test_discover_extensions() {
        let adapter = PythonAdapter::new();

        assert!(adapter.discover(Path::new("src/main.py")));
        assert!(adapter.discover(Path::new("src/types.pyi")));
        assert!(!adapter.discover(Path::new("src/index.ts")));
        assert!(!adapter.discover(Path::new("app/main.rs")));
        assert!(!adapter.discover(Path::new("src/Main.java")));
    }

    #[test]
    fn test_module_bridge_resolution() {
        let bridge = PythonModuleBridge::new(".");

        assert_eq!(bridge.resolve_absolute("pkg.mod"), "pkg/mod.py");
        assert_eq!(
            bridge.resolve_from("src/api/handler.py", 1, "service"),
            "src/api/service.py"
        );
        assert_eq!(
            bridge.resolve_from("src/api/handler.py", 2, "utils.math"),
            "src/utils/math.py"
        );
        assert_eq!(
            bridge.resolve_from("src/api/handler.py", 0, "os"),
            "src/api/os.py"
        );
    }

    #[test]
    fn test_stable_symbol_id_resilience_to_line_shifts() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");

        let code_line1 = "def add(a, b):\n    return a + b\n";
        let code_line25 = format!("{}\ndef add(a, b):\n    return a + b\n", "\n".repeat(24));

        let file = test_file_unit("src/math.py");

        let parsed1 = adapter.parse(&ctx, &file, code_line1).unwrap();
        let symbols1 = adapter.extract_symbols(&parsed1).unwrap();

        let parsed2 = adapter.parse(&ctx, &file, &code_line25).unwrap();
        let symbols2 = adapter.extract_symbols(&parsed2).unwrap();

        assert_eq!(symbols1.len(), 1);
        assert_eq!(symbols2.len(), 1);
        assert_eq!(symbols1[0].stable_id, symbols2[0].stable_id);
        assert_eq!(symbols1[0].stable_id.as_str(), "src/math.py#fn:add");
    }

    #[test]
    fn test_identity_stable_across_signature_edit() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/svc.py");

        let before = "def create(name: str):\n    return name\n";
        let after = "def create(name: str, tenant_id: str):\n    return name\n";

        let parsed_b = adapter.parse(&ctx, &file, before).unwrap();
        let syms_b = adapter.extract_symbols(&parsed_b).unwrap();
        let parsed_a = adapter.parse(&ctx, &file, after).unwrap();
        let syms_a = adapter.extract_symbols(&parsed_a).unwrap();

        assert_eq!(syms_b.len(), 1);
        assert_eq!(syms_a.len(), 1);
        assert_eq!(syms_b[0].stable_id, syms_a[0].stable_id);
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
    fn test_extract_contracts_python() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");

        let source = r#"
def process(tx_id: str, amount: float, dry_run: bool = False, *args, **kwargs) -> bool:
    return True
"#;
        let file = test_file_unit("src/billing.py");
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let contracts = adapter.extract_contracts(&parsed).unwrap();

        assert_eq!(contracts.len(), 1);
        let c = &contracts[0];
        assert_eq!(c.owner.as_str(), "src/billing.py#fn:process");
        assert_eq!(c.visibility, Visibility::Public);
        assert_eq!(
            c.inputs,
            vec![
                "tx_id: str".to_string(),
                "amount: float".to_string(),
                "dry_run: bool = default".to_string(),
                "*args".to_string(),
                "**kwargs".to_string(),
            ]
        );
        assert_eq!(c.output, Some("bool".to_string()));
    }

    #[test]
    fn test_visibility_private_underscore() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/mod.py");

        let source = "def public_fn():\n    pass\n\ndef _private_fn():\n    pass\n";
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();

        let public = symbols
            .iter()
            .find(|s| s.qualified_name.contains("public_fn"))
            .unwrap();
        let private = symbols
            .iter()
            .find(|s| s.qualified_name.contains("_private_fn"))
            .unwrap();
        assert_eq!(public.visibility, Visibility::Public);
        assert_eq!(private.visibility, Visibility::Private);
    }

    #[test]
    fn test_call_edges_local_self_new_and_typed() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/svc.py");

        let source = r#"
def hash_password(raw):
    return raw

def create_user(name):
    return hash_password(name)

class Service:
    def __init__(self, store):
        self.store = store

    def run(self):
        return self.helper()

    def helper(self):
        return self.store.load()

def boot(store):
    svc = Service(store)
    return svc
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let index = symbol_index_of(&symbols);
        let edges = adapter.extract_edges(&parsed, &index).unwrap();
        let calls: Vec<&Edge> = edges.iter().filter(|e| e.kind == EdgeKind::Calls).collect();

        let has = |src: &str, dst: &str| {
            calls
                .iter()
                .any(|e| e.source.as_str() == src && e.target.as_str() == dst)
        };

        assert!(
            has("src/svc.py#fn:create_user", "src/svc.py#fn:hash_password"),
            "missing local call: {:?}",
            calls
        );
        assert!(
            has(
                "src/svc.py#method:Service.run",
                "src/svc.py#method:Service.helper"
            ),
            "missing self call: {:?}",
            calls
        );
        // `self.store.load()` — store is untyped here (param without
        // annotation, `__init__(self, store)`), so no edge: honest silence.
        assert!(
            !calls
                .iter()
                .any(|e| e.source.as_str() == "src/svc.py#method:Service.helper"),
            "untyped receiver must stay silent: {:?}",
            calls
        );
        // `Service(store)` constructor call from boot.
        assert!(
            has("src/svc.py#fn:boot", "src/svc.py#class:Service"),
            "missing constructor call: {:?}",
            calls
        );
    }

    #[test]
    fn test_call_edges_imported_resolved_and_di_field() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");

        let svc_file = test_file_unit("src/services/user_service.py");
        let svc_source = r#"
class UserService:
    def create_user(self, name):
        return name
"#;
        let parsed_svc = adapter.parse(&ctx, &svc_file, svc_source).unwrap();
        let svc_symbols = adapter.extract_symbols(&parsed_svc).unwrap();

        let ctl_file = test_file_unit("src/controllers/user_controller.py");
        let ctl_source = r#"
from ..services.user_service import UserService


class UserController:
    def __init__(self, user_service: UserService):
        self.user_service = user_service

    def handle_create(self):
        return self.user_service.create_user("a")
"#;
        let parsed_ctl = adapter.parse(&ctx, &ctl_file, ctl_source).unwrap();
        let ctl_symbols = adapter.extract_symbols(&parsed_ctl).unwrap();

        let mut index = symbol_index_of(&svc_symbols);
        for s in &ctl_symbols {
            index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }

        let edges = adapter.extract_edges(&parsed_ctl, &index).unwrap();
        let di_edge = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str()
                        == "src/controllers/user_controller.py#method:UserController.handle_create"
            })
            .expect("DI field call edge must exist");
        assert_eq!(
            di_edge.target.as_str(),
            "src/services/user_service.py#method:UserService.create_user"
        );
        assert_eq!(di_edge.provenance, "call_expression:typed-receiver");
    }

    #[test]
    fn test_call_edges_chained_constructor_receiver() {
        // Regression: `Calculator().run(...)` is a call on an inline
        // construction. Before this was resolved, the caller was absent
        // from the backward closure entirely (silent recall loss).
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");

        let math_file = test_file_unit("src/math.py");
        let math_source = "def add(a, b):\n    return a + b\n";
        let parsed_math = adapter.parse(&ctx, &math_file, math_source).unwrap();
        let math_symbols = adapter.extract_symbols(&parsed_math).unwrap();

        let calc_file = test_file_unit("src/calc.py");
        let calc_source = r#"
from math import add


class Calculator:
    def run(self, x, y):
        return add(x, y)


def helper(n):
    return Calculator().run(n, 2)
"#;
        let parsed_calc = adapter.parse(&ctx, &calc_file, calc_source).unwrap();
        let calc_symbols = adapter.extract_symbols(&parsed_calc).unwrap();

        let mut index = symbol_index_of(&math_symbols);
        for s in &calc_symbols {
            index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }

        let edges = adapter.extract_edges(&parsed_calc, &index).unwrap();
        let chain_edge = edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Calls
                    && e.source.as_str() == "src/calc.py#fn:helper"
                    && e.provenance == "call_expression:chained-constructor"
            })
            .expect("chained-constructor call edge must exist");
        assert_eq!(
            chain_edge.target.as_str(),
            "src/calc.py#method:Calculator.run"
        );
    }

    #[test]
    fn test_hierarchy_edges() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/dog.py");

        let source = r#"
from .animal import Animal

class Dog(Animal):
    def bark(self):
        return "woof"
"#;
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let edges = adapter.extract_edges(&parsed, &BTreeMap::new()).unwrap();

        assert!(
            edges
                .iter()
                .any(|e| e.kind == EdgeKind::Imports && e.target.as_str() == "file::src/animal.py"),
            "missing import edge: {:?}",
            edges
        );
        let inherit = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Inherits)
            .expect("missing inherits edge");
        assert_eq!(inherit.source.as_str(), "src/dog.py#class:Dog");
        assert_eq!(inherit.target.as_str(), "symbol::Animal");
    }

    #[test]
    fn test_honest_coverage_dynamic_constructs() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");

        let clean = test_file_unit("src/clean.py");
        let parsed_clean = adapter.parse(&ctx, &clean, "x = 42\n").unwrap();
        assert_eq!(parsed_clean.coverage, CoverageState::Verified);

        let eval_file = test_file_unit("src/eval.py");
        let parsed_eval = adapter
            .parse(&ctx, &eval_file, "def run(s):\n    return eval(s)\n")
            .unwrap();
        assert_eq!(parsed_eval.coverage, CoverageState::Unknown);

        let exec_file = test_file_unit("src/execf.py");
        let parsed_exec = adapter
            .parse(&ctx, &exec_file, "def run(s):\n    exec(s)\n")
            .unwrap();
        assert_eq!(parsed_exec.coverage, CoverageState::Unknown);

        let import_file = test_file_unit("src/dyn.py");
        let parsed_dyn = adapter
            .parse(
                &ctx,
                &import_file,
                "def load(m):\n    return __import__(m)\n",
            )
            .unwrap();
        assert_eq!(parsed_dyn.coverage, CoverageState::Unknown);

        let getattr_file = test_file_unit("src/refl.py");
        let parsed_refl = adapter
            .parse(&ctx, &getattr_file, "val = getattr(obj, 'prop')\n")
            .unwrap();
        assert_eq!(parsed_refl.coverage, CoverageState::Partial);
    }

    #[test]
    fn test_syntax_error_diagnostics_without_crashing() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");

        let file = test_file_unit("src/malformed.py");
        let parsed = adapter.parse(&ctx, &file, "def broken(:\n").unwrap();
        assert!(!parsed.syntax_valid);
        assert!(!parsed.diagnostics.is_empty());
        assert_eq!(parsed.coverage, CoverageState::Partial);
    }

    #[test]
    fn test_module_variables_captured() {
        let adapter = PythonAdapter::new();
        let ctx = ParseContext::new(".");
        let file = test_file_unit("src/defs.py");

        let source =
            "MAX_RETRIES = 3\n\n\ndef outer():\n    inner_local = 1\n    return inner_local\n";
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let ids: Vec<&str> = symbols.iter().map(|s| s.stable_id.as_str()).collect();

        assert!(ids.contains(&"src/defs.py#variable:MAX_RETRIES"));
        assert!(ids.contains(&"src/defs.py#fn:outer"));
        assert!(
            !ids.iter().any(|id| id.contains("inner_local")),
            "locals leaked: {:?}",
            ids
        );
    }
}
