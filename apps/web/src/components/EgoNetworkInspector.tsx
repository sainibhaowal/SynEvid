'use client';

import React, { useState } from 'react';
import { EgoNetwork } from '@/lib/api';
import { Search, GitCommit, ArrowDownLeft, ArrowUpRight, CheckCircle2, ShieldCheck, Zap } from 'lucide-react';

interface EgoNetworkInspectorProps {
  ego: EgoNetwork | null;
  currentSymbol: string;
  onSearchSymbol: (symbolId: string) => void;
  isLoading: boolean;
}

export default function EgoNetworkInspector({
  ego,
  currentSymbol,
  onSearchSymbol,
  isLoading,
}: EgoNetworkInspectorProps) {
  const [searchInput, setSearchInput] = useState(currentSymbol);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (searchInput.trim()) {
      onSearchSymbol(searchInput.trim());
    }
  };

  const target = ego?.target_node;

  return (
    <div className="space-y-6">
      {/* Search Header */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4 p-4 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
        <form onSubmit={handleSearch} className="flex-1 flex items-center space-x-2">
          <div className="relative flex-1">
            <Search className="w-4 h-4 absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              type="text"
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              placeholder="Search symbol (e.g. PaymentService.chargeCard or src/index.ts)..."
              className="w-full pl-10 pr-4 py-2 text-xs font-mono rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500"
            />
          </div>
          <button
            type="submit"
            disabled={isLoading}
            className="px-4 py-2 text-xs font-semibold rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white transition-colors shrink-0 disabled:opacity-50"
          >
            {isLoading ? 'Querying...' : 'Inspect Ego'}
          </button>
        </form>

        <div className="flex items-center space-x-2 text-xs font-mono text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 px-3 py-1.5 rounded-lg border border-emerald-500/20 self-start sm:self-auto">
          <Zap className="w-3.5 h-3.5 text-emerald-500" />
          <span>&lt; 5ms SQLite WAL Ego Network</span>
        </div>
      </div>

      {/* Target Node Overview Card */}
      {target ? (
        <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-4 border-b border-[var(--border-subtle)]">
            <div>
              <div className="flex items-center space-x-2">
                <span className="text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold">
                  Ego Target Symbol
                </span>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 font-bold border border-cyan-500/20">
                  {target.node_kind.toUpperCase()}
                </span>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 font-bold border border-emerald-500/20">
                  {target.visibility.toUpperCase()}
                </span>
              </div>
              <h2 className="text-lg font-mono font-bold text-[var(--text-primary)] mt-1">
                {target.qualified_name || target.stable_id}
              </h2>
            </div>

            <div className="text-right font-mono text-xs text-[var(--text-muted)]">
              <div>File: <span className="text-[var(--text-secondary)] font-medium">{target.file_path}</span></div>
              <div className="mt-0.5">Lines: <span className="text-cyan-600 dark:text-cyan-400 font-bold">{target.start_line} &ndash; {target.end_line}</span></div>
            </div>
          </div>

          <div className="pt-3 flex flex-wrap gap-4 text-xs font-mono text-[var(--text-muted)]">
            <span>Signature: <code className="text-[var(--text-secondary)]">{target.signature_hash}</code></span>
            <span>Exported: <strong className={target.is_exported ? 'text-emerald-500' : 'text-slate-400'}>{target.is_exported ? 'TRUE' : 'FALSE'}</strong></span>
            <span>Coverage: <strong className="text-cyan-500">{target.coverage_state.toUpperCase()}</strong></span>
          </div>
        </div>
      ) : (
        <div className="p-8 text-center rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] text-[var(--text-muted)] text-xs font-mono">
          Enter a symbol identifier above to inspect its 1-hop inbound and outbound ego network.
        </div>
      )}

      {/* Two Column Inbound / Outbound Tables */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Inbound Callers Table */}
        <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center space-x-2 text-sm font-semibold text-emerald-600 dark:text-emerald-400">
              <ArrowDownLeft className="w-4 h-4" />
              <span>Inbound Callers (Who calls this?)</span>
            </div>
            <span className="text-xs font-mono px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 font-bold">
              {ego?.incoming_callers?.length || 0} Callers
            </span>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs font-mono">
              <thead>
                <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
                  <th className="pb-2.5 font-semibold">Caller Symbol</th>
                  <th className="pb-2.5 font-semibold">Source File &amp; Calling Line</th>
                  <th className="pb-2.5 font-semibold text-right">Confidence</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--border-subtle)]">
                {ego?.incoming_callers && ego.incoming_callers.length > 0 ? (
                  ego.incoming_callers.map((edge, idx) => (
                    <tr key={idx} className="hover:bg-[var(--card-elevated)] transition-colors">
                      <td className="py-2.5 font-medium text-[var(--text-primary)]">
                        {edge.source_stable_id}
                      </td>
                      <td className="py-2.5 text-cyan-600 dark:text-cyan-400">
                        {edge.source_file}:<strong className="text-[var(--text-primary)]">{edge.source_line}</strong>
                      </td>
                      <td className="py-2.5 text-right">
                        <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
                          {edge.confidence.toUpperCase()}
                        </span>
                      </td>
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td colSpan={3} className="py-4 text-center text-[var(--text-muted)]">
                      No inbound callers found in workspace index.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>

        {/* Outbound Callees Table */}
        <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center space-x-2 text-sm font-semibold text-cyan-600 dark:text-cyan-400">
              <ArrowUpRight className="w-4 h-4" />
              <span>Outbound Callees (What does this call?)</span>
            </div>
            <span className="text-xs font-mono px-2 py-0.5 rounded bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 font-bold">
              {ego?.outgoing_callees?.length || 0} Callees
            </span>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs font-mono">
              <thead>
                <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
                  <th className="pb-2.5 font-semibold">Target Symbol</th>
                  <th className="pb-2.5 font-semibold">Target File &amp; Line</th>
                  <th className="pb-2.5 font-semibold text-right">Confidence</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--border-subtle)]">
                {ego?.outgoing_callees && ego.outgoing_callees.length > 0 ? (
                  ego.outgoing_callees.map((edge, idx) => (
                    <tr key={idx} className="hover:bg-[var(--card-elevated)] transition-colors">
                      <td className="py-2.5 font-medium text-[var(--text-primary)]">
                        {edge.target_stable_id}
                      </td>
                      <td className="py-2.5 text-indigo-600 dark:text-indigo-400">
                        {edge.target_file}:<strong className="text-[var(--text-primary)]">{edge.target_line}</strong>
                      </td>
                      <td className="py-2.5 text-right">
                        <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-cyan-500/15 text-cyan-600 dark:text-cyan-400">
                          {edge.confidence.toUpperCase()}
                        </span>
                      </td>
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td colSpan={3} className="py-4 text-center text-[var(--text-muted)]">
                      No outbound callees from this symbol.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}
