'use client';

import React from 'react';
import { motion } from 'framer-motion';
import { Award, Check, X, ShieldAlert, Zap } from 'lucide-react';

export default function BenchmarkSection() {
  return (
    <section className="py-24 max-w-7xl mx-auto px-6">
      <div className="text-center max-w-3xl mx-auto mb-16">
        <div className="inline-flex items-center space-x-2 px-3 py-1 rounded-full border border-green-500/30 bg-green-950/20 text-green-400 text-xs font-mono uppercase tracking-wider mb-4">
          <Award className="w-3.5 h-3.5 mr-1" />
          <span>Phase 6 Pre-Registered Gate: PASSED</span>
        </div>
        <h2 className="text-3xl sm:text-5xl font-extrabold text-white tracking-tight mb-4">
          Proven Recall Advantage Over Grep & LSP
        </h2>
        <p className="text-gray-400 text-base sm:text-lg">
          Benchmarked across 50 realistic refactoring tasks and 5 TypeScript target repositories with committed reproduction ground truth.
        </p>
      </div>

      {/* Benchmark Graph Bars */}
      <div className="max-w-4xl mx-auto mb-20 p-8 rounded-2xl border border-white/10 bg-[#0a0f1b]/60 backdrop-blur-xl">
        <div className="space-y-6">
          {/* Baseline A */}
          <div>
            <div className="flex justify-between items-center text-sm font-mono mb-2">
              <span className="text-gray-400">Baseline A: String / Regex Search (ripgrep)</span>
              <span className="text-gray-400">38.20% Recall</span>
            </div>
            <div className="h-6 w-full bg-white/5 rounded-lg overflow-hidden p-0.5 border border-white/5">
              <motion.div
                initial={{ width: 0 }}
                whileInView={{ width: '38.2%' }}
                viewport={{ once: true }}
                transition={{ duration: 1, ease: 'easeOut' }}
                className="h-full bg-slate-600 rounded flex items-center justify-end pr-2 text-[10px] font-mono text-white font-bold"
              >
                38.2%
              </motion.div>
            </div>
          </div>

          {/* Baseline B */}
          <div>
            <div className="flex justify-between items-center text-sm font-mono mb-2">
              <span className="text-gray-300">Baseline B: Language Server Protocol (LSP Symbol Ref)</span>
              <span className="text-purple-400">59.40% Recall</span>
            </div>
            <div className="h-6 w-full bg-white/5 rounded-lg overflow-hidden p-0.5 border border-white/5">
              <motion.div
                initial={{ width: 0 }}
                whileInView={{ width: '59.4%' }}
                viewport={{ once: true }}
                transition={{ duration: 1.2, ease: 'easeOut', delay: 0.2 }}
                className="h-full bg-indigo-500 rounded flex items-center justify-end pr-2 text-[10px] font-mono text-white font-bold"
              >
                59.4%
              </motion.div>
            </div>
          </div>

          {/* Baseline C: SynEvid */}
          <div>
            <div className="flex justify-between items-center text-sm font-mono mb-2">
              <span className="text-cyan-300 font-bold flex items-center">
                <span className="w-2 h-2 rounded-full bg-cyan-400 mr-2 animate-ping" />
                Baseline C: SynEvid (Code Autopsy Engine)
              </span>
              <span className="text-cyan-400 font-bold">91.13% Recall (+31.73pp Advantage)</span>
            </div>
            <div className="h-8 w-full bg-cyan-950/40 rounded-lg overflow-hidden p-0.5 border border-cyan-500/40 shadow-lg shadow-cyan-950/60">
              <motion.div
                initial={{ width: 0 }}
                whileInView={{ width: '91.13%' }}
                viewport={{ once: true }}
                transition={{ duration: 1.4, ease: 'easeOut', delay: 0.4 }}
                className="h-full bg-gradient-to-r from-cyan-500 via-cyan-400 to-green-400 rounded flex items-center justify-end pr-3 text-xs font-mono text-black font-extrabold"
              >
                91.13% RECALL GATE PASSED
              </motion.div>
            </div>
          </div>
        </div>
      </div>

      {/* Ground Truth Matrix */}
      <div className="max-w-5xl mx-auto rounded-2xl border border-white/10 bg-[#070b13] overflow-hidden shadow-2xl">
        <div className="overflow-x-auto">
          <table className="w-full text-left border-collapse text-sm">
            <thead>
              <tr className="border-b border-white/10 bg-white/[0.02]">
                <th className="p-4 sm:p-5 font-mono text-xs uppercase text-gray-400">Capability</th>
                <th className="p-4 sm:p-5 font-mono text-xs uppercase text-gray-400">LLM Self-Inspection</th>
                <th className="p-4 sm:p-5 font-mono text-xs uppercase text-gray-400">Static Linters</th>
                <th className="p-4 sm:p-5 font-mono text-xs uppercase text-cyan-400 bg-cyan-950/20">
                  SynEvid (Code Autopsy)
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/5">
              <tr>
                <td className="p-4 sm:p-5 font-semibold text-white">Repeatability Guarantee</td>
                <td className="p-4 sm:p-5 text-red-400/90 flex items-center gap-1.5">
                  <X className="w-4 h-4 shrink-0" /> Stochastic & Hallucinates
                </td>
                <td className="p-4 sm:p-5 text-yellow-400/90">Partial (Cache/Clock dependent)</td>
                <td className="p-4 sm:p-5 text-green-400 font-bold bg-cyan-950/10 flex items-center gap-1.5">
                  <Check className="w-4 h-4 shrink-0" /> 100/100 Byte-Identical BLAKE3
                </td>
              </tr>
              <tr>
                <td className="p-4 sm:p-5 font-semibold text-white">Transitive Blast Radius</td>
                <td className="p-4 sm:p-5 text-red-400/90 flex items-center gap-1.5">
                  <X className="w-4 h-4 shrink-0" /> Misses deep callers
                </td>
                <td className="p-4 sm:p-5 text-gray-400">Direct file-level only</td>
                <td className="p-4 sm:p-5 text-cyan-300 font-medium bg-cyan-950/10 flex items-center gap-1.5">
                  <Check className="w-4 h-4 shrink-0" /> Petgraph BFS + SCC Condensation
                </td>
              </tr>
              <tr>
                <td className="p-4 sm:p-5 font-semibold text-white">Dynamic Code Handling</td>
                <td className="p-4 sm:p-5 text-red-400/90 flex items-center gap-1.5">
                  <X className="w-4 h-4 shrink-0" /> Silent ungrounded pass
                </td>
                <td className="p-4 sm:p-5 text-gray-400">Silently ignored</td>
                <td className="p-4 sm:p-5 text-green-400 font-bold bg-cyan-950/10 flex items-center gap-1.5">
                  <Check className="w-4 h-4 shrink-0" /> Honest CoverageState::Unknown
                </td>
              </tr>
              <tr>
                <td className="p-4 sm:p-5 font-semibold text-white">Network & Sandbox Security</td>
                <td className="p-4 sm:p-5 text-yellow-400/90">Requires cloud API tokens</td>
                <td className="p-4 sm:p-5 text-gray-400">Varies</td>
                <td className="p-4 sm:p-5 text-cyan-300 font-medium bg-cyan-950/10 flex items-center gap-1.5">
                  <Check className="w-4 h-4 shrink-0" /> 100% Offline (0 sockets, 0 procs)
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
