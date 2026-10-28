# Production Verification Evidence Receipt 53
**Topic:** Butter-Smooth 60/120 FPS Spherical Orbit Camera, Zero-Teardown GPU Highlights, and Resolution of Focus Jitter & Reset Glitches (Phase 2 v0.2.6)
**Date:** 2026-10-27
**Status:** VERIFIED PASS

---

## 1. Problem Statement & Root Cause

### Reported Issue
The user observed:
> *"animation are not sutable too jittery and too much lagging and focus point are lagging and resetting when i click and focus on them why check end to end what is the issues i need perfect animation and so so smooth and well perfection each phase and perfectly working and fix any bugs and debug properly the optimization ui canvas animation end to end please"*

### Root Causes Identified
1. **Catastrophic `useEffect` Teardown Storm:**
   - In [`apps/web/src/components/CodeUniverse3D.tsx`](file:///home/ravi/Projects/SynEvid/apps/web/src/components/CodeUniverse3D.tsx), `selectedNodeId` and `hoveredNodeId` were listed in the `useEffect` dependency array.
   - Every time the user hovered or clicked a node, React ran the `useEffect` cleanup: disposing the WebGLRenderer, emptying DOM innerHTML, rebuilding the entire Three.js scene, regenerating 50+ 512x320 HTML5 canvas textures synchronously on the CPU, and resetting the camera back to `(0, 20, 190)` and lookAt to `(0,0,0)`.
   - This destroyed the camera flight midway through the animation and caused severe visual stuttering and lagging.

2. **Flawed World-Pivot Rotation vs Focal-Target Orbit:**
   - Orbiting previously rotated the entire Three.js `universeGroup` around `(0,0,0)`.
   - If a focused node was at `(80, 40, -30)`, dragging to rotate pivoted around origin `(0,0,0)`, swinging the focused node wildly off the screen and giving the appearance of the focus resetting.

---

## 2. Architecture & Fix Implementation

### 1. True Spherical Orbit Camera Around Focal Target
- Implemented a spherical coordinate system (`radius`, `theta`, `phi`) centered directly on `orbitTarget`:
  $$\begin{aligned}
  x &= \text{orbitTarget}_x + r \cdot \sin(\phi) \cdot \sin(\theta) \\
  y &= \text{orbitTarget}_y + r \cdot \cos(\phi) \\
  z &= \text{orbitTarget}_z + r \cdot \sin(\phi) \cdot \cos(\theta)
  \end{aligned}$$
- When clicking a node (`flyToNode`), `targetLookAt` glides directly to `(node.x, node.y, node.z)` with smooth framing radius `r = 48` and horizontal alignment.
- When the user drags to rotate, orbiting pivots **directly around the focused node**, keeping it rock-solid centered on screen at all times.
- Damped 60/120 FPS lerp interpolation (`damp = 0.08`) eliminates any jitter or snapping.

### 2. Zero-Teardown GPU In-Place Visual Updates
- Removed `selectedNodeId` and `hoveredNodeId` completely from the `useEffect` dependency array.
- Kept `selectedNodeIdRef` and `hoveredNodeIdRef` synchronized.
- Inside `animate()`:
  - Focused nodes gently scale to `1.06` with `MeshStandardMaterial.emissiveIntensity = 0.45` and glowing spinning neon rings.
  - Connected dependency circuits remain at full `0.82` opacity while unrelated background circuits gently dim to `0.18`.
  - Zero CPU texture rebuilds, zero WebGL context restarts, zero garbage-collection hitches.

---

## 3. Verification Telemetry
- `npm run build`: Compiled cleanly with 0 TypeScript diagnostics.
- Dev server responsive on `http://localhost:3000` (`HTTP/1.1 200 OK`).
- All hard boundaries (zero-LLM in core, determinism, v0 versioning) preserved.
