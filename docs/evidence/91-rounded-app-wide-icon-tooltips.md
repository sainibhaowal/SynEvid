# Production Evidence Receipt: 91

**Topic:** Consistent app-wide icon tooltips  
**Date:** 2026-10-29  
**Traceability:** FR-P5-001-EXT, FR-P4-005

## 1. Problem

The cockpit relied on browser-native `title` popups. Their shape, placement, and timing varied by platform, and icon-only controls without a title had no hover label or accessible name.

## 2. Implementation

- Added one root-mounted tooltip layer that promotes existing interactive `title` labels to custom tooltips without replacing their accessible descriptions.
- Rendered labels in a portal above clipped panels, with the compact dark rounded-square style, pointer notch, subtle shadow, and placement that flips near viewport edges.
- Showed tooltips on pointer hover and keyboard focus, with reduced-motion support.
- Added accessible labels to icon-only close, clear, refresh, branch-delete, and tag-delete actions that previously had no label.

## 3. Verification

| Check | Result |
| :--- | :--- |
| `cd apps/web && npm run build` | Passed, including TypeScript |
| `npx eslint src/components/TooltipLayer.tsx` | Passed |
| Live Electron production UI | Hovered the Code Universe sidebar icon; the rounded tooltip rendered beside it inside the shell |
| App endpoint | Cockpit loaded from `http://127.0.0.1:3000/` |
| `cd apps/web && npm run lint` | Existing app-wide lint debt remains: 48 errors and 124 warnings; the new tooltip component passes targeted lint |

The production Electron shell remains open with the tooltip visible on the hovered sidebar icon.
