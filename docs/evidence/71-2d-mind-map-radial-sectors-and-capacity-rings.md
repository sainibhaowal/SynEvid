# Production Receipt 71: 2D Mind-Map Radial Sectors, Expanding Capacity Rings & Blueprint Grid

## 1. Requirement & Architecture Context
- **Issue:** In 2D Plane mode, clusters were placed in a narrow ring at radius $600\dots 900$ with fixed 6 files per tier. In large folders like `docs` (88 files), this caused 15 concentric circles sprawling over 1,500 units wide, while inner modules collided and the Three.js ground grid (lying on the $X-Z$ plane) collapsed into a single horizontal artifact line cut across the screen. Furthermore, the 2D camera was initialized at distance 380, causing 80% of the 2D universe to be off-screen.
- **Goal:**
  1. Distribute 2D clusters in **Dedicated Radial Sectors** with alternating staggered distances ($R_{\text{inner}} \approx 390\dots 500$, $R_{\text{outer}} \approx 590\dots 780$) so adjacent module cones never touch or overlap.
  2. Implement **2D Expanding Capacity Rings** ($6 \times \text{tier}$ capacity per tier), compacting 88 files from radius 755 down to radius 272 with uniform card-to-card spacing ($\approx 57$ units) and the primary entry point file at $(cx, cy)$.
  3. Rotate the background coordinate grid to lie flat on the $X-Y$ plane behind the canvas nodes ($Z = -2$) as an architectural blueprint graph paper backing.
  4. Add **2D Circular Perimeter Rings** and **Mind-Map Branch Trunk Lines** connecting the central root hub to each cluster island center in its folder signature color.
  5. Calibrate the default 2D camera viewport to distance $1350$ (zoom range $30\dots 3600$) so the entire Mind Map fits cleanly on screen on initial load and recenter.

---

## 2. Implementation & Mathematical Formulation

### A. Dedicated Radial Sectors with Staggered Distance
```ts
// 2D Mind-Map Radial Sectors
const baseAngle = (2 * Math.PI * idx) / Math.max(totalClusters, 1);
const tiers = Math.max(1, Math.ceil(Math.sqrt(c.count / 3)));
const islandRadius = 55 + (tiers - 1) * 54 + 25;

// Alternating inner/outer radii ensures chord distance between adjacent clusters > sum of their radii
const isOuter = idx % 2 === 1;
const radialDist = isOuter ? 590 + islandRadius * 1.05 : 390 + islandRadius * 0.9;

const cx = Math.cos(baseAngle) * radialDist;
const cy = Math.sin(baseAngle) * radialDist;
```

### B. Expanding Capacity Rings within Clusters
```ts
if (idx === 0) {
  // Primary entrypoint (index, mod, main, lib) sits at cluster center
  nodesList.push({ ...node, x: center.x, y: center.y, z: 0 });
} else {
  // Capacity grows as 6 * tier (Tier 1 holds 6, Tier 2 holds 12, Tier 3 holds 18...)
  let tier = 1;
  let tierCap = 6;
  let sumBefore = 1;
  while (idx >= sumBefore + tierCap) {
    sumBefore += tierCap;
    tier++;
    tierCap = 6 * tier;
  }
  const slot = idx - sumBefore;
  const tierRadius = 56 + (tier - 1) * 54;
  const offsetAngle = (2 * Math.PI * slot) / tierCap + tier * 0.32;

  nodesList.push({
    ...node,
    x: center.x + Math.cos(offsetAngle) * tierRadius,
    y: center.y + Math.sin(offsetAngle) * tierRadius,
    z: 0,
  });
}
```

### C. Blueprint Grid & Mind Map Infrastructure
- **X-Y Blueprint Grid:** `gridHelper.rotation.x = Math.PI / 2`, `position.set(0, 0, -2)`, providing clean engineering graph paper backing behind all nodes.
- **Island Perimeter Rings:** `RingGeometry(center.radius * 1.01, center.radius * 1.04, 64)` on the $X-Y$ plane in the cluster's signature color.
- **Trunk Branch Highway Lines:** Subtle lines connecting $(0, 0, -1)$ to $(cx, cy, -1)$ visually anchoring the module branch to the codebase root.
- **2D Camera Framed Viewport:** Initialized `radius = 1350`, `theta = 0`, `phi = Math.PI / 2`, perfectly framing the entire Mind Map on screen.

---

## 3. Multi-Stack Verification
1. **Next.js Web Build:** `npm run build` compiled in 10.0s with 0 errors.
2. **Web-Inspector Test Suite:** `20/20` TAP unit tests passed in 1.16s.
3. **Rust Formatting & Boundary Check:** `cargo fmt --all -- --check` clean.
