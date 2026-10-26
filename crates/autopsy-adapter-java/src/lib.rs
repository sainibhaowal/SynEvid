//! autopsy-adapter-java: Production-grade Java language adapter.
//!
//! Deterministic AST analysis via `tree-sitter-java`, Spring/Jakarta DI
//! semantic awareness (`@Service`, `@Autowired`, `@Component`, `@Repository`,
//! `@RestController`, `@Bean`), class and interface inheritance hierarchies,
//! method invocations, and honest reflection/dynamic escape detection
//! (FR-014, FR-030).
//!
//! Dynamic constructs (`Class.forName`, `Method.invoke`, `Proxy.newProxyInstance`,
//! `sun.misc.Unsafe`, `native` declarations) degrade to `CoverageState::Partial`
//! or `Unknown`, never collapsing to `Verified`.

use autopsy_adapter_api::{
    AdapterCapabilities, AdapterError, DynamicConstruct, DynamicConstructKind, LanguageAdapter,
    ParseContext, ParseDiagnostic, ParsedFile,
};
use autopsy_domain::{
    Contract, CoverageState, Edge, EdgeKind, FileUnit, Severity, SourceLocation, Symbol, SymbolId,
    Visibility,
};
use autopsy_symbols::{SymbolKind, qualified_name, stable_symbol_id};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

pub const ADAPTER_VERSION: &str = "0.1.0";
pub const LANGUAGE_ID: &str = "java";

/// Java package and class resolver bridge.
#[derive(Debug, Clone, Default)]
pub struct JavaModuleBridge {
    pub root_dir: PathBuf,
    pub monorepo: Option<autopsy_workspace_registry::MonorepoWorkspace>,
}

impl JavaModuleBridge {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        let root = root_dir.into();
        let monorepo = autopsy_workspace_registry::MonorepoWorkspace::detect(&root);
        Self {
            root_dir: root,
            monorepo,
        }
    }

    pub fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Resolves a fully-qualified Java class name (e.g. `com.example.service.UserService`)
    /// to candidate file paths in the workspace (standard Maven/Gradle layout or flat).
    pub fn resolve_class(&self, class_fqn: &str) -> Option<String> {
        let rel_subpath = class_fqn.replace('.', "/");
        let candidate_filenames = [
            format!("{}.java", rel_subpath),
            format!("src/main/java/{}.java", rel_subpath),
            format!("src/test/java/{}.java", rel_subpath),
            format!("{}.kt", rel_subpath),
            format!("src/main/kotlin/{}.kt", rel_subpath),
        ];

        for cand in &candidate_filenames {
            if self.root_dir.join(cand).is_file() {
                return Some(cand.clone());
            }
        }

        // Search monorepo packages if present
        if let Some(monorepo) = &self.monorepo {
            for pkg in monorepo.packages.values() {
                for cand in &candidate_filenames {
                    let full_pkg_path = Path::new(&pkg.relative_path).join(cand);
                    if self.root_dir.join(&full_pkg_path).is_file() {
                        return Some(Self::slash(&full_pkg_path));
                    }
                }
            }
        }

        None
    }
}

/// Production Java language adapter.
#[derive(Debug, Clone)]
pub struct JavaAdapter {
    capabilities: AdapterCapabilities,
    module_bridge: JavaModuleBridge,
}

impl Default for JavaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaAdapter {
    pub fn new() -> Self {
        let capabilities = AdapterCapabilities {
            language_id: LANGUAGE_ID.to_string(),
            adapter_version: ADAPTER_VERSION.to_string(),
            supported_extensions: vec!["java".to_string(), "kt".to_string()],
            supports_ast_parsing: true,
            supports_symbol_extraction: true,
            supports_edge_extraction: true,
            supports_contract_extraction: true,
            supports_type_resolution: true,
            unsupported_constructs: vec![
                "reflection".to_string(),
                "dynamic_proxies".to_string(),
                "sun_misc_unsafe".to_string(),
                "native_jni".to_string(),
                "bytecode_manipulation".to_string(),
                "generated_code".to_string(),
            ],
        };
        Self {
            capabilities,
            module_bridge: JavaModuleBridge::new("."),
        }
    }

    pub fn with_root_dir(root_dir: impl Into<PathBuf>) -> Self {
        let mut adapter = Self::new();
        adapter.module_bridge = JavaModuleBridge::new(root_dir);
        adapter
    }

    fn init_parser(&self) -> Result<Parser, AdapterError> {
        let mut parser = Parser::new();
        let language = tree_sitter_java::LANGUAGE.into();
        parser.set_language(&language).map_err(|e| {
            AdapterError::Internal(format!("Failed to configure tree-sitter Java: {:?}", e))
        })?;
        Ok(parser)
    }

    fn parse_tree(&self, content: &str) -> Result<Tree, AdapterError> {
        let mut parser = self.init_parser()?;
        parser.parse(content, None).ok_or_else(|| {
            AdapterError::ParseFailed("Tree-sitter Java returned no syntax tree".to_string())
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

    fn detect_modifiers(node: Node, content: &str) -> (Visibility, Vec<String>) {
        let mut vis = Visibility::Internal; // Package-private default in Java
        let mut annotations = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "modifiers" => {
                    let mut mod_cursor = child.walk();
                    for mod_child in child.children(&mut mod_cursor) {
                        let text = &content[mod_child.byte_range()];
                        match text {
                            "public" => vis = Visibility::Public,
                            "protected" => vis = Visibility::Protected,
                            "private" => vis = Visibility::Private,
                            _ => {}
                        }
                        if mod_child.kind() == "annotation"
                            || mod_child.kind() == "marker_annotation"
                        {
                            let ann_name = mod_child
                                .child_by_field_name("name")
                                .map(|n| &content[n.byte_range()])
                                .unwrap_or(text);
                            annotations.push(ann_name.to_string());
                        }
                    }
                }
                "annotation" | "marker_annotation" => {
                    let text = &content[child.byte_range()];
                    let ann_name = child
                        .child_by_field_name("name")
                        .map(|n| &content[n.byte_range()])
                        .unwrap_or(text);
                    annotations.push(ann_name.to_string());
                }
                _ => {}
            }
        }

        (vis, annotations)
    }

    fn detect_dynamic_constructs(&self, content: &str, file_path: &str) -> Vec<DynamicConstruct> {
        let mut constructs = Vec::new();
        let reflection_patterns = [
            ("Class.forName", "Dynamic class loading via Class.forName"),
            ("getMethod(", "Reflection method lookup via getMethod"),
            (
                "getDeclaredMethod(",
                "Reflection method lookup via getDeclaredMethod",
            ),
            ("Method.invoke(", "Reflective invocation via Method.invoke"),
            (".invoke(", "Reflective invocation call"),
            ("Proxy.newProxyInstance", "Dynamic runtime proxy generation"),
            ("sun.misc.Unsafe", "Low-level memory access via Unsafe"),
            ("MethodHandles.lookup", "MethodHandle dynamic lookup"),
            ("native ", "Native JNI function declaration"),
        ];

        for (pattern, desc) in &reflection_patterns {
            if let Some(idx) = content.find(pattern) {
                let line = content[..idx].lines().count() as u32;
                constructs.push(DynamicConstruct {
                    kind: DynamicConstructKind::ReflectionOrProxy,
                    location: Some(SourceLocation {
                        path: file_path.to_string(),
                        start_line: line.max(1),
                        end_line: line.max(1),
                        start_col: None,
                        end_col: None,
                        symbol_id: None,
                    }),
                    description: desc.to_string(),
                });
            }
        }

        constructs
    }
}

impl LanguageAdapter for JavaAdapter {
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
            .and_then(|e| e.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("java") || ext.eq_ignore_ascii_case("kt"))
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
        let has_error = root.has_error();

        let mut diagnostics = Vec::new();
        if has_error {
            diagnostics.push(ParseDiagnostic {
                severity: Severity::Error,
                message: "Java syntax error detected in file".to_string(),
                location: Some(SourceLocation {
                    path: file.path.clone(),
                    start_line: 1,
                    end_line: 1,
                    start_col: None,
                    end_col: None,
                    symbol_id: None,
                }),
            });
        }

        let dynamic_constructs = self.detect_dynamic_constructs(content, &file.path);
        let coverage = if file.is_generated {
            CoverageState::Partial
        } else if has_error {
            CoverageState::Unknown
        } else if !dynamic_constructs.is_empty() {
            CoverageState::Partial
        } else {
            CoverageState::Verified
        };

        Ok(ParsedFile {
            file: file.clone(),
            syntax_valid: !has_error,
            coverage,
            diagnostics,
            dynamic_constructs,
            raw_ast_summary: Some(format!("Java root with {} children", root.child_count())),
            source_content: Some(content.to_string()),
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> Result<Vec<Symbol>, AdapterError> {
        let content = parsed.source_content.as_deref().unwrap_or_default();
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.parse_tree(content)?;
        let root = tree.root_node();
        let file_path = &parsed.file.path;

        let mut package_name = String::new();
        let mut symbols = Vec::new();

        // 1. Scan for package declaration
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() == "package_declaration" {
                let mut c_walk = child.walk();
                for n in child.children(&mut c_walk) {
                    if n.kind().ends_with("_identifier") || n.kind() == "scoped_identifier" {
                        package_name = content[n.byte_range()].trim().to_string();
                        break;
                    }
                }
            }
        }

        // 2. Scan declarations
        fn walk_declarations(
            node: Node,
            content: &str,
            file_path: &str,
            package_name: &str,
            current_scope: &str,
            adapter: &JavaAdapter,
            symbols: &mut Vec<Symbol>,
        ) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "class_declaration"
                    | "interface_declaration"
                    | "record_declaration"
                    | "enum_declaration" => {
                        let name_node = child.child_by_field_name("name");
                        if let Some(name_node) = name_node {
                            let raw_name = content[name_node.byte_range()].trim();
                            let (vis, annotations) = JavaAdapter::detect_modifiers(child, content);
                            let kind = match child.kind() {
                                "interface_declaration" => SymbolKind::Interface,
                                "enum_declaration" => SymbolKind::Enum,
                                _ => SymbolKind::Class,
                            };

                            let scope = if current_scope.is_empty() {
                                raw_name.to_string()
                            } else {
                                format!("{}.{}", current_scope, raw_name)
                            };

                            let qname = qualified_name(file_path, package_name, &scope);
                            let stable_id =
                                stable_symbol_id(file_path, kind, current_scope, raw_name);

                            let ann_prefix = if annotations.is_empty() {
                                String::new()
                            } else {
                                format!("@{} ", annotations.join(" @"))
                            };
                            let sig = format!("{}{}: {}", ann_prefix, kind.tag(), raw_name);

                            symbols.push(Symbol {
                                stable_id,
                                language_id: LANGUAGE_ID.to_string(),
                                kind: kind.kind_str().to_string(),
                                qualified_name: qname,
                                range: Some(adapter.node_location(child, file_path)),
                                normalized_signature: Some(sig),
                                visibility: vis,
                            });

                            if let Some(body) = child.child_by_field_name("body") {
                                walk_declarations(
                                    body,
                                    content,
                                    file_path,
                                    package_name,
                                    &scope,
                                    adapter,
                                    symbols,
                                );
                            }
                        }
                    }
                    "method_declaration" | "constructor_declaration" => {
                        let is_constructor = child.kind() == "constructor_declaration";
                        let name_node = child.child_by_field_name("name");
                        if let Some(name_node) = name_node {
                            let raw_name = content[name_node.byte_range()].trim();
                            let (vis, annotations) = JavaAdapter::detect_modifiers(child, content);
                            let kind = SymbolKind::Method;

                            let mut param_types = Vec::new();
                            if let Some(params) = child.child_by_field_name("parameters") {
                                let mut p_cur = params.walk();
                                for p in params.children(&mut p_cur) {
                                    if p.kind() == "formal_parameter" {
                                        if let Some(t) = p.child_by_field_name("type") {
                                            param_types
                                                .push(content[t.byte_range()].trim().to_string());
                                        }
                                    }
                                }
                            }

                            let ret_type = child
                                .child_by_field_name("type")
                                .map(|t| content[t.byte_range()].trim().to_string())
                                .unwrap_or_else(|| {
                                    if is_constructor {
                                        current_scope.to_string()
                                    } else {
                                        "void".to_string()
                                    }
                                });

                            let stable_id =
                                stable_symbol_id(file_path, kind, current_scope, raw_name);
                            let qname = qualified_name(file_path, current_scope, raw_name);

                            let ann_prefix = if annotations.is_empty() {
                                String::new()
                            } else {
                                format!("@{} ", annotations.join(" @"))
                            };
                            let sig = format!(
                                "{}{}({}) -> {}",
                                ann_prefix,
                                raw_name,
                                param_types.join(", "),
                                ret_type
                            );

                            symbols.push(Symbol {
                                stable_id,
                                language_id: LANGUAGE_ID.to_string(),
                                kind: kind.kind_str().to_string(),
                                qualified_name: qname,
                                range: Some(adapter.node_location(child, file_path)),
                                normalized_signature: Some(sig),
                                visibility: vis,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        walk_declarations(
            root,
            content,
            file_path,
            &package_name,
            "",
            self,
            &mut symbols,
        );

        if symbols.is_empty() {
            let mod_id = SymbolId::new(format!("{}#package", file_path));
            symbols.push(Symbol {
                stable_id: mod_id,
                language_id: LANGUAGE_ID.to_string(),
                kind: SymbolKind::Class.kind_str().to_string(),
                qualified_name: qualified_name(
                    file_path,
                    "",
                    if package_name.is_empty() {
                        "default"
                    } else {
                        &package_name
                    },
                ),
                range: Some(self.node_location(root, file_path)),
                normalized_signature: Some("package".to_string()),
                visibility: Visibility::Public,
            });
        }

        Ok(symbols)
    }

    fn extract_edges(
        &self,
        parsed: &ParsedFile,
        symbol_index: &BTreeMap<String, SymbolId>,
    ) -> Result<Vec<Edge>, AdapterError> {
        let content = parsed.source_content.as_deref().unwrap_or_default();
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.parse_tree(content)?;
        let root = tree.root_node();
        let file_path = &parsed.file.path;
        let file_symbol = SymbolId::new(format!("file::{}", file_path));

        let mut edges = Vec::new();
        let extracted_symbols = self.extract_symbols(parsed)?;

        // 1. File owns every symbol declared in it (ast_contains)
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

        // 2. Imports
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() == "import_declaration" {
                let imp_text = content[child.byte_range()]
                    .trim()
                    .trim_start_matches("import ")
                    .trim_end_matches(';')
                    .trim();
                let target_id = SymbolId::new(format!("import::{}", imp_text));
                edges.push(Edge {
                    source: file_symbol.clone(),
                    target: target_id,
                    kind: EdgeKind::Imports,
                    coverage: parsed.coverage,
                    location: Some(self.node_location(child, file_path)),
                    provenance: "import_declaration".to_string(),
                });
            }
        }

        // 3. Inheritance (extends / implements) & Call invocations
        let mut class_stack: Vec<SymbolId> = Vec::new();

        fn walk_edges(
            node: Node,
            content: &str,
            file_path: &str,
            class_stack: &mut Vec<SymbolId>,
            symbol_index: &BTreeMap<String, SymbolId>,
            adapter: &JavaAdapter,
            coverage: CoverageState,
            edges: &mut Vec<Edge>,
        ) {
            match node.kind() {
                "class_declaration" | "interface_declaration" | "record_declaration" => {
                    let name_node = node.child_by_field_name("name");
                    if let Some(n) = name_node {
                        let name = content[n.byte_range()].trim();
                        let class_id = SymbolId::new(format!("{}#class:{}", file_path, name));
                        class_stack.push(class_id.clone());

                        if let Some(super_node) = node.child_by_field_name("superclass") {
                            let super_name = content[super_node.byte_range()]
                                .trim()
                                .trim_start_matches("extends ")
                                .trim();
                            edges.push(Edge {
                                source: class_id.clone(),
                                target: SymbolId::new(format!("type::{}", super_name)),
                                kind: EdgeKind::Inherits,
                                coverage,
                                location: Some(adapter.node_location(super_node, file_path)),
                                provenance: "class_extends".to_string(),
                            });
                        }
                        if let Some(ifaces) = node.child_by_field_name("interfaces") {
                            let ifaces_text = content[ifaces.byte_range()]
                                .trim()
                                .trim_start_matches("implements ")
                                .trim();
                            for iface in ifaces_text.split(',') {
                                let iface_name = iface.trim();
                                if !iface_name.is_empty() {
                                    edges.push(Edge {
                                        source: class_id.clone(),
                                        target: SymbolId::new(format!("type::{}", iface_name)),
                                        kind: EdgeKind::Inherits,
                                        coverage,
                                        location: Some(adapter.node_location(ifaces, file_path)),
                                        provenance: "class_implements".to_string(),
                                    });
                                }
                            }
                        }

                        let mut c = node.walk();
                        for ch in node.children(&mut c) {
                            walk_edges(
                                ch,
                                content,
                                file_path,
                                class_stack,
                                symbol_index,
                                adapter,
                                coverage,
                                edges,
                            );
                        }
                        class_stack.pop();
                        return;
                    }
                }
                "method_declaration" | "constructor_declaration" => {
                    let name_node = node.child_by_field_name("name");
                    if let Some(n) = name_node {
                        let name = content[n.byte_range()].trim();
                        let method_id = SymbolId::new(format!("{}#method:{}", file_path, name));

                        let mut c = node.walk();
                        for ch in node.children(&mut c) {
                            walk_calls(
                                ch,
                                content,
                                &method_id,
                                file_path,
                                symbol_index,
                                adapter,
                                coverage,
                                edges,
                            );
                        }
                        return;
                    }
                }
                _ => {}
            }

            let mut c = node.walk();
            for ch in node.children(&mut c) {
                walk_edges(
                    ch,
                    content,
                    file_path,
                    class_stack,
                    symbol_index,
                    adapter,
                    coverage,
                    edges,
                );
            }
        }

        fn walk_calls(
            node: Node,
            content: &str,
            caller_id: &SymbolId,
            file_path: &str,
            symbol_index: &BTreeMap<String, SymbolId>,
            adapter: &JavaAdapter,
            coverage: CoverageState,
            edges: &mut Vec<Edge>,
        ) {
            if node.kind() == "method_invocation" {
                let name_node = node.child_by_field_name("name");
                if let Some(n) = name_node {
                    let callee = content[n.byte_range()].trim();
                    let target_id = symbol_index
                        .iter()
                        .find(|(k, _)| {
                            k.ends_with(&format!("::{}", callee))
                                || k.ends_with(&format!(".{}", callee))
                        })
                        .map(|(_, id)| id.clone())
                        .unwrap_or_else(|| SymbolId::new(format!("symbol::{}", callee)));

                    edges.push(Edge {
                        source: caller_id.clone(),
                        target: target_id,
                        kind: EdgeKind::Calls,
                        coverage,
                        location: Some(adapter.node_location(node, file_path)),
                        provenance: "method_invocation".to_string(),
                    });
                }
            } else if node.kind() == "object_creation_expression" {
                let type_node = node.child_by_field_name("type");
                if let Some(t) = type_node {
                    let type_name = content[t.byte_range()].trim();
                    edges.push(Edge {
                        source: caller_id.clone(),
                        target: SymbolId::new(format!("type::{}", type_name)),
                        kind: EdgeKind::Calls,
                        coverage,
                        location: Some(adapter.node_location(node, file_path)),
                        provenance: "constructor_call".to_string(),
                    });
                }
            }

            let mut c = node.walk();
            for ch in node.children(&mut c) {
                walk_calls(
                    ch,
                    content,
                    caller_id,
                    file_path,
                    symbol_index,
                    adapter,
                    coverage,
                    edges,
                );
            }
        }

        walk_edges(
            root,
            content,
            file_path,
            &mut class_stack,
            symbol_index,
            self,
            parsed.coverage,
            &mut edges,
        );

        Ok(edges)
    }

    fn extract_contracts(&self, parsed: &ParsedFile) -> Result<Vec<Contract>, AdapterError> {
        let content = parsed.source_content.as_deref().unwrap_or_default();
        if content.is_empty() {
            return Ok(Vec::new());
        }
        let tree = self.parse_tree(content)?;
        let root = tree.root_node();
        let file_path = &parsed.file.path;

        let mut contracts = Vec::new();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() == "class_declaration" || child.kind() == "interface_declaration" {
                if let Some(body) = child.child_by_field_name("body") {
                    let mut b_cursor = body.walk();
                    for m in body.children(&mut b_cursor) {
                        if m.kind() == "method_declaration" {
                            if let Some(name_node) = m.child_by_field_name("name") {
                                let name = content[name_node.byte_range()].trim();
                                let (vis, _) = JavaAdapter::detect_modifiers(m, content);
                                let ret_type = m
                                    .child_by_field_name("type")
                                    .map(|t| content[t.byte_range()].trim().to_string());

                                let mut param_types = Vec::new();
                                if let Some(params) = m.child_by_field_name("parameters") {
                                    let mut p_cur = params.walk();
                                    for p in params.children(&mut p_cur) {
                                        if p.kind() == "formal_parameter" {
                                            if let Some(t) = p.child_by_field_name("type") {
                                                param_types.push(
                                                    content[t.byte_range()].trim().to_string(),
                                                );
                                            }
                                        }
                                    }
                                }

                                contracts.push(Contract {
                                    owner: SymbolId::new(format!("{}#method:{}", file_path, name)),
                                    visibility: vis,
                                    inputs: param_types,
                                    output: ret_type,
                                    effects: Vec::new(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(contracts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_java_adapter_conformance() {
        let adapter = JavaAdapter::new();
        let valid_source = r#"
package com.example.service;

import java.util.List;

public class UserService implements IUserService {
    private final String role;

    public UserService(String role) {
        this.role = role;
    }

    public boolean canExecute(String action) {
        return role != null && !role.isEmpty();
    }
}
"#;
        let invalid_source = r#"
package com.example.service;
public class BadSyntax {
    public void broken( {
"#;
        let dynamic_source = r#"
package com.example.dynamic;

public class DynamicReflector {
    public Object load(String className) throws Exception {
        Class<?> clazz = Class.forName(className);
        return clazz.getMethod("execute").invoke(null);
    }
}
"#;
        autopsy_adapter_api::conformance::verify_adapter_conformance(
            &adapter,
            valid_source,
            invalid_source,
            dynamic_source,
            "UserService.java",
        );
    }

    #[test]
    fn test_spring_di_annotations_extracted() {
        let adapter = JavaAdapter::new();
        let source = r#"
package com.example.controller;

@RestController
@RequestMapping("/api/users")
public class UserController {
    @Autowired
    private UserService userService;

    @GetMapping("/{id}")
    public User getUser(Long id) {
        return userService.findById(id);
    }
}
"#;
        let file = FileUnit {
            path: "UserController.java".to_string(),
            language: "java".to_string(),
            content_hash: "hash1".to_string(),
            is_generated: false,
        };
        let ctx = ParseContext::new(".");
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        assert_eq!(parsed.coverage, CoverageState::Verified);
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let class_sym = symbols
            .iter()
            .find(|s| s.qualified_name.contains("UserController"))
            .unwrap();
        assert!(
            class_sym
                .normalized_signature
                .as_ref()
                .unwrap()
                .contains("@RestController")
        );
    }
}
