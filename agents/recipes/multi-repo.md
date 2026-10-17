# Recipe: Multi-Repo Workspace

Register multiple repositories and query them as a unified workspace.

## Register repositories

```bash
autopsy workspace register --path /path/to/repo-a --id repo-a
autopsy workspace register --path /path/to/repo-b --id repo-b
```

## List workspace

```bash
autopsy workspace list --format json
```

Output includes per-repo `accessible` and `analyzed` status, coverage counts,
and a workspace digest (deterministic hash of all registered repos).

## Remove a repository

```bash
autopsy workspace remove --id repo-b
```

## Query across repos (MCP)

The MCP server exposes `autopsy://workspace{?repo_root}` which lists all
registered repos with honest accessibility and analysis status.

## Per-repo analysis

Each repo is analyzed independently. Use `--repo-root` to target a specific repo:

```bash
autopsy baseline --repo-root /path/to/repo-a --persist --format json
autopsy impact -s "UserService" --repo-root /path/to/repo-a --format json
```

## Key invariants

- `accessible` is checked live (storage-open + latest-snapshot check).
- `analyzed` is honest — a repo with no stored snapshot reports `analyzed: false`.
- The workspace digest is deterministic (BLAKE3 over sorted repo entries).
- Inaccessible repos degrade coverage — they are never silently skipped.
