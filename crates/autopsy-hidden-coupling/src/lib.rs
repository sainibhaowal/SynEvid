//! Hidden-coupling discovery (Pillar 5, FR-ADV-021).
//!
//! Pluggable extractors for environment variables, configuration keys, message-queue
//! topics and database table names. The extraction here is deliberately text-based and
//! conservative: every report carries `CoverageState::Partial` and a non-causality
//! disclaimer, because string matching cannot prove that two call sites actually share
//! a runtime resource.

use autopsy_domain::CoverageState;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HiddenCouplingError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceKind {
    EnvVar,
    ConfigKey,
    QueueTopic,
    DbTable,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::EnvVar => "env_var",
            ResourceKind::ConfigKey => "config_key",
            ResourceKind::QueueTopic => "queue_topic",
            ResourceKind::DbTable => "db_table",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SharedResource {
    pub kind: ResourceKind,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CouplingEdge {
    pub a: String,
    pub b: String,
    pub resource: SharedResource,
    pub coverage: CoverageState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenCouplingReport {
    pub files_scanned: usize,
    pub resources: Vec<SharedResource>,
    pub edges: Vec<CouplingEdge>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

/// Scan a repository tree for hidden coupling. Text-based, read-only, deterministic.
pub fn scan(root: &Path) -> Result<HiddenCouplingReport, HiddenCouplingError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(root, &mut files);
    files.sort();

    // resource key -> set of files that reference it
    let mut index: BTreeMap<(ResourceKind, String), BTreeSet<String>> = BTreeMap::new();
    let mut files_scanned = 0usize;
    for file in &files {
        let Ok(content) = fs::read_to_string(file) else {
            continue;
        };
        files_scanned += 1;
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let resources = extract(&content);
        for (kind, key) in resources {
            index.entry((kind, key)).or_default().insert(rel.clone());
        }
    }

    let mut resources: Vec<SharedResource> = index
        .keys()
        .map(|(k, key)| SharedResource {
            kind: *k,
            key: key.clone(),
        })
        .collect();
    resources.sort();

    let mut edges: Vec<CouplingEdge> = Vec::new();
    for ((kind, key), files) in &index {
        let mut sorted: Vec<String> = files.iter().cloned().collect();
        sorted.sort();
        for i in 0..sorted.len() {
            for j in (i + 1)..sorted.len() {
                edges.push(CouplingEdge {
                    a: sorted[i].clone(),
                    b: sorted[j].clone(),
                    resource: SharedResource {
                        kind: *kind,
                        key: key.clone(),
                    },
                    coverage: CoverageState::Partial,
                });
            }
        }
    }
    edges.sort();

    let coverage = if files_scanned == 0 {
        CoverageState::Unknown
    } else {
        CoverageState::Partial
    };

    let mut hasher = Hasher::new();
    hasher.update(b"hidden-coupling-v1");
    for r in &resources {
        hasher.update(r.kind.as_str().as_bytes());
        hasher.update(r.key.as_bytes());
    }
    for e in &edges {
        hasher.update(e.a.as_bytes());
        hasher.update(e.b.as_bytes());
        hasher.update(e.resource.key.as_bytes());
    }
    let digest = hasher.finalize().to_hex().to_string();

    let notes = vec![
        "extraction is text-based and conservative; edges are association, never causation"
            .to_string(),
        "coverage is partial: string matching cannot prove a shared runtime resource".to_string(),
    ];

    Ok(HiddenCouplingReport {
        files_scanned,
        resources,
        edges,
        coverage,
        notes,
        digest,
    })
}

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == ".git" || name == "target" || name == "node_modules" {
                    continue;
                }
                collect_files(&path, out);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
}

fn extract(content: &str) -> Vec<(ResourceKind, String)> {
    let mut out = Vec::new();
    // env vars
    for cap in regex_like(content, "process.env.", |s| {
        s.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    }) {
        if !cap.is_empty() {
            out.push((ResourceKind::EnvVar, cap));
        }
    }
    // config keys
    for line in content.lines() {
        if let Some(idx) = line.find("config.get(")
            && let Some(q) = line[idx..].find('"')
        {
            let rest = &line[idx + q + 1..];
            if let Some(end) = rest.find('"') {
                out.push((ResourceKind::ConfigKey, rest[..end].to_string()));
            }
        }
    }
    // queue topics
    for line in content.lines() {
        let lower = line.to_lowercase();
        if lower.contains("topic") || lower.contains("queue") {
            // extract first quoted string
            if let Some(q) = line.find('"') {
                let rest = &line[q + 1..];
                if let Some(end) = rest.find('"') {
                    let val = &rest[..end];
                    if !val.is_empty() {
                        out.push((ResourceKind::QueueTopic, val.to_string()));
                    }
                }
            }
        }
    }
    // db tables
    for keyword in ["FROM ", "INTO ", "UPDATE ", "JOIN "] {
        let upper = content.to_uppercase();
        let mut search = 0;
        while let Some(pos) = upper[search..].find(keyword) {
            let abs = search + pos + keyword.len();
            let rest = &content[abs..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && name.chars().next().unwrap().is_alphabetic() {
                out.push((ResourceKind::DbTable, name));
            }
            search = abs;
            if search >= content.len() {
                break;
            }
        }
    }
    out
}

fn regex_like<F>(content: &str, prefix: &str, take: F) -> Vec<String>
where
    F: Fn(&str) -> String,
{
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(pos) = content[search..].find(prefix) {
        let abs = search + pos + prefix.len();
        let rest = &content[abs..];
        let val = take(rest);
        if !val.is_empty() {
            out.push(val);
        }
        search = abs;
        if search >= content.len() {
            break;
        }
    }
    out
}
