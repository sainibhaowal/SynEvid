# SynEvid Disaster Recovery Guide

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: All SynEvid production deployments

## Overview

This guide covers recovery procedures for various disaster scenarios affecting SynEvid deployments.

## Recovery Objectives

| Metric | Target |
|--------|--------|
| RTO (Recovery Time Objective) | < 15 minutes |
| RPO (Recovery Point Objective) | < 1 hour |
| Data Loss Tolerance | Zero (with proper backups) |

## Backup Strategy

### What to Backup

| Path | Frequency | Retention |
|------|-----------|-----------|
| `.autopsy/storage.db` | Hourly | 30 days |
| `.autopsy/events.sqlite` | Hourly | 30 days |
| `.autopsy/baselines/` | Daily | 90 days |
| `autopsy.toml` | On change | Indefinite |
| `.autopsy/invariants.yml` | On change | Indefinite |
| `autopsy.toml` (repo root) | On change | Indefinite |
| `autopsy.toml` (daemon) | On change | Indefinite |

### Backup Methods

#### Automated (Recommended)

```bash
#!/bin/bash
# scripts/backup.sh - Run via cron hourly

set -euo pipefail

BACKUP_DIR="/backup/synevid/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Stop daemon briefly for consistent backup (optional)
# pkill -STOP autopsy-daemon

# Backup storage
cp -r .autopsy/storage.db "$BACKUP_DIR/"
cp .autopsy/events.sqlite "$BACKUP_DIR/" 2>/dev/null || true

# Backup baselines
cp -r .autopsy/baselines "$BACKUP_DIR/" 2>/dev/null || true

# Backup configs
cp autopsy.toml "$BACKUP_DIR/" 2>/dev/null || true
cp .autopsy/invariants.yml "$BACKUP_DIR/" 2>/dev/null || true

# Restart daemon
# pkill -CONT autopsy-daemon

# Compress
tar -czf "$BACKUP_DIR.tar.gz" -C "$(dirname "$BACKUP_DIR")" "$(basename "$BACKUP_DIR")"
rm -rf "$BACKUP_DIR"

# Cleanup old backups (keep 30 days)
find /backup/synevid -name "*.tar.gz" -mtime +30 -delete

echo "Backup completed: $BACKUP_DIR.tar.gz"
```

```bash
# Crontab entry
0 * * * * /path/to/scripts/backup.sh >> /var/log/synevid-backup.log 2>&1
```

#### Cloud Backup (AWS S3)

```bash
# Sync to S3
aws s3 sync /backup/synevid s3://my-bucket/synevid-backups/ \
  --storage-class GLACIER_IR \
  --delete
```

### Manual Backup

```bash
# Quick manual backup
tar -czf synvid-backup-$(date +%Y%m%d_%H%M%S).tar.gz \
  .autopsy/storage.db \
  .autopsy/events.sqlite \
  .autopsy/baselines/ \
  autopsy.toml \
  .autopsy/invariants.yml
```

## Recovery Scenarios

### Scenario 1: Storage Corruption

**Symptoms**: `sqlite3: database disk image is malformed` or `PRAGMA integrity_check` fails

**Recovery Steps**:

```bash
# 1. Stop daemon
pkill autopsy-daemon

# 2. Verify corruption
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# 2a. If minor corruption - try repair
sqlite3 .autopsy/storage.db ".backup repaired.db"
mv repaired.db .autopsy/storage.db
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# 2b. If severe - restore from backup
# Find latest good backup
ls -lt /backup/synevid/*.tar.gz | head -5

# Restore
tar -xzf /backup/synevid/synevid-backup-20241124_020000.tar.gz -C /tmp/restore
cp /tmp/restore/storage.db .autopsy/storage.db
cp /tmp/restore/events.sqlite .autopsy/events.sqlite

# 3. Verify
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# 4. Restart
autopsy-daemon --repo-root /path/to/repo
```

### Scenario 2: Daemon Process Lost

**Symptoms**: `autopsy-daemon` process not running, port 3333 not listening

**Recovery Steps**:

```bash
# 1. Check if process exists
ps aux | grep autopsy-daemon
ss -tlnp | grep 3333

# 2. Check logs for crash reason
journalctl -u autopsy-daemon -n 100
# or
tail -100 ~/.autopsy/daemon.log

# 3. Check for port conflict
ss -tlnp | grep :3333

# 4. Restart
autopsy-daemon --port 3333 --data-dir ~/.autopsy --repo-root /path/to/repo

# 4b. With systemd
sudo systemctl restart autopsy-daemon

# 5. Verify
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:3333/api/v1/health
```

### Scenario 3: Configuration Lost/Corrupted

```bash
# Restore from backup
cp /backup/synevid/latest/autopsy.toml autopsy.toml
cp /backup/synevid/latest/.autopsy/invariants.yml .autopsy/invariants.yml

# Verify
autopsy doctor --check-config

# Restart daemon
autopsy-daemon --repo-root /path/to/repo
```

### Scenario 4: Accidental Data Deletion

```bash
# Accidentally deleted .autopsy/
# Restore from backup
tar -xzf /backup/synevid/synevid-backup-20241124_020000.tar.gz -C /tmp/restore
cp -r /tmp/restore/.autopsy .

# Verify
autopsy doctor --check-storage
autopsy status --format json
```

### Scenario 5: Daemon Upgrade Gone Wrong

```bash
# Rollback binary
cp /backup/bin/autopsy-daemon-v0.5.0 /usr/local/bin/autopsy-daemon
cp /backup/bin/autopsy-v0.5.0 /usr/local/bin/autopsy

# Restore config if changed
cp /backup/config/autopsy.toml.v0.5.0 autopsy.toml

# Restart
autopsy-daemon --repo-root /path/to/repo
```

### Scenario 6: Repository Deleted/Corrupted

```bash
# If repo is git
git status
git checkout -- .
git clean -fd

# If completely lost, re-clone
git clone <remote-url> /path/to/repo
cd /path/to/repo
autopsy baseline --persist
```

### Scenario 7: Network/Connectivity Issues

```bash
# Daemon reachable but clients can't connect
# Check firewall
sudo ufw status
sudo firewall-cmd --list-all

# Check daemon binding
ss -tlnp | grep 3333
# Should show: LISTEN 0 128 127.0.0.1:3333

# Check loopback
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:3333/api/v1/health
```

## Recovery Procedures by Component

### Daemon Recovery

```bash
# Full daemon recovery
1. Stop: pkill autopsy-daemon
2. Verify storage: sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"
3. Restore config: cp backup/autopsy.toml .
4. Start: autopsy-daemon --port 3333 --data-dir ~/.autopsy
5. Verify: curl -H "Auth: Bearer $TOKEN" http://127.0.0.1:3333/api/v1/health
```

### CLI Recovery

```bash
# Reinstall
cargo install --git https://github.com/sainibhaowal/SynEvid --tag v0.6.0 autopsy-cli

# Verify
autopsy doctor
autopsy version
```

### Web UI Recovery

```bash
cd apps/web
npm ci
npm run build
npm run start
# Or with PM2
pm2 restart synvid-web
```

### Electron Recovery

```bash
cd apps/desktop-electron
npm ci
npm run rebuild
npm run package
```

## Data Recovery Procedures

### Recover Specific Baseline

```bash
# List baselines
sqlite3 .autopsy/storage.db "SELECT snap_id, timestamp FROM snapshots ORDER BY timestamp DESC LIMIT 10;"

# Export specific baseline
sqlite3 .autopsy/storage.db "
  SELECT json_group_object(key, value) FROM (
    SELECT 'snapshot' as key, json(snapshots.*) as value FROM snapshots WHERE snap_id='snap_abc123'
    UNION ALL
    SELECT 'symbols', json_group_array(json(symbols.*)) FROM symbols WHERE snap_id='snap_abc123'
    UNION ALL
    SELECT 'edges', json_group_array(json(edges.*)) FROM edges WHERE snap_id='snap_abc123'
  );
" > baseline_snap_abc123.json
```

### Recover Event History

```bash
# Export events for session
sqlite3 .autopsy/events.sqlite "
  SELECT * FROM events WHERE session_id='sess_abc123' ORDER BY seq_id;
" > session_events.json

# Export all events for repo
sqlite3 .autopsy/events.sqlite "
  SELECT * FROM events WHERE repository_id='repo_abc123' ORDER BY seq_id;
" > repo_events.json
```

### Recover Forensic Sessions

```bash
# List sessions
sqlite3 .autopsy/storage.db "SELECT * FROM forensic_sessions;"

# Export session with timeline
sqlite3 .autopsy/storage.db "
  SELECT f.*, 
    (SELECT json_group_array(json(t.*)) FROM trajectory_events t WHERE t.session_id=f.session_id) as events,
    (SELECT json_group_array(json(w.*)) FROM forensic_workflows w WHERE w.session_id=f.session_id) as workflows
  FROM forensic_sessions f WHERE f.session_id='sess_abc123';
" > session_complete.json
```

## Automated Recovery Scripts

### Full Recovery Script

```bash
#!/bin/bash
# scripts/disaster-recovery.sh

set -euo pipefail

BACKUP_DIR="${1:-/backup/synevid/latest}"
REPO_ROOT="${2:-.}"

echo "=== SynEvid Disaster Recovery ==="
echo "Backup: $BACKUP_DIR"
echo "Repo: $REPO_ROOT"

# 1. Stop daemon
echo "Stopping daemon..."
pkill -f autopsy-daemon || true
sleep 2

# 2. Verify backup exists
if [ ! -d "$BACKUP_DIR" ]; then
    echo "ERROR: Backup directory not found: $BACKUP_DIR"
    exit 1
fi

# 3. Restore storage
echo "Restoring storage..."
cp "$BACKUP_DIR/storage.db" .autopsy/
cp "$BACKUP_DIR/events.sqlite" .autopsy/ 2>/dev/null || true

# 4. Restore baselines
if [ -d "$BACKUP_DIR/baselines" ]; then
    echo "Restoring baselines..."
    cp -r "$BACKUP_DIR/baselines" .autopsy/
fi

# 5. Restore configs
echo "Restoring configs..."
cp "$BACKUP_DIR/autopsy.toml" . 2>/dev/null || true
cp "$BACKUP_DIR/invariants.yml" .autopsy/ 2>/dev/null || true

# 6. Verify storage
echo "Verifying storage..."
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# 6. Start daemon
echo "Starting daemon..."
autopsy-daemon --repo-root "$REPO_ROOT" --data-dir .autopsy &
DAEMON_PID=$!

# 7. Wait and verify
sleep 3
if curl -sf http://127.0.0.1:3333/api/v1/health > /dev/null; then
    echo "✅ Recovery successful! Daemon PID: $DAEMON_PID"
else
    echo "❌ Recovery failed - daemon not responding"
    kill $DAEMON_PID 2>/dev/null || true
    exit 1
fi
```

### Point-in-Time Recovery

```bash
# Restore to specific timestamp
TARGET_TIME="2024-11-24 14:30:00"

# Find backup before target time
BACKUP_FILE=$(ls -t /backup/synevid/*.tar.gz | while read f; do
  TIMESTAMP=$(tar -tzf "$f" | head -1 | xargs stat -c %Y 2>/dev/null || echo 0)
  if [ "$TIMESTAMP" -le "$(date -d "$TARGET_TIME" +%s)" ]; then
    echo "$f"
    break
  fi
done)

if [ -n "$BACKUP_FILE" ]; then
  echo "Restoring from $BACKUP_FILE"
  tar -xzf "$BACKUP_FILE" -C /tmp/restore
  # ... restore steps
else
  echo "No backup found before $TARGET_TIME"
fi
```

## Verification Checklist

After any recovery:

```bash
# 1. Daemon health
curl -sf http://127.0.0.1:3333/api/v1/health

# 2. Storage integrity
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# 3. Configuration
autopsy doctor --check-config

# 4. Basic operations
autopsy status --format json
autopsy baseline --offline --format json | jq '.snapshot_id'
autopsy impact -s "main" --offline --format json | jq '.verdict'

# 5. Verify baselines
ls -la .autopsy/baselines/

# 6. Check events
sqlite3 .autopsy/events.sqlite "SELECT COUNT(*) FROM events;"

# 6. Check daemons
ps aux | grep autopsy-daemon
```

## Runbook Templates

### Incident Response Template

```markdown
# Incident: [Title]
**Date**: YYYY-MM-DD HH:MM
**Severity**: Critical/High/Medium/Low
**Reporter**: [Name]
**Status**: Investigating/Mitigating/Resolved

## Impact
- Affected: [components]
- Users affected: [number]
- Data loss: [Yes/No]

## Timeline
- HH:MM - Detected
- HH:MM - Investigation started
- HH:MM - Root cause identified
- HH:MM - Mitigation applied
- HH:MM - Resolved

## Root Cause
[Description]

## Resolution
[Steps taken]

## Prevention
- [ ] Action item 1
- [ ] Action item 2

## Evidence
- Logs: [links]
- Metrics: [links]
- Screenshots: [links]
```

## Contact Information

| Role | Contact | Escalation |
|------|---------|------------|
| On-call Engineer | PagerDuty | 15 min |
| Team Lead | Slack #synevid-leads | 30 min |
| Security Team | security@synevid.dev | 1 hour |
| Infrastructure | #infra-oncall | 15 min |

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*