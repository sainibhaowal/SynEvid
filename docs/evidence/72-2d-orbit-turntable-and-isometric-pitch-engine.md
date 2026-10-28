# Production Receipt 72: 2D Orbit Turntable Spin and Isometric Pitch Engine

## 1. Requirement & Architecture Context
- **Issue:** In 2D Plane mode, the HUD toggle allowed switching between `Orbit` ("Orbit Mode: Drag to rotate view") and `Move Space` ("Pan Mode: Drag in any direction to move space"). However, dragging in 2D mode was hardcoded to only call `panUniverse` via `if (controlModeRef.current === 'pan' || !is3D)`. This made Orbit mode 100% inoperable in 2D. Furthermore, if spherical $\theta$ was rotated around the vertical $Y$ axis in a flat $X-Y$ plane, the 2D plane foreshortened edge-on into a thin line.
- **Goal:**
  1. Restore full **Orbit Mode in 2D Plane**:
     - Horizontal drag ($\Delta x$): Spins the 2D Mind Map smoothly around its central root hub like a precision turntable ($\alpha$ in-plane azimuth/roll angle).
     - Vertical drag ($\Delta y$): Tilts the camera into a dramatic 3D isometric perspective ($\beta \in [0, 1.05]$ rad, $\approx 0^\circ \dots 60^\circ$ pitch) without flipping under the floor.
  2. Implement proper dynamic camera orientation:
     $$X_{\text{cam}} = x_0 + R \sin\beta \sin\alpha$$
     $$Y_{\text{cam}} = y_0 - R \sin\beta \cos\alpha$$
     $$Z_{\text{cam}} = z_0 + R \cos\beta$$
     $$\text{camUp} = (-\cos\beta \sin\alpha, \cos\beta \cos\alpha, \sin\beta)$$
     Guarantees $\text{camDir} \cdot \text{camUp} = 0$ for all $(\alpha, \beta)$, with the Mind Map perfectly upright at $\alpha = 0, \beta = 0$.
  3. Support smooth 2D auto-orbiting (`targetAlpha += 0.0004`) when idle in Orbit mode, matching 3D constellation behavior.
  4. Ensure `recenterRef.current` and folder fly-to cleanly reset $\alpha = 0, \beta = 0$ to return to the calibrated top-down overview.

---

## 2. Implementation Details
In `apps/web/src/components/CodeUniverse3D.tsx`:
- Removed `|| !is3D` from `onMouseMove`:
  ```ts
  if (isDragging) {
    if (controlModeRef.current === 'pan') {
      panUniverse(-deltaX * factor, deltaY * factor);
    } else {
      if (is3D) {
        targetTheta -= deltaX * 0.005;
        targetPhi = Math.max(0.08, Math.min(Math.PI - 0.08, targetPhi + deltaY * 0.005));
      } else {
        targetAlpha -= deltaX * 0.004;
        targetBeta = Math.max(0, Math.min(1.05, targetBeta + deltaY * 0.004));
      }
    }
  }
  ```
- Added 2D damping and dynamic camera position and up vector in the Three.js render loop.

---

## 3. Verification & Telemetry
1. **Next.js Production Build (`apps/web`):** Built with 0 TypeScript/Turbopack errors in 5.5s.
2. **Headless Inspector Test Suite (`apps/web-inspector`):** All 20/20 unit and integration tests passed in 1.34s.
3. **Rust Workspace Quality:** `cargo fmt --all -- --check` clean (0 errors).
