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

/** The default kill switch, unchanged from the original 30 s contract. */
const DEFAULT_TIMEOUT_MS = 30_000;

/**
 * Resolves the per-call timeout. `AUTOPSY_TIMEOUT_MS` lets a host raise the ceiling for
 * large repositories without a code change; a tool-supplied `timeoutMs` always wins. A
 * non-positive or non-numeric env value is ignored rather than trusted.
 */
export function resolveTimeoutMs(timeoutMs?: number): number {
  if (timeoutMs && timeoutMs > 0) {
    return timeoutMs;
  }
  const fromEnv = Number(process.env.AUTOPSY_TIMEOUT_MS);
  if (Number.isFinite(fromEnv) && fromEnv > 0) {
    return fromEnv;
  }
  return DEFAULT_TIMEOUT_MS;
}

/**
 * Resolves the per-call maxBuffer. `AUTOPSY_MAX_BUFFER_BYTES` lets a host raise the
 * ceiling for large repositories without a code change; a tool-supplied maxBufferBytes
 * always wins. A non-positive or non-numeric env value is ignored rather than trusted.
 * Default: 20 MB (unchanged from the original contract).
 */
export const DEFAULT_MAX_BUFFER_BYTES = 20 * 1024 * 1024;

export function resolveMaxBufferBytes(maxBufferBytes?: number): number {
  if (maxBufferBytes && maxBufferBytes > 0) {
    return maxBufferBytes;
  }
  const fromEnv = Number(process.env.AUTOPSY_MAX_BUFFER_BYTES);
  if (Number.isFinite(fromEnv) && fromEnv > 0) {
    return fromEnv;
  }
  return DEFAULT_MAX_BUFFER_BYTES;
}

/**
 * Snapshot pinning (Tier 1.4).
 *
 * Which CLI flag the given subcommand reads a stored snapshot ID from. Every pinned
 * run re-derives its report from that exact stored state instead of re-parsing the
 * worktree, which is what turns N sequential tool calls over the same baseline into
 * one parse plus N cheap re-reads. Commands with no snapshot input return no flags.
 */
export function pinningFlags(subcommand: string, snapshotId: string): string[] {
  switch (subcommand) {
    // Impact against a stored graph state.
    case 'impact':
      return ['--snapshot-id', snapshotId];
    // diff: the pinned snapshot is the *before* state; the after state stays the
    // worktree so a pinned diff still answers "what changed since then".
    case 'diff':
      return ['--before', snapshotId];
    // verify: invariants evaluated against the pinned baseline.
    case 'verify':
      return ['--baseline-id', snapshotId];
    // baseline/status: incremental rebuild that reuses per-file cached artifacts for
    // byte-identical files (falls back to full analysis when the cache cannot satisfy).
    case 'baseline':
      return ['--incremental', '--baseline-id', snapshotId];
    // Browse family (Tier 2.4): symbols, edges, find, and the map all read the pinned
    // graph state, so a pinned session browses one immutable snapshot end to end.
    case 'symbols':
    case 'edges':
    case 'find':
    case 'map':
      return ['--snapshot-id', snapshotId];
    default:
      return [];
  }
}

/**
 * The snapshot ID a call is pinned to, if any: the explicit tool argument wins over
 * the `AUTOPSY_SNAPSHOT` environment variable, which is how a host pins one baseline
 * across a whole agent session while a single call can still override it.
 */
export function pinnedSnapshotId(explicit?: string): string | undefined {
  const trimmed = explicit?.trim();
  if (trimmed) {
    return trimmed;
  }
  const fromEnv = process.env.AUTOPSY_SNAPSHOT?.trim();
  return fromEnv ? fromEnv : undefined;
}

/**
 * Resolves the path to the compiled `autopsy` CLI binary.
 */
export function resolveAutopsyBinary(): string {
  if (process.env.AUTOPSY_BIN && fs.existsSync(process.env.AUTOPSY_BIN)) {
    return process.env.AUTOPSY_BIN;
  }

  const isWin = process.platform === 'win32';
  const exe = isWin ? 'autopsy.exe' : 'autopsy';
  const home = process.env.HOME || process.env.USERPROFILE || '';

  const candidates = [
    path.join(home, '.local', 'bin', exe),
    path.join(home, '.cargo', 'bin', exe),
    path.join(home, '.gemini', 'antigravity', 'bin', exe),
    path.join(home, 'AppData', 'Roaming', 'Antigravity', 'bin', exe),
    path.resolve(process.cwd(), '../../target/release', exe),
    path.resolve(process.cwd(), '../../target/debug', exe),
    '/usr/local/bin/autopsy',
  ];

  for (const cand of candidates) {
    if (cand && fs.existsSync(cand)) {
      return cand;
    }
  }

  // Fallback to searching on system PATH
  return exe;
}

/**
 * Executes a subcommand on the `autopsy` CLI with deterministic parameters.
 */
export async function runAutopsy(
  subcommand: string,
  args: string[] = [],
  options: {
    repoRoot?: string;
    timeoutMs?: number;
    maxBufferBytes?: number;
    /** Pin this call to a stored snapshot (Tier 1.4); wins over AUTOPSY_SNAPSHOT. */
    snapshotId?: string;
  } = {}
): Promise<AutopsyExecutionResult> {
  const binary = resolveAutopsyBinary();
  const repoRoot = options.repoRoot || '.';
  const timeout = resolveTimeoutMs(options.timeoutMs);

  // Snapshot pinning: the pin injects the subcommand-specific flag(s) BEFORE the
  // caller's own args, so an explicit flag in `args` always wins over the pin (clap
  // takes the last occurrence). A pin therefore acts as a session default, never as an
  // override of what a caller asked for.
  const snapshot = pinnedSnapshotId(options.snapshotId);
  const pinned = snapshot ? pinningFlags(subcommand, snapshot) : [];

  // Build command arguments with JSON output format
  const fullArgs = [subcommand, '-r', repoRoot, '-f', 'json', ...pinned, ...args];

  try {
    const { stdout, stderr } = await execFileAsync(binary, fullArgs, {
      timeout,
      maxBuffer: resolveMaxBufferBytes(options.maxBufferBytes), // 20 MB default, env-overridable
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

    // Exit codes 2 and 4 still produce valid canonical JSON evidence on stdout:
    //   2 — a policy gate failed (a breaking contract change, or a selection that could
    //       not narrow when --fail-if-all-tests-required was asked for);
    //   4 — the engine refused to hand back an *actionable* answer (coverage unknown or
    //       unsupported, or partial under --strict). A refusal is not a failure, and
    //       reporting it as a transport error would hide the one field the caller needs.
    // 3 remains a failure: the analysis could not be performed at all.
    let data: any = null;
    try {
      data = JSON.parse(stdout);
    } catch {
      data = { error: stderr || stdout || 'Autopsy execution failed' };
    }

    return {
      success: exitCode === 0 || exitCode === 2 || exitCode === 4,
      exitCode,
      data,
      rawStdout: stdout,
      rawStderr: stderr,
    };
  }
}
