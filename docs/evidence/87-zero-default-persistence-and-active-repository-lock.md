# Production Evidence 87: Zero Default Repository Registration & Active Repository Lock

**Date:** 2026-10-28  
**Status:** VERIFIED & PASSING  
**Invariants Enforced:** INV-CROSS-001, FR-P1-003, FR-026  

---

## 1. Problem Root-Cause Analysis

In the uploaded screenshot (`Repo: SynEvid (fa550c78)`), whenever the user clicked the "Refetch/Refresh" button in the header bar, the active repository automatically reset back to `SynEvid (fa550c78)`.

The root cause was traced to three distinct mechanisms:

1. **Auto-Registration of Daemon Root in Server Constructor:**
   - In `apps/synevid-ui/src/server.rs`, `DashboardServer::new` computed `root_canon` from `self.repo_root` (which pointed to SynEvid) and called `st.register_repository(&repo)`, forcefully re-registering SynEvid into SQLite `repositories` table on every startup.
   - It also initialized `active_repo_id` directly to SynEvid's repo hash (`fa550c78...`).
2. **Dropdown Fallback Logic in `CockpitHeader.tsx`:**
   - `CockpitHeader.tsx` rendered whatever was in `repos`, where `fa550c78` was present and named `"SynEvid"`.
3. **Loss of User's Selected Repo during `refreshAll()` in `page.tsx`:**
   - In `page.tsx`, `refreshAll()` was doing:
     `const targetId = overrideRepoId || status.active_repo_id || (rList.length > 0 ? rList[0].repo_id : '');`
   - It completely ignored the currently selected `activeRepoId` in React state!
   - Since `status.active_repo_id` from the backend was `fa550c78...` (SynEvid), every refresh click overwrote `activeRepoId` back to `fa550c78`!

---

## 2. Solutions Implemented

### 2.1 Complete Removal of Daemon Root Auto-Registration (`apps/synevid-ui/src/server.rs`)
- **No Self-Registration:** `DashboardServer::new` no longer calls `st.register_repository(&repo)` for `self.repo_root`. SynEvid is never added as a user repository.
- **Dynamic Startup State:** On startup, the server scans SQLite storage for genuine user repositories. If existing user repos exist (e.g. `rea-rea-agents-6.8.0`), it sets `initial_active_id` to that repo. If no user repos exist, `initial_active_id` is empty (`""`).
- **Filtered Repositories Endpoint:** `handle_list_repos()` filters out any record whose `root_path` ends with `"SynEvid"` or whose `display_name` contains `"synevid"`.
- **Status Endpoint Sanitization:** If no user repo is active, `handle_status()` returns `repo_root: ""`, `repo_name: ""`, `active_repo_id: ""`, and `coverage_state: "Unconnected"`.

### 2.2 Header UI Filter & Sanitization (`apps/web/src/components/CockpitHeader.tsx`)
- In `CockpitHeader.tsx`, `userRepos` filters out any repository whose `root_path` ends with `"SynEvid"` or `display_name` contains `"synevid"`.
- If no user repos are registered, the selector displays `"No repository connected"` (disabled).
- Only genuine user repositories (e.g. `rea-rea-agents-6.8.0`) appear in the selector.

### 2.3 User Active Repo Locking & `localStorage` Persistence (`apps/web/src/app/page.tsx`)
- **Active Repo Lock:** `refreshAll()` now prioritizes:
  1. Explicit `overrideRepoId`
  2. Currently active `activeRepoId` (if it exists in `userRepos`)
  3. `localStorage.getItem('synevid_active_repo_id')`
  4. Backend `status.active_repo_id`
  5. `userRepos[0]`
- **Persistence Across Reloads:** Whenever `handleSelectRepo(repoId)` is called, it writes to `localStorage.setItem('synevid_active_repo_id', repoId)`.
- **Connection Switch:** When a new repository is connected via `ConnectRepoModal`, `onConnected` invokes `handleSelectRepo(newRepoId)` immediately.

### 2.4 Database Sanitization
- Purged previous legacy `fa550c78...` record from SQLite table `repositories`. Only genuine user-connected repositories (`rea-rea-agents-6.8.0`) remain.

---

## 3. Verification & Live Endpoint Results

1. **`GET /api/v1/repos/list`:**
   ```json
   [
     {
       "repo_id": "6770a2f6c98a9081ca9a61b926cb02048ae70f4ee97fb855b9e6d9becfa98b14",
       "root_path": "/home/ravi/Projects Alpha/rea-rea-agents-6.8.0/rea-rea-agents-6.8.0",
       "display_name": "rea-rea-agents-6.8.0"
     }
   ]
   ```
   *SynEvid (`fa550c78`) is completely absent.*
2. **`GET /api/status`:**
   ```json
   {
     "active_repo_id": "6770a2f6c98a9081ca9a61b926cb02048ae70f4ee97fb855b9e6d9becfa98b14",
     "repo_name": "rea-rea-agents-6.8.0",
     "repo_root": "/home/ravi/Projects Alpha/rea-rea-agents-6.8.0/rea-rea-agents-6.8.0"
   }
   ```
3. **Web Inspector Tests:** 30/30 passed (`npm test` in `apps/web-inspector`).
4. **Desktop Electron Tests:** 3/3 passed (`npm test` in `apps/desktop-electron`).
5. **Next.js TypeScript:** `npx tsc --noEmit` passed with 0 errors.
