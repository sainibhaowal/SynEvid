'use client';

import React from 'react';
import { TrajectoryRun } from '@/lib/api';
import { Clock, ShieldCheck, CheckCircle2, AlertOctagon, Terminal, ArrowRight } from 'lucide-react';

interface ForensicTrajectorySectionProps {
  runs: TrajectoryRun[];
}

export default function ForensicTrajectorySection({ runs }: ForensicTrajectorySectionProps) {
  return (
    <div className="space-y-6">
      <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
        <div className="flex items-center space-x-2 text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold">
          <span>AI Coding Agent Trajectory Ledger (RFC-TRAJECTORY-001)</span>
        </div>
        <h2 className="text-base font-bold text-[var(--text-primary)] mt-1">
          Forensic Task-Completion Runs (.autopsy/runs/ &amp; SQLite WAL)
        </h2>
      </div>

      <div className="space-y-4">
        {runs.length > 0 ? (
          runs.map((r, idx) => (
            <div
              key={idx}
              className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm font-mono text-xs hover:border-cyan-500/40 transition-colors"
            >
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-[var(--border-subtle)]">
                <div className="flex items-center space-x-3">
                  <span className="font-bold text-sm text-[var(--text-primary)]">{r.session_id}</span>
                  <span
                    className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                      r.exit_code === 0
                        ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30'
                        : 'bg-rose-500/15 text-rose-600 dark:text-rose-400 border border-rose-500/30'
                    }`}
                  >
                    EXIT CODE {r.exit_code}
                  </span>
                </div>
                <div className="text-[var(--text-muted)] text-[11px]">
                  Digest: <code className="text-[var(--text-secondary)]">{r.report_digest || 'blake3:...'}</code>
                </div>
              </div>

              <div className="py-3 flex flex-wrap items-center gap-4 text-[var(--text-secondary)] text-[11px]">
                <div className="flex items-center space-x-2">
                  <span>Snapshot Transition:</span>
                  <span className="text-sky-700 dark:text-cyan-400 font-bold">{r.started_snapshot || 'snap_start'}</span>
                  <ArrowRight className="w-3.5 h-3.5 text-[var(--text-muted)]" />
                  <span className="text-emerald-600 dark:text-emerald-400 font-bold">
                    {r.completed_snapshot || 'snap_end'}
                  </span>
                </div>
                <div>
                  Steps Recorded: <strong>{r.steps ? r.steps.length : 2}</strong>
                </div>
              </div>

              {r.next_agent_guidance && (
                <div className="mt-2 p-3 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[11px] text-[var(--text-secondary)]">
                  <span className="text-[var(--text-muted)] uppercase tracking-wider font-bold block mb-0.5">
                    Forensic Guidance for Next Agent:
                  </span>
                  {r.next_agent_guidance}
                </div>
              )}
            </div>
          ))
        ) : (
          <div className="p-8 text-center rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] text-[var(--text-muted)] text-xs font-mono">
            No forensic trajectory runs logged yet in .autopsy/runs/ or SQLite database.
          </div>
        )}
      </div>
    </div>
  );
}
