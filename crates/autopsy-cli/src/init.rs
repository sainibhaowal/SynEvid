//! `autopsy init` — Zero-config repository onboarding (Tier 3.6).
//!
//! Deterministically inspects the target repository's directory hierarchy and
//! source extensions (`.ts`/`.tsx`/`.js`/`.jsx`, `.py`, `.go`, `.rs`), generates a
//! tailored `autopsy.toml` and `.autopsy/invariants.yml` (with architectural layer
//! rules when recognizable layers exist), and emits a canonical JSON or text
//! receipt with a BLAKE3 `result_digest`.
//!
//! Invariants:
//! - Zero wall-clock timestamps; `BTreeSet` / `BTreeMap` everywhere.
//! - Non-destructive by default: existing `autopsy.toml` or `.autopsy/invariants.yml`
//!   files are preserved (`skipped`) unless `--force` is passed.

use crate::{Cli, CliError, EXIT_PASS, OutputFormat};
use blake3::Hasher;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitReport {
    pub command: &'static str,
    pub detected_languages: Vec<String>,
    pub detected_layers: Vec<String>,
    pub files_status: BTreeMap<String, String>,
    pub next_steps: Vec<String>,
    pub result_digest: String,
}

const KNOWN_LAYER_ORDER: &[(&str, &[&str])] = &[
    (
        "presentation",
        &[
            "src/api",
            "src/controllers",
            "src/routes",
            "src/handlers",
            "src/ui",
            "src/web",
            "src/cli",
            "api",
            "controllers",
            "handlers",
            "Controllers",
            "apps",
        ],
    ),
    (
        "application",
        &[
            "src/services",
            "src/usecases",
            "src/application",
            "src/app",
            "services",
            "usecases",
            "Services",
            "packages",
        ],
    ),
    (
        "domain",
        &[
            "src/domain",
            "src/models",
            "src/entities",
            "src/core",
            "domain",
            "models",
            "entities",
            "core",
            "Domain",
            "Models",
            "Entities",
            "crates",
            "pkg",
        ],
    ),
    (
        "infrastructure",
        &[
            "src/infra",
            "src/infrastructure",
            "src/db",
            "src/repositories",
            "src/adapters",
            "infra",
            "infrastructure",
            "Infrastructure",
            "Data",
            "internal",
        ],
    ),
];

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".autopsy",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    "vendor",
];

fn scan_repo_shape(root: &Path, max_entries: usize) -> (BTreeSet<String>, Vec<(String, String)>) {
    let mut langs = BTreeSet::new();
    let mut visited = 0usize;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if visited >= max_entries {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut sorted: Vec<_> = entries.filter_map(Result::ok).collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            visited += 1;
            if visited >= max_entries {
                break;
            }
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if ft.is_dir() {
                if !IGNORED_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if ft.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
            {
                match ext {
                    "ts" | "tsx" | "js" | "jsx" => {
                        langs.insert("typescript".to_string());
                    }
                    "py" => {
                        langs.insert("python".to_string());
                    }
                    "go" => {
                        langs.insert("go".to_string());
                    }
                    "rs" => {
                        langs.insert("rust".to_string());
                    }
                    "java" => {
                        langs.insert("java".to_string());
                    }
                    "kt" | "kts" => {
                        langs.insert("kotlin".to_string());
                    }
                    "cs" => {
                        langs.insert("csharp".to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    if langs.is_empty() {
        langs.insert("typescript".to_string());
    }

    let mut layers = Vec::new();
    for (layer_name, candidates) in KNOWN_LAYER_ORDER {
        for rel in *candidates {
            if root.join(rel).is_dir() {
                layers.push(((*layer_name).to_string(), format!("{rel}/**")));
                break;
            }
        }
    }

    (langs, layers)
}

fn render_autopsy_toml(langs: &BTreeSet<String>) -> String {
    let mut include = Vec::new();
    if langs.contains("typescript") {
        include.extend(["\"**/*.ts\"", "\"**/*.tsx\"", "\"**/*.js\"", "\"**/*.jsx\""]);
    }
    if langs.contains("python") {
        include.push("\"**/*.py\"");
    }
    if langs.contains("go") {
        include.push("\"**/*.go\"");
    }
    if langs.contains("rust") {
        include.push("\"**/*.rs\"");
    }
    if langs.contains("java") {
        include.push("\"**/*.java\"");
    }
    if langs.contains("kotlin") {
        include.extend(["\"**/*.kt\"", "\"**/*.kts\""]);
    }
    if langs.contains("csharp") {
        include.push("\"**/*.cs\"");
    }
    let include_list = include.join(", ");
    let lang_list: Vec<String> = langs.iter().map(|l| format!("\"{l}\"")).collect();
    let lang_joined = lang_list.join(", ");

    format!(
        r#"# Generated by `autopsy init` — Deterministic SynEvid Engine Configuration
[repository]
include = [{include_list}]
exclude = [
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/target/**",
    "**/.git/**",
    "**/.autopsy/**",
    "**/.venv/**",
    "**/venv/**",
    "**/__pycache__/**",
    "**/vendor/**",
]
max_file_bytes = 1048576

[analysis]
languages = [{lang_joined}]
max_traversal_depth = 10
max_traversal_nodes = 1000
strict_mode = false

[storage]
db_path = ".autopsy/storage.db"
cache_dir = ".autopsy/cache"
enable_wal = true
"#
    )
}

fn render_invariants_yml(layers: &[(String, String)]) -> String {
    let mut out = String::from(
        r#"# Generated by `autopsy init` — Deterministic Architectural & Contract Policy
version: "0.0.1"
invariants:
  - id: "NO_NEW_DEPENDENCY_CYCLES"
    kind: "no_new_cycles"
    description: "Prevent introduction of new circular dependency Strongly Connected Components (SCCs)"
    severity: "error"

  - id: "PUBLIC_API_CONTRACT_COMPATIBILITY"
    kind: "api_compatibility"
    description: "Prevent breaking public signature or contract changes against baseline snapshot"
    severity: "error"
"#,
    );

    if layers.len() >= 2 {
        out.push_str(
            r#"
  - id: "CLEAN_ARCHITECTURE_LAYERS"
    kind: "layers"
    description: "Enforce unidirectional architectural layer dependencies (detected by autopsy init)"
    severity: "error"
    layers:
"#,
        );
        for (idx, (name, pattern)) in layers.iter().enumerate() {
            let allowed: Vec<String> = layers[idx + 1..]
                .iter()
                .map(|(n, _)| format!("\"{n}\""))
                .collect();
            out.push_str(&format!(
                "      - name: \"{name}\"\n        paths: [\"{pattern}\"]\n        may_depend_on: [{}]\n",
                allowed.join(", ")
            ));
        }
    }

    out
}

pub fn handle_init(cli: &Cli, force: bool) -> Result<i32, CliError> {
    if !cli.repo_root.exists() || !cli.repo_root.is_dir() {
        return Err(CliError::Config(format!(
            "No such file or directory: {}",
            cli.repo_root.display()
        )));
    }

    let (langs, layers) = scan_repo_shape(&cli.repo_root, 5_000);
    let autopsy_dir = cli.repo_root.join(".autopsy");
    fs::create_dir_all(autopsy_dir.join("runs")).map_err(CliError::Io)?;

    let mut files_status = BTreeMap::new();

    let toml_path = cli.repo_root.join("autopsy.toml");
    if toml_path.exists() && !force {
        files_status.insert("autopsy.toml".to_string(), "preserved".to_string());
    } else {
        let status = if toml_path.exists() {
            "overwritten"
        } else {
            "created"
        };
        fs::write(&toml_path, render_autopsy_toml(&langs)).map_err(CliError::Io)?;
        files_status.insert("autopsy.toml".to_string(), status.to_string());
    }

    let inv_path = autopsy_dir.join("invariants.yml");
    if inv_path.exists() && !force {
        files_status.insert(
            ".autopsy/invariants.yml".to_string(),
            "preserved".to_string(),
        );
    } else {
        let status = if inv_path.exists() {
            "overwritten"
        } else {
            "created"
        };
        fs::write(&inv_path, render_invariants_yml(&layers)).map_err(CliError::Io)?;
        files_status.insert(".autopsy/invariants.yml".to_string(), status.to_string());
    }

    let baseline_debt_path = autopsy_dir.join("baseline.json");
    if !baseline_debt_path.exists() {
        let empty_debt = serde_json::json!({
            "version": "0.0.1",
            "accepted_debt": []
        });
        fs::write(
            &baseline_debt_path,
            format!("{}\n", serde_json::to_string_pretty(&empty_debt).unwrap()),
        )
        .map_err(CliError::Io)?;
        files_status.insert(".autopsy/baseline.json".to_string(), "created".to_string());
    } else {
        files_status.insert(
            ".autopsy/baseline.json".to_string(),
            "preserved".to_string(),
        );
    }

    let detected_languages: Vec<String> = langs.into_iter().collect();
    let detected_layers: Vec<String> = layers
        .into_iter()
        .map(|(n, p)| format!("{n}:{p}"))
        .collect();
    let next_steps = vec![
        "autopsy doctor".to_string(),
        "autopsy baseline --persist".to_string(),
        "autopsy verify".to_string(),
    ];

    let mut hasher = Hasher::new();
    hasher.update(b"autopsy-init-v1");
    for l in &detected_languages {
        hasher.update(l.as_bytes());
    }
    for l in &detected_layers {
        hasher.update(l.as_bytes());
    }
    for (k, v) in &files_status {
        hasher.update(k.as_bytes());
        hasher.update(v.as_bytes());
    }
    let result_digest = hasher.finalize().to_hex().to_string();

    let report = InitReport {
        command: "init",
        detected_languages,
        detected_layers,
        files_status,
        next_steps,
        result_digest,
    };

    if cli.format == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!(
            "Synevid initialized in {} (digest: {})",
            cli.repo_root.display(),
            &report.result_digest[..16]
        );
        println!("  languages : {}", report.detected_languages.join(", "));
        if report.detected_layers.is_empty() {
            println!("  layers    : (none auto-detected; cycle & contract rules active)");
        } else {
            println!("  layers    : {}", report.detected_layers.join(", "));
        }
        for (file, status) in &report.files_status {
            println!("  {:<26} {}", file, status);
        }
        println!("Next: run `autopsy doctor` and `autopsy baseline --persist`.");
    }

    Ok(EXIT_PASS)
}

#[cfg(test)]
mod tests {
    use crate::{EXIT_PASS, run_cli};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_repo(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("autopsy-init-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src/api")).unwrap();
        fs::create_dir_all(dir.join("src/domain")).unwrap();
        fs::write(dir.join("src/api/routes.ts"), "export const r = 1;\n").unwrap();
        fs::write(dir.join("src/domain/user.py"), "x = 1\n").unwrap();
        dir
    }

    fn cli_args(root: &Path, extra: &[&str]) -> Vec<String> {
        let mut v = vec![
            "autopsy".to_string(),
            "--repo-root".to_string(),
            root.display().to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        v.extend(extra.iter().map(|s| (*s).to_string()));
        v
    }

    #[test]
    fn init_creates_configs_and_preserves_unless_forced() {
        let root = temp_repo("basic");
        assert_eq!(run_cli(&cli_args(&root, &["init"])), EXIT_PASS);
        assert!(root.join("autopsy.toml").is_file());
        assert!(root.join(".autopsy/invariants.yml").is_file());
        assert!(root.join(".autopsy/baseline.json").is_file());

        let toml_text = fs::read_to_string(root.join("autopsy.toml")).unwrap();
        assert!(toml_text.contains("\"typescript\""));
        assert!(toml_text.contains("\"python\""));

        let inv_text = fs::read_to_string(root.join(".autopsy/invariants.yml")).unwrap();
        assert!(inv_text.contains("CLEAN_ARCHITECTURE_LAYERS"));

        // Write a marker and verify re-run without --force preserves it.
        fs::write(root.join("autopsy.toml"), "# custom\n").unwrap();
        assert_eq!(run_cli(&cli_args(&root, &["init"])), EXIT_PASS);
        assert_eq!(
            fs::read_to_string(root.join("autopsy.toml")).unwrap(),
            "# custom\n"
        );

        // With --force, it is regenerated.
        assert_eq!(run_cli(&cli_args(&root, &["init", "--force"])), EXIT_PASS);
        assert!(
            fs::read_to_string(root.join("autopsy.toml"))
                .unwrap()
                .contains("[repository]")
        );
        let _ = fs::remove_dir_all(&root);
    }
}
