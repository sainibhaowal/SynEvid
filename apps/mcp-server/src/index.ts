#!/usr/bin/env node
import { StdioServerTransport } from '@modelcontextprotocol/server/stdio';
import { createAutopsyMcpServer } from './server.js';

async function main() {
  const server = createAutopsyMcpServer();
  const transport = new StdioServerTransport();

  // Handle process termination gracefully
  process.on('SIGINT', async () => {
    await server.close();
    process.exit(0);
  });

  process.on('SIGTERM', async () => {
    await server.close();
    process.exit(0);
  });

  await server.connect(transport);
  // Log startup diagnostics to stderr (stdout is reserved for JSON-RPC transport)
  console.error('[code-autopsy-mcp] Server running on stdio transport');
}

main().catch((error) => {
  console.error('[code-autopsy-mcp] Fatal error:', error);
  process.exit(1);
});
