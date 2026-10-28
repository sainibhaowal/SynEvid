# Production Evidence Receipt 68: Architecture Mind-Map Cluster Layout & Folder-Coded Rings

**Date:** 2026-10-27  
**Phase ID:** `PHASE-2-v0.3.0`  
**Target Semantic Version:** `v0.3.0`  
**Parent Traceability Matrix:** [`docs/requirements/TRACEABILITY.md`](../requirements/TRACEABILITY.md)  
**Component:** [`apps/web/src/components/CodeUniverse3D.tsx`](../../apps/web/src/components/CodeUniverse3D.tsx)

---

## 1. Problem Statement & User Experience Deficiency

In earlier canvas builds, rendering ~200 codebase files caused a dense central "hairball" graph:
1. **Dense Center Ball:** All nodes were pulled into a single circular disc of radius ~250, causing cards to overlap and cluster indiscriminately without spatial hierarchy.
2. **Blue Spiderweb Clutter:** All 206 bezier edge lines were drawn simultaneously at high opacity (`0.88`), creating a criss-crossing blue hairball that obscured the underlying cards.
3. **Uniform Color Rings:** Every single node shared the exact same cyan/cobalt outer orbital ring, giving the user zero visual context of which folder, package, or subsystem a file belonged to.
4. **Lack of Mind-Map Structure:** There was no visual sense of architectural origin (root manifests) to leaf modules (storage, CLI, tests, web).

---

## 2. Engineering Solution Implemented

### 2.1 Architectural Mind-Map & Cluster-Island Layout Engine
- **Module Partitioning:** Automatically extracts top-level packages (`crates/autopsy-storage`, `apps/web`, `crates/autopsy-cli`, `tests`, `docs`, etc.).
- **Spaced Island Placement:**
  - `root` manifests positioned in the center hub ($R = 0\dots 70$).
  - Each module island is placed at its own radial sector angle $\theta_k = \frac{2\pi \cdot k}{K} - \frac{\pi}{2}$ at distance $R \ge 480\dots 750$.
  - Member files within each module are arranged in concentric orbital tiers ($r = 55 + \text{tier} \times 50$) around the folder's leader.
- **Anti-Collision Relaxation:** Relaxed with `MIN_SEPARATION = 46`, ensuring zero overlapping cards.

### 2.2 Folder-Specific Color-Coded Orbital Rings
- Established consistent signature palettes per package:
  - `apps/web`: Sky Blue (`#0284c7` / `#38bdf8`)
  - `apps/synevid-ui`: Royal Indigo (`#6366f1` / `#818cf8`)
  - `crates/autopsy-cli`: Amber Gold (`#d97706` / `#fbbf24`)
  - `crates/autopsy-storage`: Emerald Green (`#059669` / `#34d399`)
  - `crates/autopsy-plugin-api`: Electric Violet (`#7c3aed` / `#c084fc`)
  - `crates/autopsy-graph-layout`: Deep Teal (`#0d9488` / `#2dd4bf`)
  - `tests`: Crimson Rose (`#e11d48` / `#fb7185`)
  - `docs`: Forest Green (`#16a34a` / `#4ade80`)
  - `schemas`: Violet Slate (`#6d28d9` / `#a78bfa`)
- Every card's outer orbital ring, header badge, and bottom accent strip renders in that folder's distinctive color.

### 2.3 Smart Edge Visibility & Zero-Hairball Highlighting
- **Soft Ambient Opacity:** Default line opacity lowered to $0.12\dots 0.16$ (intra-folder lines carry the folder's signature color; inter-folder bridges use elevated curves).
- **Interactive Focus Illumination:** Hovering or selecting any card isolates its exact incoming callers and outgoing callees at **`0.98` opacity** with glowing flow pulses, while unrelated edges fade to **`0.02` opacity**.

### 2.4 Interactive Folder Legends Bar
- Floating module pill bar at the bottom of the canvas displaying each detected folder, its color dot, and file count.
- Clicking any module pill glides the camera directly to that folder's cluster island and focuses its files.

---

## 3. Automated Verification Telemetry

- **Next.js Production Turbopack Build:**
  ```text
  ▲ Next.js 16.4.2 (Turbopack)
  ✓ Compiled successfully in 7.9s
  ✓ Finished TypeScript in 12.1s (0 errors)
  ✓ Generated static pages: / and /_not-found
  ```
- **Web-Inspector Headless Integration Suite:**
  ```text
  > @code-autopsy/web-inspector@0.1.0 test
  # tests 20 | pass 20 | fail 0 (1278ms)
  ```
- **Rust Formatting & Architectural Boundary Checks:**
  - `cargo fmt --all -- --check`: **Clean (0 errors)**
  - `./scripts/verify-arch-boundaries.sh`: **PASSED**
