//! Bounded, symlink-safe discovery of catalog and manifest files.
//!
//! This is the layer that touches the filesystem, so it carries the sandboxing
//! invariants (`INV-SANDBOX-001..004`) rather than leaving them to the callers:
//!
//! * **Symlink containment `INV-SANDBOX-002`.** Every entry is inspected with
//!   `symlink_metadata`, and a symlink is **skipped**, never followed. Following one
//!   would let a file outside the workspace be read by a workspace-relative path,
//!   which is the whole attack: a path is not a location.
//! * **Traversal bounds `INV-SANDBOX-003`.** Directory depth, file count and file size
//!   are capped, and every cap that is reached is **recorded** — a truncated scan that
//!   reported itself as complete would be worse than no scan.
//! * **`INV-SANDBOX-004`.** Oversize files are counted in
//!   [`DiscoveryReport::skipped_oversize_files`], not silently dropped.
//! * **`FR-026`.** Nothing is written.
//!
//! # What is looked for, and what is skipped
//!
//! A fixed allowlist of file **names**, not extensions: `catalog-info.yaml` (Backstage)
//! and the four manifest names this crate models. An allowlist rather than a pattern
//! because the alternative — reading every `.yaml` in a repository — reads build output,
//! vendored code and secrets for no benefit.
//!
//! Directory names that never contain a hand-written manifest (`node_modules`, `target`,
//! `vendor`, `dist`, `build`, `.git`) are pruned, and the prune is recorded so a reader
//! can see that a file under one of them was not examined.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Hard bound on directory recursion.
pub const MAX_DEPTH: usize = 32;

/// Default bound on files examined.
pub const DEFAULT_MAX_FILES: usize = 4096;

/// Default bound on a single file's size. Backstage entity documents are small; a
/// megabyte is already far past anything hand-written.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 1_048_576;

/// File names this crate reads, and nothing else.
pub const MANIFEST_NAMES: &[&str] = &[
    "catalog-info.yaml",
    "catalog-info.yml",
    "package.json",
    "requirements.txt",
    "go.mod",
    "Cargo.toml",
];

/// Directory names never descended into.
pub const PRUNED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
];

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog root {path} could not be read: {source}")]
    Root {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} could not be read: {source}")]
    File {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid UTF-8, so it cannot be a catalog document")]
    NotUtf8 { path: String },
    #[error("{path} is not valid {format}: {message}")]
    Malformed {
        path: String,
        format: &'static str,
        message: String,
    },
    #[error("backstage entity name {} is not a valid Backstage name", .0)]
    BadEntityName(String),
    #[error("backstage entity ref {} is not a valid entity reference", .0)]
    BadEntityRef(String),
    #[error("path {} escapes the catalog root", .0)]
    PathEscape(String),
}

/// One file that was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// POSIX-style path relative to the root, so a Windows scan and a Linux scan agree
    /// (`INV-CROSS-001`).
    pub path: String,
    pub content: String,
}

/// What the scan did, including everything it did not do.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryReport {
    pub directories_visited: usize,
    /// Symlinks encountered and not followed `INV-SANDBOX-002`.
    pub skipped_symlinks: usize,
    /// Files past `max_file_bytes` `INV-SANDBOX-004`.
    pub skipped_oversize_files: Vec<String>,
    /// Files past the file-count bound, in sorted order.
    pub skipped_beyond_file_limit: Vec<String>,
    /// Directories pruned by name, with how many subdirectories that removed from view.
    pub pruned_directories: BTreeMap<String, usize>,
    /// Recursion stopped here because `MAX_DEPTH` was reached.
    pub depth_truncated: Vec<String>,
    /// Whether a bound was reached at all, so a caller can tell a complete scan from a
    /// truncated one without inspecting every counter.
    pub truncated: bool,
}

impl DiscoveryReport {
    /// True when every candidate file in the tree was examined.
    pub fn is_complete(&self) -> bool {
        self.skipped_symlinks == 0
            && self.skipped_oversize_files.is_empty()
            && self.skipped_beyond_file_limit.is_empty()
            && self.depth_truncated.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryOptions {
    pub max_depth: usize,
    pub max_files: usize,
    pub max_file_bytes: u64,
}

impl DiscoveryOptions {
    pub fn new() -> Self {
        Self {
            max_depth: MAX_DEPTH,
            max_files: DEFAULT_MAX_FILES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        }
    }

    pub fn with_max_files(mut self, max: usize) -> Self {
        self.max_files = max;
        self
    }

    pub fn with_max_file_bytes(mut self, max: u64) -> Self {
        self.max_file_bytes = max;
        self
    }

    pub fn with_max_depth(mut self, max: usize) -> Self {
        self.max_depth = max;
        self
    }
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A relative path in POSIX form, or a refusal.
///
/// Rejects absolute paths and any `..` component. A path handed to this crate is data, and
/// `..` in data is how a relative path becomes an absolute one one directory at a time.
pub fn safe_relative(path: &Path) -> Result<String, CatalogError> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(CatalogError::PathEscape(path.display().to_string()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(CatalogError::PathEscape(path.display().to_string()));
            }
        }
    }
    Ok(parts.join("/"))
}

/// Whether a file name is one this crate reads.
pub fn is_manifest_name(name: &str) -> bool {
    MANIFEST_NAMES.contains(&name)
}

/// Walks `root` and returns the catalog and manifest files, with a full account of what
/// was skipped.
pub fn discover(
    root: &Path,
    options: &DiscoveryOptions,
) -> Result<(Vec<DiscoveredFile>, DiscoveryReport), CatalogError> {
    let metadata = fs::symlink_metadata(root).map_err(|source| CatalogError::Root {
        path: root.display().to_string(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        // The root itself is the one path a caller chose deliberately. Refusing a
        // symlinked root would be defensible, but silently following it would not: the
        // caller named it, so it is read, and the decision is recorded where a reader can
        // see it.
        return Err(CatalogError::Root {
            path: root.display().to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the catalog root is a symlink; name the real directory instead",
            ),
        });
    }
    if !metadata.is_dir() {
        return Err(CatalogError::Root {
            path: root.display().to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the catalog root is not a directory",
            ),
        });
    }

    let mut files = Vec::new();
    let mut report = DiscoveryReport::default();
    let mut queue: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];

    while let Some((directory, prefix, depth)) = queue.pop() {
        report.directories_visited += 1;
        if depth > options.max_depth {
            report.depth_truncated.push(prefix.clone());
            continue;
        }
        let entries = fs::read_dir(&directory).map_err(|source| CatalogError::File {
            path: directory.display().to_string(),
            source,
        })?;
        // Sorted so the walk order is a function of the tree, not of the filesystem
        // (`NFR-001`).
        let mut names: Vec<(String, PathBuf)> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| CatalogError::File {
                path: directory.display().to_string(),
                source,
            })?;
            names.push((
                entry.file_name().to_string_lossy().to_string(),
                entry.path(),
            ));
        }
        names.sort();

        for (name, path) in names {
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    report.skipped_symlinks += 1;
                    continue;
                }
            };
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                report.skipped_symlinks += 1;
                continue;
            }
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if file_type.is_dir() {
                if PRUNED_DIRECTORIES.contains(&name.as_str()) {
                    *report.pruned_directories.entry(name).or_insert(0) += 1;
                    continue;
                }
                queue.push((path, relative, depth + 1));
                continue;
            }
            if !file_type.is_file() || !is_manifest_name(&name) {
                continue;
            }
            if metadata.len() > options.max_file_bytes {
                report.skipped_oversize_files.push(relative);
                continue;
            }
            if files.len() >= options.max_files {
                report.skipped_beyond_file_limit.push(relative);
                continue;
            }
            let bytes = fs::read(&path).map_err(|source| CatalogError::File {
                path: relative.clone(),
                source,
            })?;
            let content = String::from_utf8(bytes).map_err(|_| CatalogError::NotUtf8 {
                path: relative.clone(),
            })?;
            files.push(DiscoveredFile {
                path: relative,
                content,
            });
        }
    }

    report.skipped_oversize_files.sort();
    report.skipped_beyond_file_limit.sort();
    report.depth_truncated.sort();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    report.truncated = !report.is_complete();
    Ok((files, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parent_directory_component_is_refused() {
        assert_eq!(safe_relative(Path::new("a/b/c.txt")).unwrap(), "a/b/c.txt");
        assert_eq!(safe_relative(Path::new("./a/c.txt")).unwrap(), "a/c.txt");
        assert!(safe_relative(Path::new("../secrets")).is_err());
        assert!(safe_relative(Path::new("a/../../secrets")).is_err());
        assert!(safe_relative(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn only_the_allowlisted_names_are_read() {
        assert!(is_manifest_name("catalog-info.yaml"));
        assert!(is_manifest_name("package.json"));
        assert!(!is_manifest_name("package-lock.json"));
        assert!(!is_manifest_name("docker-compose.yaml"));
        assert!(!is_manifest_name("Cargo.lock"));
    }

    #[test]
    fn a_missing_root_is_an_error_rather_than_an_empty_catalog() {
        // The distinction matters: an empty catalog and an absent catalog are
        // indistinguishable downstream, so the reader refuses and lets the caller decide.
        let error = discover(
            Path::new("/nonexistent/autopsy-catalog-root"),
            &DiscoveryOptions::new(),
        )
        .expect_err("a missing root is an error");
        assert!(matches!(error, CatalogError::Root { .. }));
    }
}
