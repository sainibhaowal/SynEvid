# ADR-006: GUI is optional

Status: Accepted - 2026-09-28

Accepted. CLI/JSON/SARIF/MCP are first-class; GUI is deferred.

*Update (2026-10-18):* the presentation layer now also includes
`apps/synevid-lsp` — an LSP stdio server answering hover requests with inline
impact evidence (Tier 3.2). It is text-native and consistent with this decision:
the web inspector remains zero-backend, and a GUI is still deferred.
