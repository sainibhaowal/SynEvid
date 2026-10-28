# Production Receipt 75: Interactive Cluster Ring Drag & 2.5D Isometric Mind Map Engine

## 1. Executive Summary

- **Feature / Problem Solved:** 
  1. Users requested the ability to interactively grab and drag entire cluster rings (the folder perimeter circle/halo together with all of its member files, orbital rings, and cluster title badges) in both 2D and 3D space, with all connecting data highways dynamically stretching and updating their Bezier paths in real time.
  2. Users requested that 2D mode look and feel like **2.5D** (an architectural isometric drafting tabletop with stepped depth layers, physical card elevations, and a tilted drafting board perspective instead of a completely flat, perpendicular sheet of paper).
- **Solution:**
  1. **Interactive Cluster Ring Grab & Drag Pipeline:**
     - Created invisible wide-radius hit colliders for every cluster island (`grabMesh3D` torus collider in 3D; `grabMesh2D` ring collider in 2.5D/2D) alongside cluster title badges and perimeter visual rings.
     - On cursor hover, the cursor turns to `grab` open-hand. On mousedown, it switches to `grabbing` closed-hand.
     - Projects cursor movement along a dynamically aligned drag plane ($X-Y$ table plane in 2.5D; camera-facing view plane in 3D).
     - Translates the entire cluster island: cluster center, perimeter rings, torus halos, title badges, root radial branch trunk lines, and every member file node in lockstep.
     - Calls `updateCurvedEdges()` every frame at 60–120 FPS so all incoming and outgoing Bezier data highways and socket docking beads dynamically stretch and flex like live elastic circuits.
  2. **2.5D Isometric Tabletop Drafting Engine:**
     - Calibrated initial camera pitch to an architect's drafting board angle ($\beta = 0.42 \approx 24^\circ$ tilt, radius 1,280) with full turntable spin ($\alpha$) and pitch controls.
     - Implemented **2.5D physical depth layer stepping**:
       - Central entrypoint files (`index`, `mod`, `main`, `lib`, `page`) elevated at $Z = 4.0$.
       - Inner ring files stepped at $Z = 2.4 \dots 0.6$.
       - Perimeter cluster boundary rings at $Z = -0.2$.
       - Mind map root trunk branch lines at $Z = -0.6$.
       - Blueprint coordinate grid at $Z = -2.8$.
     - Re-labeled HUD slider toggle to **`2.5D Plane`** with informative tooltips.
     - Integrated seamless 450ms dimensional morphing between 2.5D isometric drafting angle and 3D celestial constellation perspective.

---

## 2. Technical Architecture & Invariants

### 2.1 Cluster Island Architecture
```ts
interface ClusterIsland {
  key: string;
  name: string;
  center3D: THREE.Vector3;
  center2D: THREE.Vector3;
  radius: number;
  haloMesh: THREE.Mesh;
  ringMesh: THREE.Mesh;
  grabMesh3D: THREE.Mesh;
  grabMesh2D: THREE.Mesh;
  badgeMesh: THREE.Mesh | null;
  branchLine: THREE.Line;
  memberNodeIds: string[];
}
```

### 2.2 Dynamic Plane Projection Dragging
$$\text{In 2.5D}: \quad \mathbf{n}_{\text{plane}} = (0, 0, 1), \quad \mathbf{p}_0 = (0, 0, 0)$$
$$\text{In 3D}: \quad \mathbf{n}_{\text{plane}} = -\mathbf{d}_{\text{camera}}, \quad \mathbf{p}_0 = \mathbf{C}_{\text{cluster}}$$

$$\Delta \mathbf{P} = \mathbf{P}_{\text{intersect}} - \mathbf{P}_{\text{drag\_start}}$$
$$\mathbf{P}_{\text{node}}(t) = \mathbf{P}_{\text{node\_start}} + \Delta \mathbf{P}, \quad \forall \text{ node } \in \text{Island}$$

### 2.3 2.5D Isometric Pitch Projection
$$\mathbf{P}_{\text{cam}} = (0, -R \sin(\beta), R \cos(\beta)), \quad \mathbf{Up} = (0, \cos(\beta), \sin(\beta))$$
$$\mathbf{D}_{\text{sight}} \cdot \mathbf{Up} = 0 \quad (\text{exact orthogonal perspective})$$

---

## 3. Multi-Stack Verification & Telemetry

1. **Next.js Production Build:**
   - Command: `cd apps/web && npm run build`
   - Output: `Compiled successfully in 12.7s`, `Finished TypeScript in 20.5s`, `Generating static pages (4/4)`, 0 errors.
2. **Web-Inspector Test Suite:**
   - Command: `cd apps/web-inspector && npm test`
   - Output: `20/20 pass` (1,429ms duration).
3. **Rust Workspace Code Quality:**
   - Command: `cargo fmt --all -- --check`
   - Output: Clean (exit code 0, 0 warnings).

---

## 4. Modified Source References

- [`apps/web/src/components/CodeUniverse3D.tsx`](file:///home/ravi/Projects/SynEvid/apps/web/src/components/CodeUniverse3D.tsx): `ClusterIsland` definition, `grabMesh3D` / `grabMesh2D` colliders, mouse cursor grab & dragging pipeline, 2.5D depth layer stepping ($Z \in [0.6, 4.0]$), isometric drafting tilt ($\beta = 0.42$), and `2.5D Plane` HUD button.
