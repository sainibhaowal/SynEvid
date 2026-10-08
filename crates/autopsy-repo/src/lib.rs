//! autopsy-repo: Repository discovery, ignore rules, and deterministic snapshot generation.
//!
//! Hard boundary: Snapshot IDs must be derived strictly from content digests, configuration hash,
//! and analyzer version. Timestamps are excluded to guarantee 100/100 deterministic repeatability.

use autopsy_domain::{FileUnit, RepoSnapshot, SnapshotId};
use glob::Pattern;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Error types for repository and configuration operations.
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("Configuration file not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("Failed to parse configuration at {path}: {reason}")]
    ConfigParseError { path: PathBuf, reason: String },

    #[error("Invalid configuration at {path}: {reason}")]
    InvalidConfig { path: PathBuf, reason: String },

    #[error("Invalid glob pattern '{pattern}' in config: {reason}")]
    InvalidGlobPattern { pattern: String, reason: String },

    #[error("I/O error during repository scanning: {0}")]
    Io(#[from] std::io::Error),
}

/// Root configuration schema corresponding to `autopsy.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopsyConfig {
    pub config_version: String,
    pub repository: RepositoryConfig,
    pub analysis: AnalysisConfig,
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub roots: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub default_profile: String,
    pub max_traversal_nodes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheConfig {
    pub directory: String,
}

impl Default for AutopsyConfig {
    fn default() -> Self {
        Self {
            config_version: "0.0.1".to_string(),
            repository: RepositoryConfig::default(),
            analysis: AnalysisConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

impl Default for RepositoryConfig {
    fn default() -> Self {
        Self {
            roots: vec![".".to_string()],
            exclude: vec!["target/**".to_string(), "node_modules/**".to_string()],
        }
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            default_profile: "pr".to_string(),
            max_traversal_nodes: 50_000,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            directory: ".autopsy/cache".to_string(),
        }
    }
}

/// Invariant configuration schema corresponding to `.autopsy/invariants.yml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantsConfig {
    pub version: String,
    pub invariants: Vec<InvariantDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantScope {
    pub source: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantDef {
    pub id: String,
    pub description: String,
    pub kind: String,
    pub severity: String,
    #[serde(default)]
    pub scope: Option<InvariantScope>,
}

impl AutopsyConfig {
    /// Load and validate `autopsy.toml`.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, RepoError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(RepoError::ConfigNotFound(path.to_path_buf()));
        }

        let content = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content).map_err(|e| RepoError::ConfigParseError {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

        config.validate(path)?;
        Ok(config)
    }

    /// Validate configuration constraints before analysis.
    pub fn validate(&self, path: &Path) -> Result<(), RepoError> {
        if self.config_version.is_empty() {
            return Err(RepoError::InvalidConfig {
                path: path.to_path_buf(),
                reason: "'config_version' cannot be empty".to_string(),
            });
        }
        if self.repository.roots.is_empty() {
            return Err(RepoError::InvalidConfig {
                path: path.to_path_buf(),
                reason: "'repository.roots' must contain at least one root directory".to_string(),
            });
        }
        if self.analysis.max_traversal_nodes == 0 {
            return Err(RepoError::InvalidConfig {
                path: path.to_path_buf(),
                reason: "'analysis.max_traversal_nodes' must be greater than zero".to_string(),
            });
        }
        for pattern in &self.repository.exclude {
            Pattern::new(pattern).map_err(|e| RepoError::InvalidGlobPattern {
                pattern: pattern.clone(),
                reason: e.to_string(),
            })?;
        }
        Ok(())
    }

    /// Compute deterministic BLAKE3 hash of normalized configuration.
    pub fn content_hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.config_version.as_bytes());
        for root in &self.repository.roots {
            hasher.update(root.as_bytes());
        }
        for exc in &self.repository.exclude {
            hasher.update(exc.as_bytes());
        }
        hasher.update(self.analysis.default_profile.as_bytes());
        hasher.update(&self.analysis.max_traversal_nodes.to_be_bytes());
        hasher.update(self.cache.directory.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

impl InvariantsConfig {
    /// Load and validate `.autopsy/invariants.yml`.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, RepoError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(RepoError::ConfigNotFound(path.to_path_buf()));
        }

        let content = fs::read_to_string(path)?;
        let config: Self =
            serde_yaml::from_str(&content).map_err(|e| RepoError::ConfigParseError {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?;

        config.validate(path)?;
        Ok(config)
    }

    /// Validate invariants before analysis.
    pub fn validate(&self, path: &Path) -> Result<(), RepoError> {
        for inv in &self.invariants {
            if inv.id.is_empty() {
                return Err(RepoError::InvalidConfig {
                    path: path.to_path_buf(),
                    reason: "Invariant id cannot be empty".to_string(),
                });
            }
            if !inv
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(RepoError::InvalidConfig {
                    path: path.to_path_buf(),
                    reason: format!("Invariant id '{}' contains invalid characters", inv.id),
                });
            }
            match inv.severity.as_str() {
                "info" | "warning" | "error" => {}
                other => {
                    return Err(RepoError::InvalidConfig {
                        path: path.to_path_buf(),
                        reason: format!("Invariant '{}' has unknown severity '{}'", inv.id, other),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Helper function to classify language from file extension.
pub fn classify_language(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("ts") | Some("tsx") | Some("mts") | Some("cts") => "typescript".to_string(),
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => "javascript".to_string(),
        Some("rs") => "rust".to_string(),
        Some("py") => "python".to_string(),
        Some("json") => "json".to_string(),
        Some("toml") => "toml".to_string(),
        Some("yaml") | Some("yml") => "yaml".to_string(),
        Some("md") => "markdown".to_string(),
        Some("html") => "html".to_string(),
        Some("css") => "css".to_string(),
        Some(other) => other.to_ascii_lowercase(),
        None => "unknown".to_string(),
    }
}

/// Checks if a recognized language is text-based source code subject to line-ending normalization.
pub fn is_text_language(lang: &str) -> bool {
    matches!(
        lang,
        "typescript"
            | "javascript"
            | "rust"
            | "python"
            | "json"
            | "toml"
            | "yaml"
            | "markdown"
            | "html"
            | "css"
    )
}

/// Normalizes CRLF (`\r\n`) line endings to POSIX LF (`\n`) for deterministic hashing across OSs.
pub fn normalize_line_endings(bytes: &[u8]) -> Vec<u8> {
    if !bytes.contains(&b'\r') {
        return bytes.to_vec();
    }
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            normalized.push(b'\n');
            i += 2;
        } else {
            normalized.push(bytes[i]);
            i += 1;
        }
    }
    normalized
}

/// Helper function to detect if content represents generated code.
pub fn is_generated_code(content: &[u8]) -> bool {
    let prefix_len = content.len().min(4096);
    if let Ok(text) = std::str::from_utf8(&content[..prefix_len]) {
        text.contains("@generated")
            || text.contains("@auto-generated")
            || text.contains("DO NOT EDIT")
            || text.contains("<auto-generated")
    } else {
        false
    }
}

/// Compute snapshot identity from files, config hash, and analyzer version.
/// Excludes timestamps to preserve determinism.
pub fn compute_snapshot_id(
    file_set_digest: &str,
    config_hash: &str,
    analyzer_version: &str,
) -> SnapshotId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(file_set_digest.as_bytes());
    hasher.update(config_hash.as_bytes());
    hasher.update(analyzer_version.as_bytes());
    SnapshotId::new(hasher.finalize().to_hex().to_string())
}

/// Computes the canonical digest over a set of files.
/// Files are processed in canonical path order.
pub fn compute_file_set_digest(files: &BTreeMap<String, FileUnit>) -> String {
    let mut hasher = blake3::Hasher::new();
    for (path, unit) in files {
        hasher.update(path.as_bytes());
        hasher.update(unit.content_hash.as_bytes());
        hasher.update(unit.language.as_bytes());
        hasher.update(&[if unit.is_generated { 1 } else { 0 }]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Create a repository snapshot from an existing map of file units.
pub fn create_snapshot(
    files: BTreeMap<String, FileUnit>,
    revision: &str,
    config_hash: &str,
    analyzer_version: &str,
) -> RepoSnapshot {
    let file_set_digest = compute_file_set_digest(&files);
    let snapshot_id = compute_snapshot_id(&file_set_digest, config_hash, analyzer_version);

    RepoSnapshot {
        snapshot_id,
        revision: revision.to_string(),
        config_hash: config_hash.to_string(),
        file_set_digest,
        files,
    }
}

/// Discover repository files, apply ignore rules, and generate a deterministic snapshot.
pub fn scan_repository(
    repo_root: &Path,
    config: &AutopsyConfig,
    analyzer_version: &str,
    revision: Option<&str>,
) -> Result<RepoSnapshot, RepoError> {
    let mut exclude_patterns = Vec::new();
    for pat in &config.repository.exclude {
        exclude_patterns.push(
            Pattern::new(pat).map_err(|e| RepoError::InvalidGlobPattern {
                pattern: pat.clone(),
                reason: e.to_string(),
            })?,
        );
    }

    let mut files = BTreeMap::new();

    for root_rel in &config.repository.roots {
        let scan_path = if root_rel == "." {
            repo_root.to_path_buf()
        } else {
            repo_root.join(root_rel)
        };

        if !scan_path.exists() {
            continue;
        }

        let walker = WalkBuilder::new(&scan_path)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry_res in walker {
            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let rel_path = match path.strip_prefix(repo_root) {
                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            // Check against exclude glob patterns
            let mut excluded = false;
            for pattern in &exclude_patterns {
                if pattern.matches(&rel_path) {
                    excluded = true;
                    break;
                }
            }
            if excluded {
                continue;
            }

            let raw_bytes = fs::read(path)?;
            let language = classify_language(path);
            let canonical_bytes = if is_text_language(&language) {
                normalize_line_endings(&raw_bytes)
            } else {
                raw_bytes
            };
            let content_hash = blake3::hash(&canonical_bytes).to_hex().to_string();
            let is_generated = is_generated_code(&canonical_bytes);

            files.insert(
                rel_path.clone(),
                FileUnit {
                    path: rel_path,
                    language,
                    content_hash,
                    is_generated,
                },
            );
        }
    }

    let config_hash = config.content_hash();
    let rev = revision.unwrap_or("HEAD");
    Ok(create_snapshot(files, rev, &config_hash, analyzer_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_snapshot_id_golden_100_runs() {
        let file_digest = "a1b2c3d4e5f6";
        let config_hash = "config_hash_xyz";
        let analyzer_ver = "0.0.1";

        let baseline = compute_snapshot_id(file_digest, config_hash, analyzer_ver);
        for _ in 0..100 {
            let rerun = compute_snapshot_id(file_digest, config_hash, analyzer_ver);
            assert_eq!(baseline, rerun, "Snapshot ID must be 100/100 deterministic");
        }
    }

    #[test]
    fn test_file_set_digest_order_invariance() {
        // BTreeMap guarantees sorted iteration regardless of insertion order
        let mut map1 = BTreeMap::new();
        map1.insert(
            "b.ts".to_string(),
            FileUnit {
                path: "b.ts".to_string(),
                language: "typescript".to_string(),
                content_hash: "h2".to_string(),
                is_generated: false,
            },
        );
        map1.insert(
            "a.ts".to_string(),
            FileUnit {
                path: "a.ts".to_string(),
                language: "typescript".to_string(),
                content_hash: "h1".to_string(),
                is_generated: false,
            },
        );

        let mut map2 = BTreeMap::new();
        map2.insert(
            "a.ts".to_string(),
            FileUnit {
                path: "a.ts".to_string(),
                language: "typescript".to_string(),
                content_hash: "h1".to_string(),
                is_generated: false,
            },
        );
        map2.insert(
            "b.ts".to_string(),
            FileUnit {
                path: "b.ts".to_string(),
                language: "typescript".to_string(),
                content_hash: "h2".to_string(),
                is_generated: false,
            },
        );

        assert_eq!(
            compute_file_set_digest(&map1),
            compute_file_set_digest(&map2)
        );
    }

    #[test]
    fn test_autopsy_config_validation() {
        let valid_toml = r#"
            config_version = "0.0.1"
            [repository]
            roots = ["."]
            exclude = ["target/**"]
            [analysis]
            default_profile = "pr"
            max_traversal_nodes = 5000
            [cache]
            directory = ".autopsy/cache"
        "#;
        let config: AutopsyConfig = toml::from_str(valid_toml).unwrap();
        assert!(config.validate(Path::new("autopsy.toml")).is_ok());

        // Invalid config: empty roots
        let invalid_toml = r#"
            config_version = "0.0.1"
            [repository]
            roots = []
            exclude = []
            [analysis]
            default_profile = "pr"
            max_traversal_nodes = 5000
            [cache]
            directory = ".autopsy/cache"
        "#;
        let bad_config: AutopsyConfig = toml::from_str(invalid_toml).unwrap();
        assert!(bad_config.validate(Path::new("autopsy.toml")).is_err());
    }

    #[test]
    fn test_invariants_config_validation() {
        let valid_yaml = r#"
            version: "0.0.1"
            invariants:
              - id: ARCH_NO_CORE_TO_MCP
                description: Rust core must not depend on MCP
                kind: forbidden_dependency
                severity: error
                scope:
                  source: "crates/**"
                  target: "apps/**"
        "#;
        let config: InvariantsConfig = serde_yaml::from_str(valid_yaml).unwrap();
        assert!(config.validate(Path::new("invariants.yml")).is_ok());

        // Invalid severity
        let invalid_yaml = r#"
            version: "0.0.1"
            invariants:
              - id: BAD_INV
                description: test
                kind: test
                severity: fatal
        "#;
        let bad_config: InvariantsConfig = serde_yaml::from_str(invalid_yaml).unwrap();
        assert!(bad_config.validate(Path::new("invariants.yml")).is_err());
    }

    #[test]
    fn test_scan_repository_deterministic_discovery() {
        let temp_dir = std::env::temp_dir().join(format!("synevid_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::create_dir_all(temp_dir.join("target")).unwrap();

        fs::write(temp_dir.join("src/main.ts"), "export const a = 1;").unwrap();
        fs::write(temp_dir.join("src/util.ts"), "export const b = 2;").unwrap();
        fs::write(
            temp_dir.join("src/gen.ts"),
            "// @generated\nexport const c = 3;",
        )
        .unwrap();
        fs::write(temp_dir.join("target/build.js"), "ignored output").unwrap();

        let config = AutopsyConfig {
            config_version: "0.0.1".to_string(),
            repository: RepositoryConfig {
                roots: vec![".".to_string()],
                exclude: vec!["target/**".to_string()],
            },
            analysis: AnalysisConfig {
                default_profile: "pr".to_string(),
                max_traversal_nodes: 1000,
            },
            cache: CacheConfig {
                directory: ".autopsy/cache".to_string(),
            },
        };

        // Scan the directory
        let snapshot1 = scan_repository(&temp_dir, &config, "0.0.1", Some("main")).unwrap();

        // Target should be excluded
        assert!(!snapshot1.files.contains_key("target/build.js"));
        assert!(snapshot1.files.contains_key("src/main.ts"));
        assert!(snapshot1.files.contains_key("src/util.ts"));
        assert!(snapshot1.files["src/gen.ts"].is_generated);

        // 100 runs must produce exact same snapshot_id
        for _ in 0..100 {
            let rerun = scan_repository(&temp_dir, &config, "0.0.1", Some("main")).unwrap();
            assert_eq!(snapshot1.snapshot_id, rerun.snapshot_id);
            assert_eq!(snapshot1.file_set_digest, rerun.file_set_digest);
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cross_platform_path_separator_normalization() {
        let windows_path = "src\\nested\\module.ts";
        let normalized = windows_path.replace('\\', "/");
        assert_eq!(normalized, "src/nested/module.ts");

        let mut files = BTreeMap::new();
        files.insert(
            normalized.clone(),
            FileUnit {
                path: normalized,
                language: "typescript".to_string(),
                content_hash: "hash123".to_string(),
                is_generated: false,
            },
        );

        let digest = compute_file_set_digest(&files);
        assert!(!digest.is_empty());
    }

    #[test]
    fn test_cross_platform_crlf_lf_determinism() {
        let lf_content = b"export const x = 1;\nexport const y = 2;\n";
        let crlf_content = b"export const x = 1;\r\nexport const y = 2;\r\n";

        let norm_lf = normalize_line_endings(lf_content);
        let norm_crlf = normalize_line_endings(crlf_content);

        assert_eq!(norm_lf, norm_crlf);
        assert_eq!(
            blake3::hash(&norm_lf).to_hex(),
            blake3::hash(&norm_crlf).to_hex()
        );
    }
}
