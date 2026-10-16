//! Step 31 closure gate: the user-facing path for `detect_lineage`.
//!
//! The engine was implemented and unit-tested in Step 31 with nothing reaching it. These
//! tests hold the whole path together: real git objects on disk -> `GitObjectStore` ->
//! `flatten_tree` -> `FileEntry` -> `detect_lineage` -> exit code.
//!
//! # Why the fixture writes git objects by hand
//!
//! The obvious fixture is `git init` + `git mv`, and it is the wrong one: the analyzer's
//! sandbox invariant is *zero sub-process execution*, and a test that spawns git trains the
//! suite to depend on exactly the thing the engine must never do. A loose git object is
//! `zlib("<kind> <len>\0<payload>")` stored at `objects/<first 2 hex>/<rest>`, named by the
//! SHA-1 of the *uncompressed* form — four lines of fixture code, fully hermetic, and it
//! exercises the same object reader production does.
//!
//! # What the assertions are
//!
//! Invariants, not snapshots: a byte-identical rename must be `verified`, a similarity match
//! must be `partial` *with its candidate list*, and `--strict` must turn that partial into
//! `4`. The one number that may not drift is zero — no test here tolerates a `partial` event
//! being promoted to `verified`, because that direction is the whole point of the engine.

use autopsy_cli::extensions::{self, LineageArgs};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const PASS: i32 = 0;
const ANALYSIS_ERROR: i32 = 3;
const UNSUPPORTED: i32 = 4;

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let id = NEXT_SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "autopsy-cli-lineage-{}-{id}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn cli_for(repo_root: &Path) -> autopsy_cli::Cli {
    use clap::Parser;
    autopsy_cli::Cli::try_parse_from([
        "autopsy",
        "--repo-root",
        &repo_root.display().to_string(),
        "version",
    ])
    .expect("fixture cli")
}

fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    Sha1::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("fixture hex"))
        .collect()
}

/// A synthetic repository, written loose-object by loose-object.
struct Repo {
    git_dir: PathBuf,
}

impl Repo {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        let git_dir = root.join(".git");
        fs::create_dir_all(git_dir.join("objects")).expect("objects dir");
        fs::create_dir_all(git_dir.join("refs/heads")).expect("refs dir");
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        Self { git_dir }
    }

    fn root(&self) -> PathBuf {
        self.git_dir.parent().expect("worktree root").to_path_buf()
    }

    fn object(&self, kind: &str, payload: &[u8]) -> String {
        let mut raw = format!("{kind} {}\0", payload.len()).into_bytes();
        raw.extend_from_slice(payload);
        let hex = sha1_hex(&raw);
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw).expect("zlib write");
        let compressed = encoder.finish().expect("zlib finish");
        let dir = self.git_dir.join("objects").join(&hex[..2]);
        fs::create_dir_all(&dir).expect("loose object dir");
        fs::write(dir.join(&hex[2..]), compressed).expect("loose object");
        hex
    }

    fn blob(&self, content: &str) -> String {
        self.object("blob", content.as_bytes())
    }

    /// A flat tree. All fixtures are flat files, so git's "directories sort as if they ended
    /// in `/`" rule collapses to a plain name sort.
    fn tree(&self, entries: &[(&str, &str)]) -> String {
        let mut sorted: Vec<_> = entries.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let mut payload = Vec::new();
        for (name, oid) in sorted {
            payload.extend_from_slice(format!("100644 {name}\0").as_bytes());
            payload.extend_from_slice(&hex_to_bytes(oid));
        }
        self.object("tree", &payload)
    }

    fn commit(&self, tree: &str, parents: &[&str], message: &str) -> String {
        let mut payload = format!("tree {tree}\n");
        for parent in parents {
            payload.push_str(&format!("parent {parent}\n"));
        }
        payload.push_str("author Test <test@example.com> 1700000000 +0000\n");
        payload.push_str("committer Test <test@example.com> 1700000000 +0000\n");
        payload.push_str(&format!("\n{message}\n"));
        self.object("commit", payload.as_bytes())
    }

    fn set_main(&self, commit: &str) {
        fs::write(self.git_dir.join("refs/heads/main"), format!("{commit}\n"))
            .expect("refs/heads/main");
    }
}

/// `src/one.ts` renamed to `src/two.ts`, content byte-identical.
fn repo_exact_rename() -> (Repo, String, String) {
    let repo = Repo::new("exact-rename");
    let content = "export function calculateTotal(a: number, b: number) { return a + b; }";
    let one = repo.blob(content);
    let first_tree = repo.tree(&[("src/one.ts", &one)]);
    let first = repo.commit(&first_tree, &[], "one.ts");
    let two = repo.blob(content);
    let second_tree = repo.tree(&[("src/two.ts", &two)]);
    let second = repo.commit(&second_tree, &[&first], "rename to two.ts");
    repo.set_main(&second);
    (repo, first, second)
}

/// `src/one.ts` moved to `src/moved/one.ts` with a trailing comment: digest differs, tokens
/// are nearly identical, so the engine may only offer a candidate.
fn repo_similarity_move() -> Repo {
    let repo = Repo::new("similarity-move");
    let before = "export function calculateTotal(a: number, b: number) { return a + b; }";
    let one = repo.blob(before);
    let first = repo.commit(&repo.tree(&[("src/one.ts", &one)]), &[], "one.ts");
    let after = format!("{before} // note");
    let moved = repo.blob(&after);
    let second = repo.commit(
        &repo.tree(&[("src/moved/one.ts", &moved)]),
        &[&first],
        "move one.ts",
    );
    repo.set_main(&second);
    repo
}

/// Two identical files renamed to two identical names: no unique attribution exists.
fn repo_ambiguous_duplicates() -> Repo {
    let repo = Repo::new("ambiguous");
    let dup = "export const dup = 1;";
    let a = repo.blob(dup);
    let b = repo.blob(dup);
    let first = repo.commit(&repo.tree(&[("a.ts", &a), ("b.ts", &b)]), &[], "duplicates");
    let c = repo.blob(dup);
    let d = repo.blob(dup);
    let second = repo.commit(
        &repo.tree(&[("c.ts", &c), ("d.ts", &d)]),
        &[&first],
        "renamed duplicates",
    );
    repo.set_main(&second);
    repo
}

/// A single root commit: the default `from` is the empty tree.
fn repo_root_only() -> Repo {
    let repo = Repo::new("root-only");
    let one = repo.blob("export const x = 1;");
    let root = repo.commit(&repo.tree(&[("src/one.ts", &one)]), &[], "root");
    repo.set_main(&root);
    repo
}

/// Two commits, both with empty trees.
fn repo_two_empty_commits() -> Repo {
    let repo = Repo::new("empty");
    let first = repo.commit(&repo.tree(&[]), &[], "empty one");
    let second = repo.commit(&repo.tree(&[]), &[&first], "empty two");
    repo.set_main(&second);
    repo
}

/// An unchanged blob over any byte bound, plus an exact rename below it.
fn repo_oversize_and_rename() -> Repo {
    let repo = Repo::new("oversize");
    let big = repo.blob("this blob is well over eight bytes");
    let one = repo.blob("export const small = 1;");
    let first = repo.commit(
        &repo.tree(&[("big.bin", &big), ("src/one.ts", &one)]),
        &[],
        "first",
    );
    let two = repo.blob("export const small = 1;");
    let second = repo.commit(
        &repo.tree(&[("big.bin", &big), ("src/two.ts", &two)]),
        &[&first],
        "rename small",
    );
    repo.set_main(&second);
    repo
}

/// An addition only: nothing is renamed, so nothing is uncertain.
fn repo_clean_addition() -> Repo {
    let repo = Repo::new("clean-addition");
    let one = repo.blob("export const one = 1;");
    let first = repo.commit(&repo.tree(&[("src/one.ts", &one)]), &[], "first");
    let two = repo.blob("export const two = 2;");
    let second = repo.commit(
        &repo.tree(&[("src/one.ts", &one), ("src/two.ts", &two)]),
        &[&first],
        "add two.ts",
    );
    repo.set_main(&second);
    repo
}

fn run(repo: &Repo, args: &LineageArgs) -> extensions::Outcome {
    let cli = cli_for(&repo.root());
    extensions::lineage(&cli, args).expect("lineage handler")
}

fn json(outcome: &extensions::Outcome) -> serde_json::Value {
    serde_json::from_str(&outcome.json).expect("lineage output is JSON")
}

#[test]
fn lineage_default_range_verifies_an_exact_rename() {
    let (repo, first, second) = repo_exact_rename();
    let outcome = run(&repo, &LineageArgs::default());

    assert_eq!(outcome.exit_code, PASS);
    let value = json(&outcome);
    assert_eq!(value["coverage"], "verified");
    assert_eq!(value["from"]["resolved"], first);
    assert_eq!(value["to"]["resolved"], second);
    assert_eq!(value["from"]["files"], 1);
    assert_eq!(value["to"]["files"], 1);

    let events = value["events"].as_array().expect("events array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "renamed");
    assert_eq!(events[0]["from"], "src/one.ts");
    assert_eq!(events[0]["to"], "src/two.ts");
    assert_eq!(events[0]["coverage"], "verified");
    assert_eq!(events[0]["similarity"], 1.0);
    assert!(events[0]["ambiguity"].is_null());

    assert_eq!(value["summary"]["renamed"], 1);
    assert_eq!(value["summary"]["identity_preserved"], 1);
    assert_eq!(value["summary"]["partial"], 0);
    assert!(outcome.text.contains("coverage: verified"));
    assert!(outcome.text.contains("1 renamed"));
}

#[test]
fn lineage_tilde_and_hex_expressions_resolve_the_same_commit() {
    let (repo, first, _) = repo_exact_rename();
    let cli = cli_for(&repo.root());

    let by_tilde = extensions::lineage(
        &cli,
        &LineageArgs {
            from: Some("HEAD~1".to_string()),
            ..LineageArgs::default()
        },
    )
    .expect("tilde expression");
    assert_eq!(by_tilde.exit_code, PASS);
    assert_eq!(json(&by_tilde)["from"]["resolved"], first);

    let by_hex = extensions::lineage(
        &cli,
        &LineageArgs {
            from: Some(first.clone()),
            ..LineageArgs::default()
        },
    )
    .expect("hex expression");
    assert_eq!(by_hex.exit_code, PASS);
    assert_eq!(json(&by_hex)["from"]["resolved"], first);

    // HEAD~2 walks past the root commit; the expression parses, so this is a clear error,
    // not a silent empty comparison.
    let past_root = extensions::lineage(
        &cli,
        &LineageArgs {
            from: Some("HEAD~2".to_string()),
            ..LineageArgs::default()
        },
    );
    assert!(past_root.is_err());
}

#[test]
fn lineage_similarity_move_is_partial_and_strict_turns_it_into_four() {
    let repo = repo_similarity_move();
    let lenient = run(&repo, &LineageArgs::default());

    assert_eq!(lenient.exit_code, PASS);
    let value = json(&lenient);
    assert_eq!(value["coverage"], "partial");
    let moved = value["events"]
        .as_array()
        .expect("events")
        .iter()
        .find(|event| event["kind"] == "moved")
        .expect("a moved event");
    assert_eq!(moved["coverage"], "partial");
    // The engine's contract: a non-verified event always carries an `ambiguity` with a
    // stable reason. `candidates` lists *other* plausible counterparts and is legitimately
    // empty when the best match beat the runner-up by the required margin.
    assert!(
        !moved["ambiguity"]["reason"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "a partial event must say why it could not verify"
    );
    assert!(moved["ambiguity"]["candidates"].is_array());
    assert_eq!(value["summary"]["identity_preserved"], 0);

    let strict = run(
        &repo,
        &LineageArgs {
            strict: true,
            ..LineageArgs::default()
        },
    );
    assert_eq!(strict.exit_code, UNSUPPORTED);
}

#[test]
fn lineage_ambiguous_identical_files_never_assert_identity() {
    let repo = repo_ambiguous_duplicates();
    let outcome = run(&repo, &LineageArgs::default());

    assert_eq!(outcome.exit_code, PASS);
    let value = json(&outcome);
    assert_eq!(value["coverage"], "partial");
    assert_eq!(value["summary"]["identity_preserved"], 0);
    let renames: Vec<_> = value["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|event| event["kind"] == "renamed")
        .collect();
    assert!(
        !renames.is_empty(),
        "ambiguity must be surfaced, not dropped"
    );
    for rename in renames {
        assert_eq!(rename["coverage"], "partial");
        assert!(
            !rename["ambiguity"]["candidates"]
                .as_array()
                .expect("candidates")
                .is_empty()
        );
    }
}

#[test]
fn lineage_root_revision_reports_additions_and_says_why() {
    let repo = repo_root_only();
    let outcome = run(&repo, &LineageArgs::default());

    assert_eq!(outcome.exit_code, PASS);
    let value = json(&outcome);
    assert_eq!(value["coverage"], "verified");
    assert!(value["from"]["resolved"].is_null());
    assert_eq!(value["to"]["files"], 1);
    assert_eq!(value["summary"]["added"], 1);
    assert!(
        value["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|note| note.as_str().unwrap_or("").contains("root commit")),
        "the empty comparison must be explained"
    );
}

#[test]
fn lineage_two_empty_revisions_are_unsupported() {
    let repo = repo_two_empty_commits();
    let outcome = run(&repo, &LineageArgs::default());

    assert_eq!(outcome.exit_code, UNSUPPORTED);
    let value = json(&outcome);
    assert_eq!(value["coverage"], "unsupported");
    assert_eq!(value["summary"]["total"], 0);
    assert!(
        value["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|note| note.as_str().unwrap_or("").contains("no files"))
    );
}

#[test]
fn lineage_unknown_revision_and_bad_arguments_are_analysis_errors() {
    let repo = repo_clean_addition();
    let cli = cli_for(&repo.root());

    let missing = extensions::lineage(
        &cli,
        &LineageArgs {
            from: Some("no-such-branch".to_string()),
            ..LineageArgs::default()
        },
    );
    assert!(missing.is_err(), "a missing revision must not pass");

    let caret = extensions::lineage(
        &cli,
        &LineageArgs {
            from: Some("HEAD^".to_string()),
            ..LineageArgs::default()
        },
    );
    assert!(
        caret.is_err(),
        "unsupported selectors must be refused, not guessed"
    );

    let bad_threshold = extensions::lineage(
        &cli,
        &LineageArgs {
            threshold: Some(2.0),
            ..LineageArgs::default()
        },
    );
    assert!(bad_threshold.is_err());

    let bad_margin = extensions::lineage(
        &cli,
        &LineageArgs {
            margin: Some(f64::NAN),
            ..LineageArgs::default()
        },
    );
    assert!(bad_margin.is_err());

    let bad_bound = extensions::lineage(
        &cli,
        &LineageArgs {
            max_files: 0,
            ..LineageArgs::default()
        },
    );
    assert!(bad_bound.is_err());
}

#[test]
fn lineage_clean_history_is_verified_and_exits_zero() {
    let repo = repo_clean_addition();
    let outcome = run(&repo, &LineageArgs::default());

    assert_eq!(outcome.exit_code, PASS);
    let value = json(&outcome);
    assert_eq!(value["coverage"], "verified");
    assert_eq!(value["summary"]["added"], 1);
    assert_eq!(value["summary"]["partial"], 0);
    assert_eq!(value["summary"]["identity_preserved"], 0);
}

#[test]
fn lineage_oversize_skip_forces_partial_and_is_accounted_for() {
    let repo = repo_oversize_and_rename();
    let lenient = run(
        &repo,
        &LineageArgs {
            max_file_bytes: 30,
            ..LineageArgs::default()
        },
    );

    assert_eq!(lenient.exit_code, PASS);
    let value = json(&lenient);
    assert_eq!(value["coverage"], "partial");
    assert_eq!(value["to"]["oversize_skipped"], 1);
    assert_eq!(value["from"]["oversize_skipped"], 1);
    // The rename below the bound is still found; the skip only widens the caveat.
    assert_eq!(value["summary"]["renamed"], 1);
    assert!(
        value["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|note| note.as_str().unwrap_or("").contains("skipped")),
        "skipped blobs must be disclosed"
    );

    let strict = run(
        &repo,
        &LineageArgs {
            max_file_bytes: 30,
            strict: true,
            ..LineageArgs::default()
        },
    );
    assert_eq!(strict.exit_code, UNSUPPORTED);
}

#[test]
fn lineage_is_deterministic_across_runs() {
    let repo = repo_similarity_move();
    let first = run(&repo, &LineageArgs::default());
    let second = run(&repo, &LineageArgs::default());
    assert_eq!(first.json, second.json);
    assert_eq!(first.text, second.text);
    assert_eq!(first.exit_code, second.exit_code);
}

#[test]
fn lineage_truncated_enumeration_is_a_stated_lower_bound() {
    // Two paths at `to`, a bound of one: `src/two.ts` never reaches the engine, so the
    // answer must say it is incomplete instead of claiming "nothing else changed".
    let repo = repo_clean_addition();
    let outcome = run(
        &repo,
        &LineageArgs {
            max_files: 1,
            ..LineageArgs::default()
        },
    );

    assert_eq!(outcome.exit_code, PASS);
    let value = json(&outcome);
    assert_eq!(value["coverage"], "partial");
    assert_eq!(value["from"]["truncated"], false);
    assert_eq!(value["to"]["truncated"], true);
    assert_eq!(value["summary"]["total"], 0);
    assert!(
        value["notes"]
            .as_array()
            .expect("notes")
            .iter()
            .any(|note| note.as_str().unwrap_or("").contains("lower bound")),
        "a truncated comparison must say the event set is a lower bound"
    );
}

#[test]
fn lineage_refuses_a_directory_that_is_not_a_repository() {
    let dir = scratch("no-git");
    let cli = cli_for(&dir);
    let outcome = extensions::lineage(&cli, &LineageArgs::default());
    assert!(
        outcome.is_err(),
        "a directory without .git is a config error"
    );
    let error = outcome.expect_err("error").to_string();
    assert!(!error.is_empty());
}

#[test]
fn lineage_never_reports_an_identity_claim_for_unrelated_content() {
    let repo = Repo::new("unrelated");
    let alpha = repo.blob("const alpha = computeAlpha();");
    let first = repo.commit(&repo.tree(&[("src/alpha.ts", &alpha)]), &[], "alpha");
    let beta = repo.blob("class Beta { gamma() { return 9; } }");
    let second = repo.commit(&repo.tree(&[("src/beta.ts", &beta)]), &[&first], "beta");
    repo.set_main(&second);

    let outcome = run(&repo, &LineageArgs::default());
    // Whatever the exit code, the invariant is: no verified rename, ever.
    let value = json(&outcome);
    let confident_renames: Vec<_> = value["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|event| event["kind"] == "renamed" && event["coverage"] == "verified")
        .collect();
    assert!(
        confident_renames.is_empty(),
        "unrelated content must not produce a confident rename"
    );
    assert_eq!(value["summary"]["added"], 1);
    assert_eq!(value["summary"]["removed"], 1);
}

/// The one comparison the schema conformance test needs: every kind the engine can emit.
fn repo_all_event_kinds() -> Repo {
    let repo = Repo::new("all-event-kinds");
    let modified_before = repo.blob("export const m = 1;");
    let renamed = repo.blob("export const r = 1;");
    let moved_before = repo.blob("export function moveMe(x: number) { return x * 2; }");
    let removed = repo.blob("export const gone = 1;");
    let first = repo.commit(
        &repo.tree(&[
            ("src/modified.ts", &modified_before),
            ("src/rename_a.ts", &renamed),
            ("src/move.ts", &moved_before),
            ("src/removed.ts", &removed),
        ]),
        &[],
        "first",
    );

    let modified_after = repo.blob("export const m = 2;");
    let moved_after = repo.blob("export function moveMe(x: number) { return x * 2; } // note");
    let added = repo.blob("export const fresh = 1;");
    let second = repo.commit(
        &repo.tree(&[
            ("src/modified.ts", &modified_after),
            ("src/rename_b.ts", &renamed),
            ("src/moved/move.ts", &moved_after),
            ("src/added.ts", &added),
        ]),
        &[&first],
        "one of each",
    );
    repo.set_main(&second);
    repo
}

/// Names the engine's kind vocabulary through an **exhaustive** match.
///
/// This is deliberately not a list. A new `LineageEventKind` variant makes this match
/// non-exhaustive, so the schema-conformance test stops compiling — the failure mode is a
/// build error in the gate, not a schema that quietly rejects the engine's own output.
fn engine_kind_name(kind: autopsy_temporal::LineageEventKind) -> &'static str {
    use autopsy_temporal::LineageEventKind::{Added, Modified, Moved, Removed, Renamed};
    match kind {
        Renamed => "renamed",
        Moved => "moved",
        Modified => "modified",
        Added => "added",
        Removed => "removed",
    }
}

/// Set equality in both directions: the schema declares exactly what the envelope carries,
/// and the engine's vocabulary is exactly the schema's enum.
#[test]
fn gate_the_lineage_schema_declares_exactly_what_the_envelope_carries() {
    let repo = repo_all_event_kinds();
    let outcome = run(&repo, &LineageArgs::default());
    assert_eq!(outcome.exit_code, PASS);
    let document = json(&outcome);
    // The fixture must exercise every branch the schema describes.
    assert_eq!(document["coverage"], "partial");
    assert_eq!(document["summary"]["total"], 5);

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/lineage.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("schema")).expect("valid JSON");

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    assert_eq!(
        keys(&document),
        keys(&schema["properties"]),
        "envelope fields"
    );
    assert_eq!(
        schema["required"],
        serde_json::json!(["from", "to", "events", "summary", "coverage", "notes"]),
        "all six fields are always present"
    );
    assert_eq!(
        schema["additionalProperties"],
        serde_json::json!(false),
        "the envelope is closed"
    );

    for (name, def) in schema["$defs"].as_object().expect("defs") {
        assert_eq!(
            def["additionalProperties"],
            serde_json::json!(false),
            "{name} must be closed"
        );
        let mut required: Vec<String> = def["required"]
            .as_array()
            .expect("every definition has required")
            .iter()
            .map(|value| value.as_str().expect("string").to_string())
            .collect();
        required.sort();
        assert_eq!(
            required,
            keys(&def["properties"]),
            "{name}: everything declared is required and nothing else"
        );
    }

    // The revision views: same fields on both sides, compared against a real one.
    for side in ["from", "to"] {
        assert_eq!(
            keys(&document[side]),
            keys(&schema["$defs"]["revision"]["properties"]),
            "{side} fields"
        );
    }

    // The summary: every counter the schema declares is emitted and computed.
    assert_eq!(
        keys(&document["summary"]),
        keys(&schema["$defs"]["summary"]["properties"]),
        "summary fields"
    );

    let declared_event = keys(&schema["$defs"]["event"]["properties"]);
    let ambiguity_fields = keys(&schema["$defs"]["event"]["properties"]["ambiguity"]["properties"]);
    let events = document["events"].as_array().expect("events");
    assert!(!events.is_empty(), "the fixture produces one of each kind");
    let mut emitted_kinds: Vec<String> = Vec::new();
    for event in events {
        assert_eq!(keys(event), declared_event, "event fields");
        emitted_kinds.push(event["kind"].as_str().expect("kind").to_string());
        let coverage = event["coverage"].as_str().expect("coverage");
        if coverage == "verified" {
            assert!(
                event["ambiguity"].is_null(),
                "a verified event carries no ambiguity"
            );
        } else {
            assert_eq!(coverage, "partial", "events are verified or partial");
            assert_eq!(
                keys(&event["ambiguity"]),
                ambiguity_fields,
                "ambiguity fields"
            );
            assert!(
                event["ambiguity"]["reason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()),
                "a partial event must say why it could not verify"
            );
        }
    }

    use autopsy_temporal::LineageEventKind::{Added, Modified, Moved, Removed, Renamed};
    let mut vocabulary: Vec<String> = [Renamed, Moved, Modified, Added, Removed]
        .into_iter()
        .map(|kind| engine_kind_name(kind).to_string())
        .collect();
    vocabulary.sort();

    let mut declared_kinds: Vec<String> = schema["$defs"]["event"]["properties"]["kind"]["enum"]
        .as_array()
        .expect("the kind vocabulary")
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect();
    declared_kinds.sort();
    assert_eq!(
        declared_kinds, vocabulary,
        "the schema's vocabulary must be the engine's"
    );

    emitted_kinds.sort();
    emitted_kinds.dedup();
    assert_eq!(
        emitted_kinds, declared_kinds,
        "every declared kind must be producible by a real comparison"
    );

    let mut declared_coverage: Vec<String> =
        schema["$defs"]["event"]["properties"]["coverage"]["enum"]
            .as_array()
            .expect("the event coverage vocabulary")
            .iter()
            .map(|value| value.as_str().expect("string").to_string())
            .collect();
    declared_coverage.sort();
    assert_eq!(
        declared_coverage,
        vec!["partial".to_string(), "verified".to_string()],
        "the engine emits verified or partial on an event and nothing else"
    );
}

/// The exit-code constant itself is re-asserted here so a change to `EXIT_ANALYSIS_ERROR`
/// cannot silently renumber the argument-validation failures above.
#[test]
fn lineage_exit_codes_are_the_documented_ones() {
    assert_eq!(PASS, 0);
    assert_eq!(ANALYSIS_ERROR, 3);
    assert_eq!(UNSUPPORTED, 4);
    assert_eq!(autopsy_cli::EXIT_ANALYSIS_ERROR, ANALYSIS_ERROR);
    assert_eq!(autopsy_cli::EXIT_UNSUPPORTED, UNSUPPORTED);
}
