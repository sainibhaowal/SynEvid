# ADR 0011: One Authenticated Daemon as the Multi-Client Source of Truth

## Status

Accepted for Phase 0/1 implementation.

## Decision

The local SynEvid daemon owns the active canonical repository, persisted snapshot,
forensic session, and monotonically increasing live-event cursor. CLI, TUI, MCP,
Electron, and web clients are adapters; they do not create a second authoritative
repository or event stream when the daemon is available.

Clients authenticate with the daemon's loopback bearer token when one is configured.
Offline CLI analysis remains available, but its output is explicitly independent and
must not be presented as the daemon's active session.

## Contract

Every daemon JSON response carries the shared evidence fields from
`schemas/evidence-envelope.schema.json`. Missing facts are `null`, `UNKNOWN`, or
`UNAVAILABLE`; they are never replaced with a successful default. Live clients use
`since_seq`/`Last-Event-ID` to replay missed events.

## Consequences

- Reopening a client can recover the repository and cursor from `/api/v1/status`.
- MCP and CLI requests cannot silently bypass daemon authentication.
- Offline mode is useful for CI but cannot pretend to be synchronized with the UI.
- Platform-specific CI remains responsible for proving watcher, storage, and process
  behavior on Windows, Linux, and macOS.
