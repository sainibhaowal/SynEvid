import { McpServer, ResourceTemplate } from '@modelcontextprotocol/server';
import { z } from 'zod';
import path from 'node:path';
import { runAutopsy } from './cli.js';
import { asText, evidenceEnvelope, refusedEnvelope } from './evidence.js';

/**
 * Resolves a caller-supplied file argument against the tool's repo root (Tier 2.3).
 *
 * Subcommand file arguments (contract documents, trace exports, catalog roots, LCOV
 * reports) are interpreted by the engine relative to its own process CWD, not the
 * `-r` root, so the server resolves them to absolute paths here — one deterministic
 * place, POSIX-normalized, before they ever reach the CLI.
 */
export function resolveAgainstRepoRoot(input: string, repoRoot: string): string {
  return path.isAbsolute(input) ? input : path.resolve(repoRoot, input).split(path.sep).join('/');
}

/**
 * Creates and configures the Code Autopsy Model Context Protocol (MCP) server.
 * Implements the 5 mandatory read-only tools defined in CA-TECH-001 Chapter 4.1, the
 * 4 Phase 4.2 evidence tools (temporal, test selection, distributed contract diff, and
 * the organisational blast radius), and the Tier-1/2 agent-experience additions
 * (`find` for symbol resolution, `map` as a graph resource, `session` for repair-loop
 * receipts). Every one of them is read-only: they run the `autopsy` CLI with the flags
 * the caller supplied and return what the engine said — always inside one envelope,
 * with the exit code that says whether the answer is actionable.
 */
export function createAutopsyMcpServer(): McpServer {
  const server = new McpServer({
    name: 'code-autopsy',
    version: '0.1.0',
  });

  // 1. autopsy_status: repo/root -> index state, capabilities, snapshot ID
  server.registerTool(
    'autopsy_status',
    {
      description:
        'Inspect repository index status, snapshot identity, coverage, and adapter capabilities (read-only).',
      inputSchema: z.object({
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory (defaults to current directory)'),
        snapshot_id: z
          .string()
          .optional()
          .describe(
            'Pin this status call to a stored snapshot ID: the baseline reuses cached ' +
              'per-file artifacts for byte-identical files instead of re-parsing (also ' +
              'settable for the whole session via the AUTOPSY_SNAPSHOT environment variable)'
          ),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      // Runs baseline to capture current immutable snapshot and coverage
      const result = await runAutopsy('baseline', [], { repoRoot, snapshotId: args.snapshot_id });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('baseline', result)) }],
      };
    }
  );

  // 2. autopsy_impact: symbol/path/change + traversal profile -> entities, paths, coverage, truncation
  server.registerTool(
    'autopsy_impact',
    {
      description:
        'Compute deterministic bounded transitive impact analysis and blast radius for a symbol (read-only).',
      inputSchema: z.object({
        symbol: z
          .string()
          .describe('Target symbol name or qualified symbol identifier to compute blast radius from'),
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
        direction: z
          .enum(['forward', 'backward', 'bidirectional'])
          .optional()
          .describe('Traversal direction along dependency multigraph (default: forward)'),
        max_depth: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Maximum traversal search depth (default: 10)'),
        budget: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Maximum number of visited nodes before bounded truncation (default: 1000)'),
        profile: z
          .enum(['full', 'callers'])
          .optional()
          .describe(
            'Traversal preset: full (default, maximum recall) or callers (backward invocation closure with file transit suppressed + top-10 shortlist; recommended precision-first workflow)'
          ),
        edge_kinds: z
          .string()
          .optional()
          .describe(
            'Comma-separated edge kinds to traverse (e.g. calls,references). Overrides the preset kind set.'
          ),
        top_k: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Ranked shortlist size by shortest-path distance (full evidence set stays in the report)'),
        snapshot_id: z
          .string()
          .optional()
          .describe(
            'Analyze this stored snapshot ID instead of the current worktree (also ' +
              'settable for the whole session via the AUTOPSY_SNAPSHOT environment variable; ' +
              'an explicit argument always wins over the pin)'
          ),
        detail: z
          .enum(['summary', 'full'])
          .optional()
          .default('summary')
          .describe(
            'Detail level: summary (default; token-efficient agent mode with tier counts, top 5 likely/possible, and digest) or full (complete arrays of entities and paths)'
          ),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = ['-s', args.symbol];

      if (args.direction) {
        cliArgs.push('--direction', args.direction);
      }
      if (args.max_depth) {
        cliArgs.push('--max-depth', args.max_depth.toString());
      }
      if (args.budget) {
        cliArgs.push('--budget', args.budget.toString());
      }
      if (args.profile) {
        cliArgs.push('--profile', args.profile);
      }
      if (args.edge_kinds) {
        cliArgs.push('--edge-kinds', args.edge_kinds);
      }
      if (args.top_k) {
        cliArgs.push('--top-k', args.top_k.toString());
      }
      const detail = args.detail || 'summary';
      cliArgs.push('--detail', detail);

      const result = await runAutopsy('impact', cliArgs, { repoRoot, snapshotId: args.snapshot_id });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('impact', result)) }],
      };
    }
  );

  // 3. autopsy_diff: base/head -> semantic change set
  server.registerTool(
    'autopsy_diff',
    {
      description:
        'Compute semantic AST symbol delta between baseline snapshot and current worktree (read-only).',
      inputSchema: z.object({
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
        base: z
          .string()
          .optional()
          .describe('Base snapshot ID from a prior baseline (defaults to latest baseline)'),
        head: z
          .string()
          .optional()
          .describe('Stored head snapshot ID to compare against (defaults to current worktree)'),
        before: z
          .string()
          .optional()
          .describe('Alias for base snapshot ID'),
        after: z
          .string()
          .optional()
          .describe('Alias for head snapshot ID'),
        base_snapshot: z
          .string()
          .optional()
          .describe('Legacy alias for base snapshot ID'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [];

      const base = args.base || args.before || args.base_snapshot;
      const head = args.head || args.after;

      if (base) {
        cliArgs.push('--before', base);
      }
      if (head) {
        cliArgs.push('--after', head);
      }

      const result = await runAutopsy('diff', cliArgs, { repoRoot, snapshotId: args.base || args.before });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('diff', result)) }],
      };
    }
  );

  // 4. autopsy_verify: base/head + policy profile -> findings pass/fail/unknown + receipt
  server.registerTool(
    'autopsy_verify',
    {
      description:
        'Evaluate architectural invariants and contracts against repository snapshot or diff (read-only).',
      inputSchema: z.object({
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
        base: z
          .string()
          .optional()
          .describe('Optional baseline snapshot ID to verify diff against'),
        baseline_id: z
          .string()
          .optional()
          .describe('Alias for base snapshot ID'),
        invariants_file: z
          .string()
          .optional()
          .describe('Path to invariants YAML policy file (defaults to .autopsy/invariants.yml)'),
        strict: z
          .boolean()
          .optional()
          .describe('Strict mode: reject unknown constructs with exit code 4'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [];

      const base = args.base || args.baseline_id;
      if (base) {
        cliArgs.push('--baseline-id', base);
      }
      if (args.invariants_file) {
        cliArgs.push('--invariants-file', args.invariants_file);
      }
      if (args.strict) {
        cliArgs.push('--strict');
      }

      const result = await runAutopsy('verify', cliArgs, { repoRoot, snapshotId: base });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('verify', result)) }],
      };
    }
  );

  // 5. autopsy_explain: finding/evidence ID -> derivation/path expansion
  server.registerTool(
    'autopsy_explain',
    {
      description:
        'Explain evidence receipts, provenance paths, and rule derivations for a finding or symbol (read-only).',
      inputSchema: z.object({
        target: z
          .string()
          .optional()
          .describe('Target finding ID or symbol ID to explain'),
        finding_id: z
          .string()
          .optional()
          .describe('Alias for finding ID target'),
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const targetId = args.target || args.finding_id;
      if (!targetId) {
        return {
          content: [
            {
              type: 'text',
              text: asText(
                refusedEnvelope(
                  'explain',
                  'missing target or finding_id argument',
                  'call autopsy_explain with `target` set to a stable ID from find/symbols or a finding ID from a verify report'
                )
              ),
            },
          ],
        };
      }
      const result = await runAutopsy('explain', [targetId], { repoRoot });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('explain', result)) }],
      };
    }
  );

  // 6. autopsy_find: a partial name -> the symbols it resolves to (Tier 1.1).
  // `impact` resolves an *exact* identity, which is the right contract for traversal;
  // resolution from what an agent actually types — a partial, case-insensitive fragment
  // — belongs here, and its stable IDs are what feed `impact -s`.
  server.registerTool(
    'autopsy_find',
    {
      description:
        'Resolve a pattern against the analyzed graph: case-insensitive substring over ' +
        'qualified names, declaring paths, and stable IDs, returning stable IDs an agent can ' +
        'route straight into the impact tool. An empty result set is an honest "no such ' +
        'symbol" with the population searched, never an error (read-only).',
      inputSchema: z.object({
        pattern: z
          .string()
          .min(1)
          .describe('The text to match, case-insensitively (a substring of a symbol, path, or stable ID)'),
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
        kind: z
          .string()
          .optional()
          .describe('Restrict matches to one symbol kind (function, method, class, interface, type, enum, variable)'),
        name_only: z
          .boolean()
          .optional()
          .describe(
            'Match qualified names and paths only. Stable IDs remain in the output but do ' +
              'not widen the match — a hash fragment the caller did not type is not a hit.'
          ),
        limit: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Maximum entries to return (default 50, capped at 1000)'),
        offset: z
          .number()
          .int()
          .nonnegative()
          .optional()
          .describe('Zero-based offset into the deterministic stable-id order'),
        snapshot_id: z
          .string()
          .optional()
          .describe(
            'Resolve the pattern against a stored snapshot ID instead of the current ' +
              "worktree: pins one immutable graph so 'find then impact' addresses the " +
              'same evidence (the AUTOPSY_SNAPSHOT environment variable pins a whole ' +
              'session; an explicit argument always wins over the pin)'
          ),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [args.pattern];
      if (args.kind) cliArgs.push('--kind', args.kind);
      if (args.name_only) cliArgs.push('--name-only');
      if (args.limit) cliArgs.push('--limit', String(args.limit));
      if (args.offset) cliArgs.push('--offset', String(args.offset));

      const result = await runAutopsy('find', cliArgs, { repoRoot, snapshotId: args.snapshot_id });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('find', result)) }],
      };
    }
  );

  // 6. autopsy_temporal: revision journal (Pillar 1) or mined co-change (Pillar 5)
  server.registerTool(
    'autopsy_temporal',
    {
      description:
        'Replay repository history: the revision journal of facts that were added and removed, ' +
        'optionally reconstructed at one revision; the mined pairs of paths that change ' +
        'together; or the identity transitions (renames, moves, modifications) between two ' +
        'revisions. Co-change is temporal co-occurrence and never causation (read-only).',
      inputSchema: z.object({
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory (must contain .git)'),
        view: z
          .enum(['timeline', 'co_change', 'lineage'])
          .optional()
          .describe(
            'Which temporal engine to run. `timeline` is the revision journal of facts ' +
              '(Pillar 1); `co_change` is the mined historical co-change edges (Pillar 5); ' +
              '`lineage` is file identity across a revision pair, where only a byte-identical ' +
              'rename reaches `verified` and every similarity match is `partial`. ' +
              'They are different engines with different reports and different digests. ' +
              'Default: timeline.'
          ),
        at: z
          .string()
          .optional()
          .describe('Reconstruct the graph at this revision instead of emitting the whole journal (timeline only)'),
        from: z
          .string()
          .optional()
          .describe('The earlier revision for `lineage` (default: the first parent of `to`)'),
        to: z
          .string()
          .optional()
          .describe('The later revision for `lineage` (default: HEAD)'),
        max_files: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            'Bound on files read from one revision (lineage only). A bound that is hit makes ' +
              'the event set a stated lower bound instead of a silent truncation.'
          ),
        max_commits: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            'Bound on commits walked. The walker refuses a timeline that does not fit rather ' +
              'than truncating it silently, so this must cover the reachable history.'
          ),
        window_seconds: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Co-change window width in seconds (default: 183 days)'),
        min_pair_commits: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Fewest joint commits before a path pair is reported at all'),
        compact: z
          .boolean()
          .optional()
          .describe(
            'Compact the report for a context window: every count and the coverage stay ' +
              'exact, arrays above top_k are sliced and the residual is named in the report ' +
              '(never silent). Storage-format digests still describe the full report.'
          ),
        top_k: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('The compact slice size (default 20, capped at 1000); only with compact'),
        strict: z
          .boolean()
          .optional()
          .describe('Exit 4 when coverage is partial as well as when it is unknown'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const view = args.view || 'timeline';
      const cliArgs: string[] = [];
      if (args.compact) cliArgs.push('--compact');
      if (args.top_k) cliArgs.push('--top-k', String(args.top_k));

      if (view === 'co_change') {
        if (args.max_commits) cliArgs.push('--max-commits', String(args.max_commits));
        if (args.window_seconds) cliArgs.push('--window-seconds', String(args.window_seconds));
        if (args.min_pair_commits) cliArgs.push('--min-pair-commits', String(args.min_pair_commits));
      } else if (view === 'lineage') {
        if (args.from) cliArgs.push('--from', args.from);
        if (args.to) cliArgs.push('--to', args.to);
        if (args.max_files) cliArgs.push('--max-files', String(args.max_files));
      } else {
        if (args.at) cliArgs.push('--at', args.at);
        if (args.max_commits) cliArgs.push('--max-commits', String(args.max_commits));
      }
      if (args.strict) cliArgs.push('--strict');

      const command = view === 'co_change' ? 'cochange' : view === 'lineage' ? 'lineage' : 'temporal';
      const result = await runAutopsy(command, cliArgs, { repoRoot });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope(command, result)) }],
      };
    }
  );

  // 7. autopsy_test_selection: changed files + coverage -> the tests that must run (Pillar 2)
  server.registerTool(
    'autopsy_test_selection',
    {
      description:
        'Select the tests a set of changed files requires, with the engine`s own fallback and ' +
        'coverage state. A selection that could not narrow is reported as such rather than ' +
        'guessed at (read-only).',
      inputSchema: z.object({
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
        changed: z
          .array(z.string())
          .min(1)
          .describe('Changed source files, POSIX-relative to the repository root'),
        lcov: z
          .array(z.string())
          .optional()
          .describe('LCOV coverage report paths, one per suite'),
        fail_if_all_tests_required: z
          .boolean()
          .optional()
          .describe('Exit 2 when the plan is `run-all-tests`: the CI gate for a selection that did not narrow'),
        strict: z
          .boolean()
          .optional()
          .describe('Exit 4 when coverage is partial as well as when it is unknown'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [];
      for (const changed of args.changed) {
        cliArgs.push('--changed', changed);
      }
      for (const report of args.lcov || []) {
        cliArgs.push('--lcov', resolveAgainstRepoRoot(report, repoRoot));
      }
      if (args.fail_if_all_tests_required) cliArgs.push('--fail-if-all-tests-required');
      if (args.strict) cliArgs.push('--strict');

      const result = await runAutopsy('test-select', cliArgs, { repoRoot });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('test-select', result)) }],
      };
    }
  );

  // 8. autopsy_contract_diff: two contract revisions -> changes and a verdict (Pillar 3)
  server.registerTool(
    'autopsy_contract_diff',
    {
      description:
        'Diff two revisions of one distributed contract (OpenAPI, AsyncAPI, AsyncAPI flows, ' +
        'or Pact) and report breaking versus non-breaking changes with the verdict and its ' +
        'coverage. A `compatible` verdict is only ever returned with verified coverage ' +
        '(read-only).',
      inputSchema: z.object({
        repo_root: z
          .string()
          .optional()
          .describe("Base directory for relative `from`/`to` document paths (defaults to the current directory)"),
        from: z.string().describe('The earlier document: the previous revision, the Pact, or the flows document'),
        to: z.string().describe('The later document: the current revision, or the provider OpenAPI spec'),
        protocol: z
          .enum(['openapi', 'asyncapi', 'asyncapi-flows', 'pact'])
          .optional()
          .describe('Which contract protocol to compare (default: openapi)'),
        compact: z
          .boolean()
          .optional()
          .describe(
            'Compact the change list for a context window: counts and the verdict stay ' +
              'exact, and the truncated residual is named in the report (never silent)'
          ),
        top_k: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('The compact slice size (default 20, capped at 1000); only with compact'),
        strict: z
          .boolean()
          .optional()
          .describe('Exit 4 when the verdict is `unknown` or coverage is partial'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [
        resolveAgainstRepoRoot(args.from, repoRoot),
        resolveAgainstRepoRoot(args.to, repoRoot),
      ];
      if (args.protocol) cliArgs.push('--protocol', args.protocol);
      if (args.compact) cliArgs.push('--compact');
      if (args.top_k) cliArgs.push('--top-k', String(args.top_k));
      if (args.strict) cliArgs.push('--strict');

      const result = await runAutopsy('contract-dist', cliArgs, { repoRoot });

      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('contract-dist', result)) }],
      };
    }
  );

  // 9. autopsy_distributed_blast_radius: ownership + observed topology for one component
  server.registerTool(
    'autopsy_distributed_blast_radius',
    {
      description:
        'The cross-service blast radius of one catalog component: its owners, the components ' +
        'the same team owns, its siblings, the APIs it provides and consumes, and the ' +
        'components that depend on it — plus, when a trace export is supplied, which of those ' +
        'routes and callers were actually observed. Unresolved references are named and ' +
        'degrade coverage; they are never counted as edges (read-only).',
      inputSchema: z.object({
        component: z
          .string()
          .describe('The component reference to traverse from, e.g. `component:default/checkout`. A bare name is read as a component.'),
        repo_root: z
          .string()
          .optional()
          .describe('Repository root (used for the catalog scan when catalog_root is absent)'),
        catalog_root: z
          .string()
          .optional()
          .describe('The tree to scan for catalog entities (defaults to the repository root)'),
        traces: z
          .string()
          .optional()
          .describe('An OTLP/JSON trace export to correlate against the observed surface'),
        surface: z
          .string()
          .optional()
          .describe('A declared-surface JSON document to correlate the trace export against'),
        salt_hex: z
          .string()
          .optional()
          .describe(
            'The pseudonymisation salt for the trace export: exactly 64 hex characters. ' +
              'Required when `traces` is given, and never defaulted — a default salt would ' +
              'make pseudonyms comparable across repositories and therefore forgeable.'
          ),
        windows: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('Minimum observation windows a dead-code claim needs (default: 3)'),
        compact: z
          .boolean()
          .optional()
          .describe(
            'Compact the dependents list for a context window: counts and coverage stay ' +
              'exact, and the truncated residual is named in the report (never silent)'
          ),
        top_k: z
          .number()
          .int()
          .positive()
          .optional()
          .describe('The compact slice size (default 20, capped at 1000); only with compact'),
        strict: z
          .boolean()
          .optional()
          .describe('Exit 4 when coverage is partial as well as when it is unknown'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      if (args.traces && !args.salt_hex) {
        return {
          content: [
            {
              type: 'text',
              text: asText(
                refusedEnvelope(
                  'blast-radius',
                  'the trace export requires salt_hex: telemetry has no default salt, and a ' +
                    'default would make pseudonyms comparable across repositories',
                  'supply salt_hex (exactly 64 hex characters) together with traces, or run blast radius without telemetry',
                )
              ),
            },
          ],
        };
      }

      const radiusArgs: string[] = [args.component];
      if (args.catalog_root) {
        radiusArgs.push('--root', resolveAgainstRepoRoot(args.catalog_root, repoRoot));
      }
      if (args.compact) radiusArgs.push('--compact');
      if (args.top_k) radiusArgs.push('--top-k', String(args.top_k));
      if (args.strict) radiusArgs.push('--strict');
      const radius = await runAutopsy('blast-radius', radiusArgs, { repoRoot });

      let topology: ReturnType<typeof evidenceEnvelope> | null = null;
      if (args.traces) {
        const telemetryArgs: string[] = [
          resolveAgainstRepoRoot(args.traces, repoRoot),
          '--salt',
          args.salt_hex as string,
        ];
        if (args.surface) {
          telemetryArgs.push('--surface', resolveAgainstRepoRoot(args.surface, repoRoot));
        }
        if (args.windows) telemetryArgs.push('--windows', String(args.windows));
        if (args.strict) telemetryArgs.push('--strict');
        topology = evidenceEnvelope('telemetry', await runAutopsy('telemetry', telemetryArgs, { repoRoot }));
      }

      return {
        content: [
          {
            type: 'text',
            text: asText({ ...evidenceEnvelope('blast-radius', radius), topology }),
          },
        ],
      };
    }
  );

  // 10. autopsy_session (Tier 2.5 + 3.7): deterministic repair-loop receipts &
  // Task-Completion Forensic Trajectory Ledger. The store records every evidence step
  // as the CLI wrote it — no wall-clock time, verdicts derived only from recorded exit codes.
  server.registerTool(
    'autopsy_session',
    {
      description:
        'Open, record, close, report, show, or list a repair-loop session. Closing derives an honest ' +
        'verdict (pass/fail/unknown) from the last recorded evidence step — never invented — ' +
        'mirrors its exit code, and persists the Task-Completion Forensic Trajectory Report to ' +
        '`.autopsy/runs/latest.{json,md}` for subsequent AI agents and developers.',
      inputSchema: z.object({
        action: z
          .enum(['start', 'record', 'close', 'report', 'show', 'list'])
          .describe('Which session lifecycle operation to run'),
        repo_root: z
          .string()
          .optional()
          .describe('Workspace whose `.autopsy/sessions.json` holds the store'),
        session_id: z
          .string()
          .optional()
          .describe('The session id (from start/list); required for record/close/show, optional for report'),
        invoked: z
          .array(z.string())
          .optional()
          .describe('The command argv as invoked (record only), e.g. ["verify", "--strict"]'),
        exit_code: z
          .number()
          .int()
          .optional()
          .describe('The engine exit code of the recorded command (record only)'),
        snapshot_id: z
          .string()
          .optional()
          .describe('Optional snapshot id the recorded step is pinned to (record only)'),
        note: z
          .string()
          .optional()
          .describe('Optional verbatim note: what changed and why (record only)'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [args.action];
      if (args.action === 'record') {
        if (!args.session_id || !args.invoked || args.exit_code === undefined) {
          return {
            content: [
              {
                type: 'text',
                text: asText(
                  refusedEnvelope(
                    'session',
                    'recording needs session_id, the invoked argv, and exit_code',
                    'open one with action "start", then record each step with the command argv and the exit code the engine returned'
                  )
                ),
              },
            ],
          };
        }
        cliArgs.push('--session-id', args.session_id, '--exit-code', String(args.exit_code));
        if (args.snapshot_id) cliArgs.push('--snapshot-id', args.snapshot_id);
        if (args.note) cliArgs.push('--note', args.note);
        cliArgs.push('--', ...args.invoked);
      } else if (args.action === 'report') {
        if (args.session_id) cliArgs.push('--session-id', args.session_id);
      } else if (args.action !== 'start' && args.action !== 'list') {
        if (!args.session_id) {
          return {
            content: [
              {
                type: 'text',
                text: asText(
                  refusedEnvelope(
                    'session',
                    `action "${args.action}" needs session_id`,
                    'pass session_id from a previous start (or use action "list")'
                  )
                ),
              },
            ],
          };
        }
        cliArgs.push('--session-id', args.session_id);
      }

      const result = await runAutopsy('session', cliArgs, { repoRoot });
      // A failing close is a verdict, not a broken engine: if the process emitted
      // no repair advice, the envelope carries none and stays exactly the receipt.
      return {
        content: [{ type: 'text', text: asText(evidenceEnvelope('session', result)) }],
      };
    }
  );

  // Register paginated / on-demand multigraph resource (CA-TECH-001 Ch.4.1)
  // Bounded code map: autopsy://graph/map{?under,top} (Tier 1.3).
  //
  // A projection an agent can read in one screen — kind counts, package counts, and the
  // top-N nodes by degree — served by the engine's own `map` command, so its scoping and
  // ranking rules are the engine's, tested ones, not re-derived here in TypeScript. It is
  // deliberately NOT the full graph: browsing belongs to /symbols and /edges, and any
  // window that fit the whole graph would not need a map.
  server.registerResource(
    'autopsy_graph_map',
    new ResourceTemplate('autopsy://graph/map{?under,top,repo_root,snapshot}', { list: undefined }),
    {
      title: 'Code Map (bounded projection)',
      description:
        'Deterministic whole-graph projection: symbol-kind counts, package counts, and the ' +
        'top-N symbols by degree (in + out), optionally scoped to a path prefix. Counts are ' +
        'exact; the map is never the full graph.',
      mimeType: 'application/json',
    },
    async (uri, variables) => {
      const v = (variables || {}) as Record<string, unknown>;
      const cliArgs: string[] = [];
      if (v.under !== undefined) cliArgs.push('--under', String(v.under));
      if (v.top !== undefined) cliArgs.push('--top', String(v.top));
      const repoRoot = v.repo_root === undefined ? '.' : String(v.repo_root);
      const result = await runAutopsy('map', cliArgs, { repoRoot, snapshotId: v.snapshot === undefined ? undefined : String(v.snapshot) });
      return {
        contents: [
          {
            uri: uri.href,
            text: asText(
              evidenceEnvelope('map', result)
            ),
          },
        ],
      };
    }
  );

  // The graph resource proper keeps its cheap summary contract: an index-state answer,
  // not a graph dump. The usable projections live in /graph/symbols, /graph/edges, and
  // /graph/map above.
  server.registerResource(
    'autopsy_graph',
    new ResourceTemplate('autopsy://graph{?repo_root}', { list: undefined }),
    {
      title: 'Symbol Dependency Multigraph',
      description: 'On-demand dependency multigraph resource to avoid dumping full graph into agent context',
      mimeType: 'application/json',
    },
    async (uri, _variables) => {
      const repoRoot = new URL(uri.href).searchParams.get('repo_root') || '.';
      const result = await runAutopsy('baseline', [], { repoRoot });
      return {
        contents: [
          {
            uri: uri.href,
            text: JSON.stringify(
              {
                resource: 'autopsy://graph',
                status: 'available',
                summary: result.data?.coverage || {},
                snapshot_id: result.data?.snapshots?.[0]?.snapshot_id || 'unknown',
              },
              null,
              2
            ),
          },
        ],
      };
    }
  );

  // Paginated symbol listing: autopsy://graph/symbols?limit=&offset=&kind=
  // Full graph dumps are intentionally unavailable; agents page through
  // stable-id-ordered symbols instead (CA-TECH-001 Ch.4.1).
  server.registerResource(
    'autopsy_graph_symbols',
    new ResourceTemplate('autopsy://graph/symbols{?limit,offset,kind,repo_root,snapshot}', { list: undefined }),
    {
      title: 'Symbol Index (paginated)',
      description: 'Cursor-paginated symbol listing in stable-id order. Never dumps the full graph.',
      mimeType: 'application/json',
    },
    async (uri, variables) => {
      const v = (variables || {}) as Record<string, unknown>;
      const cliArgs: string[] = [];
      if (v.limit !== undefined) cliArgs.push('--limit', String(v.limit));
      if (v.offset !== undefined) cliArgs.push('--offset', String(v.offset));
      if (v.kind !== undefined) cliArgs.push('--kind', String(v.kind));
      const repoRoot = v.repo_root === undefined ? '.' : String(v.repo_root);
      const result = await runAutopsy('symbols', cliArgs, { repoRoot, snapshotId: v.snapshot === undefined ? undefined : String(v.snapshot) });
      return {
        contents: [
          {
            uri: uri.href,
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
      };
    }
  );

  // Paginated 1-hop neighborhood: autopsy://graph/edges?source=&direction=&kind=&limit=&offset=
  server.registerResource(
    'autopsy_graph_edges',
    new ResourceTemplate('autopsy://graph/edges{?source,direction,kind,limit,offset,repo_root,snapshot}', {
      list: undefined,
    }),
    {
      title: 'Symbol Neighborhood (paginated)',
      description: 'Cursor-paginated 1-hop dependency edges for one symbol. Never dumps the full graph.',
      mimeType: 'application/json',
    },
    async (uri, variables) => {
      const v = (variables || {}) as Record<string, unknown>;
      const source = v.source === undefined ? '' : String(v.source);
      if (!source) {
        return {
          contents: [
            {
              uri: uri.href,
              text: asText(
                refusedEnvelope(
                  'edges',
                  'missing source symbol ID for edge browsing',
                  'page `autopsy://graph/symbols` first and pass one of its stable_id values as `source`'
                )
              ),
            },
          ],
        };
      }
      const cliArgs: string[] = ['--source', source];
      if (v.direction !== undefined) cliArgs.push('--direction', String(v.direction));
      if (v.kind !== undefined) cliArgs.push('--kind', String(v.kind));
      if (v.limit !== undefined) cliArgs.push('--limit', String(v.limit));
      if (v.offset !== undefined) cliArgs.push('--offset', String(v.offset));
      const repoRoot = v.repo_root === undefined ? '.' : String(v.repo_root);
      const result = await runAutopsy('edges', cliArgs, { repoRoot, snapshotId: v.snapshot === undefined ? undefined : String(v.snapshot) });
      return {
        contents: [
          {
            uri: uri.href,
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
      };
    }
  );

  // Workspace registry resource (Tier 2.3): the multi-repo view with honest
  // per-repo accessibility and analysis status, served by the engine's own
  // `workspace list` command.
  server.registerResource(
    'autopsy_workspace',
    new ResourceTemplate('autopsy://workspace{?repo_root}', { list: undefined }),
    {
      title: 'Workspace Registry',
      description:
        'Registered repositories with live accessibility and analysis status, coverage counts, and the workspace digest.',
      mimeType: 'application/json',
    },
    async (uri, variables) => {
      const v = (variables || {}) as Record<string, unknown>;
      const repoRoot = v.repo_root === undefined ? '.' : String(v.repo_root);
      const result = await runAutopsy('workspace', ['list'], { repoRoot });
      return {
        contents: [
          {
            uri: uri.href,
            text: asText(evidenceEnvelope('workspace', result)),
          },
        ],
      };
    }
  );

  // Latest Forensic Trajectory Report resource (Tier 3.7): allows any AI model or IDE
  // to read the prior run's forensic trajectory, verdict, and next-agent guidance
  // before making edits in a new turn.
  server.registerResource(
    'autopsy_runs_latest',
    new ResourceTemplate('autopsy://runs/latest{?repo_root}', { list: undefined }),
    {
      title: 'Latest Task-Completion Forensic Trajectory Report',
      description:
        'Deterministic forensic report from the most recent agent session (`autopsy session report`), ' +
        'including verdict, snapshots, step trajectory, and next_agent_guidance.',
      mimeType: 'application/json',
    },
    async (uri, variables) => {
      const v = (variables || {}) as Record<string, unknown>;
      const repoRoot = v.repo_root === undefined ? '.' : String(v.repo_root);
      const result = await runAutopsy('session', ['report'], { repoRoot });
      return {
        contents: [
          {
            uri: uri.href,
            text: asText(evidenceEnvelope('session', result)),
          },
        ],
      };
    }
  );

  // Machine-readable toolspec resource (Tier 3.1): the same deterministic document
  // as `autopsy tools --format json`, so an agent can discover every subcommand,
  // flag, exit code, and the feedback channel without shelling out. The content is
  // repository-independent, so this is registered as a fixed URI: `autopsy://tools`
  // is readable exactly as documented (a template would require a query parameter
  // even though there is nothing repository-specific to parameterize).
  server.registerResource(
    'autopsy_tools',
    'autopsy://tools',
    {
      title: 'Machine-readable toolspec',
      description:
        'Deterministic toolspec: every subcommand with its arguments, snapshot pinning, ' +
        'coverage semantics, the evidence envelope, exit codes, global flags, and the ' +
        'agent feedback channel.',
      mimeType: 'application/json',
    },
    async (uri) => {
      const result = await runAutopsy('tools', []);
      return {
        contents: [
          {
            uri: uri.href,
            text: asText(result.data),
          },
        ],
      };
    }
  );

  return server;
}
