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
        base_snapshot: z
          .string()
          .optional()
          .describe('Optional base snapshot ID to compare against (defaults to HEAD snapshot)'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [];

      if (args.base_snapshot) {
        cliArgs.push('--base', args.base_snapshot);
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
        strict: z
          .boolean()
          .optional()
          .describe('Strict mode: reject unknown constructs with non-zero status'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const cliArgs: string[] = [];

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
          .describe('Target finding ID or symbol ID to explain'),
        repo_root: z
          .string()
          .optional()
          .describe('Target repository root directory'),
      }),
    },
    async (args) => {
      const repoRoot = args.repo_root || '.';
      const result = await runAutopsy('explain', [args.target], { repoRoot });

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

  return server;
}
