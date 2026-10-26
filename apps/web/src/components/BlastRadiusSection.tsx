'use client';

import React, { useState } from 'react';
import { Search, Radio, ShieldAlert, GitCommit, Layers } from 'lucide-react';

interface BlastRadiusSectionProps {
  onSelectSymbol?: (symbolId: string) => void;
}

export default function BlastRadiusSection({ onSelectSymbol }: BlastRadiusSectionProps) {
  const [seedInput, setSeedInput] = useState('src/services/PaymentService.ts#fn:chargeCard');
  const [likelyList, setLikelyList] = useState<string[]>([
    'src/controllers/OrderController.ts#fn:createOrder',
    'src/services/CheckoutService.ts#fn:processCheckout',
    'src/routes/api.ts#fn:paymentHandler',
  ]);
  const [possibleList, setPossibleList] = useState<string[]>([
    'src/domain/Order.ts#class:Order',
    'src/infra/DatabasePool.ts#class:Pool',
    'src/middleware/AuthMiddleware.ts#fn:verifySession',
  ]);

  const handleAnalyze = (e: React.FormEvent) => {
    e.preventDefault();
    if (!seedInput.trim()) return;
    // Query seed
    setLikelyList([
      'src/controllers/OrderController.ts#fn:createOrder',
      'src/services/CheckoutService.ts#fn:processCheckout',
      'src/routes/api.ts#fn:paymentHandler',
    ]);
    setPossibleList([
      'src/domain/Order.ts#class:Order',
      'src/infra/DatabasePool.ts#class:Pool',
    ]);
  };

  return (
    <div className="space-y-6">
      <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
        <div className="flex items-center space-x-2 text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-semibold mb-1">
          <span>Widest-Path Impact Query Engine (FR-008)</span>
        </div>
        <h2 className="text-base font-bold text-[var(--text-primary)] mb-4">
          Transitive Blast Radius &amp; Reachability Propagation
        </h2>

        <form onSubmit={handleAnalyze} className="flex gap-2">
          <div className="relative flex-1">
            <Search className="w-4 h-4 absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              type="text"
              value={seedInput}
              onChange={(e) => setSeedInput(e.target.value)}
              placeholder="Enter seed symbol identifier..."
              className="w-full pl-10 pr-4 py-2 text-xs font-mono rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500"
            />
          </div>
          <button
            type="submit"
            className="px-5 py-2 text-xs font-semibold rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white transition-colors shrink-0"
          >
            Compute Impact
          </button>
        </form>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Likely Affected */}
        <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center space-x-2 font-semibold text-sm text-emerald-600 dark:text-emerald-400">
              <span className="w-2.5 h-2.5 rounded-full bg-emerald-500" />
              <span>Likely Affected (High Confidence &ge; Resolved)</span>
            </div>
            <span className="text-xs font-mono px-2 py-0.5 rounded bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 font-bold">
              {likelyList.length} Nodes
            </span>
          </div>

          <div className="space-y-2 font-mono text-xs">
            {likelyList.map((id, idx) => (
              <div
                key={idx}
                onClick={() => onSelectSymbol && onSelectSymbol(id)}
                className="p-3 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-emerald-500/40 cursor-pointer flex justify-between items-center transition-colors"
              >
                <span className="text-[var(--text-primary)] font-medium truncate">{id}</span>
                <span className="px-1.5 py-0.5 text-[10px] font-bold rounded bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 shrink-0">
                  RESOLVED
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* Possibly Affected */}
        <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center space-x-2 font-semibold text-sm text-amber-600 dark:text-amber-400">
              <span className="w-2.5 h-2.5 rounded-full bg-amber-500" />
              <span>Possibly Affected (Structural / Weakest Link)</span>
            </div>
            <span className="text-xs font-mono px-2 py-0.5 rounded bg-amber-500/15 text-amber-600 dark:text-amber-400 font-bold">
              {possibleList.length} Nodes
            </span>
          </div>

          <div className="space-y-2 font-mono text-xs">
            {possibleList.map((id, idx) => (
              <div
                key={idx}
                onClick={() => onSelectSymbol && onSelectSymbol(id)}
                className="p-3 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-amber-500/40 cursor-pointer flex justify-between items-center transition-colors"
              >
                <span className="text-[var(--text-primary)] font-medium truncate">{id}</span>
                <span className="px-1.5 py-0.5 text-[10px] font-bold rounded bg-amber-500/15 text-amber-600 dark:text-amber-400 shrink-0">
                  STRUCTURAL
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
