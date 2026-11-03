//! Deterministic sandbox for optional Phase 1 policy plugins.
//!
//! A policy module is loaded only from `<repository>/.autopsy/plugins/*.wasm`.
//! The module has no host imports (therefore no filesystem, clock, process, or
//! network capability) and is executed with a fixed fuel and memory budget.
//!
//! ABI:
//!
//! * export `memory` as one linear memory with a maximum of 16 pages;
//! * export `alloc(i32) -> i32` for an input buffer;
//! * export `evaluate(i32, i32) -> i64`, returning `(ptr << 32) | len` for a
//!   UTF-8 JSON [`GateEvaluationResult`] in the module memory.
//!
//! The host writes a canonical JSON object containing `baseline` and
//! `candidate` snapshots to the allocation returned by `alloc`.  The module's
//! result is validated before it enters the core registry.

use autopsy_domain::RepoSnapshot;
use autopsy_plugin_api::{
    GateEvaluationResult, PluginDescriptor, PluginError, PluginRegistry, VerificationGatePlugin,
};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use wasmi::{Config, Engine, ExternType, Linker, Module, Store, ValType};

pub const MAX_FUEL: u64 = 2_000_000;
pub const MAX_MEMORY_PAGES: u64 = 16;
pub const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Debug, Error)]
pub enum WasmPluginError {
    #[error("WASM module is invalid: {0}")]
    Module(String),
    #[error("WASM module violates the no-import sandbox: {0}")]
    Import(String),
    #[error("WASM module has an invalid ABI: {0}")]
    Abi(String),
    #[error("WASM policy execution failed: {0}")]
    Execution(String),
    #[error("WASM policy returned invalid JSON: {0}")]
    Output(String),
    #[error("WASM plugin filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WasmPluginLoadReport {
    pub loaded_plugin_ids: Vec<String>,
    pub rejected_plugins: Vec<String>,
}

struct WasmPolicyPlugin {
    plugin_id: &'static str,
    gate_id: &'static str,
    display_name: &'static str,
    description: &'static str,
    module_bytes: Arc<[u8]>,
}

impl PluginDescriptor for WasmPolicyPlugin {
    fn plugin_id(&self) -> &'static str {
        self.plugin_id
    }

    fn version(&self) -> &'static str {
        "0.2.0"
    }

    fn display_name(&self) -> &'static str {
        self.display_name
    }

    fn description(&self) -> &'static str {
        self.description
    }
}

impl VerificationGatePlugin for WasmPolicyPlugin {
    fn gate_id(&self) -> &'static str {
        self.gate_id
    }

    fn evaluate(
        &self,
        baseline: Option<&RepoSnapshot>,
        candidate: &RepoSnapshot,
    ) -> Result<GateEvaluationResult, PluginError> {
        execute_module(&self.module_bytes, self.gate_id, baseline, candidate)
            .map_err(|error| PluginError::Execution(self.plugin_id.to_string(), error.to_string()))
    }
}

/// Loads every valid local WASM policy in deterministic path order and adds it
/// to the registry. Invalid or unsafe modules are rejected individually so one
/// optional policy cannot prevent the daemon from starting.
pub fn register_wasm_policy_plugins(
    registry: &PluginRegistry,
    repository_root: &Path,
) -> WasmPluginLoadReport {
    let plugin_dir = repository_root.join(".autopsy").join("plugins");
    let mut report = WasmPluginLoadReport::default();
    let mut paths = match fs::read_dir(&plugin_dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_file()
                    || entry.path().extension().and_then(|ext| ext.to_str()) != Some("wasm")
                {
                    return None;
                }
                Some(entry.path())
            })
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return report,
        Err(error) => {
            report
                .rejected_plugins
                .push(format!("{}: {error}", plugin_dir.display()));
            return report;
        }
    };
    paths.sort_by_key(|path| normalized_path(path));

    for path in paths {
        match load_plugin(&path) {
            Ok(plugin) => {
                let plugin_id = plugin.plugin_id().to_string();
                registry.register_gate(Arc::new(plugin));
                report.loaded_plugin_ids.push(plugin_id);
            }
            Err(error) => report
                .rejected_plugins
                .push(format!("{}: {error}", normalized_path(&path))),
        }
    }
    report
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn load_plugin(path: &Path) -> Result<WasmPolicyPlugin, WasmPluginError> {
    let bytes = fs::read(path)?;
    validate_module(&bytes)?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| WasmPluginError::Abi("plugin filename must have a non-empty stem".into()))?;
    let plugin_id = leak_string(format!("gate.wasm.{stem}"));
    let gate_id = leak_string(format!("wasm.{stem}"));
    let display_name = leak_string(format!("WASM Policy: {stem}"));
    let description = leak_string(format!(
        "Sandboxed deterministic WebAssembly policy loaded from {}",
        normalized_path(path)
    ));
    Ok(WasmPolicyPlugin {
        plugin_id,
        gate_id,
        display_name,
        description,
        module_bytes: bytes.into_boxed_slice().into(),
    })
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn module_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config)
}

fn validate_module(bytes: &[u8]) -> Result<(), WasmPluginError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(WasmPluginError::Module(format!(
            "module exceeds {} byte limit",
            MAX_INPUT_BYTES
        )));
    }
    let engine = module_engine();
    let module =
        Module::new(&engine, bytes).map_err(|error| WasmPluginError::Module(error.to_string()))?;
    if let Some(import) = module.imports().next() {
        return Err(WasmPluginError::Import(format!(
            "import {}.{} is not permitted",
            import.module(),
            import.name()
        )));
    }
    let memory = match module.get_export("memory") {
        Some(ExternType::Memory(memory)) => memory,
        _ => return Err(WasmPluginError::Abi("missing memory export".into())),
    };
    if memory.is_64() || memory.minimum() > MAX_MEMORY_PAGES {
        return Err(WasmPluginError::Abi(format!(
            "memory minimum must be <= {MAX_MEMORY_PAGES} 32-bit pages"
        )));
    }
    if memory
        .maximum()
        .is_none_or(|maximum| maximum > MAX_MEMORY_PAGES)
    {
        return Err(WasmPluginError::Abi(format!(
            "memory maximum must be <= {MAX_MEMORY_PAGES} pages"
        )));
    }
    validate_function(&module, "alloc", &[ValType::I32], &[ValType::I32])?;
    validate_function(
        &module,
        "evaluate",
        &[ValType::I32, ValType::I32],
        &[ValType::I64],
    )?;
    Ok(())
}

fn validate_function(
    module: &Module,
    name: &str,
    params: &[ValType],
    results: &[ValType],
) -> Result<(), WasmPluginError> {
    let function = match module.get_export(name) {
        Some(ExternType::Func(function)) => function,
        _ => {
            return Err(WasmPluginError::Abi(format!(
                "missing function export {name}"
            )));
        }
    };
    if function.params() != params || function.results() != results {
        return Err(WasmPluginError::Abi(format!(
            "function {name} has the wrong signature"
        )));
    }
    Ok(())
}

fn execute_module(
    bytes: &[u8],
    expected_gate_id: &str,
    baseline: Option<&RepoSnapshot>,
    candidate: &RepoSnapshot,
) -> Result<GateEvaluationResult, WasmPluginError> {
    validate_module(bytes)?;
    let engine = module_engine();
    let module =
        Module::new(&engine, bytes).map_err(|error| WasmPluginError::Module(error.to_string()))?;
    let mut store = Store::new(&engine, ());
    store
        .set_fuel(MAX_FUEL)
        .map_err(|error| WasmPluginError::Execution(error.to_string()))?;
    let linker = Linker::new(&engine);
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|error| WasmPluginError::Execution(error.to_string()))?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| WasmPluginError::Abi("missing memory export".into()))?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&store, "alloc")
        .map_err(|error| WasmPluginError::Abi(error.to_string()))?;
    let evaluate = instance
        .get_typed_func::<(i32, i32), i64>(&store, "evaluate")
        .map_err(|error| WasmPluginError::Abi(error.to_string()))?;
    let input = serde_json::to_vec(&json!({
        "baseline": baseline,
        "candidate": candidate,
    }))
    .map_err(|error| WasmPluginError::Execution(error.to_string()))?;
    if input.len() > MAX_INPUT_BYTES {
        return Err(WasmPluginError::Execution("input exceeds limit".into()));
    }
    let input_len = i32::try_from(input.len())
        .map_err(|_| WasmPluginError::Execution("input length overflow".into()))?;
    let input_ptr = alloc
        .call(&mut store, input_len)
        .map_err(|error| WasmPluginError::Execution(error.to_string()))?;
    let input_ptr = usize::try_from(input_ptr)
        .map_err(|_| WasmPluginError::Execution("alloc returned a negative pointer".into()))?;
    let input_end = input_ptr
        .checked_add(input.len())
        .ok_or_else(|| WasmPluginError::Execution("input pointer overflow".into()))?;
    if input_end > memory.data(&store).len() {
        return Err(WasmPluginError::Execution(
            "alloc returned a pointer outside linear memory".into(),
        ));
    }
    memory.data_mut(&mut store)[input_ptr..input_end].copy_from_slice(&input);

    let packed = evaluate
        .call(&mut store, (input_ptr as i32, input_len))
        .map_err(|error| WasmPluginError::Execution(error.to_string()))? as u64;
    let output_ptr = usize::try_from((packed >> 32) as u32).unwrap_or(usize::MAX);
    let output_len = usize::try_from(packed as u32).unwrap_or(usize::MAX);
    if output_len > MAX_OUTPUT_BYTES {
        return Err(WasmPluginError::Output("output exceeds limit".into()));
    }
    let output_end = output_ptr
        .checked_add(output_len)
        .ok_or_else(|| WasmPluginError::Output("output pointer overflow".into()))?;
    let output = memory
        .data(&store)
        .get(output_ptr..output_end)
        .ok_or_else(|| WasmPluginError::Output("output is outside linear memory".into()))?;
    let mut result: GateEvaluationResult = serde_json::from_slice(output)
        .map_err(|error| WasmPluginError::Output(error.to_string()))?;
    if result.gate_id.is_empty() {
        result.gate_id = expected_gate_id.to_string();
    }
    if result.gate_id != expected_gate_id {
        return Err(WasmPluginError::Output(format!(
            "returned gate_id '{}' does not match '{}'",
            result.gate_id, expected_gate_id
        )));
    }
    if !matches!(result.verdict.as_str(), "PASS" | "WARN" | "FAIL") {
        return Err(WasmPluginError::Output(format!(
            "unsupported verdict '{}'",
            result.verdict
        )));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_domain::{FileUnit, RepoSnapshot, SnapshotId};
    use std::collections::BTreeMap;

    fn fixture_wat(output: &str, import: bool) -> String {
        let output_len = output.len();
        let packed = output_len as u64;
        let output = output.replace('\\', "\\\\").replace('"', "\\\"");
        let import_text = if import {
            "(import \"host\" \"forbidden\" (func))"
        } else {
            ""
        };
        format!(
            r#"(module
                {import_text}
                (memory (export "memory") 1 1)
                (func (export "alloc") (param i32) (result i32) (i32.const 1024))
                (func (export "evaluate") (param i32 i32) (result i64) (i64.const {packed}))
                (data (i32.const 0) "{output}")
            )"#,
        )
    }

    fn candidate() -> RepoSnapshot {
        RepoSnapshot {
            snapshot_id: SnapshotId::new("candidate"),
            revision: "worktree".into(),
            config_hash: "config".into(),
            file_set_digest: "files".into(),
            files: BTreeMap::from([(
                "src/lib.ts".into(),
                FileUnit {
                    path: "src/lib.ts".into(),
                    language: "typescript".into(),
                    content_hash: "content".into(),
                    is_generated: false,
                },
            )]),
            skipped_oversize_files: Vec::new(),
            traversal_limit_reached: false,
        }
    }

    #[test]
    fn valid_module_evaluates_inside_sandbox() {
        let output = r#"{"gate_id":"wasm.policy","verdict":"PASS","violations_count":0,"summary":"ok","details_json":"{}"}"#;
        let bytes = wat::parse_str(fixture_wat(output, false)).unwrap();
        let result = execute_module(&bytes, "wasm.policy", None, &candidate()).unwrap();
        assert_eq!(result.verdict, "PASS");
    }

    #[test]
    fn imports_are_rejected_before_execution() {
        let output = r#"{"gate_id":"wasm.policy","verdict":"PASS","violations_count":0,"summary":"ok","details_json":"{}"}"#;
        let bytes = wat::parse_str(fixture_wat(output, true)).unwrap();
        let error = validate_module(&bytes).unwrap_err();
        assert!(matches!(error, WasmPluginError::Import(_)));
    }

    #[test]
    fn loader_is_deterministic_and_skips_missing_directory() {
        let root = std::env::temp_dir().join(format!("synevid_wasm_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let registry = PluginRegistry::new();
        let report = register_wasm_policy_plugins(&registry, &root);
        assert!(report.loaded_plugin_ids.is_empty());
        assert!(report.rejected_plugins.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn loader_registers_a_policy_from_repository_plugins_directory() {
        let root = std::env::temp_dir().join(format!("synevid_wasm_loaded_{}", std::process::id()));
        let plugin_dir = root.join(".autopsy").join("plugins");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&plugin_dir).unwrap();
        let output = r#"{"gate_id":"wasm.policy","verdict":"PASS","violations_count":0,"summary":"ok","details_json":"{}"}"#;
        fs::write(
            plugin_dir.join("policy.wasm"),
            wat::parse_str(fixture_wat(output, false)).unwrap(),
        )
        .unwrap();

        let registry = PluginRegistry::new();
        let report = register_wasm_policy_plugins(&registry, &root);
        assert_eq!(report.loaded_plugin_ids, vec!["gate.wasm.policy"]);
        assert!(report.rejected_plugins.is_empty());
        assert!(registry.get_gate("gate.wasm.policy").is_some());
        let _ = fs::remove_dir_all(root);
    }
}
