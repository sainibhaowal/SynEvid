import { McpServer, ResourceTemplate } from '@modelcontextprotocol/server';
import { z } from 'zod';
import { runAutopsy } from './cli.js';

/**
 * Creates and configures the Code Autopsy Model Context Protocol (MCP) server.
 * Implements the 5 mandatory read-only tools defined in CA-TECH-001 Chapter 4.1.
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
