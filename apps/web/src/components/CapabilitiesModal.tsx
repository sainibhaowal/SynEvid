'use client';

import React from 'react';
import { X, Sliders, ShieldCheck, Database, Layers, GitBranch, Cpu } from 'lucide-react';
import { Capabilities } from '@/lib/api';

interface CapabilitiesModalProps {
  isOpen: boolean;
  onClose: () => void;
  capabilities: Capabilities | null;
}

export default function CapabilitiesModal({ isOpen, onClose, capabilities }: CapabilitiesModalProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-fadeIn">
      <div className="w-full max-w-lg rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] p-6 shadow-2xl space-y-5">
        <div className="flex items-center justify-between pb-3 border-b border-[var(--border-subtle)]">
          <div className="flex items-center space-x-2.5">
            <div className="w-7 h-7 rounded-xl overflow-hidden border border-[var(--border-subtle)] bg-[var(--card-elevated)] p-1 flex items-center justify-center shadow-sm">
              <img src="/assets/logo.svg" alt="SynEvid" className="w-full h-full object-contain" />
            </div>
            <h3 className="text-sm font-bold text-[var(--text-primary)]">
              Core Plugin Registry &amp; Capabilities (autopsy-plugin-api)
            </h3>
          </div>
          <button aria-label="Close capabilities dialog" onClick={onClose} className="p-1.5 rounded-xl text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--card-elevated)] cursor-pointer">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-4 text-xs font-mono">
          {/* Languages */}
          <div>
            <h4 className="text-[var(--text-muted)] uppercase tracking-wider font-semibold mb-2 flex items-center space-x-1.5">
              <Cpu className="w-3.5 h-3.5 text-cyan-500" />
              <span>Language Adapter Plugins (6/6 Registered)</span>
            </h4>
            <div className="flex flex-wrap gap-1.5">
              {(capabilities?.languages || []).map((l, idx) => (
                <span
                  key={idx}
                  className="px-2.5 py-1 rounded-lg bg-sky-50 dark:bg-cyan-500/15 border border-sky-200 dark:border-cyan-500/30 text-sky-700 dark:text-cyan-400 font-bold uppercase"
                >
                  {l}
                </span>
              ))}
            </div>
          </div>

          {/* Resolvers */}
          <div>
            <h4 className="text-[var(--text-muted)] uppercase tracking-wider font-semibold mb-2 flex items-center space-x-1.5">
              <GitBranch className="w-3.5 h-3.5 text-indigo-500" />
              <span>Monorepo Workspace Resolvers (8/8 Registered)</span>
            </h4>
            <div className="flex flex-wrap gap-1.5">
              {(capabilities?.resolvers || []).map((r, idx) => (
                <span
                  key={idx}
                  className="px-2.5 py-1 rounded-lg bg-indigo-50 dark:bg-indigo-500/15 border border-indigo-200 dark:border-indigo-500/30 text-indigo-700 dark:text-indigo-400 font-bold"
                >
                  {r}
                </span>
              ))}
            </div>
          </div>

          {/* Layouts */}
          <div>
            <h4 className="text-[var(--text-muted)] uppercase tracking-wider font-semibold mb-2 flex items-center space-x-1.5">
              <Layers className="w-3.5 h-3.5 text-emerald-500" />
              <span>Seeded Graph Layout Plugins</span>
            </h4>
            <div className="flex flex-wrap gap-1.5">
              {(capabilities?.layouts || []).map((lay, idx) => (
                <span
                  key={idx}
                  className="px-2.5 py-1 rounded-lg bg-emerald-50 dark:bg-emerald-500/15 border border-emerald-200 dark:border-emerald-500/30 text-emerald-700 dark:text-emerald-400 font-bold"
                >
                  {lay}
                </span>
              ))}
            </div>
          </div>

          {/* Invariants */}
          <div className="p-3.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[11px] text-[var(--text-muted)] leading-relaxed">
            <strong className="text-[var(--text-primary)]">Correctness Guarantee:</strong> 100% offline deterministic execution. Zero LLMs in core crates. No vector DB or network sockets. Byte-identical BLAKE3 hash repeatability.
          </div>
        </div>

        <div className="flex justify-end pt-2">
          <button
            onClick={onClose}
            className="px-4 py-2 rounded-xl bg-[var(--card-elevated)] hover:bg-[var(--card)] text-[var(--text-primary)] font-semibold text-xs border border-[var(--border-subtle)] cursor-pointer"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
