//! Real-world repository stress test runner
//!
//! Downloads/clones real-world repositories and runs the full SynEvid pipeline
//! to validate performance and correctness at scale.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize)]
pub struct RepoSpec {
    pub name: String,
    pub url: String,
    pub commit: Option<String>,
    pub size_category: SizeCategory,
    pub languages: Vec<String>,
    pub expected_files: usize,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeCategory {
    Small,      // < 1k files
    Medium,     // 1k - 10k files
    Large,      // 10k - 100k files
    XLarge,     // 100k+ files
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StressTestResult {
    pub repo: RepoSpec,
    pub start_time: String,
    pub end_time: String,
    pub duration_ms: u64,
    pub phases: BTreeMap<String, PhaseResult>,
    pub summary: TestSummary,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PhaseResult {
    pub name: String,
    pub duration_ms: u64,
    pub success: bool,
    pub metrics: BTreeMap<String, Value>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestSummary {
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_edges: usize,
    pub baseline_duration_ms: u64,
    pub impact_duration_ms: u64,
    pub verify_duration_ms: u64,
    pub memory_peak_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub disk_io_mb: Option<f64>,
}

impl StressTestResult {
    pub fn new(repo: RepoSpec) -> Self {
        Self {
            repo,
            start_time: chrono::Utc::now().to_rfc3339(),
            end_time: String::new(),
            duration_ms: 0,
            phases: BTreeMap::new(),
            summary: TestSummary {
                total_files: 0,
                total_symbols: 0,
                total_edges: 0,
                baseline_duration_ms: 0,
                impact_duration_ms: 0,
                verify_duration_ms: 0,
                memory_peak_mb: None,
                cpu_percent: None,
                disk_io_mb: None,
            },
            errors: Vec::new(),
        }
    }

    pub fn record_phase(&mut self, name: &str, duration_ms: u64, success: bool, metrics: BTreeMap<String, Value>, error: Option<String>) {
        self.phases.insert(name.to_string(), PhaseResult {
            name: name.to_string(),
            duration_ms,
            success,
            metrics,
            error,
        });
    }

    pub fn finalize(&mut self) {
        self.end_time = chrono::Utc::now().to_rfc3339();
        let start = chrono::DateTime::parse_from_rfc3339(&self.start_time).unwrap();
        let end = chrono::DateTime::parse_from_rfc3339(&self.end_time).unwrap();
        self.duration_ms = (end - start).num_milliseconds() as u64;
    }
}

pub struct StressTestRunner {
    pub work_dir: PathBuf,
    pub autopsy_binary: PathBuf,
    pub repos: Vec<RepoSpec>,
    pub results_dir: PathBuf,
}

impl StressTestRunner {
    pub fn new(work_dir: PathBuf, autopsy_binary: PathBuf, results_dir: PathBuf) -> Self {
        let repos = Self::default_repos();
        Self {
            work_dir,
            autopsy_binary,
            repos,
            results_dir,
        }
    }

    fn default_repos() -> Vec<RepoSpec> {
        vec![
            // Small repos (< 1k files)
            RepoSpec {
                name: "rust-analyzer".to_string(),
                url: "https://github.com/rust-lang/rust-analyzer".to_string(),
                commit: Some("2024-01-15".to_string()),
                size_category: SizeCategory::Large,
                languages: vec!["Rust".to_string()],
                expected_files: 15000,
            },
            RepoSpec {
                name: "typescript".to_string(),
                url: "https://github.com/microsoft/TypeScript".to_string(),
                commit: Some("v5.3.0".to_string()),
                size_category: SizeCategory::Large,
                languages: vec!["TypeScript".to_string()],
                expected_files: 8000,
            },
            RepoSpec {
                name: "golang".to_string(),
                url: "https://github.com/golang/go".to_string(),
                commit: Some("go1.22".to_string()),
                size_category: SizeCategory::XLarge,
                languages: vec!["Go".to_string()],
                expected_files: 50000,
            },
            RepoSpec {
                name: "nodejs".to_string(),
                url: "https://github.com/nodejs/node".to_string(),
                commit: Some("v20.10.0".to_string()),
                size_category: SizeCategory::XLarge,
                languages: vec!["JavaScript".to_string(), "C++".to_string()],
                expected_files: 30000,
            },
            RepoSpec {
                name: "django".to_string(),
                url: "https://github.com/django/django".to_string(),
                commit: Some("main".to_string()),
                size_category: SizeCategory::Large,
                languages: vec!["Python".to_string()],
                expected_files: 12000,
            },
            RepoSpec {
                name: "spring-framework".to_string(),
                url: "https://github.com/spring-projects/spring-framework".to_string(),
                commit: Some("6.1.x".to_string()),
                size_category: SizeCategory::Large,
                languages: vec!["Java".to_string()],
                expected_files: 20000,
            },
            RepoSpec {
                name: "react".to_string(),
                url: "https://github.com/facebook/react".to_string(),
                commit: Some("v18.2.0".to_string()),
                size_category: SizeCategory::Medium,
                languages: vec!["JavaScript".to_string(), "TypeScript".to_string()],
                expected_files: 3000,
            },
            RepoSpec {
                name: "vue".to_string(),
                url: "https://github.com/vuejs/core".to_string(),
                commit: Some("3.4.0".to_string()),
                size_category: SizeCategory::Medium,
                languages: vec!["TypeScript".to_string()],
                expected_files: 2000,
            },
        ]
    }

    pub fn run_all(&mut self) -> Result<Vec<StressTestResult>, Box<dyn std::error::Error>> {
        let mut all_results = Vec::new();

        for repo in &self.repos {
            println!("\n=== Testing {} ({:?}) ===", repo.name, repo.size_category);
            match self.run_single_repo(repo) {
                Ok(result) => {
                    println!("  ✓ Completed in {}ms", result.duration_ms);
                    all_results.push(result);
                }
                Err(e) => {
                    println!("  ✗ Failed: {}", e);
                    let mut result = StressTestResult::new(repo.clone());
                    result.errors.push(e.to_string());
                    result.finalize();
                    all_results.push(result);
                }
            }
        }

        Ok(all_results)
    }

    fn run_single_repo(&self, repo: &RepoSpec) -> Result<StressTestResult, Box<dyn std::error::Error>> {
        let mut result = StressTestResult::new(repo.clone());
        let repo_dir = self.work_dir.join(&repo.name);

        // Phase 1: Clone/Update repo
        let phase_start = Instant::now();
        self.clone_or_update_repo(repo, &repo_dir)?;
        result.record_phase("clone", phase_start.elapsed().as_millis() as u64, true, BTreeMap::new(), None);

        // Phase 2: Baseline
        let phase_start = Instant::now();
        let baseline_result = self.run_baseline(&repo_dir)?;
        result.record_phase("baseline", phase_start.elapsed().as_millis() as u64, true, baseline_result.metrics.clone(), baseline_result.error);
        result.summary.baseline_duration_ms = baseline_result.duration_ms;

        // Phase 3: Impact analysis (sample symbols)
        let phase_start = Instant::now();
        let impact_result = self.run_impact_sample(&repo_dir)?;
        result.record_phase("impact", phase_start.elapsed().as_millis() as u64, impact_result.success, impact_result.metrics.clone(), impact_result.error);
        result.summary.impact_duration_ms = impact_result.duration_ms;

        // Phase 4: Verify
        let phase_start = Instant::now();
        let verify_result = self.run_verify(&repo_dir)?;
        result.record_phase("verify", phase_start.elapsed().as_millis() as u64, verify_result.success, verify_result.metrics.clone(), verify_result.error);
        result.summary.verify_duration_ms = verify_result.duration_ms;

        // Phase 5: Diff (if we have a previous baseline)
        let phase_start = Instant::now();
        let diff_result = self.run_diff(&repo_dir)?;
        result.record_phase("diff", phase_start.elapsed().as_millis() as u64, diff_result.success, diff_result.metrics.clone(), diff_result.error);

        result.finalize();
        Ok(result)
    }

    fn clone_or_update_repo(&self, repo: &RepoSpec, repo_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if repo_dir.exists() {
            // Update existing
            Command::new("git")
                .args(["fetch", "--all"])
                .current_dir(repo_dir)
                .output()?;
            if let Some(commit) = &repo.commit {
                Command::new("git")
                    .args(["checkout", commit])
                    .current_dir(repo_dir)
                    .output()?;
            } else {
                Command::new("git")
                    .args(["pull"])
                    .current_dir(repo_dir)
                    .output()?;
            }
        } else {
            // Clone fresh
            let mut cmd = Command::new("git");
            cmd.args(["clone", "--depth", "1"]);
            if let Some(commit) = &repo.commit {
                cmd.args(["--branch", commit]);
            }
            cmd.args([&repo.url, repo_dir.to_str().unwrap()])
                .output()?;
        }
        Ok(())
    }

    fn run_baseline(&self, repo_dir: &Path) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let output = Command::new(&self.autopsy_binary)
            .args(["--repo-root", repo_dir.to_str().unwrap(), "baseline", "--persist"])
            .output()?;
        let duration = start.elapsed().as_millis() as u64;

        let success = output.status.success();
        let metrics = self.parse_baseline_output(&output);
        let error = if !success {
            Some(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            None
        };

        Ok(PhaseOutput { duration_ms: duration, success, metrics, error })
    }

    fn run_impact_sample(&self, repo_dir: &Path) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        // Find a symbol to test impact on
        let symbols = self.find_test_symbols(repo_dir)?;
        let symbol = symbols.first().cloned().unwrap_or_else(|| "main".to_string());

        let start = Instant::now();
        let output = Command::new(&self.autopsy_binary)
            .args(["--repo-root", repo_dir.to_str().unwrap(), "impact", "-s", &symbol, "--detail", "full"])
            .output()?;
        let duration = start.elapsed().as_millis() as u64;

        let success = output.status.success();
        let metrics = self.parse_impact_output(&output);
        let error = if !success {
            Some(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            None
        };

        Ok(PhaseOutput { duration_ms: duration, success, metrics, error })
    }

    fn run_verify(&self, repo_dir: &Path) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let output = Command::new(&self.autopsy_binary)
            .args(["--repo-root", repo_dir.to_str().unwrap(), "verify", "--strict"])
            .output()?;
        let duration = start.elapsed().as_millis() as u64;

        let success = output.status.success();
        let metrics = self.parse_verify_output(&output);
        let error = if !success {
            Some(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            None
        };

        Ok(PhaseOutput { duration_ms: duration, success, metrics, error })
    }

    fn run_diff(&self, repo_dir: &Path) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let output = Command::new(&self.autopsy_binary)
            .args(["--repo-root", repo_dir.to_str().unwrap(), "diff"])
            .output()?;
        let duration = start.elapsed().as_millis() as u64;

        let success = output.status.success();
        let metrics = self.parse_diff_output(&output);
        let error = if !success {
            Some(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            None
        };

        Ok(PhaseOutput { duration_ms: duration, success, metrics, error })
    }

    fn parse_baseline_output(&self, output: &std::process::Output) -> BTreeMap<String, Value> {
        let mut metrics = BTreeMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse key metrics from output
        for line in stdout.lines() {
            if line.contains("files") || line.contains("symbols") || line.contains("edges") {
                metrics.insert("stdout_sample".to_string(), json!(line));
            }
        }
        metrics
    }

    fn parse_impact_output(&self, output: &std::process::Output) -> BTreeMap<String, Value> {
        let mut metrics = BTreeMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        for line in stdout.lines() {
            if line.contains("affected") || line.contains("coverage") || line.contains("digest") {
                metrics.insert("stdout_sample".to_string(), json!(line));
            }
        }
        metrics
    }

    fn parse_verify_output(&self, output: &std::process::Output) -> BTreeMap<String, Value> {
        let mut metrics = BTreeMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        for line in stdout.lines() {
            if line.contains("PASS") || line.contains("FAIL") || line.contains("coverage") {
                metrics.insert("stdout_sample".to_string(), json!(line));
            }
        }
        metrics
    }

    fn parse_diff_output(&self, output: &std::process::Output) -> BTreeMap<String, Value> {
        let mut metrics = BTreeMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        for line in stdout.lines() {
            if line.contains("changed") || line.contains("added") || line.contains("removed") {
                metrics.insert("stdout_sample".to_string(), json!(line));
            }
        }
        metrics
    }

    fn find_test_symbols(&self, repo_dir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // Run `autopsy find` to get symbols, pick first few
        let output = Command::new(&self.autopsy_binary)
            .args(["--repo-root", repo_dir.to_str().unwrap(), "find", "main", "--limit", "5", "--format", "json"])
            .output()?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let value: Value = serde_json::from_str(&stdout)?;
        
        if let Some(symbols) = value["symbols"].as_array() {
            Ok(symbols.iter().filter_map(|s| s["qualified_name"].as_str().map(|s| s.to_string())).collect())
        } else {
            Ok(vec!["main".to_string()])
        }
    }

    pub fn save_results(&self, results: &[StressTestResult]) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let file = self.results_dir.join(format!("stress_test_{}.json", timestamp));
        
        let report = json!({
            "run_timestamp": chrono::Utc::now().to_rfc3339(),
            "total_repos": results.len(),
            "results": results,
        });
        
        fs::write(file, serde_json::to_string_pretty(&report)?)?;
        
        // Also generate markdown report
        self.generate_markdown_report(results)?;
        
        Ok(())
    }

    fn generate_markdown_report(&self, results: &[StressTestResult]) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let file = self.results_dir.join(format!("stress_test_{}.md", timestamp));
        
        let mut md = String::new();
        md.push_str("# SynEvid Stress Test Report\n\n");
        md.push_str(&format!("**Run Date**: {}\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
        md.push_str(&format!("**Total Repositories Tested**: {}\n\n", results.len()));
        
        md.push_str("## Summary Table\n\n");
        md.push_str("| Repository | Size | Files | Baseline (ms) | Impact (ms) | Verify (ms) | Total (ms) | Status |\n");
        md.push_str("|------------|------|-------|---------------|-------------|-------------|------------|--------|\n");
        
        for result in results {
            let status = if result.errors.is_empty() { "✅ PASS" } else { "❌ FAIL" };
            md.push_str(&format!(
                "| {} | {:?} | {} | {} | {} | {} | {} | {} |\n",
                result.repo.name,
                result.repo.size_category,
                result.repo.expected_files,
                result.summary.baseline_duration_ms,
                result.summary.impact_duration_ms,
                result.summary.verify_duration_ms,
                result.duration_ms,
                status
            ));
        }
        
        md.push_str("\n## Detailed Results\n\n");
        for result in results {
            md.push_str(&format!("### {}\n\n", result.repo.name));
            md.push_str(&format!("- **URL**: {}\n", result.repo.url));
            md.push_str(&format!("- **Size Category**: {:?}\n", result.repo.size_category));
            md.push_str(&format!("- **Expected Files**: {}\n", result.repo.expected_files));
            md.push_str(&format!("- **Total Duration**: {}ms\n", result.duration_ms));
            md.push_str(&format!("- **Start Time**: {}\n", result.start_time));
            md.push_str(&format!("- **End Time**: {}\n", result.end_time));
            
            md.push_str("\n**Phase Details**:\n\n");
            md.push_str("| Phase | Duration (ms) | Success | Error |\n");
            md.push_str("|-------|---------------|---------|-------|\n");
            for (name, phase) in &result.phases {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    name,
                    phase.duration_ms,
                    if phase.success { "✅" } else { "❌" },
                    phase.error.as_deref().unwrap_or("None")
                ));
            }
            
            if !result.errors.is_empty() {
                md.push_str("\n**Errors**:\n\n");
                for err in &result.errors {
                    md.push_str(&format!("- {}\n", err));
                }
            }
            md.push_str("\n---\n\n");
        }
        
        fs::write(file, md)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PhaseOutput {
    duration_ms: u64,
    success: bool,
    metrics: BTreeMap<String, Value>,
    error: Option<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    let work_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("./benchmarks/repos")
    };
    
    let autopsy_binary = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        PathBuf::from("./target/release/autopsy-daemon")
    };
    
    let results_dir = if args.len() > 3 {
        PathBuf::from(&args[3])
    } else {
        PathBuf::from("./benchmarks/results")
    };

    let mut runner = StressTestRunner::new(work_dir, autopsy_binary, results_dir);
    
    println!("Starting stress test run with {} repositories...", runner.repos.len());
    
    match runner.run_all() {
        Ok(results) => {
            if let Err(e) = runner.save_results(&results) {
                eprintln!("Failed to save results: {}", e);
                std::process::exit(1);
            }
            
            let passed = results.iter().filter(|r| r.errors.is_empty()).count();
            let total = results.len();
            println!("\n=== Stress Test Complete ===");
            println!("Passed: {}/{}", passed, total);
            
            if passed == total {
                println!("All repositories passed!");
                std::process::exit(0);
            } else {
                println!("Some repositories failed!");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Stress test failed: {}", e);
            std::process::exit(1);
        }
    }
}