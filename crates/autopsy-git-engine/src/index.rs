//! Minimal, deterministic Git index v2 reader and writer.
//!
//! The Phase 3 UI must operate on the repository's real staging area.  The
//! previous implementation stored a sidecar JSON list under `.autopsy/`,
//! which made the UI look like Git while leaving `.git/index` unchanged.
//! This module deliberately supports the portable v2 index format used by
//! ordinary repositories and rewrites only stage-zero entries.

use autopsy_repo::{classify_language, is_text_language, normalize_line_endings};
use autopsy_temporal::git::{GitError, ObjectId};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexEntry {
    pub mode: u32,
    pub oid: ObjectId,
    pub path: String,
    pub size: u32,
}

fn io_error(path: &Path, source: std::io::Error) -> GitError {
    GitError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn index_path(git_dir: &Path) -> PathBuf {
    git_dir.join("index")
}

pub fn is_safe_index_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\\') || path.starts_with('/') || path.contains('\0') {
        return false;
    }
    let mut saw_segment = false;
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
        saw_segment = true;
    }
    saw_segment
}

/// Convert working-tree source bytes to the deterministic LF representation
/// used by Synevid's Git status/staging checks.
///
/// Git's normal Windows checkout configuration (`core.autocrlf=true`) stores
/// LF in the index/object database while materializing CRLF in the worktree.
/// The analyzer must compare the Git-clean representation, otherwise an
/// unchanged checkout is reported as modified and guarded branch operations
/// are incorrectly refused. Binary/unknown files remain byte-exact.
pub fn clean_worktree_bytes(path: &str, bytes: &[u8]) -> Vec<u8> {
    let language = classify_language(Path::new(path));
    if is_text_language(&language) {
        normalize_line_endings(bytes)
    } else {
        bytes.to_vec()
    }
}

pub fn read_index(git_dir: &Path) -> Result<BTreeMap<String, IndexEntry>, GitError> {
    let path = index_path(git_dir);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = fs::read(&path).map_err(|e| io_error(&path, e))?;
    if bytes.len() < 32 || &bytes[..4] != b"DIRC" {
        return Err(GitError::Malformed(format!(
            "invalid Git index header at {}",
            path.display()
        )));
    }
    let version = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    if version != 2 && version != 3 {
        return Err(GitError::Unsupported(format!(
            "Git index version {version} (only v2/v3 supported)"
        )));
    }
    let count = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let content_end = bytes.len() - 20;
    let mut cursor = 12usize;
    let mut entries = BTreeMap::new();

    for _ in 0..count {
        if cursor.checked_add(62).is_none_or(|end| end > content_end) {
            return Err(GitError::Malformed("truncated Git index entry".to_string()));
        }
        let mode = u32::from_be_bytes(bytes[cursor + 24..cursor + 28].try_into().unwrap());
        let size = u32::from_be_bytes(bytes[cursor + 36..cursor + 40].try_into().unwrap());
        let mut oid_bytes = [0u8; 20];
        oid_bytes.copy_from_slice(&bytes[cursor + 40..cursor + 60]);
        let flags = u16::from_be_bytes(bytes[cursor + 60..cursor + 62].try_into().unwrap());
        let path_start = cursor + 62;
        let Some(relative_end) = bytes[path_start..content_end]
            .iter()
            .position(|byte| *byte == 0)
        else {
            return Err(GitError::Malformed(
                "unterminated Git index path".to_string(),
            ));
        };
        let path_end = path_start + relative_end;
        let relative_path = std::str::from_utf8(&bytes[path_start..path_end])
            .map_err(|_| GitError::Malformed("non-UTF-8 Git index path".to_string()))?
            .to_string();
        if !is_safe_index_path(&relative_path) {
            return Err(GitError::Malformed(format!(
                "unsafe Git index path: {relative_path}"
            )));
        }
        // Conflict stages are not silently flattened into a normal entry.
        // The UI has no conflict-resolution model yet, so refuse them loudly.
        if flags & 0x3000 != 0 {
            return Err(GitError::Unsupported(format!(
                "unmerged Git index entry: {relative_path}"
            )));
        }
        entries.insert(
            relative_path.clone(),
            IndexEntry {
                mode,
                oid: ObjectId(oid_bytes),
                path: relative_path,
                size,
            },
        );
        let entry_len = path_end - cursor + 1;
        cursor += (entry_len + 7) & !7;
    }

    // The checksum protects the entire index.  Verify it before consuming
    // extension bytes; extensions are intentionally discarded on rewrite.
    let mut hasher = Sha1::new();
    hasher.update(&bytes[..content_end]);
    let digest = hasher.finalize();
    if digest.as_slice() != &bytes[content_end..] {
        return Err(GitError::Malformed(
            "Git index checksum mismatch".to_string(),
        ));
    }
    Ok(entries)
}

pub fn write_index(git_dir: &Path, entries: &BTreeMap<String, IndexEntry>) -> Result<(), GitError> {
    let path = index_path(git_dir);
    let mut bytes = Vec::with_capacity(12 + entries.len() * 80 + 20);
    bytes.extend_from_slice(b"DIRC");
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());

    for (relative_path, entry) in entries {
        if relative_path != &entry.path || !is_safe_index_path(relative_path) {
            return Err(GitError::Malformed(format!(
                "unsafe or inconsistent index path: {relative_path}"
            )));
        }
        let mut fixed = [0u8; 62];
        // ctime, mtime, device, inode, uid, gid are intentionally zeroed.
        fixed[24..28].copy_from_slice(&entry.mode.to_be_bytes());
        fixed[36..40].copy_from_slice(&entry.size.to_be_bytes());
        fixed[40..60].copy_from_slice(&entry.oid.0);
        let flags = (relative_path.len().min(0x0fff)) as u16;
        fixed[60..62].copy_from_slice(&flags.to_be_bytes());
        bytes.extend_from_slice(&fixed);
        bytes.extend_from_slice(relative_path.as_bytes());
        bytes.push(0);
        let entry_len = 62 + relative_path.len() + 1;
        let padding = (8 - (entry_len % 8)) % 8;
        bytes.extend(std::iter::repeat_n(0, padding));
    }

    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    bytes.extend_from_slice(&hasher.finalize());

    let temp = git_dir.join(format!("index.synevid-{}.tmp", std::process::id()));
    fs::write(&temp, &bytes).map_err(|e| io_error(&temp, e))?;
    // Windows cannot rename over an existing file.  The temp file is fully
    // written and checksummed before this small replacement window.
    if path.exists() {
        fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
    }
    fs::rename(&temp, &path).map_err(|e| io_error(&path, e))
}

pub fn index_digest(git_dir: &Path) -> Result<String, GitError> {
    let path = index_path(git_dir);
    if !path.exists() {
        return Ok(blake3::hash(b"empty-git-index").to_hex().to_string());
    }
    let bytes = fs::read(&path).map_err(|e| io_error(&path, e))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
