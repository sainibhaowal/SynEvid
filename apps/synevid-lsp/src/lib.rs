//! `synevid-lsp` — the IDE-native surface (Tier 3.2).
//!
//! A minimal, synchronous Language Server Protocol implementation over stdio that
//! answers `textDocument/hover` with the engine's impact analysis: the symbol at the
//! cursor, its declaration site, its direct callers/callees, the 1-hop impact size,
//! and the coverage state of the edges behind those numbers.
//!
//! # Architecture
//!
//! The server is a *presentation* surface: it depends on the engine's public library
//! API (`autopsy_cli::build_snapshot_and_graph`) and never the reverse. It runs as
//! whatever process the editor starts — no async runtime, no subprocess spawning, no
//! sockets — which keeps the zero-execution/zero-network invariants trivially intact
//! and makes the whole loop testable over in-memory buffers.
//!
//! The analysis is built **lazily on the first hover**, cached, and invalidated when
//! the editor reports a save (`textDocument/didSave`) or watched-file change. Hover
//! reads the *live buffer* for the word under the cursor (via didOpen/didChange
//! overlays) but resolves it against the last analyzed snapshot — so an unsaved edit
//! can move the cursor onto a word that does not yet exist in the graph, and the
//! honest answer for that is `null`.
//!
//! # Protocol surface
//!
//! | Method | Kind | Behavior |
//! |---|---|---|
//! | `initialize` | request | Advertises `hoverProvider` + full-sync text documents; resolves the workspace root (explicit flag > `rootUri` > workspace folders > cwd) |
//! | `initialized` | notification | No-op |
//! | `textDocument/didOpen` / `didChange` / `didClose` | notification | Maintains the live text overlay |
//! | `textDocument/didSave` / `workspace/didChangeWatchedFiles` | notification | Invalidates the cached analysis |
//! | `textDocument/hover` | request | Impact markdown, ambiguity list, or `null` |
//! | `shutdown` | request | Acknowledges; sets the clean-exit flag |
//! | `exit` | notification | Terminates; exit code 0 after `shutdown`, 1 otherwise (spec) |
//! | anything else | request | `-32601 MethodNotFound` |
//! | anything else | notification | Ignored (spec) |

pub mod hover;
pub mod protocol;

use autopsy_cli::{AnalysisArtifacts, build_snapshot_and_graph, load_or_default_config};
use hover::Resolution;
use protocol::{Incoming, METHOD_NOT_FOUND, error_response, notification, response, write_message};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

/// The server name reported in `initialize`.
pub const SERVER_NAME: &str = "synevid-lsp";

/// Options for one server process.
#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// Explicit workspace root (`--repo-root`). When absent, the root comes from the
    /// client's `initialize` request (`rootUri`, then the first workspace folder),
    /// falling back to the process CWD.
    pub repo_root: Option<PathBuf>,
}

/// How the session ended. LSP spells the rule out: `exit` after a `shutdown` is a
/// clean 0; `exit` without one is a 1 — and a stream that just ends keeps the same
/// rule, so a crashed client is distinguishable from a polite one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServeOutcome {
    pub exit_code: i32,
}

/// The lazily-built engine analysis, cached until a save/watch event says the
/// on-disk state moved.
pub struct EngineBridge {
    root: PathBuf,
    artifacts: Option<AnalysisArtifacts>,
    failure: Option<String>,
}

impl EngineBridge {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            artifacts: None,
            failure: None,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Drops the cached analysis; the next `ensure` re-analyzes.
    pub fn invalidate(&mut self) {
        self.artifacts = None;
        self.failure = None;
    }

    /// Builds the analysis on first use. A failure is cached too — a broken repo
    /// should answer every hover with the same honest message, not re-run the
    /// analyzer on every keystroke.
    pub fn ensure(&mut self) -> Result<&AnalysisArtifacts, String> {
        if self.artifacts.is_none() && self.failure.is_none() {
            match Self::build(&self.root) {
                Ok(artifacts) => self.artifacts = Some(artifacts),
                Err(failure) => self.failure = Some(failure),
            }
        }
        match (&self.artifacts, &self.failure) {
            (Some(artifacts), _) => Ok(artifacts),
            (_, Some(failure)) => Err(failure.clone()),
            _ => unreachable!("ensure builds or fails"),
        }
    }

    fn build(root: &Path) -> Result<AnalysisArtifacts, String> {
        if !root.is_dir() {
            return Err(format!(
                "workspace root `{}` does not exist or is not a directory",
                root.display()
            ));
        }
        let config = load_or_default_config(root).map_err(|e| e.to_string())?;
        build_snapshot_and_graph(root, &config, None, None).map_err(|e| e.to_string())
    }
}

/// Runs the server loop to completion. `reader`/`writer` are the client channel —
/// stdin/stdout in production, in-memory buffers in tests.
pub fn serve_loop<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    options: ServeOptions,
) -> std::io::Result<ServeOutcome> {
    let mut bridge: Option<EngineBridge> = None;
    let mut overlays: BTreeMap<String, String> = BTreeMap::new();
    let mut shutdown_received = false;

    while let Some(message) = protocol::read_message(&mut reader)? {
        match message {
            Incoming::Request { id, method, params } => match method.as_str() {
                "initialize" => {
                    let root = resolve_root(&options, &params);
                    bridge = Some(EngineBridge::new(root));
                    write_message(&mut writer, &response(&id, initialize_result()))?;
                }
                "shutdown" => {
                    shutdown_received = true;
                    write_message(&mut writer, &response(&id, Value::Null))?;
                }
                "textDocument/hover" => {
                    let result = hover_value(bridge.as_mut(), &overlays, &params);
                    write_message(&mut writer, &response(&id, result))?;
                }
                other => {
                    write_message(
                        &mut writer,
                        &error_response(
                            &id,
                            METHOD_NOT_FOUND,
                            &format!("{SERVER_NAME} does not implement `{other}`"),
                        ),
                    )?;
                }
            },
            Incoming::Notification { method, params } => match method.as_str() {
                "exit" => {
                    return Ok(ServeOutcome {
                        exit_code: if shutdown_received { 0 } else { 1 },
                    });
                }
                "textDocument/didOpen" => {
                    if let (Some(uri), Some(text)) = (
                        params.pointer("/textDocument/uri").and_then(Value::as_str),
                        params.pointer("/textDocument/text").and_then(Value::as_str),
                    ) {
                        overlays.insert(uri.to_string(), text.to_string());
                    }
                }
                "textDocument/didChange" => {
                    // We advertise full-sync (`change: 1`), so a change is the whole
                    // document. A range-bearing (incremental) change is not something
                    // this server promised to handle: drop the overlay rather than
                    // apply a partial edit wrongly, and let the next hover fall back
                    // to disk.
                    let uri = params.pointer("/textDocument/uri").and_then(Value::as_str);
                    if let Some(uri) = uri {
                        let full_text = params
                            .pointer("/contentChanges/0")
                            .filter(|change| change.get("range").is_none())
                            .and_then(|change| change.get("text"))
                            .and_then(Value::as_str);
                        match full_text {
                            Some(text) => {
                                overlays.insert(uri.to_string(), text.to_string());
                            }
                            None => {
                                overlays.remove(uri);
                            }
                        }
                    }
                }
                "textDocument/didClose" => {
                    if let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) {
                        overlays.remove(uri);
                    }
                }
                "textDocument/didSave" | "workspace/didChangeWatchedFiles" => {
                    if let Some(bridge) = bridge.as_mut() {
                        bridge.invalidate();
                    }
                }
                _ => {} // The spec: unknown notifications are ignored.
            },
        }
    }

    Ok(ServeOutcome {
        exit_code: if shutdown_received { 0 } else { 1 },
    })
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": { "openClose": true, "change": 1 },
            "hoverProvider": true
        },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") }
    })
}

/// Resolves the workspace root: explicit flag first, then the client's `rootUri`,
/// then the first workspace folder, then the process CWD.
pub fn resolve_root(options: &ServeOptions, initialize_params: &Value) -> PathBuf {
    if let Some(root) = &options.repo_root {
        return root.clone();
    }
    if let Some(path) = initialize_params
        .get("rootUri")
        .and_then(Value::as_str)
        .and_then(uri_to_path)
    {
        return path;
    }
    if let Some(path) = initialize_params
        .pointer("/workspaceFolders/0/uri")
        .and_then(Value::as_str)
        .and_then(uri_to_path)
    {
        return path;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// `file://` URI → filesystem path, percent-decoded, Windows drive letters unescaped
/// (`file:///c:/x` → `c:/x`). Non-file URIs return `None`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    let normalized =
        if decoded.len() >= 3 && decoded.as_bytes()[0] == b'/' && decoded.as_bytes()[2] == b':' {
            decoded[1..].to_string()
        } else {
            decoded
        };
    Some(PathBuf::from(normalized))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The line at a 0-based LSP line index, with any `\r` trimmed.
fn line_of(text: &str, line0: u32) -> String {
    text.split('\n')
        .nth(line0 as usize)
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string()
}

fn relative_posix(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

fn utf16_len(chars: &[char]) -> u32 {
    chars.iter().map(|c| c.len_utf16() as u32).sum()
}

fn hover_payload(markdown: String, line0: u32, start_u16: u32, end_u16: u32) -> Value {
    json!({
        "contents": { "kind": "markdown", "value": markdown },
        "range": {
            "start": { "line": line0, "character": start_u16 },
            "end": { "line": line0, "character": end_u16 }
        }
    })
}

/// Computes the hover response for one `textDocument/hover` request.
fn hover_value(
    bridge: Option<&mut EngineBridge>,
    overlays: &BTreeMap<String, String>,
    params: &Value,
) -> Value {
    let Some(bridge) = bridge else {
        return Value::Null; // Hover before initialize: nothing to say, per spec.
    };
    let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
        return Value::Null;
    };
    let line0 = params
        .pointer("/position/line")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let character = params
        .pointer("/position/character")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    let path = uri_to_path(uri);
    // The live buffer wins for reading the word under the cursor; disk is the
    // fallback when the editor never opened the document.
    let line = match overlays.get(uri) {
        Some(text) => line_of(text, line0),
        None => match path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(text) => line_of(&text, line0),
            None => return Value::Null,
        },
    };
    let Some(words) = hover::cursor_words(&line, character) else {
        return Value::Null;
    };

    let chars: Vec<char> = line.chars().collect();
    let start_u16 = utf16_len(&chars[..words.simple.start]);
    let end_u16 = utf16_len(&chars[..words.simple.end]);

    let doc_rel = path
        .as_ref()
        .and_then(|p| relative_posix(bridge.root(), p))
        .map(|rel| rel.to_string());

    let markdown = match bridge.ensure() {
        Err(failure) => format!(
            "**Synevid** — analysis unavailable: {failure}\n\nRun `autopsy doctor` in the workspace root.\n"
        ),
        Ok(artifacts) => match hover::resolve(
            artifacts,
            doc_rel.as_deref(),
            line0.saturating_add(1),
            &words,
        ) {
            Resolution::None => return Value::Null,
            Resolution::Ambiguous(hits) => hover::render_ambiguous(&words.simple_text, &hits),
            Resolution::Exact(symbol) => {
                hover::render_hover(symbol, &artifacts.graph, &artifacts.coverage_state)
            }
        },
    };

    hover_payload(markdown, line0, start_u16, end_u16)
}

/// A `window/logMessage` note the server may emit when analysis fails (kept for hosts
/// that surface server-side logs; deterministic content, no timestamps).
pub fn log_message(kind: u8, message: &str) -> Value {
    notification(
        "window/logMessage",
        json!({ "type": kind, "message": message }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn frame(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
    }

    /// A tiny two-file TypeScript workspace: `services.ts` declares a class with a
    /// method and a free function; `app.ts` calls the method from `main`.
    fn fixture_repo(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("synevid-lsp-fixture-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/services.ts"),
            "export class UserService {\n  create(payload: string): string {\n    return helper(payload);\n  }\n}\n\nexport function helper(x: string): string {\n  return x;\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/app.ts"),
            "import { UserService } from './services';\n\nexport function main(): string {\n  const svc = new UserService();\n  return svc.create('x');\n}\n",
        )
        .unwrap();
        dir
    }

    /// Runs a full session over in-memory buffers and returns the raw output frames
    /// plus the process-style exit code.
    fn run_session(root: &Path, messages: &[&str]) -> (Vec<Value>, ServeOutcome) {
        let mut input = String::new();
        for body in messages {
            input.push_str(&frame(body));
        }
        let mut output: Vec<u8> = Vec::new();
        let outcome = serve_loop(
            Cursor::new(input.into_bytes()),
            &mut output,
            ServeOptions {
                repo_root: Some(root.to_path_buf()),
            },
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        let mut frames = Vec::new();
        let mut rest = text.as_str();
        while let Some(idx) = rest.find("\r\n\r\n") {
            let headers = &rest[..idx];
            let length: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .unwrap()
                .parse()
                .unwrap();
            let body_start = idx + 4;
            let body = &rest[body_start..body_start + length];
            frames.push(serde_json::from_str(body).unwrap());
            rest = &rest[body_start + length..];
        }
        (frames, outcome)
    }

    fn response_for(frames: &[Value], id: i64) -> &Value {
        frames
            .iter()
            .find(|f| f.get("id") == Some(&json!(id)))
            .expect("response with the id")
    }

    #[test]
    fn initialize_advertises_hover_and_sync() {
        let root = fixture_repo("init");
        let (frames, _) = run_session(
            &root,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
                r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
            ],
        );
        let init = response_for(&frames, 1);
        assert_eq!(init["result"]["capabilities"]["hoverProvider"], json!(true));
        assert_eq!(
            init["result"]["capabilities"]["textDocumentSync"]["change"],
            json!(1),
            "full sync"
        );
        assert_eq!(init["result"]["serverInfo"]["name"], json!(SERVER_NAME));
    }

    #[test]
    fn hover_resolves_a_called_method_with_impact_numbers() {
        let root = fixture_repo("hover");
        let app_text = std::fs::read_to_string(root.join("src/app.ts")).unwrap();
        let uri = format!("file://{}", root.join("src/app.ts").display());
        let target_line = app_text
            .lines()
            .position(|l| l.contains("svc.create"))
            .unwrap();
        let line_text = app_text.lines().nth(target_line).unwrap();
        let col = (line_text.find("create").unwrap() + "create".len()) as u32;

        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": &uri, "languageId": "typescript", "version": 1, "text": app_text } }
        })
        .to_string();
        let hover = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": &uri },
                "position": { "line": target_line, "character": col }
            }
        })
        .to_string();

        let (frames, outcome) = run_session(
            &root,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                &open,
                &hover,
                r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
                r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
            ],
        );
        assert_eq!(outcome.exit_code, 0);
        let result = &response_for(&frames, 7)["result"];
        let markdown = result["contents"]["value"].as_str().unwrap();
        assert!(
            markdown.contains("UserService.create"),
            "resolved the method: {markdown}"
        );
        assert!(
            markdown.contains("Declared `src/services.ts:"),
            "declaration site is the other file: {markdown}"
        );
        assert!(markdown.contains("Callers **"), "impact numbers present");
        assert!(markdown.contains("Engine coverage `"), "coverage stated");
        assert_eq!(result["range"]["start"]["line"], json!(target_line));
    }

    #[test]
    fn hover_on_a_declaration_resolves_to_that_declaration() {
        let root = fixture_repo("decl");
        let services_text = std::fs::read_to_string(root.join("src/services.ts")).unwrap();
        let uri = format!("file://{}", root.join("src/services.ts").display());
        let line_idx = services_text
            .lines()
            .position(|l| l.contains("function helper"))
            .unwrap();
        let col = (services_text
            .lines()
            .nth(line_idx)
            .unwrap()
            .find("helper")
            .unwrap()
            + 2) as u32;

        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": &uri, "languageId": "typescript", "version": 1, "text": services_text } }
        })
        .to_string();
        let hover = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/hover",
            "params": { "textDocument": { "uri": &uri }, "position": { "line": line_idx, "character": col } }
        })
        .to_string();

        let (frames, _) = run_session(
            &root,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                &open,
                &hover,
                r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
                r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
            ],
        );
        let markdown = response_for(&frames, 9)["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(markdown.contains("helper"), "{markdown}");
        assert!(
            markdown.contains("Declared `src/services.ts:"),
            "{markdown}"
        );
    }

    #[test]
    fn hover_uses_the_live_overlay_not_the_disk() {
        let root = fixture_repo("overlay");
        // A document that does not exist on disk: only the overlay carries its text.
        let uri = format!("file://{}/src/virtual.ts", root.display());
        let text = "export function shadow(): string {\n  return helper('x');\n}\n";
        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": &uri, "languageId": "typescript", "version": 1, "text": text } }
        })
        .to_string();
        let hover = json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "textDocument/hover",
            "params": { "textDocument": { "uri": &uri }, "position": { "line": 1, "character": 10 } }
        })
        .to_string();

        let (frames, _) = run_session(
            &root,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                &open,
                &hover,
                r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
                r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
            ],
        );
        let markdown = response_for(&frames, 11)["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            markdown.contains("helper"),
            "the overlay's `helper` call resolved against the graph: {markdown}"
        );
    }

    #[test]
    fn unknown_methods_get_method_not_found() {
        let root = fixture_repo("unknown");
        let (frames, _) = run_session(
            &root,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{}}"#,
                r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#,
                r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
            ],
        );
        assert_eq!(response_for(&frames, 2)["error"]["code"], json!(-32601));
    }

    #[test]
    fn exit_without_shutdown_is_not_clean() {
        let root = fixture_repo("exit");
        let (_, outcome) = run_session(
            &root,
            &[r#"{"jsonrpc":"2.0","method":"exit","params":null}"#],
        );
        assert_eq!(outcome.exit_code, 1, "spec: exit without shutdown is 1");
    }

    #[test]
    fn uri_to_path_decodes_percent_escapes() {
        assert_eq!(
            uri_to_path("file:///home/a/my%20repo/src/x.ts"),
            Some(PathBuf::from("/home/a/my repo/src/x.ts"))
        );
        assert_eq!(
            uri_to_path("http://example.com/x"),
            None,
            "non-file URIs have no path"
        );
    }

    #[test]
    fn resolve_root_prefers_flag_then_root_uri() {
        let with_flag = ServeOptions {
            repo_root: Some(PathBuf::from("/flag/root")),
        };
        let params = json!({ "rootUri": "file:///client/root" });
        assert_eq!(
            resolve_root(&with_flag, &params),
            PathBuf::from("/flag/root")
        );

        let no_flag = ServeOptions::default();
        assert_eq!(
            resolve_root(&no_flag, &params),
            PathBuf::from("/client/root")
        );

        let folders = json!({ "workspaceFolders": [{ "uri": "file:///folder/root" }] });
        assert_eq!(
            resolve_root(&no_flag, &folders),
            PathBuf::from("/folder/root")
        );
    }

    #[test]
    fn analysis_failure_is_answered_honestly() {
        let missing = std::env::temp_dir().join(format!(
            "synevid-lsp-missing-{}-{}",
            std::process::id(),
            "nope"
        ));
        let uri = format!("file://{}/x.ts", missing.display());
        let open = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": &uri, "languageId": "typescript", "version": 1, "text": "helper();\n" } }
        })
        .to_string();
        let hover = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/hover",
            "params": { "textDocument": { "uri": &uri }, "position": { "line": 0, "character": 2 } }
        })
        .to_string();
        let (frames, _) = run_session(
            &missing,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                &open,
                &hover,
                r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
                r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
            ],
        );
        let markdown = response_for(&frames, 5)["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            markdown.contains("analysis unavailable"),
            "a broken workspace says so instead of pretending: {markdown}"
        );
    }
}
