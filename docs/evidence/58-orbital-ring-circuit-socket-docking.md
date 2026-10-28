# Production Receipt 58: Orbital Ring Circuit Socket Docking & Non-Intrusive Cable Routing

## Executive Summary
Resolves the visual defect where curved circuit lines were originating from the node card centers, penetrating inside the card slate body and crossing directly over file metrics, titles, and verification tags.

---

## Technical Details

### 1. Vector Offset from Node Centers to Ring Perimeter
- **Location**: [`apps/web/src/components/CodeUniverse3D.tsx`](file:///home/ravi/Projects%20Beta/SynEvid/apps/web/src/components/CodeUniverse3D.tsx)
- Given node center coordinates `p1_center` and `p2_center`:
  - Computes unit delta vector:
    $$\vec{u} = \frac{p_2 - p_1}{\|p_2 - p_1\|}$$
  - Applies orbital ring radius offset ($R = 11.2$ units):
    $$p_1 = p_{1,\text{center}} + \vec{u} \cdot R$$
    $$p_2 = p_{2,\text{center}} - \vec{u} \cdot R$$
  - The starting point $p_1$ lands exactly on the perimeter of source node's orbital ring.
  - The ending point $p_2$ lands exactly on the perimeter of target node's orbital ring.
  - Circuit cables NEVER enter or penetrate the card face, leaving all card contents, badges, and metrics 100% unobstructed.

### 2. Glowing Terminal Sockets
- Placed neon terminal socket beads (`THREE.SphereGeometry(0.75)`) at the exact dock points $p_1$ and $p_2$ on the orbital rings.
- Cables visually plug into the rings as physical electrical connection docks.
- Electric pulse packets travel along the Bézier curve strictly from ring socket to ring socket.

---

## Verification
- `npm run build` in `apps/web`: Compiled cleanly with 0 errors across all routes (3.9s).
- Visual check: Lines originate from the neon rings and terminate on the destination rings without cutting through card typography.
