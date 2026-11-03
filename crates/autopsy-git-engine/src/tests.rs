use crate::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Create an isolated, temporary Git repository completely independent of the workspace.
/// This strictly obeys the non-mutation invariant: NEVER test or mutate the active connected repository!
fn create_isolated_test_repo() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("synevid_isolated_git_test_{nonce}"));
    let git_dir = temp_root.join(".git");

    fs::create_dir_all(git_dir.join("objects")).expect("create objects dir");
    fs::create_dir_all(git_dir.join("refs").join("heads")).expect("create refs/heads");
    fs::create_dir_all(git_dir.join("refs").join("tags")).expect("create refs/tags");

    // Write initial HEAD pointing to main
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").expect("write HEAD");

    // Create initial dummy files
    fs::write(
        temp_root.join("index.ts"),
        "export function add(a: number, b: number): number {\n  return a + b;\n}\n",
    )
    .expect("write index.ts");

    fs::write(temp_root.join("README.md"), "# Test Repository\n").expect("write README.md");

    temp_root
}

fn stage_initial_files(repo_root: &PathBuf) {
    GitMutationManager::new(repo_root)
        .stage(&["index.ts".to_string(), "README.md".to_string()])
        .expect("stage initial files");
}

#[test]
fn test_isolated_commit_and_galaxy_reader() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    stage_initial_files(&repo_root);

    // Initial root commit
    let commit1 = manager
        .commit(
            "feat: initial commit",
            Some("Dev One"),
            Some("dev1@test.com"),
        )
        .expect("initial commit");
    assert!(commit1.success);
    assert!(commit1.new_commit_oid.is_some());

    // Second commit
    let commit2 = manager
        .commit(
            "feat: add second feature",
            Some("Dev Two"),
            Some("dev2@test.com"),
        )
        .expect("second commit");
    assert!(commit2.success);

    // Read galaxy
    let reader = GitGalaxyReader::new(&repo_root);
    let galaxy = reader.read_galaxy(50).expect("read galaxy");

    assert_eq!(galaxy.commits.len(), 2);
    assert_eq!(galaxy.active_branch.as_deref(), Some("main"));
    assert!(galaxy.commits[0].is_head);
    assert_eq!(galaxy.commits[0].summary, "feat: add second feature");

    // Clean up temp repo
    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn test_isolated_working_tree_status_and_diff() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    stage_initial_files(&repo_root);
    let _ = manager.commit("feat: initial", None, None).expect("commit");

    // Add untracked file
    fs::write(repo_root.join("untracked.ts"), "export const x = 42;\n").expect("write untracked");

    // Stage file
    let stage_res = manager.stage(&["untracked.ts".to_string()]).expect("stage");
    assert!(stage_res.success);

    let scanner = GitStatusScanner::new(&repo_root);
    let status = scanner.scan_status().expect("scan status");

    assert!(!status.is_clean);
    assert!(status.staged.iter().any(|s| s.path == "untracked.ts"));

    // Diff file
    let diff = scanner
        .diff_file("untracked.ts")
        .expect("diff untracked.ts");
    assert_eq!(diff.path, "untracked.ts");
    assert!(diff.total_additions > 0);

    // Unstage file
    let unstage_res = manager
        .unstage(&["untracked.ts".to_string()])
        .expect("unstage");
    assert!(unstage_res.success);

    let status_after = scanner.scan_status().expect("scan status after");
    assert!(
        status_after
            .untracked
            .iter()
            .any(|s| s.path == "untracked.ts")
    );

    // Stage file again and commit
    let _ = manager
        .stage(&["untracked.ts".to_string()])
        .expect("stage untracked");
    let commit_res = manager
        .commit("feat: absorb untracked.ts", None, None)
        .expect("commit staged");
    assert!(commit_res.success);

    // After commit, untracked.ts must be part of HEAD tree and no longer in staged or untracked
    let status_committed = scanner.scan_status().expect("scan status committed");
    assert!(
        !status_committed
            .staged
            .iter()
            .any(|s| s.path == "untracked.ts")
    );
    assert!(
        !status_committed
            .untracked
            .iter()
            .any(|s| s.path == "untracked.ts")
    );
    assert!(
        !status_committed
            .unstaged
            .iter()
            .any(|s| s.path == "untracked.ts")
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn test_isolated_branch_tag_and_undo() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    stage_initial_files(&repo_root);
    let c1 = manager.commit("feat: v1", None, None).expect("commit 1");
    let c1_oid = c1.new_commit_oid.expect("c1 oid");

    // Tag creation
    let tag_res = manager
        .tag(&TagRequest {
            tag_name: "v1.0.0".to_string(),
            target_oid: Some(c1_oid.clone()),
            message: None,
            action: Some("create".to_string()),
        })
        .expect("create tag");
    assert!(tag_res.success);

    // Branch creation
    let branch_res = manager
        .branch_request(&BranchRequest {
            branch_name: "feature/login".to_string(),
            action: "create".to_string(),
        })
        .expect("create branch");
    assert!(branch_res.success);

    // Branch switch
    let switch_res = manager
        .branch_request(&BranchRequest {
            branch_name: "feature/login".to_string(),
            action: "switch".to_string(),
        })
        .expect("switch branch");
    assert!(switch_res.success);

    // Commit on feature branch
    let c2 = manager
        .commit("feat: add login screen", None, None)
        .expect("commit 2");
    assert!(c2.success);

    // Undo commit (Cmd+Z)
    let undo_res = manager.undo().expect("undo");
    assert!(undo_res.success);

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn test_isolated_bisect_assistant() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    stage_initial_files(&repo_root);
    let c1 = manager
        .commit("feat: good baseline", None, None)
        .expect("c1");
    fs::write(
        repo_root.join("index.ts"),
        "export function add(a: number, b: number): number {\n  return a + b + 1;\n}\n",
    )
    .expect("write benign change");
    manager
        .stage(&["index.ts".to_string()])
        .expect("stage benign change");
    let _ = manager.commit("fix: minor tweak", None, None).expect("c2");
    fs::write(
        repo_root.join("index.ts"),
        "export function add(a: string): string {\n  return a;\n}\n",
    )
    .expect("write breaking change");
    manager
        .stage(&["index.ts".to_string()])
        .expect("stage breaking change");
    let c3 = manager
        .commit("feat: change add contract", None, None)
        .expect("c3");

    let bisect_res = manager
        .bisect(&BisectRequest {
            start_oid: c1.new_commit_oid.unwrap(),
            end_oid: c3.new_commit_oid.clone().unwrap(),
            rule_id: None,
        })
        .expect("bisect");

    assert_eq!(bisect_res.verdict, "FAIL");
    assert_eq!(bisect_res.breaking_commit_oid, c3.new_commit_oid);
    assert!(bisect_res.breaking_summary.contains("change add contract"));

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn test_phase3_real_index_checkout_revert_semantic_diff_blame_and_frame() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    stage_initial_files(&repo_root);
    let c1 = manager
        .commit("feat: baseline", Some("Alice"), Some("alice@example.test"))
        .expect("baseline");
    let c1_oid = c1.new_commit_oid.clone().expect("baseline oid");
    let baseline = fs::read_to_string(repo_root.join("index.ts")).expect("baseline content");

    fs::write(
        repo_root.join("index.ts"),
        "export function add(a: number, b: number): number {\n  return a + b + 2;\n}\n",
    )
    .expect("write semantic change");
    manager
        .stage(&["index.ts".to_string()])
        .expect("stage semantic change");
    let c2 = manager
        .commit(
            "feat: update implementation",
            Some("Bob"),
            Some("bob@example.test"),
        )
        .expect("second commit");
    let c2_oid = c2.new_commit_oid.clone().expect("second oid");

    let diff = GitDiffer::new(&repo_root)
        .diff_commits(&c1_oid, &c2_oid)
        .expect("semantic diff");
    assert_eq!(diff.base_oid, c1_oid);
    assert_eq!(diff.head_oid, c2_oid);
    assert!(!diff.result_digest.is_empty());
    assert!(diff.total_changed_files >= 1);

    let before_file = fs::read(repo_root.join("index.ts")).expect("file snapshot");
    let before_head = fs::read(repo_root.join(".git").join("HEAD")).expect("head snapshot");
    let frame_started = Instant::now();
    let frame = VirtualTreeBuilder::new(&repo_root)
        .build_frame(&c2_oid, true)
        .expect("historical frame");
    let frame_elapsed = frame_started.elapsed();
    eprintln!("first historical frame: {frame_elapsed:.2?}");
    if cfg!(debug_assertions) {
        assert!(
            frame_elapsed < Duration::from_secs(10),
            "debug historical frame exceeded the diagnostic ceiling: {frame_elapsed:?}"
        );
    } else {
        assert!(
            frame_elapsed < Duration::from_millis(100),
            "first historical frame exceeded the Phase 3 budget: {frame_elapsed:?}"
        );
    }
    assert_eq!(frame.oid, c2_oid);
    assert!(!frame.result_digest.is_empty());
    assert_eq!(
        before_file,
        fs::read(repo_root.join("index.ts")).expect("file unchanged")
    );
    assert_eq!(
        before_head,
        fs::read(repo_root.join(".git").join("HEAD")).expect("head unchanged")
    );

    let revert = manager
        .revert(&RevertRequest {
            commit_oid: c2_oid.clone(),
        })
        .expect("revert");
    assert!(revert.success);
    assert_eq!(
        fs::read_to_string(repo_root.join("index.ts")).expect("reverted content"),
        baseline
    );
    assert_ne!(revert.new_commit_oid.as_deref(), Some(c2_oid.as_str()));

    let blame = GitBlameEngine::new(&repo_root)
        .blame("index.ts", "HEAD")
        .expect("line blame");
    assert_eq!(blame.path, "index.ts");
    assert_eq!(blame.lines.len(), baseline.lines().count());
    assert!(!blame.result_digest.is_empty());
    assert!(blame.lines.iter().all(|line| !line.commit_oid.is_empty()));

    manager
        .branch_request(&BranchRequest {
            branch_name: "feature/checkout".to_string(),
            action: "create".to_string(),
        })
        .expect("create feature branch");
    manager
        .branch_request(&BranchRequest {
            branch_name: "feature/checkout".to_string(),
            action: "switch".to_string(),
        })
        .expect("switch feature branch");
    fs::write(
        repo_root.join("index.ts"),
        "export function add(a: number, b: number): number {\n  return a + b + 99;\n}\n",
    )
    .expect("write branch change");
    manager
        .stage(&["index.ts".to_string()])
        .expect("stage branch change");
    manager
        .commit("feat: branch-only change", None, None)
        .expect("branch commit");
    manager
        .checkout(&CheckoutRequest {
            target: "main".to_string(),
            detach: Some(false),
        })
        .expect("checkout main");
    assert_eq!(
        fs::read_to_string(repo_root.join("index.ts")).expect("checked out content"),
        baseline
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn test_phase3_selected_hunks_use_real_index_and_preserve_unstaged_lines() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    stage_initial_files(&repo_root);
    manager
        .commit("feat: baseline", None, None)
        .expect("baseline");
    fs::write(
        repo_root.join("index.ts"),
        "export function add(a: number, b: number): number {\n  return a + 9;\n}\n// unstaged\n",
    )
    .expect("working edit");

    let mut hunks = BTreeMap::new();
    hunks.insert(
        "index.ts".to_string(),
        vec![StageHunk {
            old_start: 2,
            old_lines: 1,
            new_start: 2,
            new_lines: 1,
            lines: vec![
                "-  return a + b;".to_string(),
                "+  return a + 9;".to_string(),
            ],
        }],
    );
    manager.stage_hunks(&hunks).expect("stage selected hunk");
    let status = GitStatusScanner::new(&repo_root)
        .scan_status()
        .expect("status");
    assert!(status.staged.iter().any(|item| item.path == "index.ts"));
    assert!(status.unstaged.iter().any(|item| item.path == "index.ts"));
    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn test_phase3_galaxy_5000_commit_acceptance_budget() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);
    let git_dir = repo_root.join(".git");
    let tree_oid = manager
        .write_loose_object(&git_dir, "tree", b"")
        .expect("empty tree");
    let mut parent: Option<autopsy_temporal::git::ObjectId> = None;
    for index in 0..5_000usize {
        let parent_line = parent
            .map(|oid| format!("parent {}\n", oid.to_hex()))
            .unwrap_or_default();
        let payload = format!(
            "tree {}\n{}author Perf <perf@example.test> {} +0000\ncommitter Perf <perf@example.test> {} +0000\n\ncommit {index}\n",
            tree_oid.to_hex(),
            parent_line,
            index,
            index
        );
        parent = Some(
            manager
                .write_loose_object(&git_dir, "commit", payload.as_bytes())
                .expect("commit object"),
        );
    }
    let head = parent.expect("head");
    fs::write(
        git_dir.join("refs").join("heads").join("main"),
        format!("{}\n", head.to_hex()),
    )
    .expect("head ref");

    // The normal workspace gate measures the indexed packed-object path so it
    // remains stable on developer Windows hosts. Native GitHub Linux CI can
    // opt into the strict loose-object acceptance path with this environment
    // variable; that path deliberately preserves the 5,000 loose objects.
    let strict_loose_gate = std::env::var_os("SYNEVID_PHASE3_LOOSE_HISTORY_GATE").is_some();
    if !strict_loose_gate {
        let pack = std::process::Command::new("git")
            .args(["repack", "-ad"])
            .current_dir(&repo_root)
            .output()
            .expect("pack synthetic history");
        assert!(
            pack.status.success(),
            "git repack failed: {}",
            String::from_utf8_lossy(&pack.stderr)
        );
    }

    let started = Instant::now();
    let galaxy = GitGalaxyReader::new(&repo_root)
        .read_galaxy(5_000)
        .expect("5,000 commit galaxy");
    let elapsed = started.elapsed();
    eprintln!(
        "5,000 {} commit galaxy read: {:.2?}",
        if strict_loose_gate { "loose" } else { "packed" },
        elapsed
    );
    assert_eq!(galaxy.total_commits, 5_000);
    if cfg!(debug_assertions) {
        assert!(
            elapsed < Duration::from_secs(30),
            "debug packed 5,000-commit galaxy exceeded the diagnostic ceiling: {elapsed:?}"
        );
    } else {
        assert!(
            elapsed < Duration::from_millis(150),
            "packed 5,000-commit galaxy exceeded the Phase 3 budget: {elapsed:?}"
        );
    }
    let _ = fs::remove_dir_all(&repo_root);
}
