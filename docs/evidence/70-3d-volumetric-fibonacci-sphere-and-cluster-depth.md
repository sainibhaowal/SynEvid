# Production Receipt 70: 3D Volumetric Fibonacci Spherical Lattice and Cluster Depth Engine

## 1. Requirement & Architecture Context
- **Issue:** In 3D Universe mode, repository clusters were previously positioned along a flat 2D spiral on the $X-Y$ plane ($Z \approx 0$). Large clusters (such as `docs` with 88 files) placed files in 15 concentric planar rings spanning 1500 units horizontally, which caused the 3D scene to appear as a flat vertical strip or fence standing on edge when rotated or viewed from an angle.
- **Goal:**
  1. Distribute repository clusters across true 3D space $(X, Y, Z)$ using a **Spherical Fibonacci Lattice** (celestial constellation distribution).
  2. Pack member files within each cluster in a **Volumetric 3D Spherical Cloud** (concentric spherical shells scaling with $\sqrt[3]{N}$), preventing large clusters from sprawling into 2D vertical walls.
  3. Initialize 3D camera at an elevated cinematic isometric perspective ($\phi = 60^\circ, \theta = 45^\circ$, radius $780$) to immediately reveal foreground, midground, and background depth.
  4. Preserve clean 2D Mind-Map Planar Sunflower Fermat Spiral mode ($Z = 0$, top-down camera) when switching to 2D Plane.

---

## 2. Mathematical Formulation & Implementation Details

### A. Cluster Constellation Positioning
```ts
// Spherical Fibonacci Lattice for 3D Clusters
const phiK = Math.acos(1 - (2 * (idx + 0.5)) / Math.max(totalClusters, 1));
const thetaK = idx * GOLDEN_ANGLE;

// Radius scales with cube root of count for 3D packing
const islandRadius = 55 + Math.cbrt(c.count) * 32;

// Staggered radial shells for foreground, midground & background depth
const orbitDist = 520 + (idx % 3) * 160 + Math.sqrt(idx + 1) * 70;

const cx = Math.sin(phiK) * Math.cos(thetaK) * orbitDist;
const cz = Math.sin(phiK) * Math.sin(thetaK) * orbitDist; // Full Z-depth
const cy = Math.cos(phiK) * (orbitDist * 0.76);          // Harmonic Y-height
```

### B. Volumetric 3D Spherical Packing for Member Files
Instead of flattening 88 files into concentric rings with radius 755 in $X-Y$, files are packed in a 3D spherical cloud with radius under 200:
```ts
const N = sortedMembers.length;
const nodePhi = Math.acos(1 - (2 * (idx + 0.5)) / Math.max(N, 1));
const nodeTheta = idx * GOLDEN_ANGLE;
const nodeR = 38 + Math.cbrt(idx) * 38;

const nx = center.x + Math.sin(nodePhi) * Math.cos(nodeTheta) * nodeR;
const nz = center.z + Math.sin(nodePhi) * Math.sin(nodeTheta) * nodeR;
const ny = center.y + Math.cos(nodePhi) * (nodeR * 0.85);
```

### C. 3D Spherical Collision Relaxation
```ts
const MIN_SEPARATION = is3D ? 48 : 46;
// True 3D Euclidean distance calculation and 3D spherical unit-vector push
const dist = Math.sqrt(dx * dx + dy * dy + dz * dz);
```

### D. Ambient Celestial Cluster Halos & Outward Curved Data Highways
- Each cluster in 3D is cradled by a subtle glowing ambient celestial halo ring in its folder theme color (`opacity = 0.18` dark / `0.40` light) indicating its constellation boundary.
- Inter-module Bezier data highways arch outward in 3D space (`crossVectors(lineDir, radial)`) rather than flattening into the $X-Y$ plane.

---

## 3. Verification & Telemetry
1. **Next.js Production Build (`apps/web`):**
   - Output: `next build` succeeded with 0 TypeScript/Turbopack errors in 7.2s.
2. **Headless Web-Inspector Test Suite (`apps/web-inspector`):**
   - Output: `20/20` TAP unit tests passed in 1.14s.
3. **Rust Workspace Code Quality:**
   - `cargo fmt --all -- --check` clean (0 errors).
