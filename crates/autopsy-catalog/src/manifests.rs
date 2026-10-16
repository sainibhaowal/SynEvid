//! Dependency manifests: npm, pip, Go modules, Cargo (`R29``R37`).
//!
//! GitHub's dependency graph is derived from exactly these files, so reading them is
//! reading the same evidence GitHub uses — without a network call, a token, or an API
//! dependency (`FR-026`).
//!
//! # A manifest declares a *requirement*, never a version
//!
//! `^18.0.0`, `>=2.31,<3`, `v1.2.3`, and `"1"` are ranges. What is *installed* is the
//! resolver's answer and lives in a lockfile, which this crate does not read. The field is
//! therefore named [`ManifestDependency::requirement`], and the report says that a
//! lockfile was not consulted — a report that rendered `^1.2.3` as a version would be
//! wrong in a way that matters the next time someone asks which services ship the
//! vulnerable one.
//!
//! # Conditional dependencies degrade coverage
//!
//! A dependency guarded by a PEP 508 marker (`; python_version >= "3.9"`), a Cargo
//! feature, or an npm `optionalDependencies` entry may or may not be installed. Those are
//! recorded and they degrade the manifest's coverage, because \"this repository depends on
//! X\" is not true unconditionally in those cases.
//!
//! # What is not read, and is counted rather than forgotten
//!
//! * **Transitive dependencies.** No lockfile is parsed, so only direct declarations are
//!   visible. The report states this; it is the single largest gap between this data and
//!   GitHub's graph.
//! * **Other ecosystems.** `pom.xml`, `Gemfile`, `composer.json`, `Pipfile`, `pyproject.toml`
//!   and the rest are not in the allowlist in [`mod@crate::discover`], so they are not even
//!   opened. The report lists the manifest names it looked for, so a reader can see what
//!   was out of scope instead of inferring from silence.
//! * **Directive lines** in `requirements.txt` (`-r`, `-e`, `--index-url`) point at another
//!   file or another index; they are recorded by name and degrade coverage, because the
//!   dependencies they bring in are genuinely absent from the result.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::discover::CatalogError;

/// The manifest formats this crate models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Npm,
    Pip,
    Go,
    Cargo,
}

impl DependencySource {
    /// Every variant, for the schema drift guard. A new source that is not added here fails
    /// the conformance test rather than reaching a reader undeclared.
    pub fn all() -> &'static [DependencySource] {
        &[
            Self::Registry,
            Self::Path,
            Self::Git,
            Self::Url,
            Self::Workspace,
        ]
    }
}

impl DependencyScope {
    pub fn all() -> &'static [DependencyScope] {
        &[
            Self::Runtime,
            Self::Development,
            Self::Build,
            Self::Peer,
            Self::Optional,
        ]
    }
}

impl Ecosystem {
    pub fn all() -> &'static [Ecosystem] {
        &[Self::Npm, Self::Pip, Self::Go, Self::Cargo]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pip => "pip",
            Self::Go => "go",
            Self::Cargo => "cargo",
        }
    }

    /// The file names each ecosystem is read from.
    pub fn manifest_names(self) -> &'static [&'static str] {
        match self {
            Self::Npm => &["package.json"],
            Self::Pip => &["requirements.txt"],
            Self::Go => &["go.mod"],
            Self::Cargo => &["Cargo.toml"],
        }
    }
}

/// Where a dependency comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencySource {
    /// A public or private registry.
    Registry,
    /// Another directory in this workspace. A monorepo edge, not a supply-chain edge.
    Path,
    /// A git remote.
    Git,
    /// A direct URL (a tarball, a wheel, a PEP 508 direct reference). Recorded as its own
    /// kind because it is neither a registry lookup nor a git remote, and the versions a
    /// registry would have resolved do not exist here.
    Url,
    /// Declared in `[workspace.dependencies]` for **members** to inherit. It is available
    /// to be consumed; it is not consumed by the manifest it appears in, and conflating
    /// the two would attribute every workspace-wide dependency to the root package.
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyScope {
    Runtime,
    Development,
    Build,
    Peer,
    Optional,
}

impl DependencyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Development => "development",
            Self::Build => "build",
            Self::Peer => "peer",
            Self::Optional => "optional",
        }
    }
}

/// One declared dependency.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ManifestDependency {
    pub ecosystem: Ecosystem,
    /// The package name as written. For Go this is a module path, which is not a package
    /// name, and the distinction is the language's, not this crate's.
    pub package: String,
    /// The **requirement** as written, or `None` when the manifest does not state one.
    pub requirement: Option<String>,
    pub scope: DependencyScope,
    pub source: DependencySource,
    /// Go marks these (`// indirect`); nothing else here can tell.
    pub indirect: bool,
    /// An npm `optionalDependencies` entry, a Cargo `optional = true`, or a pip marker.
    pub conditional: bool,
    /// The declaring file, POSIX-relative to the catalog root.
    pub manifest_path: String,
}

/// One manifest file's worth of dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub ecosystem: Ecosystem,
    /// The manifest's own name: `"name"` for npm, `module` for Go, `[package] name` for
    /// Cargo. `None` where the format has no such field (`requirements.txt`).
    pub subject: Option<String>,
    pub dependencies: Vec<ManifestDependency>,
    /// Directives and sections that were seen and not read, by name.
    pub unmodelled: BTreeMap<String, usize>,
    pub coverage: autopsy_domain::CoverageState,
}

/// Whether a file name belongs to an ecosystem this crate models.
pub fn ecosystem_for(path: &str) -> Option<Ecosystem> {
    let name = path.rsplit('/').next().unwrap_or(path);
    Ecosystem::all()
        .iter()
        .copied()
        .find(|ecosystem| ecosystem.manifest_names().contains(&name))
}

/// Parses one manifest file, dispatching on its name.
pub fn parse_manifest(path: &str, content: &str) -> Result<ManifestFile, CatalogError> {
    let Some(ecosystem) = ecosystem_for(path) else {
        return Err(CatalogError::Malformed {
            path: path.to_string(),
            format: "manifest",
            message: "no ecosystem claims this file name".to_string(),
        });
    };
    match ecosystem {
        Ecosystem::Npm => parse_package_json(path, content),
        Ecosystem::Pip => parse_requirements_txt(path, content),
        Ecosystem::Go => parse_go_mod(path, content),
        Ecosystem::Cargo => parse_cargo_toml(path, content),
    }
}

fn malformed(path: &str, format: &'static str, message: impl Into<String>) -> CatalogError {
    CatalogError::Malformed {
        path: path.to_string(),
        format,
        message: message.into(),
    }
}

/// Classifies a declared version string by where it points, which is all this crate can
/// know without resolving anything.
fn classify_source(requirement: &str) -> DependencySource {
    let trimmed = requirement.trim();
    if trimmed.starts_with("workspace:") {
        return DependencySource::Workspace;
    }
    for prefix in ["file:", "link:", "./", "../", "/", "path:"] {
        if trimmed.starts_with(prefix) {
            return DependencySource::Path;
        }
    }
    for prefix in ["git+", "git:", "github:", "gitlab:", "bitbucket:", "ssh:"] {
        if trimmed.starts_with(prefix) {
            return DependencySource::Git;
        }
    }
    for prefix in ["http://", "https://"] {
        if trimmed.starts_with(prefix) {
            return DependencySource::Url;
        }
    }
    DependencySource::Registry
}

// ---------------------------------------------------------------------------
// npm
// ---------------------------------------------------------------------------

/// `"^1.2.3"`, `"1.x"`, `"*"`, `"latest"`, and a bare `"1"` are all requirements. Kept
/// verbatim: normalising them into a range object would be inventing syntax the format
/// does not promise, and this crate has no resolver to check the reading against.
fn npm_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        // An alias (`"npm:other@^1"`) or a URL keeps its text. A number or boolean is
        // malformed, and `None` lets the caller count it rather than coerce it.
        _ => None,
    }
}

fn parse_package_json(path: &str, content: &str) -> Result<ManifestFile, CatalogError> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|error| malformed(path, "JSON", error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| malformed(path, "JSON", "the document is not an object"))?;

    let mut dependencies = Vec::new();
    let mut unmodelled: BTreeMap<String, usize> = BTreeMap::new();

    let sections: &[(&str, DependencyScope, bool)] = &[
        ("dependencies", DependencyScope::Runtime, false),
        ("devDependencies", DependencyScope::Development, false),
        ("peerDependencies", DependencyScope::Peer, false),
        ("optionalDependencies", DependencyScope::Optional, true),
    ];
    for (section, scope, conditional) in sections {
        let Some(entry) = object.get(*section) else {
            continue;
        };
        let Some(table) = entry.as_object() else {
            return Err(malformed(
                path,
                "package.json",
                format!("`{section}` is not an object"),
            ));
        };
        for (package, requirement) in table {
            let Some(requirement) = npm_string(requirement) else {
                *unmodelled
                    .entry(format!("{section}[{package}] is not a string"))
                    .or_insert(0) += 1;
                continue;
            };
            dependencies.push(ManifestDependency {
                ecosystem: Ecosystem::Npm,
                package: package.clone(),
                source: classify_source(&requirement),
                requirement: Some(requirement),
                scope: *scope,
                indirect: false,
                conditional: *conditional,
                manifest_path: path.to_string(),
            });
        }
    }

    // `bundledDependencies`/`bundleDependencies` is an array of names with no requirement:
    // a real dependency whose version is not stated here.
    for section in ["bundledDependencies", "bundleDependencies"] {
        let Some(entry) = object.get(section) else {
            continue;
        };
        let Some(names) = entry.as_array() else {
            *unmodelled
                .entry(format!("`{section}` is not an array"))
                .or_insert(0) += 1;
            continue;
        };
        for name in names.iter().filter_map(|value| value.as_str()) {
            dependencies.push(ManifestDependency {
                ecosystem: Ecosystem::Npm,
                package: name.to_string(),
                requirement: None,
                scope: DependencyScope::Runtime,
                source: DependencySource::Registry,
                indirect: false,
                conditional: false,
                manifest_path: path.to_string(),
            });
        }
    }

    // Sections that are read for linkage but are not dependencies.
    for section in [
        "workspaces",
        "peerDependenciesMeta",
        "resolutions",
        "overrides",
    ] {
        // `workspaces` describes the monorepo layout; `resolutions`/`overrides` rewrite
        // someone else's dependency; `peerDependenciesMeta` makes a peer optional. All of
        // them change which packages are installed and none of them is read here.
        if object.contains_key(section) {
            *unmodelled.entry(section.to_string()).or_insert(0) += 1;
        }
    }
    // Every top-level key that is neither a dependency section nor a field this crate
    // read: `name`, `version`, `private`, `scripts` and the rest are metadata.
    for key in object.keys() {
        let known = sections.iter().any(|(section, _, _)| section == key)
            || matches!(
                key.as_str(),
                "name" | "version" | "private" | "bundledDependencies" | "bundleDependencies"
            );
        if !known && !unmodelled.contains_key(key) {
            // Recorded by name, not as a generic \"other\": the point of the list is that a
            // reader can see *which* field was skipped.
            *unmodelled.entry(format!("top-level `{key}`")).or_insert(0) += 1;
        }
    }

    dependencies.sort();
    let mut coverage = autopsy_domain::CoverageState::Verified;
    if !unmodelled.is_empty() || dependencies.iter().any(|dep| dep.conditional) {
        coverage = autopsy_domain::CoverageState::Partial;
    }
    Ok(ManifestFile {
        path: path.to_string(),
        ecosystem: Ecosystem::Npm,
        subject: object
            .get("name")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        dependencies,
        unmodelled,
        coverage,
    })
}

// ---------------------------------------------------------------------------
// pip
// ---------------------------------------------------------------------------

/// Strips a trailing comment. A `#` starts a comment only at the start of a line or after
/// whitespace — `https://example.com/x#fragment` is a URL and pip reads it as one.
fn strip_pip_comment(line: &str) -> &str {
    let mut previous_was_space = true;
    for (index, character) in line.char_indices() {
        if character == '#' && previous_was_space {
            return &line[..index];
        }
        previous_was_space = character.is_whitespace();
    }
    line
}

fn parse_requirements_txt(path: &str, content: &str) -> Result<ManifestFile, CatalogError> {
    let mut dependencies = Vec::new();
    let mut unmodelled: BTreeMap<String, usize> = BTreeMap::new();

    for raw_line in content.lines() {
        let line = strip_pip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('-') {
            // A directive, not a requirement. Every one of them either points at content
            // this crate will not read or changes the resolver's inputs.
            let directive = line
                .split_whitespace()
                .next()
                .unwrap_or(line)
                .split('=')
                .next()
                .unwrap_or(line)
                .to_string();
            *unmodelled
                .entry(format!("directive `{directive}`"))
                .or_insert(0) += 1;
            continue;
        }
        if let Some(continuation) = line.strip_prefix('\\') {
            *unmodelled
                .entry(format!("continuation `{}`", continuation.trim()))
                .or_insert(0) += 1;
            continue;
        }

        // `name[extras]specifier ; marker`
        let (requirement_part, marker) = match line.split_once(';') {
            Some((requirement, marker)) => (requirement.trim(), Some(marker.trim())),
            None => (line, None),
        };
        let name_end = requirement_part
            .find(['=', '<', '>', '~', '!', '[', ' ', '@'])
            .unwrap_or(requirement_part.len());
        let name = requirement_part[..name_end].trim();
        if name.is_empty() {
            *unmodelled
                .entry(format!("unparsable requirement `{line}`"))
                .or_insert(0) += 1;
            continue;
        }
        let rest = requirement_part[name_end..].trim();
        let extras = if rest.starts_with('[') {
            rest.find(']').map(|end| &rest[..=end]).unwrap_or("")
        } else {
            ""
        };
        let requirement = rest
            .get(extras.len()..)
            .map(str::trim)
            // `name @ https://…` is a PEP 508 direct reference: the `@` introduces the
            // location and is not part of it.
            .map(|text| text.strip_prefix('@').map(str::trim).unwrap_or(text))
            .filter(|text| !text.is_empty())
            .map(str::to_string);
        let package = format!("{name}{extras}");

        if let Some(marker) = marker {
            // The dependency applies only on platforms satisfying the marker. Recorded, not
            // evaluated: evaluating it would answer for *this* machine, and the question
            // the report is used for is about every machine.
            *unmodelled
                .entry("environment marker".to_string())
                .or_insert(0) += 1;
            let _ = marker;
        }

        dependencies.push(ManifestDependency {
            ecosystem: Ecosystem::Pip,
            source: match requirement.as_deref() {
                Some(requirement) => classify_source(requirement),
                None => DependencySource::Registry,
            },
            package,
            requirement: requirement.clone(),
            scope: DependencyScope::Runtime,
            indirect: false,
            conditional: marker.is_some(),
            manifest_path: path.to_string(),
        });
    }

    dependencies.sort();
    let mut coverage = autopsy_domain::CoverageState::Verified;
    if !unmodelled.is_empty() {
        // Every `-r other.txt` is a file of requirements this crate never opened, and every
        // marker is a dependency that may not be installed.
        coverage = autopsy_domain::CoverageState::Partial;
    }
    Ok(ManifestFile {
        path: path.to_string(),
        ecosystem: Ecosystem::Pip,
        subject: None,
        dependencies,
        unmodelled,
        coverage,
    })
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

fn parse_go_mod(path: &str, content: &str) -> Result<ManifestFile, CatalogError> {
    let mut dependencies = Vec::new();
    let mut unmodelled: BTreeMap<String, usize> = BTreeMap::new();
    let mut subject = None;
    let mut in_require_block = false;
    // `replace`/`exclude` are directives that rewrite the module graph. They are recorded
    // by the module they name and they degrade coverage: a report that lists a module as a
    // dependency while a `replace` redirects it is misleading.
    let directive_targets: BTreeMap<String, usize> = BTreeMap::new();
    let replaced_modules: Vec<String> = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("module ") {
            subject = Some(rest.trim().trim_matches('"').to_string());
            continue;
        }
        if line.starts_with("go ") || line.starts_with("toolchain ") {
            continue;
        }

        let push_require = |body: &str,
                            dependencies: &mut Vec<ManifestDependency>,
                            unmodelled: &mut BTreeMap<String, usize>| {
            let commented = body.split_once("//");
            let (statement, comment) = match commented {
                Some((statement, comment)) => (statement.trim(), Some(comment.trim())),
                None => (body.trim(), None),
            };
            let mut fields = statement.split_whitespace();
            let (Some(module), Some(version)) = (fields.next(), fields.next()) else {
                *unmodelled
                    .entry(format!("unparsable require `{statement}`"))
                    .or_insert(0) += 1;
                return;
            };
            let indirect = comment.is_some_and(|comment| comment.starts_with("indirect"));
            if let Some(comment) = comment
                && !indirect
            {
                *unmodelled
                    .entry(format!("require comment `{comment}`"))
                    .or_insert(0) += 1;
            }
            dependencies.push(ManifestDependency {
                ecosystem: Ecosystem::Go,
                package: module.to_string(),
                requirement: Some(version.to_string()),
                scope: DependencyScope::Runtime,
                source: DependencySource::Registry,
                indirect,
                conditional: false,
                manifest_path: path.to_string(),
            });
        };

        match line {
            _ if line.starts_with("require (") => in_require_block = true,
            ")" => in_require_block = false,
            _ if line.starts_with("require ") => {
                push_require(
                    &line["require ".len()..],
                    &mut dependencies,
                    &mut unmodelled,
                );
            }
            _ if in_require_block => {
                push_require(line, &mut dependencies, &mut unmodelled);
            }
            _ if line.starts_with("replace") || line.starts_with("exclude") => {
                let keyword = if line.starts_with("replace") {
                    "replace"
                } else {
                    "exclude"
                };
                let body = line[keyword.len()..].trim().trim_start_matches('(');
                let fields: Vec<&str> = body.split_whitespace().collect();
                match fields.iter().position(|field| *field == "=>") {
                    Some(arrow) => {
                        let from = fields.get(arrow.wrapping_sub(1)).copied().unwrap_or("?");
                        let to = fields.get(arrow + 1).copied().unwrap_or("?");
                        let target =
                            if to.starts_with("./") || to.starts_with("../") || to.starts_with('/')
                            {
                                "a local directory"
                            } else {
                                "another module"
                            };
                        // Which module is swapped **and** what it is swapped for: a local
                        // replacement is a monorepo edit, a remote one is a fork, and the
                        // declared version is no longer what is built.
                        *unmodelled
                            .entry(format!("`replace` {from} => {target}"))
                            .or_insert(0) += 1;
                    }
                    None => {
                        let module = fields.first().copied().unwrap_or("?");
                        *unmodelled
                            .entry(format!("`{keyword}` {module}"))
                            .or_insert(0) += 1;
                    }
                }
            }
            _ => {
                *unmodelled
                    .entry(format!(
                        "directive `{}`",
                        line.split_whitespace().next().unwrap_or(line)
                    ))
                    .or_insert(0) += 1;
            }
        }
    }
    let _ = (in_require_block, directive_targets, replaced_modules);

    dependencies.sort();
    let mut coverage = autopsy_domain::CoverageState::Verified;
    if !unmodelled.is_empty() {
        coverage = autopsy_domain::CoverageState::Partial;
    }
    Ok(ManifestFile {
        path: path.to_string(),
        ecosystem: Ecosystem::Go,
        subject,
        dependencies,
        unmodelled,
        coverage,
    })
}

// ---------------------------------------------------------------------------
// Cargo
// ---------------------------------------------------------------------------

fn toml_string(value: &toml::Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

fn parse_cargo_toml(path: &str, content: &str) -> Result<ManifestFile, CatalogError> {
    let value: toml::Value =
        toml::from_str(content).map_err(|error| malformed(path, "TOML", error.to_string()))?;
    let table = value
        .as_table()
        .ok_or_else(|| malformed(path, "TOML", "the document is not a table"))?;

    let mut dependencies = Vec::new();
    let mut unmodelled: BTreeMap<String, usize> = BTreeMap::new();

    let read_section = |name: &str,
                        scope: DependencyScope,
                        source_default: DependencySource,
                        dependencies: &mut Vec<ManifestDependency>,
                        unmodelled: &mut BTreeMap<String, usize>| {
        let Some(section) = value.get(name).and_then(toml::Value::as_table) else {
            return;
        };
        for (package, specification) in section {
            let (requirement, source, conditional) = match specification {
                toml::Value::String(requirement) => (
                    Some(requirement.clone()),
                    classify_source(requirement),
                    false,
                ),
                toml::Value::Table(details) => {
                    let requirement = details.get("version").and_then(toml_string);
                    let source = if let Some(requirement_text) = requirement.as_deref() {
                        classify_source(requirement_text)
                    } else if details.contains_key("workspace") {
                        DependencySource::Workspace
                    } else if details.contains_key("path") {
                        DependencySource::Path
                    } else if details.contains_key("git") {
                        DependencySource::Git
                    } else {
                        source_default
                    };
                    // `optional = true` means a feature decides whether the crate is
                    // compiled in at all.
                    let conditional = details
                        .get("optional")
                        .and_then(toml::Value::as_bool)
                        .unwrap_or(false);
                    if details.contains_key("features") {
                        // Named features activate other permits; the crate is still a
                        // dependency, but *which* of its optional parts are built is not
                        // visible here.
                        *unmodelled
                            .entry(format!("{name}[{package}].features"))
                            .or_insert(0) += 1;
                    }
                    (requirement, source, conditional)
                }
                _ => {
                    *unmodelled
                        .entry(format!("{name}[{package}] is neither a string nor a table"))
                        .or_insert(0) += 1;
                    continue;
                }
            };
            dependencies.push(ManifestDependency {
                ecosystem: Ecosystem::Cargo,
                package: package.clone(),
                requirement,
                scope,
                source,
                indirect: false,
                conditional,
                manifest_path: path.to_string(),
            });
        }
    };

    read_section(
        "dependencies",
        DependencyScope::Runtime,
        DependencySource::Registry,
        &mut dependencies,
        &mut unmodelled,
    );
    read_section(
        "dev-dependencies",
        DependencyScope::Development,
        DependencySource::Registry,
        &mut dependencies,
        &mut unmodelled,
    );
    read_section(
        "build-dependencies",
        DependencyScope::Build,
        DependencySource::Registry,
        &mut dependencies,
        &mut unmodelled,
    );
    read_section(
        "workspace.dependencies",
        DependencyScope::Runtime,
        DependencySource::Workspace,
        &mut dependencies,
        &mut unmodelled,
    );

    // Target-specific dependencies: `[target.'cfg(windows)'.dependencies]`. They exist only
    // on some targets, so they are conditional, and the target is recorded.
    if let Some(targets) = value.get("target").and_then(toml::Value::as_table) {
        for (target, _) in targets {
            *unmodelled
                .entry(format!("target-specific dependencies for `{target}`"))
                .or_insert(0) += 1;
        }
    }
    for section in ["features", "patch", "replace", "profile"] {
        if table.contains_key(section) {
            *unmodelled.entry(section.to_string()).or_insert(0) += 1;
        }
    }

    dependencies.sort();
    let mut coverage = autopsy_domain::CoverageState::Verified;
    if !unmodelled.is_empty() || dependencies.iter().any(|dep| dep.conditional) {
        coverage = autopsy_domain::CoverageState::Partial;
    }
    Ok(ManifestFile {
        path: path.to_string(),
        ecosystem: Ecosystem::Cargo,
        subject: value
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml_string),
        dependencies,
        unmodelled,
        coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE_JSON: &str = r#"{
      "name": "checkout-web",
      "version": "1.2.3",
      "private": true,
      "dependencies": { "react": "^18.2.0", "left-pad": "1.3.0" },
      "devDependencies": { "vitest": "^1.0.0" },
      "optionalDependencies": { "fsevents": "^2.3.0" },
      "peerDependencies": { "react-dom": "^18.0.0" },
      "workspaces": ["packages/*"]
    }"#;

    #[test]
    fn package_json_sections_map_to_scopes() {
        let manifest = parse_manifest("web/package.json", PACKAGE_JSON).expect("parses");
        assert_eq!(manifest.subject.as_deref(), Some("checkout-web"));
        assert_eq!(manifest.dependencies.len(), 5);
        let by_name = |name: &str| {
            manifest
                .dependencies
                .iter()
                .find(|dependency| dependency.package == name)
                .expect("present")
        };
        assert_eq!(by_name("react").scope, DependencyScope::Runtime);
        assert_eq!(by_name("vitest").scope, DependencyScope::Development);
        assert_eq!(by_name("react-dom").scope, DependencyScope::Peer);
        assert_eq!(by_name("fsevents").scope, DependencyScope::Optional);
        assert!(
            by_name("fsevents").conditional,
            "an optional dependency may not be installed"
        );
        assert_eq!(by_name("react").requirement.as_deref(), Some("^18.2.0"));
        assert_eq!(
            manifest.coverage,
            autopsy_domain::CoverageState::Partial,
            "an optional dependency and an unread `workspaces` field both degrade it"
        );
        assert!(
            manifest.unmodelled.keys().any(|key| key == "workspaces"),
            "{:?}",
            manifest.unmodelled
        );
    }

    #[test]
    fn a_requirements_file_records_directives_and_markers_instead_of_guessing() {
        let text = "# a comment\nrequests==2.31.0\nflask[async]>=3.0 ; python_version >= \"3.9\"\n-r other.txt\n--index-url https://example.com/simple\n\nnumpy>=1.26  # pinned by the platform team\n";
        let manifest = parse_manifest("requirements.txt", text).expect("parses");
        let names: Vec<&str> = manifest
            .dependencies
            .iter()
            .map(|dependency| dependency.package.as_str())
            .collect();
        assert_eq!(names, ["flask[async]", "numpy", "requests"]);
        assert_eq!(
            manifest.dependencies[2].requirement.as_deref(),
            Some("==2.31.0")
        );
        assert_eq!(
            manifest.dependencies[0].requirement.as_deref(),
            Some(">=3.0")
        );
        assert!(
            manifest.dependencies[0].conditional,
            "the marker is recorded"
        );
        assert!(
            manifest
                .unmodelled
                .keys()
                .any(|key| key.contains("directive `-r`")),
            "{:?}",
            manifest.unmodelled
        );
        assert!(
            manifest.unmodelled.contains_key("environment marker"),
            "a marker is a fact about the dependency, and it is not evaluated here: {:?}",
            manifest.unmodelled
        );
    }

    #[test]
    fn a_url_with_a_hash_is_not_a_comment() {
        let text = "thing @ https://example.com/thing.zip#sha256=abc\n# a real comment\n";
        let manifest = parse_manifest("requirements.txt", text).expect("parses");
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(
            manifest.dependencies[0].requirement.as_deref(),
            Some("https://example.com/thing.zip#sha256=abc")
        );
        assert_eq!(
            manifest.dependencies[0].source,
            DependencySource::Url,
            "a direct URL is not a registry lookup and not a git remote, and the `@` is not \
             part of the location"
        );
    }

    #[test]
    fn go_indirect_and_replace_are_both_visible() {
        let text = "module github.com/acme/checkout\n\ngo 1.22\n\nrequire (\n\tgithub.com/foo/bar v1.2.3\n\tgithub.com/baz/qux v0.1.0 // indirect\n)\n\nrequire github.com/solo/one v1.0.0\n\nreplace github.com/foo/bar => ../bar\n";
        let manifest = parse_manifest("go.mod", text).expect("parses");
        assert_eq!(
            manifest.subject.as_deref(),
            Some("github.com/acme/checkout"),
            "the module path is the repository slug GitHub would use"
        );
        let bar = manifest
            .dependencies
            .iter()
            .find(|dependency| dependency.package == "github.com/foo/bar")
            .expect("present");
        assert!(!bar.indirect);
        assert_eq!(bar.requirement.as_deref(), Some("v1.2.3"));
        let qux = manifest
            .dependencies
            .iter()
            .find(|dependency| dependency.package == "github.com/baz/qux")
            .expect("present");
        assert!(
            qux.indirect,
            "`// indirect` is the one indirect marker in the format"
        );
        assert!(
            manifest
                .unmodelled
                .keys()
                .any(|key| key.contains("`replace` github.com/foo/bar => a local directory")),
            "a replace redirects a module and must not be silent: {:?}",
            manifest.unmodelled
        );
    }

    #[test]
    fn a_cargo_manifest_separates_scopes_and_flags_optionals() {
        let text = "[package]\nname = \"autopsy-demo\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nfancy = { version = \"2\", optional = true }\nlocal = { path = \"../local\" }\ninherited = { workspace = true }\n\n[dev-dependencies]\ntempfile = \"3\"\n\n[build-dependencies]\ncc = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n\n[features]\ndefault = [\"fancy\"]\n";
        let manifest = parse_manifest("Cargo.toml", text).expect("parses");
        assert_eq!(manifest.subject.as_deref(), Some("autopsy-demo"));
        let by_name = |name: &str| {
            manifest
                .dependencies
                .iter()
                .find(|dependency| dependency.package == name)
                .expect("present")
        };
        assert_eq!(by_name("serde").scope, DependencyScope::Runtime);
        assert_eq!(by_name("tempfile").scope, DependencyScope::Development);
        assert_eq!(by_name("cc").scope, DependencyScope::Build);
        assert!(by_name("fancy").conditional);
        assert_eq!(by_name("local").source, DependencySource::Path);
        assert_eq!(by_name("inherited").source, DependencySource::Workspace);
        assert!(
            manifest
                .unmodelled
                .keys()
                .any(|key| key.contains("target-specific")),
            "{:?}",
            manifest.unmodelled
        );
        assert!(manifest.unmodelled.keys().any(|key| key == "features"));
    }

    #[test]
    fn malformed_input_is_an_error_for_this_file_only() {
        assert!(parse_manifest("package.json", "{ not json").is_err());
        assert!(parse_manifest("Cargo.toml", "not = = toml").is_err());
        assert!(parse_manifest("README.md", "x").is_err());
    }

    #[test]
    fn every_ecosystem_claims_exactly_the_names_it_models() {
        assert_eq!(ecosystem_for("a/b/package.json"), Some(Ecosystem::Npm));
        assert_eq!(ecosystem_for("requirements.txt"), Some(Ecosystem::Pip));
        assert_eq!(ecosystem_for("go.mod"), Some(Ecosystem::Go));
        assert_eq!(ecosystem_for("crates/x/Cargo.toml"), Some(Ecosystem::Cargo));
        assert_eq!(ecosystem_for("pom.xml"), None);
        assert_eq!(ecosystem_for("pyproject.toml"), None);
        assert_eq!(Ecosystem::all().len(), 4);
    }
}
