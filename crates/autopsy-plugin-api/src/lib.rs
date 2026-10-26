//! autopsy-plugin-api: Pluggable Core Registry & Extension Interfaces (FR-P1-001, INV-PLUGIN-001..005).
//!
//! Provides zero-hardcoding trait abstractions for:
//! - Language Adapters (`LanguageAdapterPlugin`)
//! - Workspace / Monorepo Resolvers (`WorkspaceResolverPlugin`)
//! - Verification & Policy Gates (`VerificationGatePlugin`)
//! - 2D/3D Graph Layout Algorithms (`GraphLayoutPlugin`)
//! - Storage & Cache Backends (`StorageEnginePlugin`)
//!
//! Every plugin is dynamically discoverable, versioned, and hot-swappable at runtime.

use autopsy_domain::{CoverageState, RepoSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin execution error in '{0}': {1}")]
    Execution(String, String),
    #[error("Plugin registration collision for '{0}'")]
    DuplicateRegistration(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Metadata implemented by every pluggable component in SynEvid.
pub trait PluginDescriptor: Send + Sync {
    /// Stable identifier, e.g. "lang.typescript", "gate.blast_radius"
    fn plugin_id(&self) -> &'static str;
    /// Semantic version of the plugin contract
    fn version(&self) -> &'static str;
    /// Human-readable display name for UI Settings & Capabilities panel
    fn display_name(&self) -> &'static str;
    /// Short description of what this plugin performs
    fn description(&self) -> &'static str;
}

/// Summary of a registered plugin emitted dynamically by `/api/v1/capabilities`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCapabilityInfo {
    pub plugin_id: String,
    pub category: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub supported_extensions: Vec<String>,
}

/// Pluggable contract for Language Adapters (TS/JS, Python, Go, Rust, Java, C#, and future).
pub trait LanguageAdapterPlugin: PluginDescriptor {
    /// Recognized file extensions, e.g. `[".ts", ".tsx", ".js"]`
    fn file_extensions(&self) -> &'static [&'static str];
    /// Canonical language name, e.g. "TypeScript"
    fn language_name(&self) -> &'static str;
    /// Estimates semantic coverage state for given file content
    fn estimate_coverage(&self, source: &[u8]) -> CoverageState;
}

/// Pluggable contract for Monorepo & Workspace Resolvers (pnpm, Turbo, Cargo, Maven, etc.).
pub trait WorkspaceResolverPlugin: PluginDescriptor {
    /// Recognized manifest files, e.g. `["pnpm-workspace.yaml", "package.json"]`
    fn manifest_files(&self) -> &'static [&'static str];
    /// Ecosystem name, e.g. "pnpm", "Cargo", "Maven"
    fn ecosystem_name(&self) -> &'static str;
    /// Checks if this workspace type is active at the given root
    fn detect(&self, root: &Path) -> bool;
}

/// Pluggable contract for Verification & Policy Gates.
pub trait VerificationGatePlugin: PluginDescriptor {
    /// Unique gate name, e.g. "blast_radius", "dead_code", "contracts"
    fn gate_id(&self) -> &'static str;
    /// Evaluates candidate snapshot against baseline or policy
    fn evaluate(
        &self,
        baseline: Option<&RepoSnapshot>,
        candidate: &RepoSnapshot,
    ) -> Result<GateEvaluationResult, PluginError>;
}

/// Verdict emitted by a Verification Gate plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateEvaluationResult {
    pub gate_id: String,
    pub verdict: String, // PASS | WARN | FAIL
    pub violations_count: usize,
    pub summary: String,
    pub details_json: String,
}

/// 2D / 3D Coordinate for a graph node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCoordinates {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub cluster_id: u32,
}

/// Pluggable contract for 2D / 3D Graph Layout Algorithms.
pub trait GraphLayoutPlugin: PluginDescriptor {
    /// Layout algorithm name, e.g. "barnes_hut_3d", "force_atlas_2d"
    fn layout_id(&self) -> &'static str;
    /// Computes deterministic (x, y, z) coordinates seeded by BLAKE3 hash
    fn compute_layout(
        &self,
        nodes: &[String],
        edges: &[(String, String)],
        seed_blake3: &[u8; 32],
        is_3d: bool,
    ) -> Result<BTreeMap<String, NodeCoordinates>, PluginError>;
}

/// Central Thread-Safe Dynamic Plugin Registry (Zero Hardcoding Invariant).
#[derive(Default)]
pub struct PluginRegistry {
    languages: RwLock<BTreeMap<String, Arc<dyn LanguageAdapterPlugin>>>,
    workspaces: RwLock<BTreeMap<String, Arc<dyn WorkspaceResolverPlugin>>>,
    gates: RwLock<BTreeMap<String, Arc<dyn VerificationGatePlugin>>>,
    layouts: RwLock<BTreeMap<String, Arc<dyn GraphLayoutPlugin>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a Language Adapter plugin.
    pub fn register_language(&self, plugin: Arc<dyn LanguageAdapterPlugin>) {
        let mut map = self.languages.write().unwrap();
        map.insert(plugin.plugin_id().to_string(), plugin);
    }

    /// Registers a Workspace Resolver plugin.
    pub fn register_workspace(&self, plugin: Arc<dyn WorkspaceResolverPlugin>) {
        let mut map = self.workspaces.write().unwrap();
        map.insert(plugin.plugin_id().to_string(), plugin);
    }

    /// Registers a Verification Gate plugin.
    pub fn register_gate(&self, plugin: Arc<dyn VerificationGatePlugin>) {
        let mut map = self.gates.write().unwrap();
        map.insert(plugin.plugin_id().to_string(), plugin);
    }

    /// Registers a Graph Layout plugin.
    pub fn register_layout(&self, plugin: Arc<dyn GraphLayoutPlugin>) {
        let mut map = self.layouts.write().unwrap();
        map.insert(plugin.plugin_id().to_string(), plugin);
    }

    /// Finds a language adapter by file extension (e.g. ".ts", ".rs", ".java").
    pub fn find_language_for_extension(&self, ext: &str) -> Option<Arc<dyn LanguageAdapterPlugin>> {
        let map = self.languages.read().unwrap();
        for plugin in map.values() {
            if plugin.file_extensions().contains(&ext) {
                return Some(plugin.clone());
            }
        }
        None
    }

    /// Finds a verification gate by gate_id.
    pub fn get_gate(&self, gate_id: &str) -> Option<Arc<dyn VerificationGatePlugin>> {
        let map = self.gates.read().unwrap();
        map.get(gate_id).cloned()
    }

    /// Finds a layout plugin by layout_id.
    pub fn get_layout(&self, layout_id: &str) -> Option<Arc<dyn GraphLayoutPlugin>> {
        let map = self.layouts.read().unwrap();
        map.get(layout_id).cloned()
    }

    /// Emits a list of all registered capabilities dynamically (for `/api/v1/capabilities`).
    pub fn list_capabilities(&self) -> Vec<PluginCapabilityInfo> {
        let mut list = Vec::new();

        {
            let map = self.languages.read().unwrap();
            for plugin in map.values() {
                list.push(PluginCapabilityInfo {
                    plugin_id: plugin.plugin_id().to_string(),
                    category: "language_adapter".to_string(),
                    version: plugin.version().to_string(),
                    display_name: plugin.display_name().to_string(),
                    description: plugin.description().to_string(),
                    supported_extensions: plugin
                        .file_extensions()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                });
            }
        }

        {
            let map = self.workspaces.read().unwrap();
            for plugin in map.values() {
                list.push(PluginCapabilityInfo {
                    plugin_id: plugin.plugin_id().to_string(),
                    category: "workspace_resolver".to_string(),
                    version: plugin.version().to_string(),
                    display_name: plugin.display_name().to_string(),
                    description: plugin.description().to_string(),
                    supported_extensions: plugin
                        .manifest_files()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                });
            }
        }

        {
            let map = self.gates.read().unwrap();
            for plugin in map.values() {
                list.push(PluginCapabilityInfo {
                    plugin_id: plugin.plugin_id().to_string(),
                    category: "verification_gate".to_string(),
                    version: plugin.version().to_string(),
                    display_name: plugin.display_name().to_string(),
                    description: plugin.description().to_string(),
                    supported_extensions: Vec::new(),
                });
            }
        }

        {
            let map = self.layouts.read().unwrap();
            for plugin in map.values() {
                list.push(PluginCapabilityInfo {
                    plugin_id: plugin.plugin_id().to_string(),
                    category: "graph_layout".to_string(),
                    version: plugin.version().to_string(),
                    display_name: plugin.display_name().to_string(),
                    description: plugin.description().to_string(),
                    supported_extensions: Vec::new(),
                });
            }
        }

        list.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLangPlugin;
    impl PluginDescriptor for MockLangPlugin {
        fn plugin_id(&self) -> &'static str {
            "lang.mock_kotlin"
        }
        fn version(&self) -> &'static str {
            "0.1.0"
        }
        fn display_name(&self) -> &'static str {
            "Mock Kotlin Adapter"
        }
        fn description(&self) -> &'static str {
            "Test adapter for Kotlin files"
        }
    }
    impl LanguageAdapterPlugin for MockLangPlugin {
        fn file_extensions(&self) -> &'static [&'static str] {
            &[".kt", ".kts"]
        }
        fn language_name(&self) -> &'static str {
            "Kotlin"
        }
        fn estimate_coverage(&self, _source: &[u8]) -> CoverageState {
            CoverageState::Verified
        }
    }

    #[test]
    fn test_dynamic_plugin_registration_and_isolation() {
        let registry = PluginRegistry::new();
        assert!(registry.find_language_for_extension(".kt").is_none());

        registry.register_language(Arc::new(MockLangPlugin));

        let found = registry
            .find_language_for_extension(".kt")
            .expect("Should find registered mock plugin");
        assert_eq!(found.language_name(), "Kotlin");

        let caps = registry.list_capabilities();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].plugin_id, "lang.mock_kotlin");
        assert_eq!(caps[0].category, "language_adapter");
    }
}
