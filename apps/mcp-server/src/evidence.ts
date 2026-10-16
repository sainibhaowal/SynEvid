import type { AutopsyExecutionResult } from './cli.js';

/**
 * The envelope every Phase 4.2 tool returns.
 *
 * The exit code is part of the engine's answer, not transport metadata: `4` means the
 * analysis *refused* to hand back an actionable result (coverage unknown or unsupported),
 * and `2` means a policy gate failed while still producing evidence. A tool that returned
 * only the report would drop the one field a consumer needs in order to decide whether to
 * act on it, so `actionable` is computed here, in one place, rather than at four call sites.
 */
export function evidenceEnvelope(command: string, result: AutopsyExecutionResult) {
  return {
    command,
    exit_code: result.exitCode,
    actionable: result.exitCode === 0,
    report: result.data,
  };
}

/** Text out unchanged, anything else as canonical JSON with sorted keys. */
export function asText(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}
