'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { api, SimulateEditResponse } from '@/lib/api';
import { AlertOctagon, AlertTriangle, CheckCircle2, Code2, Play, RotateCcw, Zap } from 'lucide-react';

interface LiveEditSimulatorProps {
  filePath: string | null;
  repoId: string;
  onSimulationChange: (result: SimulateEditResponse | null) => void;
}

export default function LiveEditSimulator({
  filePath,
  repoId,
  onSimulationChange,
}: LiveEditSimulatorProps) {
  const [buffer, setBuffer] = useState('');
  const [response, setResponse] = useState<SimulateEditResponse | null>(null);
  const [simulationError, setSimulationError] = useState<string | null>(null);
  const [sourceLoadError, setSourceLoadError] = useState<string | null>(null);
  const [isLoadingSource, setIsLoadingSource] = useState(false);
  const [isSimulating, setIsSimulating] = useState(false);
  const [reloadSequence, setReloadSequence] = useState(0);
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const requestSequence = useRef(0);
  const callbackRef = useRef(onSimulationChange);

  useEffect(() => {
    callbackRef.current = onSimulationChange;
  }, [onSimulationChange]);

  const runSimulation = useCallback(async (targetPath: string, targetRepoId: string, content: string) => {
    const sequence = ++requestSequence.current;
    setIsSimulating(true);
    setSimulationError(null);
    try {
      const result = await api.simulateEdit(targetPath, content, targetRepoId);
      if (sequence === requestSequence.current) {
        setResponse(result);
        callbackRef.current(result);
      }
    } catch (error) {
      if (sequence === requestSequence.current) {
        const message = error instanceof Error ? error.message : 'Simulation request failed';
        setResponse(null);
        setSimulationError(message);
        callbackRef.current(null);
      }
    } finally {
      if (sequence === requestSequence.current) setIsSimulating(false);
    }
  }, []);

  useEffect(() => {
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    requestSequence.current += 1;
    setResponse(null);
    setSimulationError(null);
    setSourceLoadError(null);
    setBuffer('');
    callbackRef.current(null);

    if (!filePath || !repoId) {
      setIsLoadingSource(false);
      return;
    }

    let cancelled = false;
    setIsLoadingSource(true);
    void api.readSourceFile(filePath, repoId).then((source) => {
      if (cancelled) return;
      setBuffer(source.content);
      setIsLoadingSource(false);
      void runSimulation(filePath, repoId, source.content);
    }).catch((error: unknown) => {
      if (cancelled) return;
      const message = error instanceof Error ? error.message : 'Unable to load this source file';
      setSimulationError(message);
      setSourceLoadError(message);
      setIsLoadingSource(false);
    });

    return () => {
      cancelled = true;
      requestSequence.current += 1;
    };
  }, [filePath, repoId, reloadSequence, runSimulation]);

  useEffect(() => () => {
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    requestSequence.current += 1;
  }, []);

  const handleBufferChange = (content: string) => {
    setBuffer(content);
    if (!filePath || !repoId) return;
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      void runSimulation(filePath, repoId, content);
    }, 150);
  };

  const coverage = response?.coverage_state?.toLowerCase() || 'unknown';
  const verdict = !response
    ? 'UNKNOWN'
    : coverage === 'unknown'
    ? 'UNKNOWN'
    : coverage === 'partial' && response.verdict.toUpperCase() === 'PASS'
    ? 'PARTIAL'
    : response.verdict.toUpperCase();
  const verdictClass = verdict === 'FAIL'
    ? 'bg-rose-500/15 border-rose-500/30 text-rose-600 dark:text-rose-400'
    : verdict === 'WARN' || verdict === 'PARTIAL'
    ? 'bg-amber-500/15 border-amber-500/30 text-amber-600 dark:text-amber-400'
    : verdict === 'PASS'
    ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-600 dark:text-emerald-400'
    : 'bg-slate-500/10 border-slate-500/30 text-slate-600 dark:text-slate-300';

  return (
    <div className="space-y-6" data-testid="live-edit-simulator">
      <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <div className="flex items-center space-x-2">
            <span className="text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold">
              In-Memory Buffer Engine (FR-P2-005)
            </span>
            <span className="text-[10px] font-mono px-2 py-0.5 rounded-xl bg-sky-50 dark:bg-cyan-500/15 text-sky-700 dark:text-cyan-400 border border-sky-200 dark:border-transparent font-bold">
              SOURCE FILE IS NEVER WRITTEN
            </span>
          </div>
          <h2 className="text-base font-bold text-[var(--text-primary)] mt-1 flex items-center space-x-2">
            <Code2 className="w-4 h-4" />
            <span>Live Edit &amp; Blast Radius Simulator</span>
          </h2>
        </div>

        <div className="flex flex-wrap items-center gap-2 font-mono text-xs">
          <div className="px-3 py-1.5 rounded-xl border bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-secondary)] flex items-center gap-1.5">
            <Zap className="w-3.5 h-3.5" />
            <span>Engine: {response ? `${response.computation_latency_ms}ms` : 'not measured'}</span>
          </div>
          <div className="px-3 py-1.5 rounded-xl border bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-secondary)]">
            Coverage: <strong className="uppercase">{coverage}</strong>
          </div>
          <div className={`px-3 py-1.5 rounded-xl border font-bold flex items-center gap-1.5 ${verdictClass}`}>
            {verdict === 'FAIL' ? <AlertOctagon className="w-3.5 h-3.5" /> :
              verdict === 'WARN' || verdict === 'PARTIAL' ? <AlertTriangle className="w-3.5 h-3.5" /> :
                verdict === 'PASS' ? <CheckCircle2 className="w-3.5 h-3.5" /> : null}
            <span>VERDICT: {verdict}</span>
          </div>
        </div>
      </div>

      {response?.coverage_note && (
        <div className="px-4 py-3 rounded-xl border border-amber-500/30 bg-amber-500/10 text-xs text-amber-700 dark:text-amber-300" role="status">
          {response.coverage_note}
        </div>
      )}

      {!filePath || !repoId ? (
        <div className="p-8 text-center rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] text-sm text-[var(--text-secondary)]">
          Select a source file from the repository tree or canvas to load its real contents into the unsaved editor.
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <section className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm flex flex-col gap-3">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0 text-xs font-mono text-[var(--text-muted)]">
                Target: <strong className="text-[var(--text-primary)] break-all">{filePath}</strong>
              </div>
              <button
                type="button"
                onClick={() => setReloadSequence((sequence) => sequence + 1)}
                disabled={isLoadingSource || isSimulating}
                className="shrink-0 p-2 rounded-lg border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] disabled:opacity-50"
                aria-label="Reload source from repository and discard draft"
                title="Reload source and discard draft"
              >
                <RotateCcw className="w-4 h-4" />
              </button>
            </div>
            <textarea
              aria-label="Unsaved source buffer"
              data-testid="unsaved-source-buffer"
              value={buffer}
              onChange={(event) => handleBufferChange(event.target.value)}
              disabled={isLoadingSource || Boolean(sourceLoadError)}
              spellCheck={false}
              className="w-full h-[min(55vh,520px)] min-h-[320px] p-4 text-xs font-mono leading-relaxed rounded-xl bg-[var(--background)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500 resize-y disabled:opacity-60"
            />
            <div className="flex items-center justify-between gap-3 text-[11px] font-mono text-[var(--text-muted)]">
              <span>{isLoadingSource ? 'Loading source from the active repository…' : 'Draft stays in memory; no disk writes.'}</span>
              <button
                type="button"
                onClick={() => void runSimulation(filePath, repoId, buffer)}
                disabled={isLoadingSource || isSimulating || Boolean(sourceLoadError)}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cyan-600 text-white font-semibold disabled:opacity-50"
              >
                <Play className="w-3.5 h-3.5" />
                {isSimulating ? 'Analyzing…' : 'Analyze draft'}
              </button>
            </div>
          </section>

          <section className="space-y-4" aria-live="polite">
            {simulationError && (
              <div className="p-4 rounded-xl border border-amber-500/30 bg-amber-500/10 text-sm text-amber-700 dark:text-amber-300">
                <strong>Analysis unavailable — coverage is Unknown.</strong>
                <div className="mt-1 text-xs font-mono break-all">{simulationError}</div>
                <button
                  type="button"
                  onClick={() => sourceLoadError
                    ? setReloadSequence((sequence) => sequence + 1)
                    : void runSimulation(filePath, repoId, buffer)}
                  className="mt-3 px-3 py-1.5 rounded-lg border border-amber-500/40 text-xs font-semibold"
                >
                  {sourceLoadError ? 'Retry source load' : 'Retry analysis'}
                </button>
              </div>
            )}

            <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
              <h3 className="text-xs uppercase font-mono tracking-wider text-rose-600 dark:text-rose-400 font-bold mb-3">
                Damaged Callers ({response?.damaged_callers.length ?? 0})
              </h3>
              {response?.damaged_callers.length ? response.damaged_callers.map((caller) => (
                <div key={`${caller.stable_id}:${caller.calling_line}`} className="p-3 mb-2 rounded-xl bg-rose-500/10 border border-rose-500/25 font-mono text-xs">
                  <strong>{caller.stable_id}</strong>
                  <div>{caller.file_path}:{caller.calling_line}:{caller.calling_col}</div>
                  <div className="mt-1">{caller.diagnostic}</div>
                </div>
              )) : (
                <p className="text-xs text-[var(--text-muted)]">No verified damaged callers reported by the analyzer.</p>
              )}
            </div>

            <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
              <h3 className="text-xs uppercase font-mono tracking-wider text-amber-600 dark:text-amber-400 font-bold mb-3">
                Syntax &amp; Import Diagnostics
              </h3>
              {[...(response?.syntax_errors ?? []), ...(response?.broken_imports ?? [])].length ? (
                [...(response?.syntax_errors ?? []), ...(response?.broken_imports ?? [])].map((diagnostic, index) => (
                  <div key={`${index}:${diagnostic}`} className="p-2 mb-1 rounded-lg bg-amber-500/10 text-xs font-mono text-[var(--text-secondary)]">{diagnostic}</div>
                ))
              ) : (
                <p className="text-xs text-[var(--text-muted)]">No syntax or import diagnostics reported.</p>
              )}
            </div>

            <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
              <h3 className="text-xs uppercase font-mono tracking-wider text-slate-600 dark:text-slate-300 font-bold mb-3">
                Newly Dead Symbols ({response?.newly_dead_symbols.length ?? 0})
              </h3>
              {response?.newly_dead_symbols.length ? response.newly_dead_symbols.map((symbol) => (
                <div key={`${symbol.stable_id}:${symbol.line}`} className="p-3 mb-2 rounded-xl bg-slate-500/10 border border-slate-500/25 font-mono text-xs">
                  <strong>{symbol.stable_id}</strong>
                  <div>{symbol.file_path}:{symbol.line}</div>
                  <div className="mt-1">{symbol.diagnostic}</div>
                </div>
              )) : (
                <p className="text-xs text-[var(--text-muted)]">No newly dead symbols reported.</p>
              )}
              {response?.impact_truncated && (
                <p className="mt-3 text-xs font-semibold text-amber-600 dark:text-amber-400">Impact analysis was truncated; treat coverage as Partial.</p>
              )}
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
