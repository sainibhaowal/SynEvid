use autopsy_git_engine::{
    BlameReport, BranchRequest, GitBlameEngine, GitDiffer, GitMutationManager, RevertRequest,
    VirtualTreeBuilder,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn isolated_repo() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("synevid_phase3_e2e_{nonce}"));
    fs::create_dir_all(root.join(".git").join("objects")).expect("objects");
    fs::create_dir_all(root.join(".git").join("refs").join("heads")).expect("heads");
    fs::write(root.join(".git").join("HEAD"), "ref: refs/heads/main\n").expect("head");
    root
}

#[test]
fn native_git_phase3_flow_is_end_to_end_and_non_destructive_for_history_reads() {
    let root = isolated_repo();
    fs::write(
        root.join("service.ts"),
        "export function service(value: number): number {\n  return value + 1;\n}\n",
    )
    .expect("baseline");
    let manager = GitMutationManager::new(&root);
    manager
        .stage(&["service.ts".to_string()])
        .expect("stage baseline");
    let first = manager
        .commit(
            "feat: baseline",
            Some("Phase3"),
            Some("phase3@example.test"),
        )
        .expect("first commit");
    let first_oid = first.new_commit_oid.expect("first oid");
    let baseline_bytes = fs::read(root.join("service.ts")).expect("baseline bytes");

    fs::write(
        root.join("service.ts"),
        "export function service(value: number): number {\n  return value + 2;\n}\n",
    )
    .expect("change");
    let working_bytes = fs::read(root.join("service.ts")).expect("working bytes");
    manager
        .stage(&["service.ts".to_string()])
        .expect("stage change");
    let second = manager
        .commit("feat: changed implementation", None, None)
        .expect("second commit");
    let second_oid = second.new_commit_oid.expect("second oid");

    let head_before_frame = fs::read(root.join(".git").join("HEAD")).expect("head before");
    let diff = GitDiffer::new(&root)
        .diff_commits(&first_oid, &second_oid)
        .expect("semantic diff");
    assert_eq!(diff.total_changed_files, 1);
    assert!(!diff.result_digest.is_empty());

    let frame = VirtualTreeBuilder::new(&root)
        .build_frame(&first_oid, true)
        .expect("time travel");
    assert_eq!(frame.oid, first_oid);
    assert!(!frame.result_digest.is_empty());
    assert_eq!(
        working_bytes,
        fs::read(root.join("service.ts")).expect("unchanged bytes")
    );
    assert_eq!(
        head_before_frame,
        fs::read(root.join(".git").join("HEAD")).expect("unchanged head")
    );

    let blame: BlameReport = GitBlameEngine::new(&root)
        .blame("service.ts", "HEAD")
        .expect("blame");
    assert_eq!(blame.lines.len(), 3);
    assert!(!blame.result_digest.is_empty());

    let reverted = manager
        .revert(&RevertRequest {
            commit_oid: second_oid,
        })
        .expect("revert");
    assert!(reverted.success);
    assert_eq!(
        fs::read(root.join("service.ts")).expect("reverted"),
        baseline_bytes
    );

    manager
        .branch_request(&BranchRequest {
            branch_name: "feature/phase3".to_string(),
            action: "create".to_string(),
        })
        .expect("branch create");
    assert!(
        root.join(".git")
            .join("refs")
            .join("heads")
            .join("feature")
            .join("phase3")
            .exists()
    );
    let _ = fs::remove_dir_all(root);
}
