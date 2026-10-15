# SynEvid Forensics — Agent Skill

Use this skill whenever you edit code in a repository with an `autopsy.toml`
(or any TypeScript/JavaScript/Python project). It makes your edits verifiable
instead of hopeful.

## 1. Before editing a public symbol or an unfamiliar file

Run blast-radius first (callers profile = who breaks if you change this):

```bash
autopsy --repo-root . impact -s "<file>::<Symbol>" --profile callers --top-k 10
autopsy --repo-root . impact -s "<file>" --profile callers --top-k 10   # file seed also works
```

Read `ranked_top` (reviewer shortlist) and `total_impacted` (full scope).
`truncated: true` means the frontier is partial — say so, don't extrapolate.

## 2. After editing, verify before you claim done

```bash
autopsy --repo-root . baseline                          # snapshot the new state
autopsy --repo-root . verify --baseline-id <before>     # exit 0 pass, 2 fail, 4 unknown
```

On exit 2, read `findings[].rule_id`, `message`, and `entities`, repair the
code following the evidence `locations`, and re-verify. Do not declare the
task complete while a `fail` finding stands.

## 3. Coverage honesty (non-negotiable)

- `coverage.state: unknown` (eval, dynamic import) or `partial`
  (reflection, generated code) limits what the engine can prove.
- Report `unknown` findings as unknown. Never convert them into a pass,
  and never let them silently justify skipping verification.
- If a callee resolves only to `symbol::<name>` (Partial), say the binding
  is name-level, not binding-level.

## 4. Digest discipline (how anyone checks your work)

Every report carries `result_digest` (BLAKE3 over canonical JSON). When you
present engine evidence, always quote the digest and the exact rerun command.
A human re-running the command must get the byte-identical digest —
otherwise the evidence is yours, not the engine's.

## 5. MCP equivalents (when the synevid MCP server is connected)

- `autopsy_impact` (use `profile: "callers"`, `top_k: 10`)
- `autopsy_verify` (use `baseline_id` for two-state checks)
- `autopsy_diff` (`base` = before snapshot, `head` = after or worktree)
- `autopsy://graph/symbols{?limit,offset,kind}` for browsing without dumps
