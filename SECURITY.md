# Security Policy

## 1. Scope & Philosophy

SynEvid (**Code Autopsy**) is a forensic, deterministic verification engine designed to operate as an external ground-truth oracle for AI coding agents and safety-critical engineering pipelines.

Security in SynEvid is rooted in non-negotiable architectural boundaries:
- **Zero LLM in Core Verifiers:** The core verification engine (`crates/*`) never imports or queries LLM APIs. Verification decisions cannot be manipulated via prompt injection or LLM jailbreaks.
- **Strict Read-Only Execution (FR-026):** SynEvid analyzes source code and git histories in an inspect-only capacity. It never mutates repository files, git index, or working trees.
- **Deterministic Cryptographic Ground Truth (NFR-001):** All evidence receipts and snapshot identifiers use cryptographic BLAKE3 hashes. Outputs are deterministic and tamper-evident.
- **Offline-First & Air-Gapped Operation:** SynEvid runs entirely locally. It does not transmit analyzed source code, ASTs, or verification telemetry to remote servers.

---

## 2. Supported Versions

We actively maintain and provide security patches for the following versions:

| Version | Supported          | Release Date | Status                     |
|---------|--------------------|--------------|----------------------------|
| 0.1.x   | :white_check_mark: | 2026-10-08   | Active Development (Current)|

---

## 3. Reporting a Vulnerability

We take the security and integrity of SynEvid seriously. If you discover a security vulnerability or an invariant bypass, please report it privately.

### Preferred Disclosure Channels
1. **GitHub Private Vulnerability Advisory:** Submit via GitHub's [Security Advisories](https://github.com/sainibhaowal/SynEvid/security/advisories/new) dashboard.
2. **Direct Security Contact:** Email the lead maintainer directly at:
   - **Email:** `rav.singh039@gmail.com`
   - **Subject Line:** `[SECURITY] SynEvid Vulnerability Report: <Brief Description>`

> [!WARNING]
> Please **do not** report security vulnerabilities or invariant breaches in public GitHub issues, discussions, or pull requests.

---

## 4. What to Include in Your Report

To help us triage and resolve the issue quickly, please provide:
1. **Type of Vulnerability:** (e.g., path traversal, symlink escape, unbounded memory/CPU denial-of-service, non-deterministic state leak, cache collision).
2. **Affected Component:** Specific crate (e.g., `crates/autopsy-repo`, `crates/autopsy-storage`, `crates/autopsy-cli`) or schema.
3. **Step-by-step Reproduction:** Minimal code repository or input files that trigger the vulnerability.
4. **Impact Assessment:** Expected vs. observed behavior and potential security implications.
5. **Environment Details:** Operating System, Rust version (`rustc --version`), and SynEvid commit hash.

---

## 5. Vulnerability Response Timeline

- **Initial Acknowledgment:** Within **48 hours** of receiving your report.
- **Triage & Reproduction:** Within **5 business days**, with confirmation of severity and impact.
- **Fix & Verification:** Security fixes are engineered with deterministic test coverage and regression gates.
- **Coordinated Disclosure:** We will coordinate with you on a mutual disclosure timeline once a patch is tested and released.

---

## 6. Threat Model Reference

For an in-depth analysis of repository input threats, resource exhaustion mitigations, cache integrity controls, and tool boundaries, consult [`docs/security/THREAT_MODEL.md`](docs/security/THREAT_MODEL.md).
