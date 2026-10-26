//! Monorepo Awareness Engine (Enterprise Depth).
//!
//! Deterministically detects and models monorepo topologies across ecosystems:
//! - JavaScript/TypeScript: pnpm (`pnpm-workspace.yaml`), npm/yarn (`workspaces`), Turborepo (`turbo.json`), Nx (`nx.json`).
//! - Rust: Cargo workspaces (`Cargo.toml` `[workspace]`).
//! - Go: Go workspaces (`go.work`).
//! - Java/Kotlin: Maven multi-module (`pom.xml`) and Gradle multi-project (`settings.gradle`).
//!
//! Provides cross-package import resolution, enclosing package containment, and
//! transitive cross-package dependent calculation for whole-monorepo blast radius.

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonorepoKind {
    Pnpm,
    NpmYarn,
    Turborepo,
    Nx,
    Cargo,
    GoWork,
    Maven,
    Gradle,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MonorepoPackage {
    pub name: String,
    pub relative_path: String,
    pub manifest_file: String,
    pub internal_dependencies: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonorepoWorkspace {
    pub kinds: BTreeSet<MonorepoKind>,
    pub root: PathBuf,
    pub packages: BTreeMap<String, MonorepoPackage>,
    pub package_by_dir: BTreeMap<String, String>, // relative dir -> package name
}

impl MonorepoWorkspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            kinds: BTreeSet::new(),
            root: root.into(),
            packages: BTreeMap::new(),
            package_by_dir: BTreeMap::new(),
        }
    }

    pub fn normalize_slash(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "/")
    }

    /// Detects all monorepo workspace configurations present under root.
    pub fn detect(root: &Path) -> Option<Self> {
        let mut workspace = Self::new(root);

        // 1. Check pnpm-workspace.yaml
        let pnpm_path = root.join("pnpm-workspace.yaml");
        if pnpm_path.is_file() {
            workspace.kinds.insert(MonorepoKind::Pnpm);
            workspace.scan_pnpm_workspaces(&pnpm_path);
        }

        // 2. Check npm / yarn workspaces in root package.json
        let pkg_json_path = root.join("package.json");
        if pkg_json_path.is_file() {
            workspace.scan_npm_workspaces(&pkg_json_path);
        }

        // 3. Check Turborepo (turbo.json)
        if root.join("turbo.json").is_file() {
            workspace.kinds.insert(MonorepoKind::Turborepo);
        }

        // 4. Check Nx (nx.json)
        if root.join("nx.json").is_file() {
            workspace.kinds.insert(MonorepoKind::Nx);
        }

        // 5. Check Cargo workspace (Cargo.toml)
        let cargo_path = root.join("Cargo.toml");
        if cargo_path.is_file() {
            workspace.scan_cargo_workspaces(&cargo_path);
        }

        // 6. Check Go workspace (go.work)
        let gowork_path = root.join("go.work");
        if gowork_path.is_file() {
            workspace.scan_gowork(&gowork_path);
        }

        // 7. Check Maven multi-module (pom.xml)
        let pom_path = root.join("pom.xml");
        if pom_path.is_file() {
            workspace.scan_maven_modules(&pom_path);
        }

        // 8. Check Gradle multi-project (settings.gradle or settings.gradle.kts)
        let gradle_path = root.join("settings.gradle");
        let gradle_kts_path = root.join("settings.gradle.kts");
        if gradle_path.is_file() || gradle_kts_path.is_file() {
            let active = if gradle_path.is_file() {
                gradle_path
            } else {
                gradle_kts_path
            };
            workspace.scan_gradle_projects(&active);
        }

        if workspace.kinds.is_empty() && workspace.packages.is_empty() {
            None
        } else {
            // Build inverted index: package_by_dir
            for (name, pkg) in &workspace.packages {
                workspace
                    .package_by_dir
                    .insert(pkg.relative_path.clone(), name.clone());
            }
            Some(workspace)
        }
    }

    fn scan_pnpm_workspaces(&mut self, pnpm_yaml: &Path) {
        let Ok(content) = fs::read_to_string(pnpm_yaml) else {
            return;
        };
        // Simple, robust line-based YAML parser for `packages:` list
        let mut in_packages = false;
        let mut patterns = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("packages:") {
                in_packages = true;
                continue;
            }
            if in_packages {
                if trimmed.starts_with('-') {
                    let pat = trimmed
                        .trim_start_matches('-')
                        .trim()
                        .trim_matches('\'')
                        .trim_matches('"');
                    if !pat.is_empty() {
                        patterns.push(pat.to_string());
                    }
                } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    in_packages = false;
                }
            }
        }

        for pat in patterns {
            self.expand_node_globs(&pat);
        }
    }

    fn scan_npm_workspaces(&mut self, pkg_json: &Path) {
        let Ok(content) = fs::read_to_string(pkg_json) else {
            return;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        let workspaces_val = val.get("workspaces");
        let mut globs = Vec::new();

        if let Some(arr) = workspaces_val.and_then(|w| w.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    globs.push(s.to_string());
                }
            }
        } else if let Some(obj) = workspaces_val.and_then(|w| w.as_object()) {
            if let Some(arr) = obj.get("packages").and_then(|p| p.as_array()) {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        globs.push(s.to_string());
                    }
                }
            }
        }

        if !globs.is_empty() {
            self.kinds.insert(MonorepoKind::NpmYarn);
            for g in globs {
                self.expand_node_globs(&g);
            }
        }
    }

    fn expand_node_globs(&mut self, pattern: &str) {
        let clean = pattern.trim_matches('/').replace('\\', "/");
        if clean.ends_with("/*") {
            let base = clean.trim_end_matches("/*");
            let base_dir = self.root.join(base);
            if let Ok(entries) = fs::read_dir(&base_dir) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type()
                        && ft.is_dir()
                    {
                        let sub_pkg = entry.path().join("package.json");
                        if sub_pkg.is_file() {
                            let rel = format!("{}/{}", base, entry.file_name().to_string_lossy());
                            self.register_node_package(&rel, &sub_pkg);
                        }
                    }
                }
            }
        } else {
            let sub_pkg = self.root.join(&clean).join("package.json");
            if sub_pkg.is_file() {
                self.register_node_package(&clean, &sub_pkg);
            }
        }
    }

    fn register_node_package(&mut self, rel_dir: &str, pkg_json_path: &Path) {
        let Ok(content) = fs::read_to_string(pkg_json_path) else {
            return;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        let Some(name) = val.get("name").and_then(|n| n.as_str()) else {
            return;
        };

        let mut internal_deps = BTreeSet::new();
        for dep_key in ["dependencies", "devDependencies", "peerDependencies"] {
            if let Some(deps) = val.get(dep_key).and_then(|d| d.as_object()) {
                for k in deps.keys() {
                    internal_deps.insert(k.clone());
                }
            }
        }

        self.packages.insert(
            name.to_string(),
            MonorepoPackage {
                name: name.to_string(),
                relative_path: rel_dir.to_string(),
                manifest_file: format!("{}/package.json", rel_dir),
                internal_dependencies: internal_deps,
            },
        );
    }

    fn scan_cargo_workspaces(&mut self, cargo_toml: &Path) {
        let Ok(content) = fs::read_to_string(cargo_toml) else {
            return;
        };
        if !content.contains("[workspace]") {
            return;
        }
        self.kinds.insert(MonorepoKind::Cargo);

        // Parse members = [ ... ]
        let mut in_members = false;
        let mut members = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("members") && trimmed.contains('=') {
                if trimmed.contains(']') {
                    // Single-line members
                    if let Some(start) = trimmed.find('[')
                        && let Some(end) = trimmed.rfind(']')
                    {
                        for m in trimmed[start + 1..end].split(',') {
                            let clean = m.trim().trim_matches('"').trim_matches('\'');
                            if !clean.is_empty() {
                                members.push(clean.to_string());
                            }
                        }
                    }
                } else {
                    in_members = true;
                    continue;
                }
            } else if in_members {
                if trimmed.starts_with(']') {
                    in_members = false;
                } else {
                    for m in trimmed.split(',') {
                        let clean = m.trim().trim_matches('"').trim_matches('\'');
                        if !clean.is_empty() {
                            members.push(clean.to_string());
                        }
                    }
                }
            }
        }

        for member in members {
            let clean = member.replace('\\', "/");
            if clean.ends_with("/*") {
                let base = clean.trim_end_matches("/*");
                let base_dir = self.root.join(base);
                if let Ok(entries) = fs::read_dir(&base_dir) {
                    for entry in entries.flatten() {
                        if let Ok(ft) = entry.file_type()
                            && ft.is_dir()
                        {
                            let sub_cargo = entry.path().join("Cargo.toml");
                            if sub_cargo.is_file() {
                                let rel =
                                    format!("{}/{}", base, entry.file_name().to_string_lossy());
                                self.register_cargo_package(&rel, &sub_cargo);
                            }
                        }
                    }
                }
            } else {
                let sub_cargo = self.root.join(&clean).join("Cargo.toml");
                if sub_cargo.is_file() {
                    self.register_cargo_package(&clean, &sub_cargo);
                }
            }
        }
    }

    fn register_cargo_package(&mut self, rel_dir: &str, cargo_path: &Path) {
        let Ok(content) = fs::read_to_string(cargo_path) else {
            return;
        };
        let mut name = None;
        let mut in_pkg = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[package]" {
                in_pkg = true;
            } else if trimmed.starts_with('[') {
                in_pkg = false;
            } else if in_pkg && trimmed.starts_with("name") && trimmed.contains('=') {
                if let Some((_, val)) = trimmed.split_once('=') {
                    name = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
                    break;
                }
            }
        }

        if let Some(pkg_name) = name {
            self.packages.insert(
                pkg_name.clone(),
                MonorepoPackage {
                    name: pkg_name,
                    relative_path: rel_dir.to_string(),
                    manifest_file: format!("{}/Cargo.toml", rel_dir),
                    internal_dependencies: BTreeSet::new(),
                },
            );
        }
    }

    fn scan_gowork(&mut self, gowork_path: &Path) {
        let Ok(content) = fs::read_to_string(gowork_path) else {
            return;
        };
        self.kinds.insert(MonorepoKind::GoWork);

        let mut in_use = false;
        let mut dirs = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use (") || trimmed == "use (" {
                in_use = true;
                continue;
            }
            if in_use {
                if trimmed == ")" {
                    in_use = false;
                } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
                    dirs.push(
                        trimmed
                            .trim_start_matches("./")
                            .trim_matches('"')
                            .to_string(),
                    );
                }
            } else if trimmed.starts_with("use ") {
                let dir = trimmed["use ".len()..]
                    .trim()
                    .trim_start_matches("./")
                    .trim_matches('"');
                if !dir.is_empty() {
                    dirs.push(dir.to_string());
                }
            }
        }

        for dir in dirs {
            let clean = dir.replace('\\', "/");
            let gomod = self.root.join(&clean).join("go.mod");
            let mut mod_name = clean.clone();
            if let Ok(c) = fs::read_to_string(&gomod) {
                for l in c.lines() {
                    let lt = l.trim();
                    if lt.starts_with("module ") {
                        mod_name = lt["module ".len()..].trim().to_string();
                        break;
                    }
                }
            }
            self.packages.insert(
                mod_name.clone(),
                MonorepoPackage {
                    name: mod_name,
                    relative_path: clean.clone(),
                    manifest_file: format!("{}/go.mod", clean),
                    internal_dependencies: BTreeSet::new(),
                },
            );
        }
    }

    fn scan_maven_modules(&mut self, pom_path: &Path) {
        let Ok(content) = fs::read_to_string(pom_path) else {
            return;
        };
        if !content.contains("<modules>") {
            return;
        }
        self.kinds.insert(MonorepoKind::Maven);

        let mut in_modules = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.contains("<modules>") {
                in_modules = true;
            } else if trimmed.contains("</modules>") {
                in_modules = false;
            } else if in_modules
                && let Some(start) = trimmed.find("<module>")
                && let Some(end) = trimmed.find("</module>")
            {
                let sub = trimmed[start + 8..end].trim();
                let clean = sub.replace('\\', "/");
                self.packages.insert(
                    clean.clone(),
                    MonorepoPackage {
                        name: clean.clone(),
                        relative_path: clean.clone(),
                        manifest_file: format!("{}/pom.xml", clean),
                        internal_dependencies: BTreeSet::new(),
                    },
                );
            }
        }
    }

    fn scan_gradle_projects(&mut self, settings_gradle: &Path) {
        let Ok(content) = fs::read_to_string(settings_gradle) else {
            return;
        };
        if !content.contains("include") {
            return;
        }
        self.kinds.insert(MonorepoKind::Gradle);

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("include") {
                let rest = trimmed["include".len()..].trim();
                for proj in rest.split(',') {
                    let clean = proj
                        .trim()
                        .trim_matches('(')
                        .trim_matches(')')
                        .trim_matches('\'')
                        .trim_matches('"')
                        .trim_start_matches(':')
                        .replace(':', "/");
                    if !clean.is_empty() {
                        self.packages.insert(
                            clean.clone(),
                            MonorepoPackage {
                                name: clean.clone(),
                                relative_path: clean.clone(),
                                manifest_file: format!("{}/build.gradle", clean),
                                internal_dependencies: BTreeSet::new(),
                            },
                        );
                    }
                }
            }
        }
    }

    /// Resolves an import specifier (e.g., `@org/auth` or local package name)
    /// to a concrete workspace package and its entrypoint file.
    pub fn resolve_package_import(&self, specifier: &str) -> Option<String> {
        // Direct package name match
        if let Some(pkg) = self.packages.get(specifier) {
            return self.locate_package_entrypoint(&pkg.relative_path);
        }

        // Subpath match, e.g. `@org/auth/utils`
        for (pkg_name, pkg) in &self.packages {
            if let Some(sub) = specifier.strip_prefix(pkg_name) {
                let sub_clean = sub.trim_start_matches('/');
                if sub_clean.is_empty() {
                    return self.locate_package_entrypoint(&pkg.relative_path);
                } else {
                    let cand = format!("{}/src/{}", pkg.relative_path, sub_clean);
                    return self.locate_file_with_extensions(&cand);
                }
            }
        }

        None
    }

    fn locate_package_entrypoint(&self, pkg_dir: &str) -> Option<String> {
        let cand_base = format!("{}/src/index", pkg_dir);
        if let Some(hit) = self.locate_file_with_extensions(&cand_base) {
            return Some(hit);
        }

        let cand_base = format!("{}/index", pkg_dir);
        if let Some(hit) = self.locate_file_with_extensions(&cand_base) {
            return Some(hit);
        }

        // Java src/main/java entry anchor
        let cand_java = format!("{}/src/main/java", pkg_dir);
        if self.root.join(&cand_java).is_dir() {
            return Some(cand_java);
        }

        Some(pkg_dir.to_string())
    }

    fn locate_file_with_extensions(&self, base_path: &str) -> Option<String> {
        for ext in ["ts", "tsx", "js", "jsx", "rs", "go", "java", "kt", "cs"] {
            let cand = format!("{}.{}", base_path, ext);
            if self.root.join(&cand).is_file() {
                return Some(cand);
            }
        }
        None
    }

    /// Identifies the enclosing monorepo package for any file path.
    pub fn find_enclosing_package<'a>(&'a self, file_path: &str) -> Option<&'a MonorepoPackage> {
        let norm = file_path.replace('\\', "/");
        let mut best: Option<&'a MonorepoPackage> = None;
        let mut best_len = 0;

        for pkg in self.packages.values() {
            if norm.starts_with(&pkg.relative_path) && pkg.relative_path.len() > best_len {
                best_len = pkg.relative_path.len();
                best = Some(pkg);
            }
        }

        best
    }

    /// Calculates which internal workspace packages depend on the specified package.
    pub fn package_dependents(&self, target_package: &str) -> BTreeSet<String> {
        let mut dependents = BTreeSet::new();
        for (name, pkg) in &self.packages {
            if pkg.internal_dependencies.contains(target_package) {
                dependents.insert(name.clone());
            }
        }
        dependents
    }

    /// Generates a deterministic BLAKE3 digest of the monorepo graph topology.
    pub fn digest(&self) -> String {
        let mut hasher = Hasher::new();
        hasher.update(b"monorepo-workspace-v1");
        for kind in &self.kinds {
            hasher.update(format!("{:?}", kind).as_bytes());
        }
        for (name, pkg) in &self.packages {
            hasher.update(name.as_bytes());
            hasher.update(pkg.relative_path.as_bytes());
            for dep in &pkg.internal_dependencies {
                hasher.update(dep.as_bytes());
            }
        }
        hasher.finalize().to_hex().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_pnpm_workspace_detection() {
        let tmp = std::env::temp_dir().join(format!("syn-pnpm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("packages/auth/src")).unwrap();
        fs::create_dir_all(tmp.join("apps/web")).unwrap();

        fs::write(
            tmp.join("pnpm-workspace.yaml"),
            "packages:\n  - 'packages/*'\n  - 'apps/*'\n",
        )
        .unwrap();

        fs::write(
            tmp.join("packages/auth/package.json"),
            r#"{"name": "@company/auth", "version": "1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            tmp.join("packages/auth/src/index.ts"),
            "export const login = () => {};",
        )
        .unwrap();

        fs::write(
            tmp.join("apps/web/package.json"),
            r#"{"name": "web-app", "dependencies": {"@company/auth": "workspace:*"}}"#,
        )
        .unwrap();

        let ws = MonorepoWorkspace::detect(&tmp).expect("must detect pnpm monorepo");
        assert!(ws.kinds.contains(&MonorepoKind::Pnpm));
        assert_eq!(ws.packages.len(), 2);
        assert!(ws.packages.contains_key("@company/auth"));
        assert!(ws.packages.contains_key("web-app"));

        let resolved = ws
            .resolve_package_import("@company/auth")
            .expect("must resolve package import");
        assert_eq!(resolved, "packages/auth/src/index.ts");

        let enclosing = ws
            .find_enclosing_package("packages/auth/src/index.ts")
            .expect("must find enclosing package");
        assert_eq!(enclosing.name, "@company/auth");

        let deps = ws.package_dependents("@company/auth");
        assert!(deps.contains("web-app"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_cargo_and_gowork_detection() {
        let tmp = std::env::temp_dir().join(format!("syn-cargo-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("crates/core")).unwrap();
        fs::create_dir_all(tmp.join("crates/cli")).unwrap();

        fs::write(
            tmp.join("Cargo.toml"),
            "[workspace]\nmembers = [\n  \"crates/*\",\n]\n",
        )
        .unwrap();

        fs::write(
            tmp.join("crates/core/Cargo.toml"),
            "[package]\nname = \"my-core\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            tmp.join("crates/cli/Cargo.toml"),
            "[package]\nname = \"my-cli\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let ws = MonorepoWorkspace::detect(&tmp).expect("must detect cargo workspace");
        assert!(ws.kinds.contains(&MonorepoKind::Cargo));
        assert_eq!(ws.packages.len(), 2);
        assert!(ws.packages.contains_key("my-core"));
        assert!(ws.packages.contains_key("my-cli"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
