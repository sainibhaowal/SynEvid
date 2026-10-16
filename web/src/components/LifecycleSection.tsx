'use client';

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { GitBranch, Shield, Zap, Search, RefreshCw, CheckCircle2 } from 'lucide-react';

interface StepDetail {
  step: string;
  phase: string;
  title: string;
  cmd: string;
  desc: string;
  details: string[];
}

const LIFECYCLE_STEPS: StepDetail[] = [
  {
    step: '01',
    phase: 'BASELINE',
    title: 'Snapshot State A',
    cmd: 'autopsy baseline --format json',
    desc: 'Takes an immutable cryptographic snapshot of the repository AST, files, configs, and stable symbol identities.',
    details: [
      'Computes BLAKE3 digests of all tracked source trees',
      'Normalizes CRLF/LF line endings and paths deterministically',
      'Stores baseline in local SQLite & content-addressed cache',
    ],
  },
  {
    step: '02',
    phase: 'ASSESSMENT',
    title: 'Impact Assessment',
    cmd: 'autopsy impact -s <symbol> --profile callers',
    desc: 'Traverses the bounded AST multigraph to discover all impacted callers, breaking changes, and contract hazards.',
    details: [
      'Bounded BFS traversal with SCC cycle condensation',
      'Discloses explicit CoverageState::Unknown for dynamic calls',
      'Calculates ranked top-k shortlist of critical dependents',
    ],
  },
  {
    step: '03',
    phase: 'MUTATION',
    title: 'Agent Code Edit',
    cmd: 'Autonomous AI Refactoring (A → B)',
    desc: 'The AI coding agent (Claude, Codex, Gemini) proposes code edits informed by SynEvid blast radius boundaries.',
    details: [
      'Agent edits targeted symbol implementations',
      'Scoped only to safe contract-compatible boundary zones',
      'Prevents blind cross-package file modifications',
    ],
  },
  {
    step: '04',
    phase: 'VERIFICATION',
    title: 'Strict Verification',
    cmd: 'autopsy verify --strict',
    desc: 'Mathematical evaluation of architectural layers, cycles, forbidden dependencies, and type contracts.',
    details: [
      'Zero LLM in verification loop (100% deterministic code)',
      'Asserts no cyclic regressions or broken architectural axioms',
      'Exit Code 0 on PASS, 2 on Invariant Failure, 4 on Unknown',
    ],
  },
  {
    step: '05',
    phase: 'EVIDENCE',
    title: 'Tamper-Proof Receipt',
    cmd: 'autopsy explain <finding_id>',
    desc: 'Generates cryptographic BLAKE3 evidence receipts for CI/CD gates, PR approvals, or self-healing repairs.',
    details: [
      'Cryptographically sealed BLAKE3 evidence receipt',
      'SARIF 2.1.0 output for GitHub Code Scanning integration',
      'Exact line-span path sequences for instant agent repair',
    ],
  },
];

export default function LifecycleSection() {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const active = LIFECYCLE_STEPS[selectedIdx];

  return (
    <section className="py-24 border-t border-b border-white/5 bg-[#070b13]/35 relative backdrop-blur-sm">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center max-w-3xl mx-auto mb-16">
          <div className="inline-flex items-center space-x-2 px-3 py-1 rounded-full border border-cyan-500/30 bg-cyan-950/20 text-cyan-400 text-xs font-mono uppercase tracking-wider mb-4">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
            <span>Closed-Loop Verification Protocol</span>
          </div>
          <h2 className="text-3xl sm:text-5xl font-extrabold text-white tracking-tight mb-4">
            The In-Session Agent Lifecycle
          </h2>
          <p className="text-gray-400 text-base sm:text-lg">
            Frontier AI coding agents cannot reliably self-verify. SynEvid acts as an external ground-truth oracle, enforcing a closed-loop verify-and-repair cycle.
          </p>
        </div>

        {/* Step Cards Grid */}
        <div className="grid grid-cols-1 md:grid-cols-5 gap-3 mb-10">
          {LIFECYCLE_STEPS.map((s, idx) => {
            const isSelected = idx === selectedIdx;
            return (
              <motion.button
                key={s.step}
                onClick={() => setSelectedIdx(idx)}
                whileHover={{ y: -3 }}
                whileTap={{ scale: 0.98 }}
                className={`text-left p-5 rounded-xl border transition-all relative ${
                  isSelected
                    ? 'bg-cyan-950/40 border-cyan-500/60 shadow-lg shadow-cyan-950/50'
                    : 'bg-[#0d1322]/60 border-white/10 hover:border-cyan-500/30'
                }`}
              >
                <div className="flex items-center justify-between mb-3">
                  <span className="font-mono text-xs text-cyan-400 font-bold">{s.step}</span>
                  <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-white/5 text-gray-400 border border-white/5">
                    {s.phase}
                  </span>
                </div>
                <h3 className="text-sm font-bold text-white mb-1.5 leading-snug">{s.title}</h3>
                <div className="text-xs font-mono text-gray-500 truncate">{s.cmd}</div>

                {isSelected && (
                  <motion.div
                    layoutId="active-indicator"
                    className="absolute -bottom-1 left-4 right-4 h-0.5 bg-gradient-to-r from-cyan-400 via-purple-500 to-green-400 rounded-full"
                  />
                )}
              </motion.button>
            );
          })}
        </div>

        {/* Detailed Inspector Card */}
        <AnimatePresence mode="wait">
          <motion.div
            key={active.step}
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.25 }}
            className="rounded-2xl border border-cyan-500/20 bg-[#0a0f1b]/80 p-8 backdrop-blur-xl"
          >
            <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-6 border-b border-white/10 pb-6 mb-6">
              <div>
                <div className="flex items-center space-x-3 mb-2">
                  <span className="text-xs font-mono text-cyan-400 font-bold uppercase tracking-wider">
                    Phase {active.step} // {active.phase}
                  </span>
                  <span className="text-gray-600">•</span>
                  <h3 className="text-2xl font-bold text-white">{active.title}</h3>
                </div>
                <p className="text-gray-300 text-sm max-w-2xl">{active.desc}</p>
              </div>

              <div className="bg-[#050810] border border-cyan-500/20 rounded-xl px-5 py-3 font-mono text-sm text-cyan-300 flex items-center space-x-3">
                <span className="text-green-400 font-bold">$</span>
                <span>{active.cmd}</span>
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {active.details.map((item, i) => (
                <div
                  key={i}
                  className="flex items-start space-x-3 p-4 rounded-xl bg-white/[0.02] border border-white/5"
                >
                  <CheckCircle2 className="w-4 h-4 text-cyan-400 shrink-0 mt-0.5" />
                  <span className="text-xs text-gray-300 leading-relaxed font-sans">{item}</span>
                </div>
              ))}
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
    </section>
  );
}
