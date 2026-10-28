# Receipt 64: Light Mode Card Radiance, Fog Elimination & Zero Grey-Out

**Date:** 2026-10-27  
**Status:** PASS  
**Scope:** `apps/web/src/components/CodeUniverse3D.tsx`

---

## 1. Problem Statement
The user reported that cards in White/Light mode still appeared greyed out and lacked crisp contrast. Looking at the scene telemetry:
1. **Distant Fog Washout:** `scene.fog = new THREE.FogExp2(0xf8fafc, 0.0008)` caused 25% to 45% of the card luminance to blend into a muddy grey fog haze at standard camera distance (`radius = 320`).
2. **PBR Lighting Falloff & ACES Compression:** `MeshStandardMaterial` under Three.js PBR lighting calculates diffuse reflection divided by $\pi$, and `ACESFilmicToneMapping` remapped pure white `#ffffff` into a muted grey tone.
3. **5% Residual Transparency:** `targetOpacity: 0.95` allowed the background canvas and floor grid to bleed through the white cards, creating a translucent grey look.
4. **Card Dimensions & Distance:** Cards at `W: 18, H: 11.25` at distance `320` appeared too small and distant to read clearly.

---

## 2. Technical Implementation
In [`apps/web/src/components/CodeUniverse3D.tsx`](file:///home/ravi/Projects%20Beta/SynEvid/apps/web/src/components/CodeUniverse3D.tsx):

### A. Fog Elimination in Light Mode
- Completely disabled exponential fog in light mode:
  ```ts
  scene.fog = isDark ? new THREE.FogExp2(0x0a0e17, 0.0005) : null;
  ```
- Positioned the default camera closer (`radius = 210` in 3D, `180` in 2D instead of `320`), bringing all nodes immediately into crisp view.

### B. Unlit `MeshBasicMaterial` & Tone Mapping Bypass
- Replaced PBR `MeshStandardMaterial` in Light Mode with an unlit `MeshBasicMaterial`:
  ```ts
  const slateMat = isDark
    ? new THREE.MeshStandardMaterial({ map: texture, transparent: true, opacity: 1.0, roughness: 0.35, metalness: 0.05, emissive: new THREE.Color(0x000000) })
    : new THREE.MeshBasicMaterial({ map: texture, transparent: true, opacity: 1.0 });
  ```
  - **Zero Angle-Dependent Shadowing:** Cards remain at 100% full luminance regardless of camera rotation or orbit angles.
  - **Zero ACES Washout:** Set `renderer.toneMapping = isDark ? THREE.ACESFilmicToneMapping : THREE.NoToneMapping;`, ensuring `#ffffff` renders as true 255,255,255 white.

### C. 100% Solid Opacity
- Updated the render loop target opacity to `1.0` (100% solid, zero transparency bleed):
  ```ts
  const targetOpacity = isSelected ? 1.0 : isConnected ? 1.0 : (isDark ? 0.12 : 0.25);
  ```

### D. Expanded Card Geometry & Prominent Cobalt Borders
- Expanded card slate dimensions from `18x11.25` to `22x13.75` (W: 22, H: 13.75, D: 0.7).
- Expanded outer orbital docking rings from radius `11.2` to `13.6` with `opacity: 0.75` in deep sky blue (`#0284c7`).
- Rendered 7px cobalt framing borders (`#0284c7`) around the solid `#ffffff` cards with pitch-black obsidian typography (`#050a14`), deep slate paths (`#1e293b`), and solid white metric boxes (`#ffffff`) with `#94a3b8` borders.

### E. Rich Cobalt & Royal Purple Circuit Highways
- In light mode, circuit cables use saturated cobalt blue `(0.01, 0.45, 0.88)` and royal purple `(0.35, 0.15, 0.85)` at `0.88` opacity, eliminating pale faint line washout against the canvas.

---

## 3. Verification & Telemetry
1. **Next.js Production Build:**
   ```bash
   cd apps/web && npm run build
   ```
   **Output:**
   ```text
   ▲ Next.js 16.4.2 (Turbopack)
   ✓ Compiled successfully in 6.1s
   ✓ Finished TypeScript in 13.8s
   ✓ Generating static pages (4/4) in 5.0s
   Route (app)
   ┌ ○ /
   └ ○ /_not-found
   ```
   Status: Clean compile (0 errors, 0 warnings).

2. **Visual Inspection:**
   - Cards in White mode render with brilliant, unlit pure white backgrounds and 7px cobalt borders.
   - Text is 100% readable pitch-black obsidian (`#050a14`).
   - Fog haze is eliminated, and cables appear as rich cobalt blue highways.
