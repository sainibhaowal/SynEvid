# SynEvid Threat Model (STRIDE)

**Version**: 1.0
**Date**: 2024-11-24
**Author**: Security Team

## System Overview

### System Description
SynEvid is an offline, deterministic software verification and change-impact engine. It operates as a local daemon (`autopsy-daemon`) with multiple client interfaces (CLI, TUI, MCP, Web UI, Electron).

### Trust Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                      USER WORKSTATION                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   CLI/TUI    │  │    Web UI    │  │   Electron App       │  │
│  │  (Trusted)   │  │  (Sandboxed) │  │  (Native + Sandbox)  │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬────────────┘  │
│         │                 │                       │              │
│         └─────────────────┼───────────────────────┘              │
│                           ▼                                      │
│              ┌──────────────────────────┐                        │
│              │   autopsy-daemon (3333)  │ ◄── Trust Boundary    │
│              │   (Single Authority)     │                        │
│              └───────────┬──────────────┘                        │
│                           ▼                                      │
│              ┌──────────────────────────┐                        │
│              │   Local Filesystem       │                        │
│              │   (.autopsy/, repos)     │                        │
│              └──────────────────────────┘                        │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Client → Daemon**: Authenticated JSON-RPC over HTTP/WS (loopback only)
2. **Daemon → Storage**: SQLite (`.autopsy/storage.db`) + loose objects
3. **Daemon → Git**: Native git operations (no subprocess)
4. **Daemon → File System**: Repo scanning (sandboxed, no symlinks)
5. **Daemon → Network**: **NONE** (offline-only, no outbound connections)

---

## STRIDE Analysis

### Spoofing (S)

| ID | Threat | Asset | Likelihood | Impact | Mitigation |
|----|--------|-------|------------|--------|------------|
| S-01 | Client spoofs another client | Daemon lease | Low | Medium | Loopback token + client ID validation |
| S-02 | Malicious config impersonates repo | Repo registry | Low | High | Canonical path + symlink check |
| S-03 | Fake daemon response | Client trust | Low | High | Loopback token verification |
| S-04 | Git commit spoofing | Audit trail | Medium | High | Native git (no subprocess), signed commits |

**Mitigations**:
- Loopback token (BLAKE3, 256-bit) required for all daemon connections
- Client ID + type tracked per lease
- Config paths canonicalized, symlinks rejected

### Tampering (T)

| ID | Threat | Asset | Likelihood | Impact | Mitigation |
|----|--------|-------|------------|--------|------------|
| T-01 | Modify storage.db | `.autopsy/storage.db` | Low | Critical | SQLite WAL + file permissions (0600) |
| T-02 | Modify repo index | `.autopsy/<repo>/` | Low | High | Read-only during analysis |
| T-03 | Inject malicious git objects | `.git/objects/` | Low | Critical | Native git, SHA-1 verification |
| T-04 | Modify daemon memory | Daemon process | Very Low | Critical | ASLR, DEP, no JIT |
| T-05 | Config injection | `autopsy.toml` | Medium | High | Schema validation, path traversal check |

**Mitigations**:
- File permissions: `.autopsy/` 0700, files 0600
- SQLite WAL mode with checksum
- Config schema validation (JSON Schema)
- Path canonicalization + `..` rejection

### Repudiation (R)

| ID | Threat | Asset | Likelihood | Impact | Mitigation |
|----|--------|-------|------------|--------|------------|
| R-01 | Deny analysis execution | Audit log | Medium | Medium | Immutable event store (append-only) |
| R-02 | Forge analysis result | Evidence envelope | Low | High | BLAKE3 digest + evidence receipt |
| R-03 | Delete audit trail | `.autopsy/events.sqlite` | Low | High | Append-only, no DELETE API |

**Mitigations**:
- Event store: append-only SQLite, no DELETE
- Evidence envelope: BLAKE3 digest over canonical JSON
- BLAKE3 receipt in every envelope

### Information Disclosure (I)

| ID | Threat | Asset | Likelihood | Impact | Mitigation |
|----|--------|-------|------------|--------|------------|
| I-01 | Repo source in error messages | Source code | Medium | Low | Sanitized errors, no paths in JSON |
| I-02 | Config values in logs | Secrets/tokens | Low | High | No secrets in logs, redacted |
| I-03 | Daemon memory dump | In-memory state | Very Low | High | No secrets in memory |
| I-04 | Git history exposure | `.git/` | Medium | Low | User-controlled, opt-in |
| I-05 | Daemon memory swap | Swap file | Very Low | Medium | `mlockall()` on sensitive data |

**Mitigations**:
- Error sanitization in `CliError::repair()`
- No secrets in logs (structured logging)
- `mlockall()` for token storage (Linux)

### Denial of Service (D)

| ID | Threat | Asset | Likelihood | Impact | Mitigation |
|----|--------|-------|------------|--------|------------|
| D-01 | Resource exhaustion (large repo) | CPU/Memory | Medium | High | Bounds: `max_file_bytes`, `max_traversal_nodes` |
| D-02 | Request flooding | Daemon CPU | Medium | High | Rate limiting (100 RPM) |
| D-03 | Large request body | Memory | Low | High | `max_body_size` (default 10MB) |
| D-04 | Pathological git history | CPU | Low | High | Commit limit, depth bounds |
| D-05 | Malformed input parsing | CPU | Low | Medium | Parser timeouts, size limits |

**Mitigations**:
- `max_file_bytes` (default 1MB), `max_traversal_nodes` (100k)
- Rate limiter: 100 RPM per client ID
- `max_body_size` (10MB default)
- Commit depth limit (10k), file size limits
- Parser timeouts (30s), input size limits

### Elevation of Privilege (E)

| ID | Threat | Asset | Likelihood | Impact | Mitigation |
|----|--------|-------|------------|--------|------------|
| E-01 | Loopback token brute-force | Daemon API | Low | Critical | 256-bit entropy, constant-time compare |
| E-02 | Config path traversal | Filesystem | Medium | High | Canonicalization + `..` rejection |
| E-03 | Symlink escape | Filesystem | Medium | High | `follow_links(false)`, containment |
| E-04 | Daemon restart with malicious config | Daemon state | Low | Critical | Config validation, no auto-reload |

**Mitigations**:
- Token: 256-bit BLAKE3, constant-time compare
- Path: `canonicalize()` + `..` check + symlink containment
- Config: Schema validation, no auto-reload

---

## Attack Surface Summary

### External Attack Surface: **MINIMAL**
- **Network**: NONE (loopback only, no outbound)
- **File System**: User-controlled repos only
- **Config**: User-controlled `autopsy.toml`

### Internal Attack Surface
| Component | Exposure | Hardening |
|-----------|----------|-----------|
| Daemon HTTP | Loopback only | Token auth, rate limit |
| Storage | Local files | 0600 perms, WAL |
| Git | Local repos | Native, no subprocess |
| Config | User file | Schema validation |

---

## Risk Matrix

| Threat | Likelihood | Impact | Risk | Status |
|--------|------------|--------|------|--------|
| Spoofing | Low | Medium | **LOW** | ✅ Mitigated |
| Tampering | Low | Critical | **LOW** | ✅ Mitigated |
| Repudiation | Low | High | **LOW** | ✅ Mitigated |
| Info Disclosure | Medium | Low | **LOW** | ✅ Mitigated |
| DoS | Medium | High | **MEDIUM** | 🟡 Monitored |
| Elevation | Low | Critical | **LOW** | ✅ Mitigated |

---

## Security Controls Summary

| Control | Implementation | Verification |
|---------|----------------|--------------|
| Auth | Loopback token (BLAKE3) | Unit tests |
| Rate Limit | Token bucket (100 RPM) | Integration tests |
| Path Safety | Canonicalize + `..` check | Unit tests |
| Symlink Safety | `follow_links(false)` | Unit tests |
| Resource Limits | `max_*` config | Integration tests |
| Rate Limiting | Token bucket | Integration tests |
| Error Sanitization | No paths in JSON | Unit tests |
| Constant-time Compare | `subtle::ConstantTimeEq` | Unit tests |
| Audit Logging | Append-only SQLite | Integration tests |

---

## Residual Risk

| Risk | Residual Level | Acceptance |
|------|----------------|------------|
| DoS via pathological input | Low | Accepted (bounds enforced) |
| Local privilege escalation | Very Low | Accepted (token entropy) |
| Supply chain compromise | Very Low | Accepted (cargo deny) |
| Zero-day in dependencies | Low | Monitored (cargo audit) |

---

## Review Schedule

| Review Type | Frequency | Owner |
|-------------|-----------|-------|
| Threat Model Review | 6 months | Security Team |
| Architecture Changes | Per PR | Security Team |
| New Dependencies | Per PR | Automated (cargo deny) |
| Incident Response | Post-incident | Security Team |

---

## Appendices

### A. Data Flow Diagram (DFD) Level 1

```
[Client] ──(HTTPS/WS + Token)──► [Daemon] ──(SQLite)──► [Storage]
    │                              │
    │                              ├──► [Git Repo] (read-only)
    │                              │
    │                              └──► [Config] (validated)
    │
    └──► [User] (Human/Automated)
```

### B. Trust Levels

| Zone | Trust Level | Components |
|------|-------------|------------|
| User Workstation | High | CLI, TUI, Electron |
| Daemon Loopback | High | Daemon HTTP, Storage |
| Git Repos | Medium | User-controlled |
| Network | None | No outbound |

---

*Document Version: 1.0*
*Classification: Internal*
*Review Date: 2025-05-24*