'use client';

import React, { useRef, useState } from 'react';
import { Search, Radio, Layers } from 'lucide-react';
import { api, EgoNetwork } from '@/lib/api';

interface BlastRadiusSectionProps {
  onSelectSymbol?: (symbolId: string) => void;
}

export default function BlastRadiusSection({ onSelectSymbol }: BlastRadiusSectionProps) {
  const [seedInput, setSeedInput] = useState('');
  const [network, setNetwork] = useState<EgoNetwork | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [queryError, setQueryError] = useState<string | null>(null);
  const requestSequence = useRef(0);

  const handleAnalyze = async (e: React.FormEvent) => {
    e.preventDefault();
    const stableId = seedInput.trim();
    if (!stableId) return;
    const sequence = ++requestSequence.current;
    setIsLoading(true);
    setQueryError(null);
    try {
      const result = await api.getEgoNetwork(stableId);
      if (sequence === requestSequence.current) setNetwork(result);
    } catch (error) {
      if (sequence === requestSequence.current) {
        setNetwork(null);
        setQueryError(error instanceof Error ? error.message : 'Unable to query the active repository graph');
      }
    } finally {
      if (sequence === requestSequence.current) setIsLoading(false);
    }
  };

  const callers = network?.incoming_callers ?? [];
  const callees = network?.outgoing_callees ?? [];
  const coverage = network?.coverage_state?.toUpperCase() ?? 'UNKNOWN';

  return (
    <div className="space-y-6">
      <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
        <div className="flex items-center space-x-2 text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold mb-1">
          <span>Persisted Dependency Explorer (FR-008)</span>
        </div>
        <h2 className="text-base font-bold text-[var(--text-primary)] mb-4">
          Direct Callers &amp; Callees
        </h2>

        <form onSubmit={handleAnalyze} className="flex gap-2">
          <div className="relative flex-1">
            <Search className="w-4 h-4 absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              type="text"
              value={seedInput}
              onChange={(e) => setSeedInput(e.target.value)}
              placeholder="Enter a stable symbol ID from the active repository..."
              className="w-full pl-10 pr-4 py-2 text-xs font-mono rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500"
            />
          </div>
          <button
            type="submit"
            className="px-5 py-2 text-xs font-semibold rounded-xl bg-cyan-600 hover:bg-cyan-500 text-white transition-colors shrink-0"
              disabled={isLoading || !seedInput.trim()}
            >
            {isLoading ? 'Querying…' : 'Inspect Dependencies'}
          </button>
        </form>
        <p className="mt-3 text-[11px] font-mono text-[var(--text-muted)]">
          Results come from the active repository’s persisted graph. Coverage: <strong>{coverage}</strong>.
        </p>
        {queryError && <p role="alert" className="mt-2 text-xs text-rose-600 dark:text-rose-400">{queryError}</p>}
        {network && !network.target_node && (
          <p role="status" className="mt-3 text-xs text-[var(--text-muted)]">No persisted symbol matches that ID.</p>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center space-x-2 font-semibold text-sm text-emerald-600 dark:text-emerald-400">
              <Radio className="w-4 h-4" />
              <span>Direct Callers</span>
            </div>
            <span className="text-xs font-mono px-2 py-0.5 rounded-xl bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 font-bold">
              {callers.length} Edges
            </span>
          </div>

          <div className="space-y-2 font-mono text-xs">
            {callers.map((edge) => (
              <button
                type="button"
                key={edge.edge_id}
                onClick={() => onSelectSymbol?.(edge.source_stable_id)}
                className="w-full text-left p-3 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-emerald-500/40 transition-colors"
                aria-label={`Focus caller ${edge.source_stable_id}`}
              >
                <span className="block text-[var(--text-primary)] font-medium break-all">{edge.source_stable_id}</span>
                <span className="block mt-1 text-[var(--text-muted)]">{edge.source_file}:{edge.source_line}</span>
              </button>
            ))}
            {network?.target_node && callers.length === 0 && <p className="text-[var(--text-muted)]">No persisted callers found.</p>}
            {!network && !queryError && <p className="text-[var(--text-muted)]">Search a symbol to inspect its actual callers.</p>}
          </div>
        </div>

        <div className="p-5 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center space-x-2 font-semibold text-sm text-amber-600 dark:text-amber-400">
              <Layers className="w-4 h-4" />
              <span>Direct Callees</span>
            </div>
            <span className="text-xs font-mono px-2 py-0.5 rounded-xl bg-amber-500/15 text-amber-600 dark:text-amber-400 font-bold">
              {callees.length} Edges
            </span>
          </div>

          <div className="space-y-2 font-mono text-xs">
            {callees.map((edge) => (
              <button
                type="button"
                key={edge.edge_id}
                onClick={() => onSelectSymbol?.(edge.target_stable_id)}
                className="w-full text-left p-3 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-amber-500/40 transition-colors"
                aria-label={`Focus callee ${edge.target_stable_id}`}
              >
                <span className="block text-[var(--text-primary)] font-medium break-all">{edge.target_stable_id}</span>
                <span className="block mt-1 text-[var(--text-muted)]">{edge.target_file}:{edge.target_line}</span>
              </button>
            ))}
            {network?.target_node && callees.length === 0 && <p className="text-[var(--text-muted)]">No persisted callees found.</p>}
            {!network && !queryError && <p className="text-[var(--text-muted)]">Search a symbol to inspect its actual callees.</p>}
          </div>
        </div>
      </div>
    </div>
  );
}
