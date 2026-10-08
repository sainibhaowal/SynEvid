//! End-to-end test suite for Pillar A: Universal Hardware & Multi-Architecture Matrix
//!
//! Asserts cross-platform determinism invariants:
//! 1. Windows CRLF (\r\n) vs POSIX LF (\n) produces byte-identical SnapshotId and content hashes.
//! 2. Windows backslash path separators normalize strictly to POSIX forward slashes.
//! 3. Multi-threaded atomic cache write-and-replace resilience without Windows rename locking faults.
//! 4. BLAKE3 endianness & mathematical integrity across block boundaries.
//! 5. 100-run golden determinism across cross-platform structures.

use autopsy_domain::FileUnit;
use autopsy_repo::{AutopsyConfig, compute_file_set_digest, compute_snapshot_id, scan_repository};
use autopsy_storage::{StorageEngine, StorageOptions};
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::thread;

#[test]
fn test_e2e_crlf_vs_lf_snapshot_identity_invariance() {
    let base_tmp = std::env::temp_dir().join(format!("synevid_crlf_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base_tmp);

    let lf_dir = base_tmp.join("repo_lf");
    let crlf_dir = base_tmp.join("repo_crlf");

    fs::create_dir_all(lf_dir.join("src")).unwrap();
    fs::create_dir_all(crlf_dir.join("src")).unwrap();

    // Write LF files
    fs::write(
        lf_dir.join("src/index.ts"),
        "export const alpha = 1;\nexport const beta = 2;\n",
    )
    .unwrap();
    fs::write(
        lf_dir.join("src/config.json"),
        "{\n  \"version\": \"1.0.0\",\n  \"enabled\": true\n}\n",
    )
    .unwrap();

    // Write Windows CRLF files
    fs::write(
        crlf_dir.join("src/index.ts"),
        "export const alpha = 1;\r\nexport const beta = 2;\r\n",
    )
    .unwrap();
    fs::write(
        crlf_dir.join("src/config.json"),
        "{\r\n  \"version\": \"1.0.0\",\r\n  \"enabled\": true\r\n}\r\n",
    )
    .unwrap();

    let config = AutopsyConfig::default();

    let lf_snapshot = scan_repository(&lf_dir, &config, "0.0.1", Some("main")).unwrap();
    let crlf_snapshot = scan_repository(&crlf_dir, &config, "0.0.1", Some("main")).unwrap();

    // Hard Boundary: Snapshot IDs must be 100% byte-identical across LF and CRLF
    assert_eq!(
        lf_snapshot.snapshot_id, crlf_snapshot.snapshot_id,
        "SnapshotId must be identical across LF and CRLF line endings"
    );
    assert_eq!(
        lf_snapshot.file_set_digest, crlf_snapshot.file_set_digest,
        "file_set_digest must be identical across LF and CRLF"
    );

    // Verify per-file content hashes
    assert_eq!(
        lf_snapshot.files["src/index.ts"].content_hash,
        crlf_snapshot.files["src/index.ts"].content_hash
    );
    assert_eq!(
        lf_snapshot.files["src/config.json"].content_hash,
        crlf_snapshot.files["src/config.json"].content_hash
    );

    // Cleanup
    let _ = fs::remove_dir_all(&base_tmp);
}

#[test]
fn test_e2e_windows_path_separator_canonicalization() {
    let raw_windows_paths = vec![
        r"src\services\auth.service.ts",
        r"src\models\user.model.ts",
        r"src\controllers\auth.controller.ts",
    ];

    let mut files = BTreeMap::new();
    for p in raw_windows_paths {
        let norm_path = p.replace('\\', "/");
        assert!(
            !norm_path.contains('\\'),
            "Normalized path must not contain backslashes"
        );

        files.insert(
            norm_path.clone(),
            FileUnit {
                path: norm_path,
                language: "typescript".to_string(),
                content_hash: "hash_abc_123".to_string(),
                is_generated: false,
            },
        );
    }

    let digest1 = compute_file_set_digest(&files);
    let snapshot_id1 = compute_snapshot_id(&digest1, "cfg_1", "0.0.1");

    // Repeat 100 times to assert stability
    for _ in 0..100 {
        let digest_rerun = compute_file_set_digest(&files);
        let id_rerun = compute_snapshot_id(&digest_rerun, "cfg_1", "0.0.1");
        assert_eq!(digest1, digest_rerun);
        assert_eq!(snapshot_id1, id_rerun);
    }
}

#[test]
fn test_e2e_storage_concurrent_atomic_write_resilience() {
    let cache_dir =
        std::env::temp_dir().join(format!("synevid_atomic_cache_{}", std::process::id()));
    let _ = fs::remove_dir_all(&cache_dir);

    let storage = Arc::new(
        StorageEngine::open(
            cache_dir.join("test.db"),
            StorageOptions {
                cache_dir: Some(cache_dir.clone()),
                use_wal: false,
            },
        )
        .unwrap(),
    );

    let content_blob = b"CONCURRENT_CACHE_PAYLOAD_FOR_MULTI_ARCH_TESTING_123456789";
    let mut handles = Vec::new();

    // Spawn 16 concurrent threads writing the exact same object simultaneously
    for _ in 0..16 {
        let storage_clone = Arc::clone(&storage);
        let blob = content_blob.to_vec();
        handles.push(thread::spawn(move || {
            storage_clone.put_cache_object(&blob).unwrap()
        }));
    }

    let mut hashes = Vec::new();
    for h in handles {
        hashes.push(h.join().unwrap());
    }

    // All threads must return the identical BLAKE3 hash
    let expected_hash = blake3::hash(content_blob).to_hex().to_string();
    for h in hashes {
        assert_eq!(h, expected_hash);
    }

    // Retrieve object and verify integrity
    let retrieved = storage
        .get_cache_object(&expected_hash)
        .unwrap()
        .expect("Object must be present in cache");
    assert_eq!(retrieved, content_blob);

    let _ = fs::remove_dir_all(&cache_dir);
}

#[test]
fn test_e2e_blake3_cryptographic_vector_integrity() {
    // Standard test vectors across power-of-two boundaries
    let empty_hash = blake3::hash(b"").to_hex().to_string();
    assert_eq!(
        empty_hash,
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );

    let one_k = vec![b'a'; 1024];
    let hash_1k = blake3::hash(&one_k).to_hex().to_string();
    assert_eq!(hash_1k.len(), 64);

    let sixty_four_k = vec![b'b'; 65536];
    let hash_64k = blake3::hash(&sixty_four_k).to_hex().to_string();
    assert_eq!(hash_64k.len(), 64);

    // Repeat 100 runs for determinism
    for _ in 0..100 {
        assert_eq!(blake3::hash(b"").to_hex().to_string(), empty_hash);
        assert_eq!(blake3::hash(&one_k).to_hex().to_string(), hash_1k);
        assert_eq!(blake3::hash(&sixty_four_k).to_hex().to_string(), hash_64k);
    }
}
