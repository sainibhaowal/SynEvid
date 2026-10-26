//! autopsy-adapter-csharp: Production-grade C# language adapter.
//!
//! Deterministic AST analysis via `tree-sitter-c-sharp`, ASP.NET Core & DI
//! attribute awareness (`[ApiController]`, `[Inject]`, `[Route]`, `[HttpGet]`,
//! `[HttpPost]`, `[Service]`), namespace and class hierarchies, properties,
//! method invocations, and honest reflection/dynamic escape detection
//! (FR-014, FR-030).
//!
//! Dynamic constructs (`dynamic`, `Type.GetType`, `MethodInfo.Invoke`,
//! `Activator.CreateInstance`, `[DllImport]`, `unsafe`) degrade to `CoverageState::Partial`
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
pub const LANGUAGE_ID: &str = "csharp";

/// C# namespace and project resolver bridge.
#[derive(Debug, Clone, Default)]
pub struct CSharpModuleBridge {
    pub root_dir: PathBuf,
    pub monorepo: Option<autopsy_workspace_registry::MonorepoWorkspace>,
}

impl CSharpModuleBridge {
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

    /// Resolves a type or namespace to candidate `.cs` files in the repository.
    pub fn resolve_type(&self, type_fqn: &str) -> Option<String> {
        let rel_subpath = type_fqn.replace('.', "/");
        let candidate_filenames = [
            format!("{}.cs", rel_subpath),
            format!("src/{}.cs", rel_subpath),
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

/// Production C# language adapter.
#[derive(Debug, Clone)]
pub struct CSharpAdapter {
    capabilities: AdapterCapabilities,
    module_bridge: CSharpModuleBridge,
}

impl Default for CSharpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CSharpAdapter {
    pub fn new() -> Self {
        let capabilities = AdapterCapabilities {
            language_id: LANGUAGE_ID.to_string(),
            adapter_version: ADAPTER_VERSION.to_string(),
            supported_extensions: vec!["cs".to_string()],
            supports_ast_parsing: true,
            supports_symbol_extraction: true,
            supports_edge_extraction: true,
            supports_contract_extraction: true,
            supports_type_resolution: true,
            unsupported_constructs: vec![
                "reflection".to_string(),
                "dynamic_keyword".to_string(),
                "unsafe_code".to_string(),
                "reflection_emit".to_string(),
                "pinvoke".to_string(),
                "generated_code".to_string(),
            ],
        };
        Self {
            capabilities,
            module_bridge: CSharpModuleBridge::new("."),
        }
    }

    pub fn with_root_dir(root_dir: impl Into<PathBuf>) -> Self {
        let mut adapter = Self::new();
        adapter.module_bridge = CSharpModuleBridge::new(root_dir);
        adapter
    }

    fn init_parser(&self) -> Result<Parser, AdapterError> {
        let mut parser = Parser::new();
        let language = tree_sitter_c_sharp::LANGUAGE.into();
        parser.set_language(&language).map_err(|e| {
            AdapterError::Internal(format!("Failed to configure tree-sitter C#: {:?}", e))
        })?;
        Ok(parser)
    }

    fn parse_tree(&self, content: &str) -> Result<Tree, AdapterError> {
        let mut parser = self.init_parser()?;
        parser.parse(content, None).ok_or_else(|| {
            AdapterError::ParseFailed("Tree-sitter C# returned no syntax tree".to_string())
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
        let mut vis = Visibility::Internal; // C# default class visibility is internal
        let mut attributes = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "modifier" => {
                    let text = &content[child.byte_range()];
                    match text {
                        "public" => vis = Visibility::Public,
                        "protected" => vis = Visibility::Protected,
                        "private" => vis = Visibility::Private,
                        "internal" => vis = Visibility::Internal,
                        _ => {}
                    }
                }
                "attribute_list" => {
                    let mut attr_cursor = child.walk();
                    for attr_child in child.children(&mut attr_cursor) {
                        if attr_child.kind() == "attribute" {
                            let text = &content[attr_child.byte_range()];
                            let attr_name = attr_child
                                .child_by_field_name("name")
                                .map(|n| &content[n.byte_range()])
                                .unwrap_or(text);
                            attributes.push(attr_name.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        (vis, attributes)
    }

    fn detect_dynamic_constructs(&self, content: &str, file_path: &str) -> Vec<DynamicConstruct> {
        let mut constructs = Vec::new();
        let reflection_patterns = [
            ("Type.GetType", "Dynamic type loading via Type.GetType"),
            ("GetMethod(", "Reflection method lookup via GetMethod"),
            (
                "MethodInfo.Invoke",
                "Reflective invocation via MethodInfo.Invoke",
            ),
            (
                "Activator.CreateInstance",
                "Dynamic activation via CreateInstance",
            ),
            ("dynamic ", "Dynamic typing via dynamic keyword"),
            ("unsafe ", "Unsafe pointer arithmetic block"),
            (
                "[DllImport",
                "Platform Invoke native interop via [DllImport]",
            ),
            ("System.Reflection.Emit", "Runtime bytecode emission"),
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

impl LanguageAdapter for CSharpAdapter {
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
            .map(|ext| ext.eq_ignore_ascii_case("cs"))
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
                message: "C# syntax error detected in file".to_string(),
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
            raw_ast_summary: Some(format!("C# root with {} children", root.child_count())),
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

        let mut symbols = Vec::new();

        fn walk_declarations(
            node: Node,
            content: &str,
            file_path: &str,
            current_namespace: &str,
            current_scope: &str,
            adapter: &CSharpAdapter,
            symbols: &mut Vec<Symbol>,
        ) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "namespace_declaration" | "file_scoped_namespace_declaration" => {
                        let ns_name = child
                            .child_by_field_name("name")
                            .map(|n| content[n.byte_range()].trim().to_string())
                            .unwrap_or_default();
                        let next_ns = if current_namespace.is_empty() {
                            ns_name
                        } else {
                            format!("{}.{}", current_namespace, ns_name)
                        };

                        if let Some(body) = child.child_by_field_name("body") {
                            walk_declarations(
                                body,
                                content,
                                file_path,
                                &next_ns,
                                current_scope,
                                adapter,
                                symbols,
                            );
                        } else {
                            // File-scoped namespace affects remaining siblings
                            walk_declarations(
                                child,
                                content,
                                file_path,
                                &next_ns,
                                current_scope,
                                adapter,
                                symbols,
                            );
                        }
                    }
                    "class_declaration"
                    | "interface_declaration"
                    | "struct_declaration"
                    | "record_declaration"
                    | "record_struct_declaration"
                    | "enum_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let raw_name = content[name_node.byte_range()].trim();
                            let (vis, attributes) = CSharpAdapter::detect_modifiers(child, content);
                            let kind = match child.kind() {
                                "interface_declaration" => SymbolKind::Interface,
                                "enum_declaration" => SymbolKind::Enum,
                                "struct_declaration" | "record_struct_declaration" => {
                                    SymbolKind::Class
                                }
                                _ => SymbolKind::Class,
                            };

                            let scope = if current_scope.is_empty() {
                                raw_name.to_string()
                            } else {
                                format!("{}.{}", current_scope, raw_name)
                            };

                            let qname = qualified_name(file_path, current_namespace, &scope);
                            let stable_id =
                                stable_symbol_id(file_path, kind, current_scope, raw_name);

                            let attr_prefix = if attributes.is_empty() {
                                String::new()
                            } else {
                                format!("[{}] ", attributes.join("]["))
                            };
                            let sig = format!("{}{}: {}", attr_prefix, kind.tag(), raw_name);

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
                                    current_namespace,
                                    &scope,
                                    adapter,
                                    symbols,
                                );
                            }
                        }
                    }
                    "method_declaration" | "constructor_declaration" => {
                        let is_constructor = child.kind() == "constructor_declaration";
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let raw_name = content[name_node.byte_range()].trim();
                            let (vis, attributes) = CSharpAdapter::detect_modifiers(child, content);
                            let kind = SymbolKind::Method;

                            let mut param_types = Vec::new();
                            if let Some(params) = child.child_by_field_name("parameters") {
                                let mut p_cur = params.walk();
                                for p in params.children(&mut p_cur) {
                                    if p.kind() == "parameter" {
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

                            let attr_prefix = if attributes.is_empty() {
                                String::new()
                            } else {
                                format!("[{}] ", attributes.join("]["))
                            };
                            let sig = format!(
                                "{}{}({}) -> {}",
                                attr_prefix,
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
                    "property_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let raw_name = content[name_node.byte_range()].trim();
                            let (vis, attributes) = CSharpAdapter::detect_modifiers(child, content);
                            let kind = SymbolKind::Variable;

                            let prop_type = child
                                .child_by_field_name("type")
                                .map(|t| content[t.byte_range()].trim().to_string())
                                .unwrap_or_else(|| "object".to_string());

                            let stable_id =
                                stable_symbol_id(file_path, kind, current_scope, raw_name);
                            let qname = qualified_name(file_path, current_scope, raw_name);

                            let attr_prefix = if attributes.is_empty() {
                                String::new()
                            } else {
                                format!("[{}] ", attributes.join("]["))
                            };
                            let sig = format!("{}prop {}: {}", attr_prefix, raw_name, prop_type);

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

        walk_declarations(root, content, file_path, "", "", self, &mut symbols);

        if symbols.is_empty() {
            let mod_id = SymbolId::new(format!("{}#root", file_path));
            symbols.push(Symbol {
                stable_id: mod_id,
                language_id: LANGUAGE_ID.to_string(),
                kind: SymbolKind::Class.kind_str().to_string(),
                qualified_name: qualified_name(file_path, "", "Program"),
                range: Some(self.node_location(root, file_path)),
                normalized_signature: Some("root".to_string()),
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

        // 2. Usings (Imports)
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() == "using_directive" {
                let using_text = content[child.byte_range()]
                    .trim()
                    .trim_start_matches("using ")
                    .trim_end_matches(';')
                    .trim();
                let target_id = SymbolId::new(format!("using::{}", using_text));
                edges.push(Edge {
                    source: file_symbol.clone(),
                    target: target_id,
                    kind: EdgeKind::Imports,
                    coverage: parsed.coverage,
                    location: Some(self.node_location(child, file_path)),
                    provenance: "using_directive".to_string(),
                });
            }
        }

        // 3. Inheritance (base_list) & Call invocations
        let mut class_stack: Vec<SymbolId> = Vec::new();

        fn walk_edges(
            node: Node,
            content: &str,
            file_path: &str,
            class_stack: &mut Vec<SymbolId>,
            symbol_index: &BTreeMap<String, SymbolId>,
            adapter: &CSharpAdapter,
            coverage: CoverageState,
            edges: &mut Vec<Edge>,
        ) {
            match node.kind() {
                "class_declaration"
                | "interface_declaration"
                | "struct_declaration"
                | "record_declaration" => {
                    if let Some(n) = node.child_by_field_name("name") {
                        let name = content[n.byte_range()].trim();
                        let class_id = SymbolId::new(format!("{}#class:{}", file_path, name));
                        class_stack.push(class_id.clone());

                        if let Some(base_list) = node.child_by_field_name("bases") {
                            let base_text = content[base_list.byte_range()]
                                .trim()
                                .trim_start_matches(':')
                                .trim();
                            for base_type in base_text.split(',') {
                                let bt = base_type.trim();
                                if !bt.is_empty() {
                                    edges.push(Edge {
                                        source: class_id.clone(),
                                        target: SymbolId::new(format!("type::{}", bt)),
                                        kind: EdgeKind::Inherits,
                                        coverage,
                                        location: Some(adapter.node_location(base_list, file_path)),
                                        provenance: "base_type_list".to_string(),
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
                    if let Some(n) = node.child_by_field_name("name") {
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
            adapter: &CSharpAdapter,
            coverage: CoverageState,
            edges: &mut Vec<Edge>,
        ) {
            if node.kind() == "invocation_expression" {
                let expr_node = node.child_by_field_name("expression").unwrap_or(node);
                let callee_text = content[expr_node.byte_range()].trim();
                let callee_name = callee_text.rsplit('.').next().unwrap_or(callee_text);

                let target_id = symbol_index
                    .iter()
                    .find(|(k, _)| {
                        k.ends_with(&format!("::{}", callee_name))
                            || k.ends_with(&format!(".{}", callee_name))
                    })
                    .map(|(_, id)| id.clone())
                    .unwrap_or_else(|| SymbolId::new(format!("symbol::{}", callee_name)));

                edges.push(Edge {
                    source: caller_id.clone(),
                    target: target_id,
                    kind: EdgeKind::Calls,
                    coverage,
                    location: Some(adapter.node_location(node, file_path)),
                    provenance: "invocation_expression".to_string(),
                });
            } else if node.kind() == "object_creation_expression" {
                if let Some(t) = node.child_by_field_name("type") {
                    let type_name = content[t.byte_range()].trim();
                    edges.push(Edge {
                        source: caller_id.clone(),
                        target: SymbolId::new(format!("type::{}", type_name)),
                        kind: EdgeKind::Calls,
                        coverage,
                        location: Some(adapter.node_location(node, file_path)),
                        provenance: "object_creation".to_string(),
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

        fn walk_contracts(
            node: Node,
            content: &str,
            file_path: &str,
            contracts: &mut Vec<Contract>,
        ) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "method_declaration" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = content[name_node.byte_range()].trim();
                        let (vis, _) = CSharpAdapter::detect_modifiers(child, content);
                        let ret_type = child
                            .child_by_field_name("type")
                            .map(|t| content[t.byte_range()].trim().to_string());

                        let mut param_types = Vec::new();
                        if let Some(params) = child.child_by_field_name("parameters") {
                            let mut p_cur = params.walk();
                            for p in params.children(&mut p_cur) {
                                if p.kind() == "parameter" {
                                    if let Some(t) = p.child_by_field_name("type") {
                                        param_types
                                            .push(content[t.byte_range()].trim().to_string());
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
                } else if child.kind() == "class_declaration"
                    || child.kind() == "interface_declaration"
                    || child.kind() == "namespace_declaration"
                {
                    walk_contracts(child, content, file_path, contracts);
                }
            }
        }

        walk_contracts(root, content, file_path, &mut contracts);

        Ok(contracts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csharp_adapter_conformance() {
        let adapter = CSharpAdapter::new();
        let valid_source = r#"
namespace Enterprise.Billing;

using System;
using System.Collections.Generic;

public class InvoiceService : IInvoiceService {
    private readonly string _tenantId;

    public InvoiceService(string tenantId) {
        _tenantId = tenantId;
    }

    public bool ProcessInvoice(string invoiceId, decimal amount) {
        return amount > 0;
    }
}
"#;
        let invalid_source = r#"
namespace Enterprise.Billing;
public class BrokenSyntax {
    public void Fail( {
"#;
        let dynamic_source = r#"
namespace Enterprise.Dynamic;

using System;
using System.Reflection;

public class DynamicRunner {
    public object Execute(string typeName) {
        Type t = Type.GetType(typeName);
        dynamic d = Activator.CreateInstance(t);
        return d.Compute();
    }
}
"#;
        autopsy_adapter_api::conformance::verify_adapter_conformance(
            &adapter,
            valid_source,
            invalid_source,
            dynamic_source,
            "InvoiceService.cs",
        );
    }

    #[test]
    fn test_aspnet_attributes_extracted() {
        let adapter = CSharpAdapter::new();
        let source = r#"
namespace Enterprise.Controllers;

[ApiController]
[Route("api/[controller]")]
public class OrderController {
    [Inject]
    private IOrderService _orderService;

    [HttpGet("{id}")]
    public Order GetOrder(int id) {
        return _orderService.GetOrder(id);
    }
}
"#;
        let file = FileUnit {
            path: "OrderController.cs".to_string(),
            language: "csharp".to_string(),
            content_hash: "hashcs1".to_string(),
            is_generated: false,
        };
        let ctx = ParseContext::new(".");
        let parsed = adapter.parse(&ctx, &file, source).unwrap();
        assert_eq!(parsed.coverage, CoverageState::Verified);
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let class_sym = symbols
            .iter()
            .find(|s| s.qualified_name.contains("OrderController"))
            .unwrap();
        assert!(
            class_sym
                .normalized_signature
                .as_ref()
                .unwrap()
                .contains("[ApiController]")
        );
    }
}
