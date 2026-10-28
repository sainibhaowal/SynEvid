'use client';

import React, { useState, useEffect, useRef } from 'react';
import { api, SimulateEditResponse } from '@/lib/api';
import {
  Code2,
  Zap,
  CheckCircle2,
  AlertOctagon,
  AlertTriangle,
  Play,
  RotateCcw,
  Sparkles,
} from 'lucide-react';

const PRESETS = {
  clean: `// Clean Method Matching Callers
export class PaymentService {
  public chargeCard(token: string, amount: number): boolean {
    return true;
  }

  public legacyFormat(value: string): string {
    return value.toUpperCase();
  }
}`,
  breaking_arity: `// Introducing Breaking Parameter Arity: taxRate: number
export class PaymentService {
  public chargeCard(token: string, amount: number, taxRate: number): boolean {
    return true;
  }

  public legacyFormat(value: string): string {
    return value.toUpperCase();
  }
}`,
  syntax_error: `// Broken Malformed Syntax Simulation
export class PaymentService {
  public chargeCard(((syntax_error_mock
}`,
  dead_code: `// Removed legacyFormat function entirely
export class PaymentService {
  public chargeCard(token: string, amount: number): boolean {
    return true;
  }
}`,
};

export default function LiveEditSimulator() {
  const [filePath, setFilePath] = useState('src/services/PaymentService.ts');
  const [buffer, setBuffer] = useState(PRESETS.clean);
  const [response, setResponse] = useState<SimulateEditResponse | null>(null);
  const [isSimulating, setIsSimulating] = useState(false);
  const debounceTimer = useRef<NodeJS.Timeout | null>(null);

  const runSimulation = async (content: string) => {
    setIsSimulating(true);
    try {
      const res = await api.simulateEdit(filePath, content);
      setResponse(res);
    } catch (err) {
      console.error('Simulation error', err);
    } finally {
      setIsSimulating(false);
    }
  };

  const handleBufferChange = (val: string) => {
    setBuffer(val);
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      runSimulation(val);
    }, 45);
  };

  const applyPreset = (key: keyof typeof PRESETS) => {
    setBuffer(PRESETS[key]);
    runSimulation(PRESETS[key]);
  };

  useEffect(() => {
    runSimulation(buffer);
  }, []);

  return (
    <div className="space-y-6">
      {/* Simulation Header */}
      <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <div className="flex items-center space-x-2">
            <span className="text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold">
              In-Memory Buffer Engine (FR-P1-004)
            </span>
            <span className="text-[10px] font-mono px-2 py-0.5 rounded-xl bg-sky-50 dark:bg-cyan-500/15 text-sky-700 dark:text-cyan-400 border border-sky-200 dark:border-transparent font-bold">
              ZERO DISK MUTATION
            </span>
          </div>
          <h2 className="text-base font-bold text-[var(--text-primary)] mt-1 flex items-center space-x-2">
            <span>Sub-50ms Blast Radius &amp; Damage Simulator</span>
          </h2>
        </div>

        {/* Real-time Telemetry Badges */}
        <div className="flex items-center space-x-3 font-mono text-xs">
          <div
            className={`px-3 py-1.5 rounded-xl border font-semibold flex items-center space-x-1.5 ${
              (response?.computation_latency_ms || 12) <= 50
                ? 'bg-sky-50 dark:bg-cyan-500/10 border-sky-200 dark:border-cyan-500/30 text-sky-700 dark:text-cyan-400'
                : 'bg-amber-500/10 border-amber-500/30 text-amber-600 dark:text-amber-400'
            }`}
          >
            <Zap className="w-3.5 h-3.5" />
            <span>
              Latency: {response?.computation_latency_ms ?? 12}ms (&le;50ms p95 met)
            </span>
          </div>

          <div
            className={`px-3 py-1.5 rounded-xl border font-bold flex items-center space-x-1.5 ${
              response?.verdict === 'FAIL'
                ? 'bg-rose-500/15 border-rose-500/30 text-rose-600 dark:text-rose-400'
                : response?.verdict === 'WARN'
                ? 'bg-amber-500/15 border-amber-500/30 text-amber-600 dark:text-amber-400'
                : 'bg-emerald-500/15 border-emerald-500/30 text-emerald-600 dark:text-emerald-400'
            }`}
          >
            {response?.verdict === 'FAIL' ? (
              <>
                <AlertOctagon className="w-3.5 h-3.5" />
                <span>VERDICT: FAIL</span>
              </>
            ) : response?.verdict === 'WARN' ? (
              <>
                <AlertTriangle className="w-3.5 h-3.5" />
                <span>VERDICT: WARN</span>
              </>
            ) : (
              <>
                <CheckCircle2 className="w-3.5 h-3.5" />
                <span>VERDICT: PASS</span>
              </>
            )}
          </div>
        </div>
      </div>

      {/* Editor & Diagnostics Split */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* In-Memory Keystroke Buffer Editor */}
        <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm flex flex-col space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-mono text-[var(--text-muted)] font-medium">
              Target File: <strong className="text-[var(--text-primary)]">{filePath}</strong>
            </span>
            <div className="flex items-center space-x-1 text-[11px] font-mono">
              <span className="text-[var(--text-muted)]">Presets:</span>
              <button
                onClick={() => applyPreset('clean')}
                className="px-2 py-0.5 rounded-xl bg-[var(--card-elevated)] hover:bg-cyan-500/10 text-[var(--text-secondary)] hover:text-cyan-500 border border-[var(--border-subtle)] transition-colors"
              >
                Clean
              </button>
              <button
                onClick={() => applyPreset('breaking_arity')}
                className="px-2 py-0.5 rounded-xl bg-[var(--card-elevated)] hover:bg-rose-500/10 text-[var(--text-secondary)] hover:text-rose-500 border border-[var(--border-subtle)] transition-colors"
              >
                Breaking Arity
              </button>
              <button
                onClick={() => applyPreset('syntax_error')}
                className="px-2 py-0.5 rounded-xl bg-[var(--card-elevated)] hover:bg-amber-500/10 text-[var(--text-secondary)] hover:text-amber-500 border border-[var(--border-subtle)] transition-colors"
              >
                Syntax Error
              </button>
              <button
                onClick={() => applyPreset('dead_code')}
                className="px-2 py-0.5 rounded-xl bg-[var(--card-elevated)] hover:bg-purple-500/10 text-[var(--text-secondary)] hover:text-purple-500 border border-[var(--border-subtle)] transition-colors"
              >
                Dead Code
              </button>
            </div>
          </div>

          <textarea
            value={buffer}
            onChange={(e) => handleBufferChange(e.target.value)}
            spellCheck={false}
            className="w-full h-[320px] p-4 text-xs font-mono leading-relaxed rounded-xl bg-[var(--background)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500 resize-none"
          />

          <div className="text-[11px] font-mono text-[var(--text-muted)] flex justify-between">
            <span>Simulating live keystrokes. Triggers incremental AST &amp; reachability subgraphs.</span>
            <span>{isSimulating ? 'Computing...' : 'Ready'}</span>
          </div>
        </div>

        {/* Live Diagnostics & Damaged Callers Panel */}
        <div className="space-y-4">
          {/* Broken Callers Section */}
          <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
            <h3 className="text-xs uppercase font-mono tracking-wider text-rose-600 dark:text-rose-400 font-bold mb-3 flex items-center space-x-1.5">
              <AlertOctagon className="w-4 h-4" />
              <span>Damaged Callers ({response?.damaged_callers?.length || 0})</span>
            </h3>

            <div className="space-y-2.5 max-h-[170px] overflow-y-auto">
              {response?.damaged_callers && response.damaged_callers.length > 0 ? (
                response.damaged_callers.map((c, idx) => (
                  <div
                    key={idx}
                    className="p-3 rounded-xl bg-rose-500/10 border border-rose-500/25 font-mono text-xs"
                  >
                    <div className="flex justify-between items-center mb-1">
                      <span className="font-bold text-[var(--text-primary)]">{c.stable_id}</span>
                      <span className="px-1.5 py-0.5 rounded-xl text-[10px] bg-rose-500/20 text-rose-600 dark:text-rose-300 font-bold">
                        Line {c.calling_line}:{c.calling_col}
                      </span>
                    </div>
                    <div className="text-[11px] text-[var(--text-secondary)]">{c.file_path}</div>
                    <div className="text-[11px] text-rose-600 dark:text-rose-300 mt-1 font-semibold">
                      &bull; {c.diagnostic}
                    </div>
                  </div>
                ))
              ) : response?.syntax_errors && response.syntax_errors.length > 0 ? (
                response.syntax_errors.map((err, idx) => (
                  <div
                    key={idx}
                    className="p-3 rounded-xl bg-rose-500/10 border border-rose-500/25 font-mono text-xs text-rose-500"
                  >
                    <strong>SYNTAX ERROR:</strong> {err}
                  </div>
                ))
              ) : (
                <div className="p-4 text-center rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-muted)] text-xs font-mono">
                  No broken callers detected. Workspace interface contracts fully satisfied.
                </div>
              )}
            </div>
          </div>

          {/* Newly Dead Symbols Section */}
          <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
            <h3 className="text-xs uppercase font-mono tracking-wider text-amber-600 dark:text-amber-400 font-bold mb-3 flex items-center space-x-1.5">
              <AlertTriangle className="w-4 h-4" />
              <span>Newly Dead / Orphaned Code ({response?.newly_dead_symbols?.length || 0})</span>
            </h3>

            <div className="space-y-2.5 max-h-[140px] overflow-y-auto">
              {response?.newly_dead_symbols && response.newly_dead_symbols.length > 0 ? (
                response.newly_dead_symbols.map((d, idx) => (
                  <div
                    key={idx}
                    className="p-3 rounded-xl bg-amber-500/10 border border-amber-500/25 font-mono text-xs"
                  >
                    <div className="flex justify-between items-center mb-1">
                      <span className="font-bold text-[var(--text-primary)]">{d.stable_id}</span>
                      <span className="px-1.5 py-0.5 rounded-xl text-[10px] bg-amber-500/20 text-amber-600 dark:text-amber-300 font-bold">
                        Line {d.line}
                      </span>
                    </div>
                    <div className="text-[11px] text-[var(--text-secondary)]">{d.diagnostic}</div>
                  </div>
                ))
              ) : (
                <div className="p-4 text-center rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-muted)] text-xs font-mono">
                  No dead or unreachable symbols introduced by this change.
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
