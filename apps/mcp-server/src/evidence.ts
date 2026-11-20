import type { AutopsyExecutionResult } from './cli.js';

/**
 * Deterministic next-best-action advice (Tier 2.1).
 *
 * The Rust engine already derives a repair triple for every CLI error —
 * `what_failed`, `why`, and `next_action` — and prints it on stderr alongside
 * the error line. The MCP layer does not re-derive advice (two derivations
 * would drift); it parses the engine's own lines and falls back to a
 * command-specific suffix for refusals that never reached the engine. The
 * markers make the attachment exact: an exit-2 *policy failure with evidence*
 * (verify refusing a violated invariant) carries a report and no advice, while
 * a genuine error carries all three fields.
 */
export interface RepairAdvice {
  what_failed: string;
  why: string;
  next_action: string;
}

/**
 * The stderr markers the engine's `run_cli` prints for every fatal error.
 * Parsing these rather than mapping exit codes keeps the advice honest: the
 * engine owns the derivation, the envelope merely reproduces it.
 */
export function repairAdviceFromStderr(stderr: string): RepairAdvice | null {
  const parse = (marker: string): string | null => {
    const line = stderr
      .split(/\r?\n/)
      .find((l) => l.startsWith(`${marker}: `));
    return line ? line.slice(marker.length + 2).trim() || null : null;
  };
  const whatFailed = parse('what_failed');
  const why = parse('why');
  const nextAction = parse('next_action');
  if (whatFailed && why && nextAction) {
    return { what_failed: whatFailed, why, next_action: nextAction };
  }
  return null;
}

/** One place decides when an envelope carries repair advice. */
export function withRepair(
  envelope: Record<string, unknown>,
  result: AutopsyExecutionResult
): Record<string, unknown> {
  if (result.exitCode === 0) {
    return envelope;
  }
  const advice = repairAdviceFromStderr(result.rawStderr || '');
  if (advice) {
    envelope.repair = advice;
  }
  return envelope;
}

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
  const report = result.data && typeof result.data === 'object' ? result.data : null;
  const snapshotId = report?.snapshot_id ?? report?.snapshots?.[0]?.snapshot_id ?? null;
  const coverageState = String(
    report?.coverage_state ?? report?.coverage?.state ?? (result.exitCode === 3 ? 'error' : 'unknown')
  ).toLowerCase();
  const normalizedCoverage = ['verified', 'partial', 'unknown', 'error', 'unavailable'].includes(coverageState)
    ? coverageState
    : 'unknown';
  const explicitVerdict = typeof report?.verdict === 'string' ? report.verdict.toUpperCase() : null;
  const verdict = explicitVerdict || (result.exitCode === 0 ? 'PASS' : result.exitCode === 4 ? 'UNKNOWN' : 'ERROR');
  const digest =
    [report?.result_digest, report?.evidence_digest, report?.blake3_digest].find(
      (candidate): candidate is string => typeof candidate === 'string' && /^blake3:[0-9a-f]{64}$/.test(candidate)
    ) ?? null;
  const warnings = Array.isArray(report?.warnings)
    ? report.warnings.filter((warning: unknown): warning is string => typeof warning === 'string')
    : [];
  return withRepair(
    {
      command,
      exit_code: result.exitCode,
      actionable: result.exitCode === 0,
      report,
      operation_id: digest,
      repository_id: report?.repository_id ?? report?.repo_id ?? report?.active_repo_id ?? null,
      canonical_path: report?.canonical_path ?? report?.repo_root ?? null,
      snapshot_id: snapshotId,
      coverage_state: normalizedCoverage,
      verdict: normalizedCoverage !== 'verified' && verdict === 'PASS' ? 'UNKNOWN' : verdict,
      warnings,
      // Wall-clock process time is operational telemetry, not canonical
      // evidence. Only a producer-measured deterministic duration belongs in
      // the shared envelope; otherwise expose null instead of making repeated
      // identical queries produce different evidence bytes.
      latency_ms: typeof report?.duration_ms === 'number' ? report.duration_ms : null,
      blake3_digest: digest,
      evidence_receipt: digest
        ? { algorithm: 'BLAKE3', digest, source: 'autopsy CLI result' }
        : null,
    },
    result
  );
}

/** Text out unchanged, anything else as canonical JSON with sorted keys. */
export function asText(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

/**
 * The envelope for an input refusal: the same fields the engine's envelope carries, but
 * for a call the engine never got to run. Never a bare `{error}` object: a consumer that
 * branches on `actionable` should be able to branch on every answer a tool can produce,
 * and a refusal *is* an answer. A refusal also carries repair advice (Tier 2.1): the
 * caller supplied something structurally unusable, so the next action names the
 * parameters to fix.
 */
export function refusedEnvelope(
  command: string,
  reason: string,
  nextAction: string
): {
  command: string;
  exit_code: number;
  actionable: boolean;
  repair: RepairAdvice;
  report: { error: string };
  operation_id: null;
  repository_id: null;
  canonical_path: null;
  snapshot_id: null;
  coverage_state: 'error';
  verdict: 'ERROR';
  warnings: string[];
  latency_ms: null;
  blake3_digest: null;
  evidence_receipt: null;
} {
  const whatFailed = 'input_refused';
  const why = `${command} refused the input before the engine ran`;
  const advice: RepairAdvice = {
    what_failed: whatFailed,
    why,
    next_action: nextAction,
  };
  // Exit code 3 is the engine's own "the analysis could not be performed" contract.
  return {
    command,
    exit_code: 3,
    actionable: false,
    repair: advice,
    report: { error: reason },
    operation_id: null,
    repository_id: null,
    canonical_path: null,
    snapshot_id: null,
    coverage_state: 'error',
    verdict: 'ERROR',
    warnings: [reason],
    latency_ms: null,
    blake3_digest: null,
    evidence_receipt: null,
  };
}
