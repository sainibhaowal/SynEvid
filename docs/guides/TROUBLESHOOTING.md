# SynEvid Troubleshooting Guide

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: All SynEvid components

## Quick Diagnosis

### Health Check

```bash
# Daemon health
curl -H "Authorization: Bearer $SYNEVID_LOOPBACK_TOKEN" \
  http://127.0.0.1:3333/api/v1/health

# Expected: {"status":"ok","version":"0.6.0","daemon":"autopsy-daemon"}

# CLI doctor
autopsy doctor

# Full diagnostics
autopsy doctor --verbose --check-all
```

## Common Issues

### Daemon Won't Start

| Symptom | Cause | Solution |
|---------|-------|----------|
| `address already in use` | Port 3333 occupied | `autopsy-daemon --port 3334` or kill existing process |
| `permission denied` | Data dir permissions | `chmod 700 ~/.autopsy && chmod 600 ~/.autopsy/*` |
| `config parse error` | Invalid TOML | `autopsy doctor --check-config` |
| `storage corruption` | SQLite corruption | See [Storage Corruption](#storage-corruption) |
| `loopback token invalid` | Token mismatch | `rm ~/.autopsy/loopback-token` then restart |

### Daemon Not Responding

```bash
# Check if running
ps aux | grep autopsy-daemon
ss -tlnp | grep 3333

# Check logs
journalctl -u autopsy-daemon -f
# or
tail -f ~/.autopsy/daemon.log

# Check port
curl -v http://127.0.0.1:3333/api/v1/health

# Restart
pkill autopsy-daemon
autopsy-daemon --port 3333 --no-auth &
```

### CLI Connection Issues

```bash
# Test connection
autopsy status --format json

# With explicit daemon
autopsy --daemon-host 127.0.0.1 --daemon-port 3333 status

# Check token
echo $SYNEVID_LOOPBACK_TOKEN
# Regenerate if needed
rm ~/.autopsy/loopback-token
autopsy-daemon --generate-token
export SYNEVID_LOOPBACK_TOKEN=$(cat ~/.autopsy/loopback-token)
```

### Baseline Issues

| Problem | Cause | Solution |
|----------|-------|----------|
| `no commits` | Empty repo | Add commits first |
| `snapshot not found` | Missing baseline | Run `autopsy baseline --persist` |
| `storage full` | Disk full | Clean up: `autopsy doctor --clean` |
| `incremental failed` | Corrupted state | `autopsy baseline --persist` (full) |
| `timeout` | Large repo | Increase timeout: `--timeout 300` |

```bash
# Debug baseline
RUST_LOG=debug autopsy baseline --persist --offline 2>&1 | tee baseline.log

# Force full re-scan
autopsy baseline --persist --offline --force

# Check storage
sqlite3 ~/.autopsy/storage.db "PRAGMA integrity_check;"
```

### Impact Analysis Issues

| Problem | Cause | Solution |
|---------|-------|----------|
| `unknown symbol` | Symbol not indexed | Run `autopsy baseline --persist` first |
| `no results` | Symbol not found | Check spelling: `autopsy find "symbol"` |
| `coverage unknown` | Dynamic constructs | Add type annotations |
| `timeout` | Large blast radius | Reduce: `--max-depth 3 --budget 500` |
| `partial coverage` | Dynamic imports | Add type annotations |

```bash
# Debug impact
RUST_LOG=debug autopsy impact -s "main" --offline --format json 2>&1 | tee impact.log

# Test specific symbol
autopsy find "UserService" --kind function

# Check coverage
autopsy impact -s "UserService" --detail full --offline
```

### Verify Issues

| Problem | Cause | Solution |
|---------|-------|----------|
| `unknown coverage` | Dynamic code | Add type annotations |
| `strict fails` | Partial coverage | Remove `--strict` or fix coverage |
| `invariant not found` | Missing config | Check `.autopsy/invariants.yml` |
| `policy fail` | Breaking change | Fix code or update invariants |

```bash
# Debug verify
RUST_LOG=debug autopsy verify --strict --offline 2>&1 | tee verify.log

# Check specific invariant
autopsy verify --invariants-file .autopsy/invariants.yml --strict

# Explain failure
autopsy explain --rule-id CLEAN_ARCHITECTURE_LAYERS
```

### Git Issues

| Problem | Cause | Solution |
|---------|-------|----------|
| `not a git repo` | No .git | `git init` |
| `bad revision` | Invalid ref | Check `git log --oneline -5` |
| `bisect failed` | No test script | Provide `--test-script` |
| `merge conflict` | Unresolved | `git status` then resolve |

```bash
# Debug git
RUST_LOG=debug autopsy git galaxy --limit 100

# Check git health
autopsy git status
autopsy doctor --check-git
```

## Storage Issues

### Storage Corruption

```bash
# Check integrity
sqlite3 ~/.autopsy/storage.db "PRAGMA integrity_check;"

# Check WAL
ls -la ~/.autopsy/*.wal

# Backup and rebuild
cp ~/.autopsy/storage.db ~/.autopsy/storage.db.backup
sqlite3 ~/.autopsy/storage.db ".backup ~/.autopsy/storage.db.rebuilt"
mv ~/.autopsy/storage.db.rebuilt ~/.autopsy/storage.db
```

### Storage Full

```bash
# Check size
du -sh ~/.autopsy

# Clean old baselines
autopsy doctor --clean --max-baselines 10

# Compact
sqlite3 ~/.autopsy/storage.db "VACUUM;"

# Check WAL size
ls -la ~/.autopsy/*-wal
```

### Migration Issues

```bash
# Check migration status
sqlite3 ~/.autopsy/storage.db "SELECT * FROM migrations;"

# Force re-run migrations
rm -rf ~/.autopsy/migrations
autopsy-daemon --repo-root /path/to/repo --persist
```

## Performance Issues

### Slow Baseline

```bash
# Profile
RUST_LOG=debug autopsy baseline --persist 2>&1 | tee baseline.log

# Check file count
find . -name "*.ts" -o -name "*.py" -o -name "*.rs" | wc -l

# Use incremental
autopsy baseline --incremental --persist

# Increase parallelism
autopsy baseline --jobs 8 --persist
```

### Slow Impact

```bash
# Reduce scope
autopsy impact -s "symbol" --max-depth 3 --budget 500 --offline

# Limit detail
autopsy impact -s "sym" --detail summary --offline

# Use cached baseline
autopsy impact -s "sym" --snapshot-id snap_abc123 --offline
```

### High Memory

```bash
# Check memory
ps aux | grep autopsy

# Reduce cache
autopsy baseline --max-file-bytes 524288 --persist

# Limit jobs
autopsy baseline --jobs 2 --persist
```

## Network Issues

### Daemon Unreachable

```bash
# Check firewall
sudo ufw status
sudo firewall-cmd --list-all

# Check loopback
curl -v http://127.0.0.1:3333/api/v1/health

# Check IPv6
curl -v http://[::1]:3333/api/v1/health

# Check Docker
docker ps | grep autopsy
docker logs <container_id>
```

### WebSocket Issues

```bash
# Test WebSocket
wscat -c ws://127.0.0.1:3333/api/v1/live

# Check proxy
curl -v -H "Connection: Upgrade" -H "Upgrade: websocket" \
  http://127.0.0.1:3333/api/v1/live
```

## Web UI Issues

### Page Not Loading

```bash
# Check Next.js
cd apps/web && npm run dev

# Check build
cd apps/web && npm run build

# Check daemon connectivity
curl http://localhost:3000/api/health
```

### WebSocket Disconnects

```bash
# Check proxy
# nginx config:
# proxy_set_header Upgrade $http_upgrade;
# proxy_set_header Connection "upgrade";
# proxy_read_timeout 86400;
```

## Electron Issues

### App Won't Start

```bash
# Rebuild
cd apps/desktop-electron
npm run rebuild

# Clear cache
rm -rf node_modules package-lock.json
npm install

# Check logs
# Windows: %APPDATA%\SynEvid\logs
# macOS: ~/Library/Logs/SynEvid
# Linux: ~/.config/SynEvid/logs
```

### Daemon Not Starting

```bash
# Check path
which autopsy-daemon

# Manual start
/absolute/path/to/autopsy-daemon --port 3333 --no-auth
```

## MCP Server Issues

### Connection Failed

```bash
# Check daemon
curl http://127.0.0.1:3333/api/v1/health

# Check MCP server
cd apps/mcp-server && npm run dev

# Check logs
tail -f apps/mcp-server/logs/mcp.log
```

### Tool Failures

```bash
# Test tool directly
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"autopsy_impact","arguments":{"seed":"main"}}}' | node apps/mcp-server/dist/index.js
```

## Logging & Debugging

### Enable Debug Logging

```bash
# Daemon
RUST_LOG=debug autopsy-daemon --port 3333 --no-auth

# CLI
RUST_LOG=debug autopsy impact -s "main" --offline

# Web
DEBUG=synevid:* npm run dev

# Electron
DEBUG=synevid:* npm run dev
```

### Log Locations

| Component | Location |
|-----------|----------|
| Daemon (systemd) | `journalctl -u autopsy-daemon` |
| Daemon (manual) | `~/.autopsy/daemon.log` |
| CLI | Stdout/stderr |
| Web | Browser console |
| Electron | `~/Library/Logs/SynEvid` (macOS) |
| Electron | `%APPDATA%\SynEvid\logs` (Windows) |
| Electron | `~/.config/SynEvid/logs` (Linux) |
| MCP | `apps/mcp-server/logs/` |

### Log Levels

```bash
# Levels: trace, debug, info, warn, error
RUST_LOG=debug
RUST_LOG=autopsy_cli=debug,autopsy_daemon=info
RUST_LOG=autopsy_impact=trace,autopsy_verify=debug
```

## Performance Tuning

### Baseline Slow

```bash
# Use incremental
autopsy baseline --incremental --persist

# Reduce file size limit
autopsy baseline --max-file-bytes 524288 --persist

# Increase parallelism
autopsy baseline --jobs 8 --persist

# Disable unused adapters
# Edit autopsy.toml: disable unused adapters
```

### High Memory

```bash
# Limit memory
export RAYON_NUM_THREADS=4
export MALLOC_CONF="dirty_decay_ms:0,muzzy_decay_ms:0"

# Reduce cache
autopsy baseline --max-file-bytes 524288 --persist
```

### Slow Impact

```bash
# Reduce scope
autopsy impact -s "sym" --max-depth 2 --budget 100 --offline

# Use specific baseline
autopsy impact -s "sym" --snapshot-id snap_abc --offline
```

## Getting Help

### Collect Diagnostics

```bash
# Create support bundle
autopsy doctor --bundle > support-bundle.txt

# Include:
# - autopsy doctor --verbose output
# - autopsy doctor --check-all output
# - Relevant logs
# - Steps to reproduce
```

### Report Issues

```bash
# GitHub Issues
https://github.com/sainibhaowal/SynEvid/issues

# Include:
# - autopsy doctor output
# - Steps to reproduce
# - Expected vs actual behavior
# - Logs (sanitized)
```

### Debug Commands

```bash
# Daemon status
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:3333/api/v1/status

# List repos
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:3333/api/v1/repos/list

# Live events
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:3333/api/v1/live?since_seq=0

# Capabilities
curl http://127.0.0.1:3333/api/v1/capabilities
```

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*