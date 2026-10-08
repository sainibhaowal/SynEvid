import { McpServer } from '@modelcontextprotocol/server';
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
          .describe('Base snapshot ID or git revision (defaults to latest baseline)'),
        head: z
          .string()
          .optional()
          .describe('Head snapshot ID or git revision to compare against (defaults to current worktree)'),
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
        profile: z
          .string()
          .optional()
          .describe('Policy profile name'),
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

  return server;
}
