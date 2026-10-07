//! autopsy-adapter-api: Formal language analyzer contract and conformance harness.
//!
//! Requirements:
//! - FR-003: Core exposes versioned adapter interface for parse/symbol/reference/contract.
//! - FR-004: First adapter shall parse supported files and report unsupported/invalid explicitly.
//! - FR-005: Stable symbol IDs resilient to line movement where semantics permit.
//! - FR-009: Contract extraction: emits normalized callable/interface contracts.
//! - FR-030: Coverage state: distinguishes verified/unsupported/unknown; never collapses to Pass.

use autopsy_domain::{
    Contract, CoverageState, Edge, FileUnit, Severity, SourceLocation, Symbol, SymbolId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Capabilities declared by a language adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    pub language_id: String,
    pub adapter_version: String,
    pub supported_extensions: Vec<String>,
    pub supports_ast_parsing: bool,
    pub supports_symbol_extraction: bool,
    pub supports_edge_extraction: bool,
    pub supports_contract_extraction: bool,
    pub supports_type_resolution: bool,
    pub unsupported_constructs: Vec<String>,
}

/// Context supplied to an adapter during parsing.
#[derive(Debug, Clone, Default)]
pub struct ParseContext {
    pub project_root: PathBuf,
    pub options: BTreeMap<String, String>,
}

impl ParseContext {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            options: BTreeMap::new(),
        }
    }
}

/// Diagnostic message produced during source parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub location: Option<SourceLocation>,
}

/// Category of dynamic or unsupported construct discovered during analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DynamicConstructKind {
    Eval,
    DynamicImport,
    ReflectionOrProxy,
    GeneratedCode,
    DynamicPropertyAccess,
    UnknownDynamic,
}

/// Dynamic or unsupported construct discovered in source code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicConstruct {
    pub kind: DynamicConstructKind,
    pub description: String,
    pub location: Option<SourceLocation>,
}

/// Intermediate representation emitted by `LanguageAdapter::parse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedFile {
    pub file: FileUnit,
    pub syntax_valid: bool,
    pub coverage: CoverageState,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub dynamic_constructs: Vec<DynamicConstruct>,
    pub raw_ast_summary: Option<String>,
    pub source_content: Option<String>,
}

/// Errors returned by language adapters.
#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("I/O error during analysis: {0}")]
    Io(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("Unsupported file '{path}': {reason}")]
    UnsupportedFile { path: String, reason: String },

    #[error("Extraction error: {0}")]
    ExtractionError(String),

    #[error("Internal adapter error: {0}")]
    Internal(String),
}

/// Core interface for language-specific analyzers.
///
/// Adheres strictly to FR-003, FR-004, FR-005, FR-009, and FR-030.
pub trait LanguageAdapter: Send + Sync {
    /// Canonical language identifier (e.g., "typescript").
    fn language_id(&self) -> &str;

    /// Semantic version of the adapter implementation.
    fn version(&self) -> &str;

    /// Complete capabilities declaration.
    fn capabilities(&self) -> &AdapterCapabilities;

    /// Returns true if this adapter handles the target file.
    fn discover(&self, path: &Path) -> bool;

    /// Parses file content and emits an AST representation, coverage state, and diagnostics.
    fn parse(
        &self,
        context: &ParseContext,
        file: &FileUnit,
        content: &str,
    ) -> Result<ParsedFile, AdapterError>;

    /// Extracts normalized symbols with stable identities.
    fn extract_symbols(&self, parsed: &ParsedFile) -> Result<Vec<Symbol>, AdapterError>;

    /// Extracts directed multigraph edges (Contains, Imports, Calls, References, etc.).
    fn extract_edges(
        &self,
        parsed: &ParsedFile,
        symbol_index: &BTreeMap<String, SymbolId>,
    ) -> Result<Vec<Edge>, AdapterError>;

    /// Extracts normalized interface and callable contracts.
    fn extract_contracts(&self, parsed: &ParsedFile) -> Result<Vec<Contract>, AdapterError>;
}

/// Conformance testing utilities ensuring all adapter implementations satisfy
/// core architectural boundaries and invariant contracts.
pub mod conformance {
    use super::*;

    /// Verifies that an adapter implementation passes standard conformance requirements.
    pub fn verify_adapter_conformance<A: LanguageAdapter>(
        adapter: &A,
        valid_source: &str,
        invalid_source: &str,
        dynamic_source: &str,
        valid_filename: &str,
    ) {
        // 1. Identification & Versioning
        assert!(
            !adapter.language_id().is_empty(),
            "Language ID must not be empty"
        );
        assert!(!adapter.version().is_empty(), "Version must not be empty");

        // 2. Honest Capabilities
        let caps = adapter.capabilities();
        assert_eq!(caps.language_id, adapter.language_id());
        assert!(
            !caps.supported_extensions.is_empty(),
            "Must declare supported extensions"
        );
        assert!(
            caps.supports_ast_parsing,
            "Adapter must support AST parsing"
        );
        assert!(
            caps.supports_symbol_extraction,
            "Adapter must support symbol extraction"
        );
        assert!(
            !caps.unsupported_constructs.is_empty(),
            "Must honestly declare unsupported constructs"
        );

        // 3. Discovery
        let valid_path = Path::new(valid_filename);
        assert!(
            adapter.discover(valid_path),
            "Adapter must discover valid file extension"
        );
        let invalid_ext_path = Path::new("test.unsupported_unknown_xyz");
        assert!(
            !adapter.discover(invalid_ext_path),
            "Adapter must not discover arbitrary unsupported extension"
        );

        let ctx = ParseContext::new(".");

        // 4. Parse Valid Source
        let valid_file = FileUnit {
            path: valid_filename.to_string(),
            language: adapter.language_id().to_string(),
            content_hash: blake3::hash(valid_source.as_bytes()).to_hex().to_string(),
            is_generated: false,
        };
        let parsed_valid = adapter
            .parse(&ctx, &valid_file, valid_source)
            .expect("Valid source must parse without adapter error");
        assert!(
            parsed_valid.syntax_valid,
            "Valid source must have syntax_valid == true"
        );
        assert_eq!(
            parsed_valid.coverage,
            CoverageState::Verified,
            "Valid static source must be CoverageState::Verified"
        );

        // 5. Symbol Extraction on Valid Source
        let symbols = adapter
            .extract_symbols(&parsed_valid)
            .expect("extract_symbols must succeed on valid parsed file");
        assert!(
            !symbols.is_empty(),
            "Valid source must extract at least one symbol"
        );
        for sym in &symbols {
            assert!(
                !sym.stable_id.as_str().is_empty(),
                "SymbolId must not be empty"
            );
            assert_eq!(sym.language_id, adapter.language_id());
        }

        // 6. Contract Extraction
        let _contracts = adapter
            .extract_contracts(&parsed_valid)
            .expect("extract_contracts must succeed");
        // Contracts extraction executed without panic/error

        // 7. Edge Extraction
        let mut symbol_index = BTreeMap::new();
        for s in &symbols {
            symbol_index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }
        let edges = adapter
            .extract_edges(&parsed_valid, &symbol_index)
            .expect("extract_edges must succeed");
        assert!(
            !edges.is_empty(),
            "Valid source must emit at least one edge (e.g. contains/imports)"
        );

        // 8. Parse Invalid Source (Syntax Error)
        let invalid_file = FileUnit {
            path: valid_filename.to_string(),
            language: adapter.language_id().to_string(),
            content_hash: blake3::hash(invalid_source.as_bytes()).to_hex().to_string(),
            is_generated: false,
        };
        let parsed_invalid = adapter
            .parse(&ctx, &invalid_file, invalid_source)
            .expect("Parser should return ParsedFile with syntax_valid=false rather than crashing");
        assert!(
            !parsed_invalid.syntax_valid,
            "Malformed source must have syntax_valid == false"
        );
        assert!(
            !parsed_invalid.diagnostics.is_empty(),
            "Malformed source must emit diagnostics"
        );

        // 9. Parse Dynamic / Unsupported Constructs (FR-014, FR-030)
        let dynamic_file = FileUnit {
            path: valid_filename.to_string(),
            language: adapter.language_id().to_string(),
            content_hash: blake3::hash(dynamic_source.as_bytes()).to_hex().to_string(),
            is_generated: false,
        };
        let parsed_dynamic = adapter
            .parse(&ctx, &dynamic_file, dynamic_source)
            .expect("Dynamic source must parse without adapter error");
        assert_ne!(
            parsed_dynamic.coverage,
            CoverageState::Verified,
            "CRITICAL INVARIANT (FR-030): Dynamic or unsupported constructs must NEVER emit CoverageState::Verified"
        );
        assert!(
            parsed_dynamic.coverage == CoverageState::Unknown
                || parsed_dynamic.coverage == CoverageState::Partial,
            "Dynamic constructs must emit CoverageState::Unknown or Partial"
        );
        assert!(
            !parsed_dynamic.dynamic_constructs.is_empty(),
            "Dynamic constructs must be explicitly recorded in parsed.dynamic_constructs"
        );

        // 10. Generated Code Handling (FR-030)
        let generated_file = FileUnit {
            path: valid_filename.to_string(),
            language: adapter.language_id().to_string(),
            content_hash: blake3::hash(valid_source.as_bytes()).to_hex().to_string(),
            is_generated: true,
        };
        let parsed_generated = adapter
            .parse(&ctx, &generated_file, valid_source)
            .expect("Generated file must parse");
        assert_ne!(
            parsed_generated.coverage,
            CoverageState::Verified,
            "CRITICAL INVARIANT (FR-030): Generated code files must NEVER emit CoverageState::Verified"
        );
    }
}
