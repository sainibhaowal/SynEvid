# Recipe: Pinned Evidence Browse

Pin a session to one immutable snapshot so every browse call addresses the same state.

## Setup

1. **Create the baseline once**
   ```bash
   autopsy baseline --persist --format json
   # → snapshot_id: <id>
   ```

2. **Pin the session** (choose one)
   ```bash
   export AUTOPSY_SNAPSHOT=<id>          # session-wide
   # or pass --snapshot-id <id> per call
   ```

## Browse the pinned graph

```bash
# Symbols (paginated, stable-id order)
autopsy symbols --limit 50 --offset 0 --format json

# Find a symbol
autopsy find "UserService" --format json

# 1-hop neighborhood
autopsy edges --source "UserService" --direction out --format json

# Whole-graph projection
autopsy map --top 25 --format json
```

Every call above reads the **same** stored snapshot — no re-parsing, no drift.

## Impact against the pinned state

```bash
autopsy impact -s "UserService" --snapshot-id <id> --format json
```

## Verify against the pinned baseline

```bash
autopsy verify --baseline-id <id> --strict --format json
```

## Key invariants

- Blank pin values are treated as unpinned (not an error).
- Unknown snapshot IDs fail exit `3` with `analysis_snapshot_missing` advice.
- Explicit `--snapshot-id` on a call wins over `AUTOPSY_SNAPSHOT` env var.
- Pin flags inject **before** caller args (clap last-occurrence wins), so an
  explicit flag in `args` always overrides the session default.
