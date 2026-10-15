//! Shared deterministic git fixture builders for `autopsy-temporal` tests.
//!
//! Fixtures are constructed byte-by-byte rather than by invoking `git`, so the
//! zero-sub-process invariant holds under test and the suite is hermetic.
#![allow(dead_code)]

use autopsy_temporal::git::{ObjectId, compute_object_id};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------

/// Builds the loose-object body `"<kind> <len>\0<payload>"`.
pub fn loose_body(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = format!("{kind} {}\0", payload.len()).into_bytes();
    out.extend_from_slice(payload);
    out
}

pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

/// Writes a loose object and returns its computed id.
pub fn write_loose(git_dir: &Path, kind: &str, payload: &[u8]) -> ObjectId {
    let oid = compute_object_id(kind, payload);
    let hex = oid.to_hex();
    let dir = git_dir.join("objects").join(&hex[0..2]);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(&hex[2..]), zlib(&loose_body(kind, payload))).unwrap();
    oid
}

/// Encodes a git pack "size" varint (7 bits/byte, little-endian, MSB continues).
pub fn encode_size_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// A single pack entry to emit.
pub enum PackEntry {
    /// Concrete object: (git type number, payload).
    Object(u8, Vec<u8>),
    /// Delta against a base identified by object id (type 7).
    RefDelta(ObjectId, Vec<u8>),
    /// Delta against a base identified by a negative pack offset (type 6).
    OfsDelta(u64, Vec<u8>),
}

/// Serializes pack entries, returning the pack bytes (with SHA-1 trailer)
/// plus the pack offset of each entry in order.
pub fn build_pack(entries: &[PackEntry]) -> (Vec<u8>, Vec<u64>) {
    let mut pack = Vec::new();
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&2u32.to_be_bytes());
    pack.extend_from_slice(&(entries.len() as u32).to_be_bytes());

    let mut offsets = Vec::new();
    for entry in entries {
        offsets.push(pack.len() as u64);
        match entry {
            PackEntry::Object(type_num, payload) => {
                let mut header = ((type_num & 0x7) << 4) | ((payload.len() as u8) & 0x0f);
                let mut rest = payload.len() >> 4;
                let mut hdr = Vec::new();
                if rest != 0 {
                    header |= 0x80;
                }
                hdr.push(header);
                while rest != 0 {
                    let mut b = (rest & 0x7f) as u8;
                    rest >>= 7;
                    if rest != 0 {
                        b |= 0x80;
                    }
                    hdr.push(b);
                }
                pack.extend_from_slice(&hdr);
                pack.extend_from_slice(&zlib(payload));
            }
            PackEntry::RefDelta(base, delta) => {
                let mut header = (7u8 << 4) | ((delta.len() as u8) & 0x0f);
                let mut rest = delta.len() >> 4;
                let mut hdr = Vec::new();
                if rest != 0 {
                    header |= 0x80;
                }
                hdr.push(header);
                while rest != 0 {
                    let mut b = (rest & 0x7f) as u8;
                    rest >>= 7;
                    if rest != 0 {
                        b |= 0x80;
                    }
                    hdr.push(b);
                }
                pack.extend_from_slice(&hdr);
                pack.extend_from_slice(&base.0);
                pack.extend_from_slice(&zlib(delta));
            }
            PackEntry::OfsDelta(rel, delta) => {
                let mut header = (6u8 << 4) | ((delta.len() as u8) & 0x0f);
                let mut rest = delta.len() >> 4;
                let mut hdr = Vec::new();
                if rest != 0 {
                    header |= 0x80;
                }
                hdr.push(header);
                while rest != 0 {
                    let mut b = (rest & 0x7f) as u8;
                    rest >>= 7;
                    if rest != 0 {
                        b |= 0x80;
                    }
                    hdr.push(b);
                }
                pack.extend_from_slice(&hdr);
                // Negative offset, big-endian, 7 bits/byte, continuation on MSB.
                let mut buf = vec![(rel & 0x7f) as u8];
                let mut v = rel >> 7;
                while v != 0 {
                    buf.push(((v & 0x7f) as u8) | 0x80);
                    v >>= 7;
                }
                buf.reverse();
                for (i, b) in buf.iter().enumerate() {
                    if i + 1 == buf.len() {
                        pack.push(*b);
                    } else {
                        pack.push(*b | 0x80);
                    }
                }
                pack.extend_from_slice(&zlib(delta));
            }
        }
    }

    let trailer = compute_object_id("", &pack);
    pack.extend_from_slice(&trailer.0);
    (pack, offsets)
}

/// Writes a v2 pack index for `(oid, offset)` pairs sorted by oid.
pub fn write_index(
    pack_dir: &Path,
    stem: &str,
    pack_bytes: &[u8],
    mut pairs: Vec<(ObjectId, u64)>,
) {
    pairs.sort_by_key(|a| a.0.0);

    let mut idx = Vec::new();
    idx.extend_from_slice(b"\xfftOc");
    idx.extend_from_slice(&2u32.to_be_bytes());

    // Fanout: cumulative counts per high byte.
    let mut counts = [0u32; 256];
    for (oid, _) in &pairs {
        counts[oid.0[0] as usize] += 1;
    }
    let mut running = 0u32;
    for c in counts.iter() {
        running += c;
        idx.extend_from_slice(&running.to_be_bytes());
    }

    for (oid, _) in &pairs {
        idx.extend_from_slice(&oid.0);
    }
    for _ in &pairs {
        idx.extend_from_slice(&0u32.to_be_bytes()); // crc32 (unused by reader)
    }
    for (_, off) in &pairs {
        idx.extend_from_slice(&(*off as u32).to_be_bytes());
    }

    let pack_sha = compute_object_id("", pack_bytes);
    idx.extend_from_slice(&pack_sha.0);
    let idx_sha = compute_object_id("", &idx);
    idx.extend_from_slice(&idx_sha.0);

    std::fs::write(pack_dir.join(format!("{stem}.pack")), pack_bytes).unwrap();
    std::fs::write(pack_dir.join(format!("{stem}.idx")), &idx).unwrap();
}

/// Creates a scratch `.git` directory for a test.
pub fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("autopsy-temporal-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("objects/pack")).unwrap();
    std::fs::create_dir_all(dir.join("refs/heads")).unwrap();
    dir
}

pub fn write_head(git_dir: &Path, ref_name: &str) {
    std::fs::write(git_dir.join("HEAD"), format!("ref: {ref_name}\n")).unwrap();
}

pub fn write_ref(git_dir: &Path, ref_name: &str, oid: ObjectId) {
    let path = git_dir.join(ref_name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("{}\n", oid.to_hex())).unwrap();
}

/// Builds a commit payload with deterministic identities.
pub fn commit_payload(tree: ObjectId, parents: &[ObjectId], message: &str, ts: i64) -> Vec<u8> {
    let mut out = format!("tree {tree}\n");
    for p in parents {
        out.push_str(&format!("parent {p}\n"));
    }
    out.push_str(&format!("author T <t@t.t> {ts} +0000\n"));
    out.push_str(&format!("committer T <t@t.t> {ts} +0000\n"));
    out.push_str(&format!("\n{message}\n"));
    out.into_bytes()
}

/// Builds a tree payload from `(mode, name, oid)` entries.
pub fn tree_payload(entries: &[(u32, &str, ObjectId)]) -> Vec<u8> {
    let mut out = Vec::new();
    // Git requires tree entries to be sorted by raw name bytes.
    let mut sorted: Vec<&(u32, &str, ObjectId)> = entries.iter().collect();
    sorted.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
    for (mode, name, oid) in sorted {
        out.extend_from_slice(format!("{mode:o} {name}\0").as_bytes());
        out.extend_from_slice(&oid.0);
    }
    out
}
