---
name: Invariant Violation Report
about: Report a determinism flaw, architectural boundary leak, or false-pass coverage issue
title: "[INVARIANT VIOLATION] "
labels: ["invariant-violation", "correctness"]
assignees: []
---

### Violation Category
<!-- Please check which non-negotiable invariant was violated: -->
- [ ] **Non-Determinism (NFR-001):** Differing BLAKE3 hashes on identical inputs.
- [ ] **Unintended Mutation (FR-026):** Engine modified source code, files, or git status.
- [ ] **Dishonest Coverage (FR-014, FR-030):** Dynamic/unsupported code reported as `PASS` instead of `UNKNOWN`/`PARTIAL`.
- [ ] **Architectural Boundary Leak:** Core crate imported or invoked LLM, MCP, or UI presentation APIs.

### Steps to Reproduce & Evidence
1. Target input repository or code snippet:
2. Command executed:
3. First execution output / BLAKE3 digest:
4. Subsequent execution output / differing digest:

### Expected Deterministic Behavior
<!-- What invariant should have been enforced? -->

### Environment & Toolchain Details
- **SynEvid Commit Hash:**
- **Rust Toolchain:**
- **OS Platform:**
- **Filesystem Type:** (e.g., ext4, APFS, NTFS)
