# SynEvid Migration Guide

**Version**: 1.0
**Last Updated**: 2024-11-24
**Applies To**: All SynEvid deployments

## Overview

This guide covers upgrading SynEvid across versions, including storage migrations, configuration changes, and rollback procedures.

## Version Compatibility Matrix

| From Version | To Version | Storage Migration | Config Migration | Breaking Changes |
|--------------|------------|-------------------|------------------|------------------|
| 0.5.x | 0.6.0 | ✅ Auto | ✅ Auto | Daemon consolidation |
| 0.4.x | 0.5.x | ✅ Auto | ✅ Auto | New envelope format |
| 0.3.x | 0.4.x | ✅ Auto | Manual | New invariants API |
| 0.2.x | 0.3.x | Manual | Manual | Storage v2 |

## Pre-Upgrade Checklist

- [ ] Backup `.autopsy/storage.db` and `.autopsy/events.sqlite`
- [ ] Backup `autopsy.toml` and `.autopsy/invariants.yml`
- [ ] Stop all SynEvid processes (`autopsy`, `autopsy-daemon`, `synevid-ui`)
- [ ] Verify disk space (2x storage size recommended)
- [ ] Read release notes for breaking changes

## Upgrade Procedures

### Standard Upgrade (0.5.x → 0.6.0)

```bash
# 1. Backup
cp -r .autopsy .autopsy.backup.$(date +%Y%m%d)
cp autopsy.toml autopsy.toml.backup.$(date +%Y%m%d)

# 2. Stop services
pkill autopsy-daemon
pkill synevid-ui

# 3. Install new version
cargo install --git https://github.com/sainibhaowal/SynEvid --tag v0.6.0 autopsy-daemon autopsy-cli

# 4. Run migration (automatic on first run)
autopsy-daemon --repo-root /path/to/repo --persist

# 4. Verify
autopsy status --format json
```

### Storage Migration Details (0.5.x → 0.6.0)

The following migrations run automatically on first daemon start:

| Migration | Description | Reversible |
|-----------|-------------|------------|
| `001_add_lease_table` | Runtime lease management | ❌ |
| `002_add_event_cursor` | Event sequence tracking | ❌ |
| `003_unify_envelope` | Single evidence envelope | ❌ |
| `004_add_offline_flag` | Command offline support | ❌ |

**Rollback**: Not supported for 0.5.x → 0.6.0 (schema changes). Restore from backup.

### Configuration Migration (0.5.x → 0.6.0)

| Old Config | New Config | Action |
|------------|------------|--------|
| `daemon.port` | `port` | Auto-mapped |
| `daemon.host` | `host` | Auto-mapped |
| `daemon.data_dir` | `data_dir` | Auto-mapped |
| `ui.port` | Removed | Use `autopsy-daemon --port` |
| `synevid_ui.port` | Removed | Use `autopsy-daemon --port` |

**New Required Fields**:
```toml
# autopsy.toml (0.6.0+)
[daemon]
port = 3333
host = "127.0.0.1"
data_dir = "~/.autopsy"
managed_runtime = false
no_auth = false
log_level = "info"
json_logs = false
```

### Breaking Changes in 0.6.0

| Change | Impact | Migration |
|--------|--------|-----------|
| `autopsy ui` removed | CLI command | Use `autopsy-daemon` |
| `synevid-ui` binary removed | Electron sidecar | Use `autopsy-daemon` |
| Single envelope format | All clients | Regenerate TypeScript types |
| Offline mode explicit | `--offline` flag | Add to CI scripts |
| Daemon port fixed to 3333 | Default port | Update firewall rules |

## Rollback Procedures

### Emergency Rollback (0.6.0 → 0.5.x)

```bash
# 1. Stop new version
pkill autopsy-daemon

# 2. Restore storage
rm -rf .autopsy
mv .autopsy.backup.YYYYMMDD .autopsy

# 3. Restore config
mv autopsy.toml.backup.YYYYMMDD autopsy.toml

# 4. Reinstall old version
cargo install --git https://github.com/sainibhaowal/SynEvid --tag v0.5.0 autopsy-cli

# 5. Verify
autopsy status
```

### Partial Rollback (Config Only)

```bash
# Restore config only
mv autopsy.toml.backup.YYYYMMDD autopsy.toml

# Restart daemon
autopsy-daemon --repo-root /path/to/repo
```

## Migration Verification

### Automated Checks

```bash
# Run migration verification
autopsy doctor --check-migrations

# Expected output:
# ✅ Storage schema v4 (current)
# ✅ All migrations applied
# ✅ No pending migrations
# ✅ Config valid
```

### Manual Verification

```bash
# 1. Check storage integrity
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# 2. Verify envelope format
autopsy baseline --persist --format json | jq '.evidence_envelope'

# 3. Test all clients
autopsy status --format json
autopsy impact -s "main" --format json
```

## Troubleshooting

### Common Issues

| Error | Cause | Resolution |
|-------|-------|------------|
| `storage schema version mismatch` | Old storage with new binary | Run migration or restore backup |
| `config field 'ui.port' not recognized` | Old config format | Remove `ui.port`, use `autopsy-daemon --port` |
| `daemon unavailable at 127.0.0.1:3333` | Port conflict | Check port 3333, use `--port` |
| `loopback token invalid` | Token mismatch | Regenerate: `rm ~/.autopsy/loopback-token` |

### Recovery Commands

```bash
# Force migration re-run
rm -rf .autopsy/migrations
autopsy-daemon --repo-root /path/to/repo --persist

# Reset loopback token
rm ~/.autopsy/loopback-token
export SYNEVID_LOOPBACK_TOKEN=$(autopsy-daemon --generate-token)

# Rebuild storage from scratch
rm -rf .autopsy
autopsy baseline --persist --repo-root /path/to/repo
```

## Version History

| Version | Date | Key Changes |
|---------|------|-------------|
| 0.6.0 | 2024-11-24 | Daemon consolidation, single envelope, offline mode |
| 0.5.0 | 2024-10-15 | Phase 5: CI, installers, evidence envelope |
| 0.4.0 | 2024-09-01 | Phase 4: Invariants, contracts, AI cache |
| 0.3.0 | 2024-07-15 | Phase 3: Native Git, timeline, lineage |
| 0.2.0 | 2024-05-01 | Phase 2: Test impact, coverage, inventory |
| 0.1.0 | 2024-03-01 | Phase 1: Baseline, impact, verify, diff |

## Support Matrix

| Version | Supported | EOL Date |
|---------|-----------|----------|
| 0.6.x | ✅ Active | 2025-11-24 |
| 0.5.x | 🟡 Security only | 2025-05-24 |
| 0.4.x | ❌ EOL | 2024-11-24 |
| 0.3.x | ❌ EOL | 2024-08-15 |
| 0.2.x | ❌ EOL | 2024-05-01 |

## Support

- **Migration Issues**: GitHub Issues with `migration` label
- **Emergency**: security@synevid.dev
- **Documentation**: This directory