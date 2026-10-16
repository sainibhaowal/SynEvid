# SynEvid Security Architecture & Threat Model

This document specifies the security boundaries, threat analysis, sandbox defense mechanisms, and untrusted input containment policies for **SynEvid (Code Autopsy)**.

---

## 1. Security Stance: Repositories Are Hostile Inputs

In modern autonomous coding agent environments, target repositories and dependency trees must be treated as **untrusted, potentially hostile input**. A malicious or compromised repository may attempt:
- Filesystem escapes via circular or external symlinks.
- Path traversal outside workspace boundaries via relative path tricks (`../../`).
- Resource exhaustion (zip bombs, multi-gigabyte generated files, infinite parser recursion).
- Arbitrary code execution via build hooks, pre-commit scripts, or language servers.
- Exfiltration of private source code or environment variables over network sockets.
- Cache poisoning across multi-branch or concurrent agent sessions.

SynEvid is engineered with a **defense-in-depth offline containment model** that neutralizes each of these vectors without relying on OS virtualization.

---

## 2. Threat Matrix & Defense-in-Depth Controls

| Threat Vector | Attack Scenario | SynEvid Hard Invariant Control | Verification Suite |
| :--- | :--- | :--- | :--- |
| **Filesystem Escape** | Symlink pointing to `/etc/passwd`, `~/.ssh`, or parent directories. | **`INV-SANDBOX-001`**: Repository traversal enforces `follow_links(false)` and canonical path validation. | `test_e2e_symlink_escape_containment` |
| **Data Exfiltration** | Malicious AST or analyzer attempting to send code or hashes to remote server. | **`INV-SANDBOX-002`**: Core crates contain zero network socket code (`TcpStream`, `UdpSocket`, `reqwest`). | `scripts/verify-arch-boundaries.sh` |
| **Path Traversal** | Specifying configuration roots or baseline snapshots with `../` sequences. | **`INV-SANDBOX-003`**: All paths are sanitized via `sanitize_relative_path`; traversal sequences cause immediate parse rejection. | `test_e2e_path_traversal_sanitization` |
| **Remote Code Execution** | Triggering compilers, `npm install`, or subprocesses during parsing. | **`INV-SANDBOX-004`**: In-memory Tree-sitter parsers only; core crates prohibit `std::process::Command` and `libc::exec`. | `test_e2e_zero_code_execution_in_memory_only` |
| **Resource Exhaustion** | Huge generated bundles, Minified code (>100MB), circular AST depth. | Explicit file cap `max_file_bytes` (10MB default); oversize files recorded in metadata rather than read into memory. | `test_e2e_oversize_file_cap_never_reads_huge_files` |
| **Cache Poisoning** | Concurrent agent processes attempting to corrupt shared SQLite state. | **`ATOMIC-CACHE-001`**: Content-addressed cache writes use unique temporary files and atomic filesystem renames. | `test_e2e_storage_concurrent_atomic_write_resilience` |
| **Telemetry PII Leakage** | OpenTelemetry spans containing authorization headers, passwords, or query tokens. | **Pillar 4 Redaction**: Automated pattern stripping for bearer tokens, credentials, and salted BLAKE3 span hashing. | `test_step40_otlp.rs` (17 tests) |

---

## 3. Sandboxing Invariants

### Invariant INV-SANDBOX-001: Symlink Escape Containment
SynEvid traverses repositories using non-following directory iterators. When a symlink is encountered:
1. It is never traversed into.
2. Its presence is noted in repository telemetry without expanding its target.
3. If an explicit repository root resolves outside the expected boundary, analysis halts with an exit code `3` (Configuration Error).

### Invariant INV-SANDBOX-002: Zero Network Socket Execution
Core crates (`crates/*`) must never declare dependencies on networking libraries (`reqwest`, `hyper`, `tokio-net`, `socket2`, `curl`).
- Verification is enforced by [`./scripts/verify-arch-boundaries.sh`](../../scripts/verify-arch-boundaries.sh).
- If any networking symbol or dependency is detected in core, CI halts immediately.

### Invariant INV-SANDBOX-003: Path Traversal Sanitization
All file identifiers, snapshot keys, and URNs pass through strict lexical normalization:
- Backslashes (`\`) are normalized to standard forward slashes (`/`).
- Leading `/` and relative `..` tokens are strictly forbidden and trigger an analysis error.

### Invariant INV-SANDBOX-004: In-Memory AST Analysis (Zero Subprocess Spawning)
Language extraction (TypeScript, Python) is conducted strictly via statically linked in-memory Tree-sitter grammars.
- No Node.js runtime, Python interpreter, or external compiler is executed during core verification.
- Static semantic graphs are built entirely within safe Rust memory space.

---

## 4. Privacy & Telemetry Redaction Model (Pillar 4)

When ingesting OpenTelemetry traces (`autopsy telemetry`), SynEvid enforces a zero-trust data scrubber:
1. **Header Sanitization:** `Authorization`, `Cookie`, `X-Api-Key`, and bearer tokens are automatically stripped.
2. **Query String Scrubbing:** URL query parameters (`?token=...`, `?user=...`) are truncated before correlation.
3. **Keyed Pseudonymization:** Raw trace IDs and span IDs are hashed using a user-provided keyed BLAKE3 salt:
   $$\text{Pseudonym} = \text{BLAKE3}(\text{SpanID} \parallel \text{Salt})$$
   Raw internal IDs never enter the persistent topology report.
