//! Read-only, pure-Rust Git object access.
//!
//! # Why this exists
//! Pillar 1 needs commit-level history, but the engine is bound by
//! `INV-SANDBOX-001..004`: **zero sub-process execution**, zero outbound
//! network sockets, and **zero mutation** of source or git history
//! (`FR-026`). Shelling out to `git log` is therefore architecturally
//! forbidden, not merely discouraged.
//!
//! This module reads `.git` directly:
//! * ref resolution (`HEAD`, loose refs, `packed-refs`);
//! * loose objects (zlib-inflated);
//! * packfiles (`.idx` v2 + `.pack`) including `ofs-delta` / `ref-delta`;
//! * commit, tree, tag, and blob decoding.
//!
//! Everything is read-only. Nothing is ever written back to the repository.
//!
//! # Deliberate limitations
//! Object ids are SHA-1 (Git's default format). Repositories using the
//! SHA-256 object format, `.idx` v1, pack index extensions, multi-pack
//! indexes, and alternates are reported as explicit
//! [`GitError::Unsupported`] rather than being silently misread
//! (`FR-014`, `FR-030`).

use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Upper bound on a single decompressed object, guarding against zlib bombs.
const MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;

/// Upper bound on delta resolution chain depth, guarding against cycles.
const MAX_DELTA_DEPTH: usize = 64;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("not a git repository: {0}")]
    NotARepository(PathBuf),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("malformed git object: {0}")]
    Malformed(String),
    #[error("object {0} not found")]
    ObjectNotFound(String),
    #[error("unsupported git feature: {0}")]
    Unsupported(String),
    #[error("zlib failure decoding {0}: {1}")]
    Inflate(String, String),
    #[error("reference {0} does not resolve to a commit")]
    NotACommit(String),
}

fn io_err(path: &Path, source: std::io::Error) -> GitError {
    GitError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A 20-byte SHA-1 git object identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub [u8; 20]);

impl ObjectId {
    /// Renders the canonical 40-character lowercase hex form.
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(40);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// Parses 40 hex characters. Rejects any other length or charset.
    pub fn from_hex(s: &str) -> Result<Self, GitError> {
        if s.len() != 40 {
            return Err(GitError::Malformed(format!(
                "object id must be 40 hex chars, got {}",
                s.len()
            )));
        }
        let mut out = [0u8; 20];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = hex_val(chunk[0])?;
            let lo = hex_val(chunk[1])?;
            out[i] = (hi << 4) | lo;
        }
        Ok(Self(out))
    }

    /// First byte, used for pack-index fanout lookups.
    fn first_byte(self) -> usize {
        self.0[0] as usize
    }
}

fn hex_val(b: u8) -> Result<u8, GitError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        other => Err(GitError::Malformed(format!(
            "non-hex character 0x{other:02x} in object id"
        ))),
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Entry within a git tree object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Octal mode, e.g. `0o100644` for a blob, `0o40000` for a subtree.
    pub mode: u32,
    pub name: String,
    pub oid: ObjectId,
}

impl TreeEntry {
    /// True when this entry is a subtree rather than a blob or symlink.
    pub fn is_tree(&self) -> bool {
        self.mode & 0o170_000 == 0o040_000
    }
}

/// Decoded git commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub oid: ObjectId,
    pub tree: ObjectId,
    pub parents: Vec<ObjectId>,
    /// Raw `author` line, retained verbatim for evidence.
    pub author: String,
    /// Raw `committer` line.
    pub committer: String,
    pub message: String,
}

impl Commit {
    /// First line of the commit message.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or("")
    }

    /// Timestamp from the `committer` line, as seconds since the epoch.
    ///
    /// Used only for human-facing ordering and co-change windows. It must
    /// never enter a canonical digest (NFR-001).
    pub fn committer_timestamp(&self) -> Option<i64> {
        parse_git_timestamp(&self.committer)
    }
}

/// Extracts the trailing `<seconds> <tz>` from an identity line.
fn parse_git_timestamp(line: &str) -> Option<i64> {
    let mut parts = line.rsplit(' ');
    let _tz = parts.next()?;
    parts.next()?.parse::<i64>().ok()
}

/// Decoded tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub oid: ObjectId,
    pub entries: Vec<TreeEntry>,
}

impl Tree {
    /// Looks up a direct child by name.
    pub fn get(&self, name: &str) -> Option<&TreeEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

/// A decoded git object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitObject {
    Commit(Box<Commit>),
    Tree(Box<Tree>),
    Blob(Vec<u8>),
    Tag { target: ObjectId, kind: String },
}

impl GitObject {
    /// Git type name: `commit`, `tree`, `blob`, or `tag`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Commit(_) => "commit",
            Self::Tree(_) => "tree",
            Self::Blob(_) => "blob",
            Self::Tag { .. } => "tag",
        }
    }
}

/// Parsed `.idx` v2 pack index.
#[derive(Debug)]
struct PackIndex {
    /// Sorted object ids, parallel to `offsets`.
    oids: Vec<ObjectId>,
    /// Pack offsets, parallel to `oids`.
    offsets: Vec<u64>,
    /// Cumulative per-high-byte counts; `fanout[i]` = number of ids whose
    /// first byte is `<= i`.
    fanout: [u32; 256],
}

impl PackIndex {
    /// Parses a v2 pack index. v1 and exotic extensions are rejected.
    fn parse(bytes: &[u8]) -> Result<Self, GitError> {
        // v2 magic: \377tOc
        if bytes.len() < 8 || &bytes[0..4] != b"\xfftOc" {
            return Err(GitError::Unsupported(
                "pack index v1 or unrecognized magic (only idx v2 supported)".to_string(),
            ));
        }
        let version = read_u32(bytes, 4)?;
        if version != 2 {
            return Err(GitError::Unsupported(format!(
                "pack index version {version} (only v2 supported)"
            )));
        }

        // Fanout table: 256 big-endian u32 cumulative counts.
        let fanout_start = 8;
        let fanout_end = fanout_start + 256 * 4;
        if bytes.len() < fanout_end {
            return Err(GitError::Malformed("truncated pack index fanout".into()));
        }
        let fanout: Vec<u32> = (0..256)
            .map(|i| read_u32(bytes, fanout_start + i * 4))
            .collect::<Result<_, _>>()?;
        let count = *fanout.last().unwrap() as usize;
        let mut fanout_arr = [0u32; 256];
        fanout_arr.copy_from_slice(&fanout);
        if count == 0 {
            return Ok(Self {
                oids: Vec::new(),
                offsets: Vec::new(),
                fanout: fanout_arr,
            });
        }

        let names_start = fanout_end;
        let names_end = names_start + count * 20;
        let crc_start = names_end;
        let crc_end = crc_start + count * 4;
        let off_start = crc_end;
        let off_end = off_start + count * 4;
        if bytes.len() < off_end {
            return Err(GitError::Malformed("truncated pack index tables".into()));
        }

        let mut oids = Vec::with_capacity(count);
        for i in 0..count {
            let base = names_start + i * 20;
            let mut arr = [0u8; 20];
            arr.copy_from_slice(&bytes[base..base + 20]);
            oids.push(ObjectId(arr));
        }

        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            let raw = read_u32(bytes, off_start + i * 4)?;
            if raw & 0x8000_0000 != 0 {
                // Large-offset table follows the 32-bit table.
                let idx = (raw & 0x7fff_ffff) as usize;
                let large_base = off_end + idx * 8;
                if bytes.len() < large_base + 8 {
                    return Err(GitError::Malformed("truncated large-offset table".into()));
                }
                offsets.push(read_u64(bytes, large_base)?);
            } else {
                offsets.push(u64::from(raw));
            }
        }

        Ok(Self {
            oids,
            offsets,
            fanout: fanout_arr,
        })
    }

    /// Binary-searches the index, using the fanout table to bound the range.
    ///
    /// The fanout bounds the search to the bucket for the id's high byte, so a
    /// corrupt or non-monotonic index cannot cause a whole-table scan.
    fn offset_of(&self, oid: ObjectId) -> Option<u64> {
        if self.oids.is_empty() {
            return None;
        }
        let bucket = oid.first_byte();
        let start = if bucket == 0 {
            0
        } else {
            self.fanout[bucket - 1] as usize
        };
        let end = self.fanout[bucket] as usize;
        if start > end || end > self.oids.len() {
            return None;
        }
        self.oids[start..end]
            .binary_search_by(|probe| probe.0.cmp(&oid.0))
            .ok()
            .map(|i| self.offsets[start + i])
    }
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, GitError> {
    let end = at + 4;
    if bytes.len() < end {
        return Err(GitError::Malformed("unexpected end of buffer (u32)".into()));
    }
    Ok(u32::from_be_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, GitError> {
    let end = at + 8;
    if bytes.len() < end {
        return Err(GitError::Malformed("unexpected end of buffer (u64)".into()));
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes[at..end]);
    Ok(u64::from_be_bytes(arr))
}

/// Pack object header types, per the packfile format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackEntryKind {
    Commit,
    Tree,
    Blob,
    Tag,
    OfsDelta,
    RefDelta,
}

impl PackEntryKind {
    fn from_bits(bits: u8) -> Result<Self, GitError> {
        Ok(match bits {
            1 => Self::Commit,
            2 => Self::Tree,
            3 => Self::Blob,
            4 => Self::Tag,
            6 => Self::OfsDelta,
            7 => Self::RefDelta,
            other => {
                return Err(GitError::Malformed(format!(
                    "unknown pack object type {other}"
                )));
            }
        })
    }

    /// Resolved git type name for non-delta kinds.
    fn type_name(self) -> Option<&'static str> {
        match self {
            Self::Commit => Some("commit"),
            Self::Tree => Some("tree"),
            Self::Blob => Some("blob"),
            Self::Tag => Some("tag"),
            Self::OfsDelta | Self::RefDelta => None,
        }
    }
}

/// A read-only git object database supporting loose and packed objects.
#[derive(Debug)]
pub struct GitObjectStore {
    git_dir: PathBuf,
    loose_objects_dir: PathBuf,
    packs: Vec<PackedObjects>,
    packed_refs: BTreeMap<String, ObjectId>,
    resolved_refs: BTreeMap<String, ObjectId>,
}

/// A single `.pack` / `.idx` pair with in-memory pack cache.
#[derive(Debug)]
struct PackedObjects {
    index: PackIndex,
    pack_path: PathBuf,
    pack_data: Vec<u8>,
}

impl GitObjectStore {
    /// Discovers the `.git` directory upward from `start`.
    ///
    /// Honours `GITDIR`-style `.git` *files* (`gitdir: <path>`) used by
    /// worktrees and submodules. Performs no writes and no traversal
    /// outside the repository (symlinks are not followed).
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, GitError> {
        let mut dir = start.as_ref().canonicalize().map_err(|e| GitError::Io {
            path: start.as_ref().to_path_buf(),
            source: e,
        })?;

        loop {
            let candidate = dir.join(".git");
            if candidate.is_dir() {
                let store = Self::open(&candidate)?;
                return Ok(store);
            }
            if candidate.is_file() {
                let contents = fs::read_to_string(&candidate).map_err(|e| io_err(&candidate, e))?;
                let target = contents
                    .trim()
                    .strip_prefix("gitdir:")
                    .map(str::trim)
                    .ok_or_else(|| {
                        GitError::Malformed(format!(
                            "{} is not a gitdir pointer",
                            candidate.display()
                        ))
                    })?;
                let resolved = if Path::new(target).is_absolute() {
                    PathBuf::from(target)
                } else {
                    dir.join(target)
                };
                return Self::open(&resolved);
            }
            if !dir.pop() {
                return Err(GitError::NotARepository(start.as_ref().to_path_buf()));
            }
        }
    }

    /// Opens an explicit git directory (e.g. `/repo/.git`).
    pub fn open(git_dir: impl AsRef<Path>) -> Result<Self, GitError> {
        let git_dir = git_dir.as_ref().canonicalize().map_err(|e| GitError::Io {
            path: git_dir.as_ref().to_path_buf(),
            source: e,
        })?;

        if git_dir.join("commondir").exists() {
            return Err(GitError::Unsupported(
                "linked worktrees (commondir) are not yet supported".to_string(),
            ));
        }
        if git_dir.join("objects/info/alternates").exists() {
            return Err(GitError::Unsupported(
                "object alternates are not supported".to_string(),
            ));
        }

        let loose_objects_dir = git_dir.join("objects");
        let packed_refs = Self::load_packed_refs(&git_dir)?;
        let packs = Self::load_packs(&loose_objects_dir)?;

        Ok(Self {
            git_dir,
            loose_objects_dir,
            packs,
            packed_refs,
            resolved_refs: BTreeMap::new(),
        })
    }

    /// Path to the discovered git directory.
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    fn load_packed_refs(git_dir: &Path) -> Result<BTreeMap<String, ObjectId>, GitError> {
        let path = git_dir.join("packed-refs");
        let mut map = BTreeMap::new();
        if !path.exists() {
            return Ok(map);
        }
        let contents = fs::read_to_string(&path).map_err(|e| io_err(&path, e))?;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            let (oid_hex, name) = line
                .split_once(' ')
                .ok_or_else(|| GitError::Malformed(format!("bad packed-refs line: {line}")))?;
            map.insert(name.to_string(), ObjectId::from_hex(oid_hex.trim())?);
        }
        Ok(map)
    }

    fn load_packs(objects_dir: &Path) -> Result<Vec<PackedObjects>, GitError> {
        let pack_dir = objects_dir.join("pack");
        let mut packs = Vec::new();
        if !pack_dir.is_dir() {
            return Ok(packs);
        }
        let mut idx_paths: Vec<PathBuf> = fs::read_dir(&pack_dir)
            .map_err(|e| io_err(&pack_dir, e))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "idx"))
            .collect();
        // Sorted for deterministic pack precedence.
        idx_paths.sort();

        for idx_path in idx_paths {
            let bytes = fs::read(&idx_path).map_err(|e| io_err(&idx_path, e))?;
            let index = PackIndex::parse(&bytes)?;
            let pack_path = idx_path.with_extension("pack");
            let pack_data = fs::read(&pack_path).map_err(|e| io_err(&pack_path, e))?;
            packs.push(PackedObjects {
                index,
                pack_path,
                pack_data,
            });
        }
        Ok(packs)
    }

    /// Resolves a fully-qualified ref (`HEAD`, `refs/heads/main`, ...).
    ///
    /// Resolution order is loose ref, then packed ref, then `HEAD` deref.
    /// Results are memoized for repeat lookups.
    pub fn resolve_ref(&mut self, name: &str) -> Result<Option<ObjectId>, GitError> {
        if let Some(oid) = self.resolved_refs.get(name) {
            return Ok(Some(*oid));
        }
        let oid = self.resolve_ref_inner(name, 0)?;
        if let Some(oid) = oid {
            self.resolved_refs.insert(name.to_string(), oid);
        }
        Ok(oid)
    }

    fn resolve_ref_inner(&self, name: &str, depth: usize) -> Result<Option<ObjectId>, GitError> {
        if depth > 8 {
            return Err(GitError::Malformed(format!(
                "symbolic ref chain too deep at {name}"
            )));
        }

        let loose = self.git_dir.join(name);
        if loose.exists() {
            let contents = fs::read_to_string(&loose).map_err(|e| io_err(&loose, e))?;
            let contents = contents.trim();
            if let Some(target) = contents.strip_prefix("ref:") {
                return self.resolve_ref_inner(target.trim(), depth + 1);
            }
            return Ok(Some(ObjectId::from_hex(contents)?));
        }

        if let Some(oid) = self.packed_refs.get(name) {
            return Ok(Some(*oid));
        }

        Ok(None)
    }

    /// Resolves the current `HEAD` to a commit, dereferencing annotated tags.
    pub fn head_commit(&mut self) -> Result<Commit, GitError> {
        let oid = self
            .resolve_ref("HEAD")?
            .ok_or_else(|| GitError::Malformed("HEAD does not resolve".into()))?;
        self.commit(oid)
    }

    /// Number of objects reachable in packfiles (diagnostics only).
    pub fn packed_object_count(&self) -> usize {
        self.packs.iter().map(|p| p.index.oids.len()).sum()
    }

    /// Reads and decodes any object by id.
    pub fn object(&self, oid: ObjectId) -> Result<GitObject, GitError> {
        let (kind, data) = self.raw_object(oid, 0)?;
        decode_object(&kind, &data, oid)
    }

    /// Reads a commit, following tag indirection.
    pub fn commit(&self, oid: ObjectId) -> Result<Commit, GitError> {
        match self.object(oid)? {
            GitObject::Commit(c) => Ok(*c),
            GitObject::Tag { target, .. } => match self.object(target)? {
                GitObject::Commit(c) => Ok(*c),
                other => Err(GitError::NotACommit(format!(
                    "{} resolves to {}",
                    oid,
                    other.type_name()
                ))),
            },
            other => Err(GitError::NotACommit(format!(
                "{} is a {}",
                oid,
                other.type_name()
            ))),
        }
    }

    /// Reads a tree object.
    pub fn tree(&self, oid: ObjectId) -> Result<Tree, GitError> {
        match self.object(oid)? {
            GitObject::Tree(t) => Ok(*t),
            other => Err(GitError::Malformed(format!(
                "{oid} is a {}, expected tree",
                other.type_name()
            ))),
        }
    }

    /// Reads a blob's raw bytes.
    pub fn blob(&self, oid: ObjectId) -> Result<Vec<u8>, GitError> {
        match self.object(oid)? {
            GitObject::Blob(b) => Ok(b),
            other => Err(GitError::Malformed(format!(
                "{oid} is a {}, expected blob",
                other.type_name()
            ))),
        }
    }

    /// Walks first-parent history from `start`, newest first.
    ///
    /// Deterministic: parents are always visited in recorded order and the
    /// result is capped by `limit`. Merge commits are traversed through the
    /// first parent only, matching `git log --first-parent`.
    pub fn walk_first_parent(
        &self,
        start: ObjectId,
        limit: usize,
    ) -> Result<Vec<Commit>, GitError> {
        let mut out = Vec::new();
        let mut cursor = Some(start);
        while let Some(oid) = cursor {
            if out.len() >= limit {
                break;
            }
            let commit = self.commit(oid)?;
            cursor = commit.parents.first().copied();
            out.push(commit);
        }
        Ok(out)
    }

    /// Reads raw `(type_name, payload)` for an object, resolving deltas.
    fn raw_object(&self, oid: ObjectId, depth: usize) -> Result<(String, Vec<u8>), GitError> {
        if depth > MAX_DELTA_DEPTH {
            return Err(GitError::Malformed(format!(
                "delta chain deeper than {MAX_DELTA_DEPTH} at {oid}"
            )));
        }

        if let Some(loose) = self.read_loose(oid)? {
            return Ok(loose);
        }

        for pack in &self.packs {
            if let Some(offset) = pack.index.offset_of(oid) {
                return self.read_packed(pack, offset, oid, depth);
            }
        }

        Err(GitError::ObjectNotFound(oid.to_hex()))
    }

    /// Reads a loose object into `(type, payload)`.
    fn read_loose(&self, oid: ObjectId) -> Result<Option<(String, Vec<u8>)>, GitError> {
        let hex = oid.to_hex();
        let path = self.loose_objects_dir.join(&hex[0..2]).join(&hex[2..]);
        if !path.exists() {
            return Ok(None);
        }
        let compressed = fs::read(&path).map_err(|e| io_err(&path, e))?;
        let inflated = inflate(&compressed, &hex)?;
        let (kind, payload) = split_loose_header(&inflated)?;
        Ok(Some((kind, payload)))
    }

    /// Reads an object stored at a pack offset, resolving delta chains.
    fn read_packed(
        &self,
        pack: &PackedObjects,
        offset: u64,
        oid: ObjectId,
        depth: usize,
    ) -> Result<(String, Vec<u8>), GitError> {
        let bytes = &pack.pack_data;
        let at = offset as usize;

        if bytes.len() < 12 || &bytes[0..4] != b"PACK" {
            return Err(GitError::Malformed(format!(
                "{} is not a packfile",
                pack.pack_path.display()
            )));
        }
        let pack_version = read_u32(bytes, 4)?;
        if pack_version != 2 && pack_version != 3 {
            return Err(GitError::Unsupported(format!(
                "pack version {pack_version}"
            )));
        }

        // Variable-length object header: 3 type bits + continuation.
        let mut cursor = at;
        if cursor >= bytes.len() {
            return Err(GitError::Malformed("pack offset past end".into()));
        }
        let mut byte = bytes[cursor];
        cursor += 1;
        let kind = PackEntryKind::from_bits((byte >> 4) & 0x7)?;
        let mut _size = u64::from(byte & 0x0f);
        let mut shift = 4;
        while byte & 0x80 != 0 {
            if cursor >= bytes.len() {
                return Err(GitError::Malformed("truncated pack object header".into()));
            }
            byte = bytes[cursor];
            cursor += 1;
            _size |= u64::from(byte & 0x7f) << shift;
            shift += 7;
            if shift > 63 {
                return Err(GitError::Malformed("pack size varint overflow".into()));
            }
        }

        match kind {
            PackEntryKind::OfsDelta => {
                // Negative offset to the base, big-endian, 7 bits per byte.
                let mut b = bytes
                    .get(cursor)
                    .copied()
                    .ok_or_else(|| GitError::Malformed("truncated ofs-delta".into()))?;
                cursor += 1;
                let mut base_rel = u64::from(b & 0x7f);
                while b & 0x80 != 0 {
                    b = *bytes
                        .get(cursor)
                        .ok_or_else(|| GitError::Malformed("truncated ofs-delta".into()))?;
                    cursor += 1;
                    base_rel = ((base_rel + 1) << 7) | u64::from(b & 0x7f);
                }
                let base_offset = offset
                    .checked_sub(base_rel)
                    .ok_or_else(|| GitError::Malformed("ofs-delta base underflow".into()))?;

                let (kind_name, base) = self.read_packed_at(bytes, base_offset, depth + 1)?;
                let delta = inflate_from(bytes, cursor, &oid.to_hex())?;
                let resolved = apply_delta(&base, &delta)?;
                Ok((kind_name, resolved))
            }
            PackEntryKind::RefDelta => {
                if cursor + 20 > bytes.len() {
                    return Err(GitError::Malformed("truncated ref-delta id".into()));
                }
                let mut arr = [0u8; 20];
                arr.copy_from_slice(&bytes[cursor..cursor + 20]);
                cursor += 20;
                let base_oid = ObjectId(arr);

                let (kind_name, base) = self.raw_object(base_oid, depth + 1)?;
                let delta = inflate_from(bytes, cursor, &oid.to_hex())?;
                let resolved = apply_delta(&base, &delta)?;
                Ok((kind_name, resolved))
            }
            concrete => {
                let data = inflate_from(bytes, cursor, &oid.to_hex())?;
                let name = concrete
                    .type_name()
                    .ok_or_else(|| GitError::Malformed("unexpected delta kind".into()))?;
                Ok((name.to_string(), data))
            }
        }
    }

    /// Reads a pack object at an offset without a known id (delta bases).
    fn read_packed_at(
        &self,
        bytes: &[u8],
        offset: u64,
        depth: usize,
    ) -> Result<(String, Vec<u8>), GitError> {
        if depth > MAX_DELTA_DEPTH {
            return Err(GitError::Malformed("delta chain too deep".into()));
        }
        let at = offset as usize;
        let mut cursor = at;
        let mut byte = *bytes
            .get(cursor)
            .ok_or_else(|| GitError::Malformed("offset past end".into()))?;
        cursor += 1;
        let kind = PackEntryKind::from_bits((byte >> 4) & 0x7)?;
        // The declared size is not needed for delta resolution: the delta
        // stream carries its own base/target sizes. We only advance the cursor
        // past the varint.
        while byte & 0x80 != 0 {
            byte = *bytes
                .get(cursor)
                .ok_or_else(|| GitError::Malformed("truncated header".into()))?;
            cursor += 1;
        }

        match kind {
            PackEntryKind::OfsDelta => {
                let mut b = *bytes
                    .get(cursor)
                    .ok_or_else(|| GitError::Malformed("truncated ofs-delta".into()))?;
                cursor += 1;
                let mut base_rel = u64::from(b & 0x7f);
                while b & 0x80 != 0 {
                    b = *bytes
                        .get(cursor)
                        .ok_or_else(|| GitError::Malformed("truncated ofs-delta".into()))?;
                    cursor += 1;
                    base_rel = ((base_rel + 1) << 7) | u64::from(b & 0x7f);
                }
                let base_offset = offset
                    .checked_sub(base_rel)
                    .ok_or_else(|| GitError::Malformed("ofs-delta base underflow".into()))?;
                let (kind_name, base) = self.read_packed_at(bytes, base_offset, depth + 1)?;
                let delta = inflate_from(bytes, cursor, "<delta>")?;
                let resolved = apply_delta(&base, &delta)?;
                Ok((kind_name, resolved))
            }
            PackEntryKind::RefDelta => {
                if cursor + 20 > bytes.len() {
                    return Err(GitError::Malformed("truncated ref-delta id".into()));
                }
                let mut arr = [0u8; 20];
                arr.copy_from_slice(&bytes[cursor..cursor + 20]);
                cursor += 20;
                let (kind_name, base) = self.raw_object(ObjectId(arr), depth + 1)?;
                let delta = inflate_from(bytes, cursor, "<delta>")?;
                let resolved = apply_delta(&base, &delta)?;
                Ok((kind_name, resolved))
            }
            concrete => {
                let data = inflate_from(bytes, cursor, "<pack>")?;
                let name = concrete
                    .type_name()
                    .ok_or_else(|| GitError::Malformed("unexpected delta kind".into()))?;
                Ok((name.to_string(), data))
            }
        }
    }
}

/// Decodes raw object bytes into a [`GitObject`].
fn decode_object(kind: &str, data: &[u8], oid: ObjectId) -> Result<GitObject, GitError> {
    match kind {
        "commit" => Ok(GitObject::Commit(Box::new(parse_commit(data, oid)?))),
        "tree" => Ok(GitObject::Tree(Box::new(parse_tree(data, oid)?))),
        "blob" => Ok(GitObject::Blob(data.to_vec())),
        "tag" => {
            let text = String::from_utf8_lossy(data);
            let mut target = None;
            let mut obj_type = String::new();
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("object ") {
                    target = Some(ObjectId::from_hex(rest.trim())?);
                } else if let Some(rest) = line.strip_prefix("type ") {
                    obj_type = rest.trim().to_string();
                }
                if line.is_empty() {
                    break;
                }
            }
            let target = target
                .ok_or_else(|| GitError::Malformed(format!("tag {oid} has no target object")))?;
            Ok(GitObject::Tag {
                target,
                kind: obj_type,
            })
        }
        other => Err(GitError::Malformed(format!(
            "unknown object type `{other}`"
        ))),
    }
}

/// Parses a commit payload.
fn parse_commit(data: &[u8], oid: ObjectId) -> Result<Commit, GitError> {
    let text = std::str::from_utf8(data)
        .map_err(|_| GitError::Malformed(format!("commit {oid} is not valid UTF-8")))?;

    let (headers, message) = match text.split_once("\n\n") {
        Some((h, m)) => (h, m.to_string()),
        None => (text, String::new()),
    };

    let mut tree = None;
    let mut parents = Vec::new();
    let mut author = String::new();
    let mut committer = String::new();

    // Continuation lines (indented) belong to the previous header; they are
    // irrelevant to the fields we extract, so they are skipped explicitly.
    for line in headers.lines() {
        if line.starts_with(' ') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("tree ") {
            tree = Some(ObjectId::from_hex(rest.trim())?);
        } else if let Some(rest) = line.strip_prefix("parent ") {
            parents.push(ObjectId::from_hex(rest.trim())?);
        } else if let Some(rest) = line.strip_prefix("author ") {
            author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("committer ") {
            committer = rest.to_string();
        }
    }

    let tree = tree.ok_or_else(|| GitError::Malformed(format!("commit {oid} has no tree")))?;

    Ok(Commit {
        oid,
        tree,
        parents,
        author,
        committer,
        message,
    })
}

/// Parses a tree payload. Entries are returned in recorded (sorted) order.
fn parse_tree(data: &[u8], oid: ObjectId) -> Result<Tree, GitError> {
    let mut entries = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        // `<mode> <name>\0<20-byte sha>`
        let space = data[i..]
            .iter()
            .position(|&b| b == b' ')
            .ok_or_else(|| GitError::Malformed(format!("tree {oid}: missing mode space")))?;
        let mode_str = std::str::from_utf8(&data[i..i + space])
            .map_err(|_| GitError::Malformed(format!("tree {oid}: non-UTF8 mode")))?;
        let mode = u32::from_str_radix(mode_str, 8)
            .map_err(|_| GitError::Malformed(format!("tree {oid}: bad mode `{mode_str}`")))?;
        i += space + 1;

        let nul = data[i..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| GitError::Malformed(format!("tree {oid}: unterminated name")))?;
        let name = String::from_utf8_lossy(&data[i..i + nul]).into_owned();
        i += nul + 1;

        if i + 20 > data.len() {
            return Err(GitError::Malformed(format!(
                "tree {oid}: truncated object id"
            )));
        }
        let mut arr = [0u8; 20];
        arr.copy_from_slice(&data[i..i + 20]);
        i += 20;

        entries.push(TreeEntry {
            mode,
            name,
            oid: ObjectId(arr),
        });
    }
    Ok(Tree { oid, entries })
}

/// Splits the `"<type> <len>\0"` loose-object header from its payload.
fn split_loose_header(inflated: &[u8]) -> Result<(String, Vec<u8>), GitError> {
    let nul = inflated
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| GitError::Malformed("loose object missing header terminator".into()))?;
    let header = std::str::from_utf8(&inflated[..nul])
        .map_err(|_| GitError::Malformed("loose object header is not UTF-8".into()))?;
    let (kind, len_str) = header
        .split_once(' ')
        .ok_or_else(|| GitError::Malformed(format!("bad loose header `{header}`")))?;
    let recorded: usize = len_str
        .parse()
        .map_err(|_| GitError::Malformed(format!("bad loose length `{len_str}`")))?;
    let payload = &inflated[nul + 1..];
    if payload.len() != recorded {
        return Err(GitError::Malformed(format!(
            "loose object length mismatch: header says {recorded}, found {}",
            payload.len()
        )));
    }
    Ok((kind.to_string(), payload.to_vec()))
}

/// Fully inflates a buffer, enforcing [`MAX_OBJECT_BYTES`].
fn inflate(compressed: &[u8], context: &str) -> Result<Vec<u8>, GitError> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut out = Vec::new();
    decoder
        .by_ref()
        .take(MAX_OBJECT_BYTES)
        .read_to_end(&mut out)
        .map_err(|e| GitError::Inflate(context.to_string(), e.to_string()))?;
    Ok(out)
}

/// Inflates a zlib stream starting at `start`, ignoring trailing bytes.
fn inflate_from(bytes: &[u8], start: usize, context: &str) -> Result<Vec<u8>, GitError> {
    let slice = bytes
        .get(start..)
        .ok_or_else(|| GitError::Malformed("inflate offset past end".into()))?;
    inflate(slice, context)
}

/// Applies a git delta (`base` + instructions) producing the target payload.
fn apply_delta(base: &[u8], delta: &[u8]) -> Result<Vec<u8>, GitError> {
    let mut i = 0usize;
    let read_varint = |i: &mut usize, delta: &[u8]| -> Result<u64, GitError> {
        let mut value: u64 = 0;
        let mut shift = 0;
        loop {
            let b = *delta
                .get(*i)
                .ok_or_else(|| GitError::Malformed("truncated delta varint".into()))?;
            *i += 1;
            value |= u64::from(b & 0x7f) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
            if shift > 63 {
                return Err(GitError::Malformed("delta varint overflow".into()));
            }
        }
        Ok(value)
    };

    let base_size = read_varint(&mut i, delta)? as usize;
    if base_size != base.len() {
        return Err(GitError::Malformed(format!(
            "delta base size mismatch: expected {base_size}, have {}",
            base.len()
        )));
    }
    let target_size = read_varint(&mut i, delta)? as usize;
    let mut out = Vec::with_capacity(target_size);

    while i < delta.len() {
        let op = delta[i];
        i += 1;
        if op & 0x80 != 0 {
            // Copy from base: 4 offset bytes + 3 size bytes, each optional.
            let mut copy_offset: u64 = 0;
            let mut copy_size: u64 = 0;
            for (bit, shift) in [(0x01u8, 0u32), (0x02, 8), (0x04, 16), (0x08, 24)] {
                if op & bit != 0 {
                    let b = *delta
                        .get(i)
                        .ok_or_else(|| GitError::Malformed("truncated delta copy".into()))?;
                    i += 1;
                    copy_offset |= u64::from(b) << shift;
                }
            }
            for (bit, shift) in [(0x10u8, 0u32), (0x20, 8), (0x40, 16)] {
                if op & bit != 0 {
                    let b = *delta
                        .get(i)
                        .ok_or_else(|| GitError::Malformed("truncated delta copy".into()))?;
                    i += 1;
                    copy_size |= u64::from(b) << shift;
                }
            }
            if copy_size == 0 {
                copy_size = 0x10000; // Git's implicit 64 KiB.
            }
            let start = copy_offset as usize;
            let end = start
                .checked_add(copy_size as usize)
                .ok_or_else(|| GitError::Malformed("delta copy range overflow".into()))?;
            if end > base.len() {
                return Err(GitError::Malformed(format!(
                    "delta copy out of range: {start}..{end} of {}",
                    base.len()
                )));
            }
            out.extend_from_slice(&base[start..end]);
        } else if op != 0 {
            // Insert `op` literal bytes.
            let end = i + op as usize;
            if end > delta.len() {
                return Err(GitError::Malformed("truncated delta insert".into()));
            }
            out.extend_from_slice(&delta[i..end]);
            i = end;
        } else {
            return Err(GitError::Malformed("delta opcode 0 is reserved".into()));
        }
    }

    if out.len() != target_size {
        return Err(GitError::Malformed(format!(
            "delta produced {} bytes, expected {target_size}",
            out.len()
        )));
    }
    Ok(out)
}

/// Computes a git object id for `payload` of `kind`, for integrity checks.
///
/// Git hashes `"<kind> <len>\0<payload>"` with SHA-1.
pub fn compute_object_id(kind: &str, payload: &[u8]) -> ObjectId {
    let mut hasher = Sha1::new();
    hasher.update(kind.as_bytes());
    hasher.update(b" ");
    hasher.update(payload.len().to_string().as_bytes());
    hasher.update([0u8]);
    hasher.update(payload);
    let digest = hasher.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest);
    ObjectId(out)
}

/// Recursively flattens a tree into `path -> (mode, ObjectId)`.
///
/// Paths use POSIX separators and are sorted, so the map is deterministic.
/// Submodules (mode `0o160000`) are recorded as entries but never descended
/// into, since their objects live in another repository.
pub fn flatten_tree(
    store: &GitObjectStore,
    tree_oid: ObjectId,
) -> Result<BTreeMap<String, (u32, ObjectId)>, GitError> {
    let mut out = BTreeMap::new();
    flatten_into(store, tree_oid, "", &mut out, 0)?;
    Ok(out)
}

fn flatten_into(
    store: &GitObjectStore,
    tree_oid: ObjectId,
    prefix: &str,
    out: &mut BTreeMap<String, (u32, ObjectId)>,
    depth: usize,
) -> Result<(), GitError> {
    if depth > 128 {
        return Err(GitError::Malformed("tree nesting too deep".into()));
    }
    let tree = store.tree(tree_oid)?;
    for entry in tree.entries {
        let path = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{prefix}/{}", entry.name)
        };
        if entry.is_tree() {
            flatten_into(store, entry.oid, &path, out, depth + 1)?;
        } else {
            out.insert(path, (entry.mode, entry.oid));
        }
    }
    Ok(())
}
