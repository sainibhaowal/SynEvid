'use client';

import React, { useState } from 'react';
import { InvariantFinding } from '@/lib/api';
import { ShieldCheck, CheckCircle2, AlertOctagon, AlertTriangle, Filter } from 'lucide-react';

interface InvariantVerificationSectionProps {
  findings: InvariantFinding[];
}

export default function InvariantVerificationSection({ findings }: InvariantVerificationSectionProps) {
  const [filter, setFilter] = useState<'all' | 'fail' | 'pass'>('all');

  const filtered = findings.filter((f) => {
    if (filter === 'fail') return f.status === 'fail';
    if (filter === 'pass') return f.status === 'pass';
    return true;
  });

  const fails = findings.filter((f) => f.status === 'fail').length;

  return (
    <div className="space-y-6">
      <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4">
        <div>
          <div className="flex items-center space-x-2 text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold">
            <span>Deterministic Policy Evaluation (FR-011..014)</span>
          </div>
          <h2 className="text-base font-bold text-[var(--text-primary)] mt-1">
            Evaluated Architectural Invariants (.autopsy/invariants.yml)
          </h2>
        </div>

        <div className="flex items-center space-x-2 text-xs font-mono">
          <button
            onClick={() => setFilter('all')}
            className={`px-3 py-1.5 rounded-lg border font-semibold transition-colors ${
              filter === 'all'
                ? 'bg-sky-50 dark:bg-cyan-500/15 border-sky-300 dark:border-cyan-500/40 text-sky-800 dark:text-cyan-400'
                : 'bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-secondary)]'
            }`}
          >
            All ({findings.length})
          </button>
          <button
            onClick={() => setFilter('fail')}
            className={`px-3 py-1.5 rounded-lg border font-semibold transition-colors ${
              filter === 'fail'
                ? 'bg-rose-500/15 border-rose-500/40 text-rose-600 dark:text-rose-400'
                : 'bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-secondary)]'
            }`}
          >
            Failing ({fails})
          </button>
          <button
            onClick={() => setFilter('pass')}
            className={`px-3 py-1.5 rounded-lg border font-semibold transition-colors ${
              filter === 'pass'
                ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-600 dark:text-emerald-400'
                : 'bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-secondary)]'
            }`}
          >
            Passing ({findings.length - fails})
          </button>
        </div>
      </div>

      <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead>
              <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
                <th className="pb-3 font-semibold">Status</th>
                <th className="pb-3 font-semibold">Severity</th>
                <th className="pb-3 font-semibold">Rule Identifier</th>
                <th className="pb-3 font-semibold">Policy Message</th>
                <th className="pb-3 font-semibold text-right">Target Scope</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--border-subtle)]">
              {filtered.length > 0 ? (
                filtered.map((f, idx) => (
                  <tr key={idx} className="hover:bg-[var(--card-elevated)] transition-colors">
                    <td className="py-3 font-bold">
                      <span
                        className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                          f.status === 'fail'
                            ? 'bg-rose-500/15 text-rose-600 dark:text-rose-400 border border-rose-500/30'
                            : 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30'
                        }`}
                      >
                        {f.status.toUpperCase()}
                      </span>
                    </td>
                    <td className="py-3 uppercase text-[11px] font-semibold text-[var(--text-muted)]">
                      {f.severity}
                    </td>
                    <td className="py-3 font-bold text-[var(--text-primary)]">
                      {f.rule_id}
                    </td>
                    <td className="py-3 text-[var(--text-secondary)]">
                      {f.message}
                    </td>
                    <td className="py-3 text-right text-[var(--text-muted)]">
                      {f.locations && f.locations[0] ? f.locations[0].path : 'workspace'}
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={5} className="py-6 text-center text-[var(--text-muted)]">
                    No findings match current filter.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
