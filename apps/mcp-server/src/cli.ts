import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import fs from 'node:fs';
import path from 'node:path';

const execFileAsync = promisify(execFile);

export interface AutopsyExecutionResult {
  success: boolean;
  exitCode: number;
  data: any;
  rawStdout: string;
  rawStderr: string;
}

/**
 * Resolves the path to the compiled `autopsy` CLI binary.
 */
export function resolveAutopsyBinary(): string {
  if (process.env.AUTOPSY_BIN && fs.existsSync(process.env.AUTOPSY_BIN)) {
    return process.env.AUTOPSY_BIN;
  }

  // Check user cargo bin
  const cargoBin = path.join(process.env.HOME || '', '.cargo', 'bin', 'autopsy');
  if (fs.existsSync(cargoBin)) {
    return cargoBin;
  }

  // Check workspace target dirs relative to apps/mcp-server
  const releasePath = path.resolve(process.cwd(), '../../target/release/autopsy');
  if (fs.existsSync(releasePath)) {
    return releasePath;
  }

  const debugPath = path.resolve(process.cwd(), '../../target/debug/autopsy');
  if (fs.existsSync(debugPath)) {
    return debugPath;
  }

  // Fallback to searching on system PATH
  return 'autopsy';
}

/**
 * Executes a subcommand on the `autopsy` CLI with deterministic parameters.
 */
export async function runAutopsy(
  subcommand: string,
  args: string[] = [],
  options: { repoRoot?: string; timeoutMs?: number } = {}
): Promise<AutopsyExecutionResult> {
  const binary = resolveAutopsyBinary();
  const repoRoot = options.repoRoot || '.';
  const timeout = options.timeoutMs || 30_000;

  // Build command arguments with JSON output format
  const fullArgs = [subcommand, '-r', repoRoot, '-f', 'json', ...args];

  try {
    const { stdout, stderr } = await execFileAsync(binary, fullArgs, {
      timeout,
      maxBuffer: 20 * 1024 * 1024, // 20 MB buffer
      env: {
        ...process.env,
        RUST_BACKTRACE: '0',
      },
    });

    let data: any = null;
    try {
      data = JSON.parse(stdout);
    } catch {
      data = stdout.trim();
    }

    return {
      success: true,
      exitCode: 0,
      data,
      rawStdout: stdout,
      rawStderr: stderr,
    };
  } catch (error: any) {
    const exitCode = typeof error.code === 'number' ? error.code : 1;
    const stdout = error.stdout ? error.stdout.toString() : '';
    const stderr = error.stderr ? error.stderr.toString() : error.message || '';

    // Exit code 2 indicates a policy violation (e.g. invariant failed),
    // which still produces valid canonical JSON evidence on stdout.
    let data: any = null;
    try {
      data = JSON.parse(stdout);
    } catch {
      data = { error: stderr || stdout || 'Autopsy execution failed' };
    }

    return {
      success: exitCode === 0 || exitCode === 2,
      exitCode,
      data,
      rawStdout: stdout,
      rawStderr: stderr,
    };
  }
}
