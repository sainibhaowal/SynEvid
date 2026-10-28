use crate::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Create an isolated, temporary Git repository completely independent of the workspace.
/// This strictly obeys the non-mutation invariant: NEVER test or mutate the active connected repository!
fn create_isolated_test_repo() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("synevid_isolated_git_test_{nonce}"));
    let git_dir = temp_root.join(".git");

    fs::create_dir_all(&git_dir.join("objects")).expect("create objects dir");
    fs::create_dir_all(&git_dir.join("refs").join("heads")).expect("create refs/heads");
    fs::create_dir_all(&git_dir.join("refs").join("tags")).expect("create refs/tags");

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

#[test]
fn test_isolated_commit_and_galaxy_reader() {
    let repo_root = create_isolated_test_repo();
    let manager = GitMutationManager::new(&repo_root);

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
    let c1 = manager
        .commit("feat: good baseline", None, None)
        .expect("c1");
    let _ = manager.commit("fix: minor tweak", None, None).expect("c2");
    let c3 = manager
        .commit("feat: BREAKING contract drift in api", None, None)
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
    assert!(bisect_res.breaking_summary.contains("BREAKING"));

    let _ = fs::remove_dir_all(&repo_root);
}
