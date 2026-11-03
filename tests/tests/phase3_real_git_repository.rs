//! Real-repository Phase 3 acceptance flow.
//!
//! The fixture is created with the local Git executable because this test is
//! specifically validating interoperability with a real Git repository. The
//! production reader and mutation engine remain pure Rust and never spawn Git.

use autopsy_git_engine::{
    GitBlameEngine, GitDiffer, GitGalaxyReader, GitMutationManager, GitStatusScanner,
    RevertRequest, VirtualTreeBuilder,
};
use autopsy_temporal::git::GitObjectStore;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn run_git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "SynEvid Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@synevid.test")
        .env("GIT_COMMITTER_NAME", "SynEvid Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@synevid.test")
        .env("GIT_AUTHOR_DATE", "2024-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2024-01-01T00:00:00Z")
        .output()
        .unwrap_or_else(|error| panic!("failed to execute git {:?}: {error}", args));
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("git {:?} returned non-UTF8 output: {error}", args))
        .trim()
        .to_string()
}

fn fixture_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("synevid_phase3_real_git_{nonce}"));
    fs::create_dir_all(&root).expect("create real Git fixture root");
    root
}

#[test]
fn phase3_real_packed_repo_covers_history_snapshot_dirty_tree_and_mutations() {
    let root = fixture_root();
    run_git(&root, &["init", "-b", "main"]);
    run_git(&root, &["config", "user.name", "SynEvid Fixture"]);
    run_git(&root, &["config", "user.email", "fixture@synevid.test"]);

    fs::write(
        root.join("app.ts"),
        "export function app(value: number): number {\n  return value + 1;\n}\n",
    )
    .expect("write baseline");
    run_git(&root, &["add", "app.ts"]);
    run_git(&root, &["commit", "-m", "baseline"]);
    let initial_oid = run_git(&root, &["rev-parse", "HEAD"]);

    run_git(&root, &["switch", "-c", "feature/packed"]);
    fs::write(root.join("feature.ts"), "export const feature = true;\n").expect("write feature");
    run_git(&root, &["add", "feature.ts"]);
    run_git(&root, &["commit", "-m", "feature"]);

    run_git(&root, &["switch", "main"]);
    fs::write(root.join("main.ts"), "export const main = true;\n").expect("write main");
    run_git(&root, &["add", "main.ts"]);
    run_git(&root, &["commit", "-m", "main change"]);
    run_git(
        &root,
        &["merge", "--no-ff", "feature/packed", "-m", "merge feature"],
    );
    run_git(&root, &["tag", "v1.0.0"]);

    // Pack the repository so all subsequent native reads exercise Git pack
    // indexes and compressed object access rather than only loose objects.
    run_git(&root, &["gc", "--prune=now"]);
    run_git(&root, &["repack", "-ad"]);
    run_git(&root, &["pack-refs", "--all", "--prune"]);
    let pack_index = fs::read_dir(root.join(".git/objects/pack"))
        .expect("read packed object directory")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("idx"))
        .expect("packed index exists");
    let verify_pack = run_git(
        &root,
        &[
            "verify-pack",
            "-v",
            pack_index.to_str().expect("UTF-8 pack index path"),
        ],
    );
    assert!(
        verify_pack
            .lines()
            .any(|line| line.starts_with("chain length =") && !line.ends_with(" 0")),
        "real fixture must contain at least one packed delta chain: {verify_pack}"
    );
    assert!(
        root.join(".git/packed-refs").is_file(),
        "refs must be packed too"
    );
    let mut store = GitObjectStore::discover(&root).expect("discover packed repository");
    assert!(
        store.packed_object_count() > 0,
        "fixture must contain packed objects"
    );
    let head_oid = store
        .resolve_ref("HEAD")
        .expect("resolve packed HEAD")
        .expect("packed HEAD exists");
    let head_commit = store.commit(head_oid).expect("read packed merge commit");
    assert_eq!(
        head_commit.parents.len(),
        2,
        "merge parents must be preserved"
    );

    let galaxy = GitGalaxyReader::new(&root)
        .read_galaxy(100)
        .expect("read real packed galaxy");
    assert!(galaxy.total_commits >= 4);
    assert!(galaxy.branches.iter().any(|branch| branch.name == "main"));
    assert!(
        galaxy
            .branches
            .iter()
            .any(|branch| branch.name == "feature/packed")
    );
    assert!(galaxy.tags.iter().any(|tag| tag.name == "v1.0.0"));
    assert!(
        galaxy
            .commits
            .iter()
            .any(|commit| commit.parents.len() == 2)
    );

    // Make the working tree dirty and record both bytes and mtime. Historical
    // analysis must not read from or rewrite this file.
    fs::write(
        root.join("app.ts"),
        "export function app(value: number): number {\n  return value + 99;\n}\n",
    )
    .expect("write dirty working tree");
    let dirty_bytes = fs::read(root.join("app.ts")).expect("read dirty bytes");
    let dirty_mtime = fs::metadata(root.join("app.ts"))
        .expect("dirty metadata")
        .modified()
        .expect("dirty mtime");

    let frame_started = Instant::now();
    let frame = VirtualTreeBuilder::new(&root)
        .build_frame(&initial_oid, true)
        .expect("build packed historical frame");
    let frame_elapsed = frame_started.elapsed();
    eprintln!("packed first historical frame: {frame_elapsed:.2?}");
    assert!(
        frame_elapsed <= Duration::from_millis(100),
        "packed first historical frame exceeded the Phase 3 budget: {frame_elapsed:?}"
    );
    assert_eq!(frame.oid, initial_oid);
    assert!(!frame.result_digest.is_empty());
    assert_eq!(
        dirty_bytes,
        fs::read(root.join("app.ts")).expect("bytes unchanged")
    );
    assert_eq!(
        dirty_mtime,
        fs::metadata(root.join("app.ts"))
            .expect("metadata unchanged")
            .modified()
            .expect("mtime unchanged")
    );

    let diff = GitDiffer::new(&root)
        .diff_commits(&initial_oid, &head_oid.to_hex())
        .expect("packed semantic diff");
    assert!(!diff.result_digest.is_empty());
    let blame = GitBlameEngine::new(&root)
        .blame("app.ts", &head_oid.to_hex())
        .expect("packed line blame");
    assert!(!blame.lines.is_empty());
    let status = GitStatusScanner::new(&root)
        .scan_status()
        .expect("dirty status");
    assert!(status.unstaged.iter().any(|item| item.path == "app.ts"));

    // Explicit mutation mode is exercised only after the read-only assertions.
    // Each operation must return an undo checkpoint and operate on the real
    // Git index/working tree, not a sidecar staging file.
    let manager = GitMutationManager::new(&root);
    let committed = manager
        .stage(&["app.ts".to_string()])
        .and_then(|_| manager.commit("native mutation", None, None))
        .expect("stage and commit packed fixture");
    let main_mutation_oid = committed.new_commit_oid.clone().expect("mutation oid");
    assert!(committed.undo_checkpoint_id.is_some());

    manager
        .branch_request(&autopsy_git_engine::BranchRequest {
            branch_name: "feature/control".to_string(),
            action: "create".to_string(),
        })
        .expect("create control branch");
    manager
        .branch_request(&autopsy_git_engine::BranchRequest {
            branch_name: "feature/control".to_string(),
            action: "switch".to_string(),
        })
        .expect("switch control branch");
    fs::write(root.join("control.ts"), "export const controlled = true;\n")
        .expect("write control change");
    manager
        .stage(&["control.ts".to_string()])
        .and_then(|_| manager.commit("control change", None, None))
        .expect("commit control branch");
    manager
        .checkout(&autopsy_git_engine::CheckoutRequest {
            target: "main".to_string(),
            detach: Some(false),
        })
        .expect("checkout main");
    assert!(!root.join("control.ts").exists());

    let reverted = manager
        .revert(&RevertRequest {
            commit_oid: main_mutation_oid,
        })
        .expect("revert main mutation");
    assert!(reverted.success);
    assert!(reverted.undo_checkpoint_id.is_some());
    assert!(
        manager.undo().expect("undo revert").success,
        "undo must restore the pre-revert state"
    );

    assert!(
        root.join(".git/refs/autopsy/undo")
            .read_dir()
            .expect("undo refs")
            .next()
            .is_some(),
        "mutations must leave recovery refs"
    );
    let _ = fs::remove_dir_all(root);
}
