# Receipt 59: Motion Slide Toggle Buttons for 2D/3D and Orbit/Pan Controls

**Date:** 2026-10-27  
**Status:** PASS  
**Scope:** `apps/web/src/components/CodeUniverse3D.tsx`

---

## 1. Problem Statement
The mode toggle controls for:
1. `[ 2D Plane | 3D Universe ]`
2. `[ Orbit | Move Space ]`
previously used instant text-color snapping (`bg-neutral-800` vs `bg-cyan-500/20`), giving an abrupt digital feel without visual physics or sliding motion feedback. The user requested fluid motion slide buttons where an animated indicator pill smoothly glides across the container when toggling states.

---

## 2. Technical Implementation
In [`apps/web/src/components/CodeUniverse3D.tsx`](file:///home/ravi/Projects%20Beta/SynEvid/apps/web/src/components/CodeUniverse3D.tsx#L969-L1022):
- **Motion Track Container:**
  - Designed a high-contrast rounded squircle container (`rounded-xl p-1 bg-neutral-900/90 border border-neutral-700/60 shadow-inner backdrop-blur-md`).
  - Defined explicit width (`w-[200px]`) and layout constraints (`relative flex items-center`).
- **Sliding Indicator Pill:**
  - Absolute positioned indicator element with smooth GPU-accelerated transition:
    `transition-all duration-300 ease-[cubic-bezier(0.2,1,0.3,1)]`.
  - Dynamic horizontal offset:
    - Left state (`2D` / `Orbit`): `left: '4px'`, `width: 'calc(50% - 6px)'`.
    - Right state (`3D` / `Move Space`): `left: 'calc(50% + 2px)'`, `width: 'calc(50% - 6px)'`.
  - Styled with vibrant gradient and neon drop shadow:
    `bg-gradient-to-r from-cyan-500 to-cyan-600 shadow-md shadow-cyan-500/30`.
- **Z-Index Layering:**
  - Placed toggle buttons above the indicator with `relative z-10 flex-1 flex items-center justify-center` and color transition `transition-colors duration-200`. Active state text renders high-contrast white text (`text-white font-semibold drop-shadow-sm`), while inactive text remains legible neutral (`text-neutral-400 hover:text-neutral-200`).

---

## 3. Verification & Telemetry
1. **Next.js Production Build:**
   ```bash
   cd apps/web && npm run build
   ```
   **Output:**
   ```text
   ▲ Next.js 16.4.2 (Turbopack)
   ✓ Compiled successfully in 4.4s
   ✓ Finished TypeScript in 9.9s
   ✓ Generating static pages (4/4) in 3.1s
   Route (app)
   ┌ ○ /
   └ ○ /_not-found
   ```
   Status: Clean compile (0 errors, 0 warnings).

2. **Interactive UI Verification:**
   - Clicking `3D Universe` slides the glowing cyan pill to the right smoothly within 300ms using natural cubic-bezier deceleration.
   - Clicking `2D Plane` slides the pill back to the left, seamlessly switching projection and camera lock.
   - Same physical motion interaction operates on `Orbit` / `Move Space`.
