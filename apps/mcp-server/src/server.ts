import { McpServer, ResourceTemplate } from '@modelcontextprotocol/server';
import { z } from 'zod';
import { runAutopsy } from './cli.js';
import { asText, evidenceEnvelope } from './evidence.js';

/**
 * Creates and configures the Code Autopsy Model Context Protocol (MCP) server.
 * Implements the 5 mandatory read-only tools defined in CA-TECH-001 Chapter 4.1, plus the
 * 4 Phase 4.2 evidence tools (temporal, test selection, distributed contract diff, and the
 * organisational blast radius). Every one of them is read-only: they run the `autopsy` CLI
 * with the flags the caller supplied and return what the engine said, including the exit
 * code that says whether the answer is actionable.
 */
export function createAutopsyMcpServer(): McpServer {
  const server = new McpServer({
    name: 'code-autopsy',
    version: '0.0.1',
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
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      // Runs baseline to capture current immutable snapshot and coverage
      const result = await runAutopsy('baseline', [], { repoRoot });

      return {
        content: [
          {
            type: 'text',
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
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

      const result = await runAutopsy('impact', cliArgs, { repoRoot });

      return {
        content: [
          {
            type: 'text',
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
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

      const result = await runAutopsy('diff', cliArgs, { repoRoot });

      return {
        content: [
          {
            type: 'text',
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
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

      const result = await runAutopsy('verify', cliArgs, { repoRoot });

      return {
        content: [
          {
            type: 'text',
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
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
              text: JSON.stringify({ error: 'Missing target or finding_id argument' }),
            },
          ],
        };
      }
      const result = await runAutopsy('explain', [targetId], { repoRoot });

      return {
        content: [
          {
            type: 'text',
            text:
              typeof result.data === 'string'
                ? result.data
                : JSON.stringify(result.data, null, 2),
          },
        ],
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
        cliArgs.push('--lcov', report);
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
        from: z.string().describe('The earlier document: the previous revision, the Pact, or the flows document'),
        to: z.string().describe('The later document: the current revision, or the provider OpenAPI spec'),
        protocol: z
          .enum(['openapi', 'asyncapi', 'asyncapi-flows', 'pact'])
          .optional()
          .describe('Which contract protocol to compare (default: openapi)'),
        strict: z
          .boolean()
          .optional()
          .describe('Exit 4 when the verdict is `unknown` or coverage is partial'),
      }),
    },
    async (args) => {
      const cliArgs: string[] = [args.from, args.to];
      if (args.protocol) cliArgs.push('--protocol', args.protocol);
      if (args.strict) cliArgs.push('--strict');

      const result = await runAutopsy('contract-dist', cliArgs, { repoRoot: '.' });

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
              text: asText({
                error:
                  'the trace export requires salt_hex: telemetry has no default salt, and a ' +
                  'default would make pseudonyms comparable across repositories',
              }),
            },
          ],
        };
      }

      const radiusArgs: string[] = [args.component];
      if (args.catalog_root) radiusArgs.push('--root', args.catalog_root);
      if (args.strict) radiusArgs.push('--strict');
      const radius = await runAutopsy('blast-radius', radiusArgs, { repoRoot });

      let topology: ReturnType<typeof evidenceEnvelope> | null = null;
      if (args.traces) {
        const telemetryArgs: string[] = [args.traces, '--salt', args.salt_hex as string];
        if (args.surface) telemetryArgs.push('--surface', args.surface);
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

  // Register paginated / on-demand multigraph resource (CA-TECH-001 Ch.4.1)
  server.registerResource(
    'autopsy_graph',
    'autopsy://graph',
    {
      title: 'Symbol Dependency Multigraph',
      description: 'On-demand dependency multigraph resource to avoid dumping full graph into agent context',
      mimeType: 'application/json',
    },
    async (uri) => {
      const result = await runAutopsy('baseline', [], { repoRoot: '.' });
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
    new ResourceTemplate('autopsy://graph/symbols{?limit,offset,kind}', { list: undefined }),
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
      const result = await runAutopsy('symbols', cliArgs, { repoRoot: '.' });
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
    new ResourceTemplate('autopsy://graph/edges{?source,direction,kind,limit,offset}', {
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
              text: JSON.stringify({ error: 'Missing source symbol ID for edge browsing' }),
            },
          ],
        };
      }
      const cliArgs: string[] = ['--source', source];
      if (v.direction !== undefined) cliArgs.push('--direction', String(v.direction));
      if (v.kind !== undefined) cliArgs.push('--kind', String(v.kind));
      if (v.limit !== undefined) cliArgs.push('--limit', String(v.limit));
      if (v.offset !== undefined) cliArgs.push('--offset', String(v.offset));
      const result = await runAutopsy('edges', cliArgs, { repoRoot: '.' });
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

  return server;
}
