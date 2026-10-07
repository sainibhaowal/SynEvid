//! Test harness and fixtures for Synevid end-to-end and integration tests.

use autopsy_repo::{AutopsyConfig, InvariantsConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create an isolated temporary test repository sandbox on disk.
pub struct TestSandbox {
    pub root: PathBuf,
}

impl TestSandbox {
    pub fn new(prefix: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "synevid_e2e_{}_{}_{}",
            prefix,
            std::process::id(),
            id
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("Failed to create test sandbox root directory");
        Self { root: path }
    }

    pub fn write_file(
        &self,
        relative_path: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
    ) -> PathBuf {
        let dest = self.root.join(relative_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }
        fs::write(&dest, content).expect("Failed to write sandbox test file");
        dest
    }

    pub fn write_autopsy_toml(&self, config_str: &str) -> PathBuf {
        self.write_file("autopsy.toml", config_str)
    }

    pub fn write_invariants_yml(&self, config_str: &str) -> PathBuf {
        self.write_file(".autopsy/invariants.yml", config_str)
    }

    pub fn default_config(&self) -> AutopsyConfig {
        AutopsyConfig {
            config_version: "0.0.1".to_string(),
            repository: autopsy_repo::RepositoryConfig {
                roots: vec![".".to_string()],
                exclude: vec![
                    "target/**".to_string(),
                    "dist/**".to_string(),
                    "node_modules/**".to_string(),
                ],
            },
            analysis: autopsy_repo::AnalysisConfig {
                default_profile: "pr".to_string(),
                max_traversal_nodes: 50_000,
            },
            cache: autopsy_repo::CacheConfig {
                directory: ".autopsy/cache".to_string(),
            },
        }
    }

    pub fn default_invariants(&self) -> InvariantsConfig {
        InvariantsConfig {
            version: "0.0.1".to_string(),
            invariants: vec![autopsy_repo::InvariantDef {
                id: "NO_CORE_TO_APP_DEP".to_string(),
                description: "Rust core must not depend on GUI or MCP apps".to_string(),
                kind: "forbidden_dependency".to_string(),
                severity: "error".to_string(),
                scope: Some(autopsy_repo::InvariantScope {
                    source: Some("crates/**".to_string()),
                    target: Some("apps/**".to_string()),
                }),
            }],
        }
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
