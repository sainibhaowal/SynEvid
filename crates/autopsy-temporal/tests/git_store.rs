use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, compute_object_id, flatten_tree};
use std::collections::BTreeMap;
use std::path::Path;

mod common;
use common::*;

// ---------------------------------------------------------------------------
// Object hashing
// ---------------------------------------------------------------------------

#[test]
fn test_empty_tree_matches_published_git_constant() {
    // `git hash-object -t tree /dev/null` is a well-known, stable constant.
    assert_eq!(
        compute_object_id("tree", b"").to_hex(),
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    );
}

#[test]
fn test_known_blob_id() {
    // `printf 'hello world\n' | git hash-object --stdin`
    assert_eq!(
        compute_object_id("blob", b"hello world\n").to_hex(),
        "3b18e512dba79e4c8300dd08aeb37f8e728b8dad"
    );
}

#[test]
fn test_object_id_hex_roundtrip() {
    let oid = compute_object_id("blob", b"x");
    let hex = oid.to_hex();
    assert_eq!(hex.len(), 40);
    assert_eq!(ObjectId::from_hex(&hex).unwrap(), oid);
}

#[test]
fn test_object_id_rejects_bad_hex() {
    assert!(ObjectId::from_hex("abc").is_err());
    assert!(ObjectId::from_hex(&"z".repeat(40)).is_err());
    assert!(ObjectId::from_hex(&"a".repeat(41)).is_err());
}

#[test]
fn test_object_id_accepts_uppercase_hex() {
    let lower = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
    let upper = lower.to_ascii_uppercase();
    assert_eq!(
        ObjectId::from_hex(lower).unwrap(),
        ObjectId::from_hex(&upper).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Loose objects, refs
// ---------------------------------------------------------------------------

#[test]
fn test_reads_loose_blob_tree_commit_and_walks_history() {
    let git = scratch("loose");
    let blob = write_loose(&git, "blob", b"export const a = 1;\n");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a.ts", blob)]));
    let c1 = write_loose(
        &git,
        "commit",
        &commit_payload(tree, &[], "c1 initial", 1_700_000_000),
    );
    let c2 = write_loose(
        &git,
        "commit",
        &commit_payload(tree, &[c1], "c2 follow", 1_700_000_100),
    );

    write_head(&git, "refs/heads/main");
    write_ref(&git, "refs/heads/main", c2);

    let mut store = GitObjectStore::open(&git).unwrap();
    let head = store.head_commit().unwrap();
    assert_eq!(head.oid, c2);
    assert_eq!(head.summary(), "c2 follow");
    assert_eq!(head.tree, tree);

    let files = flatten_tree(&store, head.tree).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files["a.ts"].0, 0o100644);
    assert_eq!(
        store.blob(files["a.ts"].1).unwrap(),
        b"export const a = 1;\n"
    );

    let history = store.walk_first_parent(head.oid, 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].oid, c2);
    assert_eq!(history[1].oid, c1);

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_history_walk_respects_limit() {
    let git = scratch("limit");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));

    let mut prev: Option<ObjectId> = None;
    let mut ids = Vec::new();
    for i in 0..6 {
        let parents: Vec<ObjectId> = prev.into_iter().collect();
        let c = write_loose(
            &git,
            "commit",
            &commit_payload(tree, &parents, &format!("c{i}"), 1_700_000_000 + i),
        );
        prev = Some(c);
        ids.push(c);
    }

    let store = GitObjectStore::open(&git).unwrap();
    let all = store.walk_first_parent(*ids.last().unwrap(), 100).unwrap();
    assert_eq!(all.len(), 6);

    let first3 = store.walk_first_parent(*ids.last().unwrap(), 3).unwrap();
    assert_eq!(first3.len(), 3);

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_resolves_refs_from_packed_refs_file() {
    let git = scratch("packedrefs");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));
    let commit = write_loose(&git, "commit", &commit_payload(tree, &[], "only", 1));

    // No loose ref exists; only packed-refs holds it.
    std::fs::write(
        git.join("packed-refs"),
        format!("# pack-refs with: peeled fully-peeled sorted \n{commit} refs/heads/main\n"),
    )
    .unwrap();
    write_head(&git, "refs/heads/main");

    let mut store = GitObjectStore::open(&git).unwrap();
    assert_eq!(store.resolve_ref("refs/heads/main").unwrap(), Some(commit));
    assert_eq!(store.head_commit().unwrap().oid, commit);

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_loose_ref_shadows_packed_ref() {
    let git = scratch("shadow");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));
    let old = write_loose(&git, "commit", &commit_payload(tree, &[], "old", 1));
    let new = write_loose(&git, "commit", &commit_payload(tree, &[old], "new", 2));

    std::fs::write(git.join("packed-refs"), format!("{old} refs/heads/main\n")).unwrap();
    write_ref(&git, "refs/heads/main", new);
    write_head(&git, "refs/heads/main");

    let mut store = GitObjectStore::open(&git).unwrap();
    assert_eq!(
        store.resolve_ref("refs/heads/main").unwrap(),
        Some(new),
        "loose ref must take precedence over packed-refs"
    );

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_resolves_annotated_tag_to_commit() {
    let git = scratch("tag");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));
    let commit = write_loose(&git, "commit", &commit_payload(tree, &[], "tagged", 1));

    let tag_payload =
        format!("object {commit}\ntype commit\ntag v1\ntagger T <t@t.t> 1 +0000\n\nrelease\n");
    let tag = write_loose(&git, "tag", tag_payload.as_bytes());
    write_head(&git, "refs/heads/main");
    write_ref(&git, "refs/heads/main", tag);

    let mut store = GitObjectStore::open(&git).unwrap();
    let head = store.head_commit().unwrap();
    assert_eq!(head.oid, commit, "tag must dereference to the commit");

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_head_unborn_reports_error() {
    let git = scratch("unborn");
    // HEAD points at a branch that does not exist yet.
    write_head(&git, "refs/heads/main");

    let mut store = GitObjectStore::open(&git).unwrap();
    assert!(store.resolve_ref("HEAD").unwrap().is_none());
    assert!(store.head_commit().is_err());

    let _ = std::fs::remove_dir_all(&git);
}

// ---------------------------------------------------------------------------
// Packed objects and delta resolution
// ---------------------------------------------------------------------------

#[test]
fn test_reads_non_delta_object_from_pack() {
    let git = scratch("packplain");
    let payload = b"packed blob contents";
    let oid = compute_object_id("blob", payload);
    let (pack, offsets) = build_pack(&[PackEntry::Object(3, payload.to_vec())]);
    write_index(
        &git.join("objects/pack"),
        "pack-t1",
        &pack,
        vec![(oid, offsets[0])],
    );

    let store = GitObjectStore::open(&git).unwrap();
    assert_eq!(store.packed_object_count(), 1);
    assert_eq!(store.blob(oid).unwrap(), payload);
    assert_eq!(store.object(oid).unwrap().type_name(), "blob");

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_resolves_ref_delta_against_packed_base() {
    let git = scratch("refdelta");
    let base = b"AAAA".to_vec();
    let base_oid = compute_object_id("blob", &base);

    // Delta: base_size=4, target_size=8, copy(offset 0, size 4), insert "BBBB".
    let mut delta = Vec::new();
    delta.extend_from_slice(&encode_size_varint(4));
    delta.extend_from_slice(&encode_size_varint(8));
    delta.push(0x80 | 0x10); // copy, size byte 0 present
    delta.push(4);
    delta.push(4); // insert 4 literal bytes
    delta.extend_from_slice(b"BBBB");

    let (pack, offsets) = build_pack(&[
        PackEntry::Object(3, base.clone()),
        PackEntry::RefDelta(base_oid, delta),
    ]);

    let target_oid = compute_object_id("blob", b"AAAABBBB");
    write_index(
        &git.join("objects/pack"),
        "pack-t2",
        &pack,
        vec![(base_oid, offsets[0]), (target_oid, offsets[1])],
    );

    let store = GitObjectStore::open(&git).unwrap();
    assert_eq!(store.blob(base_oid).unwrap(), b"AAAA");
    assert_eq!(
        store.blob(target_oid).unwrap(),
        b"AAAABBBB",
        "ref-delta must resolve via the packed base object"
    );

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_resolves_ofs_delta_against_packed_base() {
    let git = scratch("ofsdelta");
    let base = b"XXXXXXXXXX".to_vec();
    let base_oid = compute_object_id("blob", &base);

    let mut delta = Vec::new();
    delta.extend_from_slice(&encode_size_varint(10));
    delta.extend_from_slice(&encode_size_varint(12));
    delta.push(0x80 | 0x10); // copy from offset 0
    delta.push(10);
    delta.push(2); // insert 2 bytes
    delta.extend_from_slice(b"YZ");

    // The second entry's base is the first entry, at relative offset 0.
    let (pack, offsets) = build_pack(&[PackEntry::Object(3, base.clone())]);
    let second_offset = pack.len() as u64 - 20; // minus SHA-1 trailer
    let rel = second_offset - offsets[0];

    // Rebuild with both entries now that the relative offset is known.
    let (pack, offsets) = build_pack(&[
        PackEntry::Object(3, base.clone()),
        PackEntry::OfsDelta(rel, delta),
    ]);

    let target_oid = compute_object_id("blob", b"XXXXXXXXXXYZ");
    write_index(
        &git.join("objects/pack"),
        "pack-t3",
        &pack,
        vec![(base_oid, offsets[0]), (target_oid, offsets[1])],
    );

    let store = GitObjectStore::open(&git).unwrap();
    assert_eq!(
        store.blob(target_oid).unwrap(),
        b"XXXXXXXXXXYZ",
        "ofs-delta must resolve via the negative pack offset"
    );

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_loose_object_takes_precedence_over_pack() {
    let git = scratch("precedence");
    let payload = b"shared payload";
    let oid = compute_object_id("blob", payload);

    // Same object present both loose and packed.
    write_loose(&git, "blob", payload);
    let (pack, offsets) = build_pack(&[PackEntry::Object(3, payload.to_vec())]);
    write_index(
        &git.join("objects/pack"),
        "pack-t4",
        &pack,
        vec![(oid, offsets[0])],
    );

    let store = GitObjectStore::open(&git).unwrap();
    assert_eq!(store.blob(oid).unwrap(), payload);

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_missing_object_is_explicit_error() {
    let git = scratch("missing");
    let store = GitObjectStore::open(&git).unwrap();
    let absent = compute_object_id("blob", b"never written");
    match store.object(absent) {
        Err(GitError::ObjectNotFound(id)) => assert_eq!(id, absent.to_hex()),
        other => panic!("expected ObjectNotFound, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&git);
}

// ---------------------------------------------------------------------------
// Trees
// ---------------------------------------------------------------------------

#[test]
fn test_flatten_tree_is_sorted_and_descends_subtrees() {
    let git = scratch("flatten");
    let leaf = write_loose(&git, "blob", b"leaf");
    let inner = write_loose(&git, "tree", &tree_payload(&[(0o100644, "inner.ts", leaf)]));
    let root = write_loose(
        &git,
        "tree",
        &tree_payload(&[
            (0o40000, "src", inner),
            (0o100644, "z.ts", leaf),
            (0o100644, "a.ts", leaf),
        ]),
    );

    let store = GitObjectStore::open(&git).unwrap();
    let files = flatten_tree(&store, root).unwrap();

    let keys: Vec<&String> = files.keys().collect();
    assert_eq!(keys, vec!["a.ts", "src/inner.ts", "z.ts"]);
    assert_eq!(files["src/inner.ts"].0, 0o100644);

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_tree_entry_lookup_and_is_tree() {
    let git = scratch("treeentry");
    let blob = write_loose(&git, "blob", b"x");
    let sub = write_loose(&git, "tree", &[]);
    let root = write_loose(
        &git,
        "tree",
        &tree_payload(&[(0o100644, "f.ts", blob), (0o40000, "d", sub)]),
    );

    let store = GitObjectStore::open(&git).unwrap();
    let tree = store.tree(root).unwrap();
    assert!(!tree.get("f.ts").unwrap().is_tree());
    assert!(tree.get("d").unwrap().is_tree());
    assert!(tree.get("absent").is_none());

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_submodule_entry_is_not_descended() {
    let git = scratch("submodule");
    let gitlink = compute_object_id("commit", b"foreign");
    let root = write_loose(
        &git,
        "tree",
        &tree_payload(&[(0o160000, "vendor", gitlink)]),
    );

    let store = GitObjectStore::open(&git).unwrap();
    let files = flatten_tree(&store, root).unwrap();
    assert_eq!(files["vendor"].0, 0o160000, "gitlink is recorded as a leaf");

    let _ = std::fs::remove_dir_all(&git);
}

// ---------------------------------------------------------------------------
// Negative and adversarial cases
// ---------------------------------------------------------------------------

#[test]
fn test_rejects_directory_that_is_not_a_repository() {
    let dir = std::env::temp_dir().join(format!("autopsy-temporal-norepo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(matches!(
        GitObjectStore::discover(&dir),
        Err(GitError::NotARepository(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_rejects_pack_index_v1_magic() {
    let git = scratch("idxv1");
    let pack_dir = git.join("objects/pack");
    std::fs::create_dir_all(&pack_dir).unwrap();
    // v1 indexes begin with the fanout table (no magic).
    std::fs::write(pack_dir.join("pack-v1.idx"), vec![0u8; 1024 + 4]).unwrap();

    match GitObjectStore::open(&git) {
        Err(GitError::Unsupported(msg)) => assert!(msg.contains("v1"), "got: {msg}"),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_rejects_object_alternates() {
    let git = scratch("alternates");
    std::fs::create_dir_all(git.join("objects/info")).unwrap();
    std::fs::write(git.join("objects/info/alternates"), "/elsewhere/objects\n").unwrap();

    match GitObjectStore::open(&git) {
        Err(GitError::Unsupported(msg)) => assert!(msg.contains("alternates")),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_rejects_linked_worktree_commondir() {
    let git = scratch("commondir");
    std::fs::write(git.join("commondir"), "../..\n").unwrap();

    match GitObjectStore::open(&git) {
        Err(GitError::Unsupported(msg)) => assert!(msg.contains("worktree")),
        other => panic!("expected Unsupported, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_corrupt_loose_object_reports_inflate_or_malformed_error() {
    let git = scratch("corrupt");
    let oid = compute_object_id("blob", b"payload");
    let hex = oid.to_hex();
    let dir = git.join("objects").join(&hex[0..2]);
    std::fs::create_dir_all(&dir).unwrap();
    // Valid zlib stream, but the declared length disagrees with the payload.
    std::fs::write(dir.join(&hex[2..]), zlib(b"blob 999\0payload")).unwrap();

    let store = GitObjectStore::open(&git).unwrap();
    match store.blob(oid) {
        Err(GitError::Malformed(msg)) => assert!(msg.contains("length mismatch"), "got: {msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_truncated_pack_reports_error_not_panic() {
    let git = scratch("truncpack");
    let payload = b"contents that will be truncated";
    let oid = compute_object_id("blob", payload);
    let (pack, offsets) = build_pack(&[PackEntry::Object(3, payload.to_vec())]);
    let truncated = pack[..pack.len() / 2].to_vec();
    write_index(
        &git.join("objects/pack"),
        "pack-trunc",
        &pack,
        vec![(oid, offsets[0])],
    );
    // Overwrite the pack with its truncated form.
    std::fs::write(git.join("objects/pack/pack-trunc.pack"), &truncated).unwrap();

    let store = GitObjectStore::open(&git).unwrap();
    assert!(
        store.blob(oid).is_err(),
        "truncated pack must error, never panic"
    );

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_delta_base_size_mismatch_is_rejected() {
    let git = scratch("deltamismatch");
    let base = b"AAAA".to_vec();
    let base_oid = compute_object_id("blob", &base);

    // Declares base_size 99 while the real base is 4 bytes.
    let mut delta = Vec::new();
    delta.extend_from_slice(&encode_size_varint(99));
    delta.extend_from_slice(&encode_size_varint(8));
    delta.push(0x80 | 0x10);
    delta.push(4);
    delta.push(4);
    delta.extend_from_slice(b"BBBB");

    let (pack, offsets) = build_pack(&[
        PackEntry::Object(3, base.clone()),
        PackEntry::RefDelta(base_oid, delta),
    ]);
    // The id this delta *would* have if it were well-formed; the index must
    // still route the lookup to the delta entry so the mismatch surfaces.
    let target_oid = compute_object_id("blob", b"AAAABBBB");
    assert_ne!(target_oid, base_oid);
    write_index(
        &git.join("objects/pack"),
        "pack-mm",
        &pack,
        vec![(base_oid, offsets[0]), (target_oid, offsets[1])],
    );

    let store = GitObjectStore::open(&git).unwrap();
    match store.blob(target_oid) {
        Err(GitError::Malformed(msg)) => assert!(msg.contains("base size mismatch"), "got: {msg}"),
        other => panic!("expected Malformed, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_discover_works_from_nested_subdirectory() {
    let git = scratch("discover");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));
    let commit = write_loose(&git, "commit", &commit_payload(tree, &[], "c", 1));
    write_head(&git, "refs/heads/main");
    write_ref(&git, "refs/heads/main", commit);

    // `discover` expects a working tree containing `.git`; the scratch dir
    // *is* the .git dir, so simulate a worktree one level up.
    let worktree = git.parent().unwrap().join(format!(
        "autopsy-temporal-discover-wt-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&worktree);
    std::fs::create_dir_all(worktree.join("deep/nested")).unwrap();
    let real_git = worktree.join(".git");
    let _ = std::fs::remove_dir_all(&real_git);
    copy_dir(&git, &real_git);

    let store = GitObjectStore::discover(worktree.join("deep/nested")).unwrap();
    assert!(store.git_dir().ends_with(".git"));

    let _ = std::fs::remove_dir_all(&worktree);
    let _ = std::fs::remove_dir_all(&git);
}

fn copy_dir(from: &Path, to: &Path) {
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((src, dst)) = stack.pop() {
        std::fs::create_dir_all(&dst).unwrap();
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                stack.push((entry.path(), target));
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}

#[test]
fn test_git_object_type_names() {
    let git = scratch("types");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));

    let store = GitObjectStore::open(&git).unwrap();
    assert_eq!(store.object(blob).unwrap().type_name(), "blob");
    assert_eq!(store.object(tree).unwrap().type_name(), "tree");

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_commit_committer_timestamp_parsed_without_entering_digest() {
    let git = scratch("ts");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));
    let commit = write_loose(
        &git,
        "commit",
        &commit_payload(tree, &[], "c", 1_700_000_000),
    );

    let store = GitObjectStore::open(&git).unwrap();
    let c = store.commit(commit).unwrap();
    assert_eq!(c.committer_timestamp(), Some(1_700_000_000));

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_multiple_parents_are_preserved_and_merge_walk_uses_first_parent() {
    let git = scratch("merge");
    let blob = write_loose(&git, "blob", b"x");
    let tree = write_loose(&git, "tree", &tree_payload(&[(0o100644, "a", blob)]));
    let root = write_loose(&git, "commit", &commit_payload(tree, &[], "root", 1));
    let left = write_loose(&git, "commit", &commit_payload(tree, &[root], "left", 2));
    let right = write_loose(&git, "commit", &commit_payload(tree, &[root], "right", 3));
    let merge = write_loose(
        &git,
        "commit",
        &commit_payload(tree, &[left, right], "merge", 4),
    );

    let store = GitObjectStore::open(&git).unwrap();
    let m = store.commit(merge).unwrap();
    assert_eq!(m.parents, vec![left, right], "both parents preserved");

    let walk = store.walk_first_parent(merge, 10).unwrap();
    let summaries: Vec<&str> = walk.iter().map(|c| c.summary()).collect();
    assert_eq!(summaries, vec!["merge", "left", "root"]);

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_binary_blob_roundtrips_exactly() {
    let git = scratch("binary");
    let payload: Vec<u8> = (0u8..=255).collect();
    let oid = write_loose(&git, "blob", &payload);

    let store = GitObjectStore::open(&git).unwrap();
    assert_eq!(
        store.blob(oid).unwrap(),
        payload,
        "binary content must be byte-exact"
    );

    let _ = std::fs::remove_dir_all(&git);
}

#[test]
fn test_tree_map_is_deterministic_across_repeated_reads() {
    let git = scratch("determinism");
    let blob = write_loose(&git, "blob", b"x");
    let root = write_loose(
        &git,
        "tree",
        &tree_payload(&[
            (0o100644, "m.ts", blob),
            (0o100644, "a.ts", blob),
            (0o100644, "z.ts", blob),
        ]),
    );

    let store = GitObjectStore::open(&git).unwrap();
    let runs: Vec<BTreeMap<String, (u32, ObjectId)>> = (0..5)
        .map(|_| flatten_tree(&store, root).unwrap())
        .collect();
    for window in runs.windows(2) {
        assert_eq!(window[0], window[1], "flatten must be order-stable");
    }

    let _ = std::fs::remove_dir_all(&git);
}
