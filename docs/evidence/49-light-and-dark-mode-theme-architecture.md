# Evidence Receipt 49: Full Light & Dark Mode Architecture Closure

- **Date:** 2026-10-27
- **Target Version:** `v0.2.5` (Strictly under permanent zero-major invariant `v0.xx.xxx`)
- **Parent Specifications:**
  - `Resource/5.0_Visual_Cockpit_And_Canvas_PRD/07_UI_BUTTONS_ANIMATIONS_MOTION_AND_WIRING_SPEC.md`
  - `Resource/5.0_Visual_Cockpit_And_Canvas_PRD/00_MASTER_PRD_AND_MOSCOW_MATRIX.md`

## 1. Problem Statement & Root Cause

The user reported:
> "ligh mode and other is not working i need full light mode nd dark funial dark is owrking but light is nto wlroking"

### Root Cause Analysis:
1. **Hardcoded Body Classes:** In `apps/web/src/app/layout.tsx`, `<body className="min-h-screen bg-[#05080e] text-[#f0f4fc] ...">` was hardcoded with Tailwind utilities for pitch black `#05080e`. In CSS specificity, class selectors override tag-level CSS variables, keeping the entire page canvas dark regardless of the theme.
2. **Tailwind CSS v4 Variant Missing:** Tailwind v4 (`@import "tailwindcss";`) by default uses system media query `@media (prefers-color-scheme: dark)`. Without `@custom-variant dark (&:where(.dark, .dark *));`, any `dark:` utility class in JSX never toggled based on the HTML `.dark` class.
3. **Hardcoded SSR HTML Class:** `layout.tsx` hardcoded `<html className="dark ...">`, causing hydration mismatch and preventing Light Mode persistence on reload.
4. **Hardcoded Scrollbar Tracks:** `globals.css` hardcoded `::-webkit-scrollbar-track { background: #04060a; }` and `scrollbar-color: #00f0ff #04060a;`.
5. **Provider Scoping:** `ThemeProvider` was wrapped only inside `page.tsx` rather than top-level in `layout.tsx`.

## 2. Technical Solution Applied

1. **Tailwind v4 Custom Variant & Theme Registration (`apps/web/src/app/globals.css`):**
   - Added `@custom-variant dark (&:where(.dark, .dark *));` to map `dark:` utilities directly to the `.dark` class.
   - Registered all theme color variables (`--color-background`, `--color-foreground`, `--color-card`, `--color-card-elevated`, `--color-border-subtle`, `--color-border-strong`, `--color-text-primary`, `--color-text-secondary`, `--color-text-muted`) in `@theme`.
   - Explicitly defined `:root, html.light` and `html.dark` with crisp high-contrast palettes and `color-scheme: light` / `color-scheme: dark`.
   - Dynamic custom scrollbars adapting to `--background` and `--border-strong`.

2. **Top-Level Root Layout & Anti-Flicker Pre-Render Script (`apps/web/src/app/layout.tsx`):**
   - Added synchronous pre-hydration `<script>` in `<head>` that reads `localStorage.getItem('synevid_theme')` and immediately applies `.light` or `.dark` class before the first paint.
   - Switched `<body>` to `bg-[var(--background)] text-[var(--foreground)]` with smooth color transitions.
   - Wrapped `{children}` inside `<ThemeProvider>` at the root layout level.

3. **Theme Context Modernization (`apps/web/src/context/ThemeContext.tsx`):**
   - Automatically synchronizes `root.style.colorScheme` and `root.setAttribute('data-theme', t)`.
   - Safely persists preference to `localStorage`.

4. **Component Surface & Brand Logo Identity Audit:**
   - Replaced the plain `SE` gradient square placeholder in `CockpitHeader.tsx` with the official, full-vector cybernetic SynEvid brand emblem (`/assets/logo.svg` / `/assets/logo_icon.png`), complete with animated online status indicator and glowing hover micro-interactions.
   - Added branded emblem anchors to the Left Navigation Rail dock (`apps/web/src/app/page.tsx`), `CapabilitiesModal.tsx`, and `ConnectRepoModal.tsx`.
   - Header, left rail, context drawer, HUD toolbars, modals, and status bars updated to use CSS variables and clean `dark:` variants.
   - 3D spatial viewport styled with `bg-[#080d1a] dark:bg-[#04060b]`, floating HUDs styled with `bg-[var(--card)]`, maintaining 100% readability across both modes.

## 3. Verification

- `cd apps/web && npm run build`: **PASS** (12.0s Turbopack compilation, 14.9s TypeScript check, 4/4 static generation).
- Next.js development server: **HTTP 200 OK** (`GET / 200 in 245ms`).
- E2E Daemon endpoints: **5/5 PASS** via `bash scripts/tests/test-live-daemon-e2e.sh`.
