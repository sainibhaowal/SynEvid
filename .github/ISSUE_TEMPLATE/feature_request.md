---
name: Feature Request
about: Suggest an idea or enhancement for SynEvid
title: "[FEAT] "
labels: ["enhancement"]
assignees: []
---

### Is your feature request related to a problem? Please describe.
<!-- A clear and concise description of the problem or limitation you are encountering. -->

### Describe the Solution You'd Like
<!-- A clear and concise description of the proposed feature or capability. -->

### Hard Boundaries & Invariants Assessment
<!-- SynEvid strictly enforces non-negotiable boundaries. Please review: -->
- [ ] **Zero LLM in Core:** Does this feature keep the core verification engine 100% deterministic code without LLM API calls?
- [ ] **Read-Only Invariant:** Does this feature preserve the engine's read-only guarantee (no mutating target source or git state)?
- [ ] **Determinism:** Can this feature produce identical, deterministic results across runs?

### Describe Alternatives Considered
<!-- A clear and concise description of any alternative solutions or workarounds you've considered. -->

### Additional Context
<!-- Any diagrams, related ADRs, or references to requirements (FR/NFR). -->
