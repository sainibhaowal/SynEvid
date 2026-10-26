'use client';

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Terminal, Copy, Check, ShieldCheck, AlertTriangle, ArrowRight } from 'lucide-react';

interface TerminalTab {
  id: string;
  name: string;
  cmd: string;
  output: string;
  exitCode: number;
  statusText: string;
}

const TABS: TerminalTab[] = [
  {
    id: 'verify',
    name: 'autopsy verify',
    cmd: 'autopsy verify --strict',
    exitCode: 0,
    statusText: 'PASS (Exit 0)',
    output: `[2026-10-16T13:00:00Z INFO autopsy_repo] Scanning repository at /workspace/payments-service
[2026-10-16T13:00:00Z INFO autopsy_repo] Computed RepoSnapshot: BLAKE3(512 files, 48 configs)
[2026-10-16T13:00:01Z INFO autopsy_adapter_typescript] Parsing AST via tree-sitter (0 errors)
[2026-10-16T13:00:01Z INFO autopsy_graph] Multigraph built: 4,821 nodes, 19,402 edges
[2026-10-16T13:00:02Z INFO autopsy_invariants] Evaluating 4 configured policies (.autopsy/invariants.yml):
  ✔ architecture-layers  (layers): PASS (presentation -> domain -> infrastructure)
  ✔ no-circular-deps     (no_new_cycle): PASS (0 new cycles)
  ✔ api-compatibility    (api_compatibility): PASS (0 breaking changes)
  ✔ forbidden-deps       (forbidden_dependency): PASS (0 matched edges)
[2026-10-16T13:00:02Z INFO autopsy_evidence] Sealed BLAKE3 Evidence Receipt:
  Snapshot: 9f82d1c47a98ef10342ab7168b0c5d2e4f6a8193b7c0d5e2a1f4b8c6d3e2a1f0
  Digest:   b3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
[2026-10-16T13:00:02Z SUCCESS] Verification Gate PASSED. Exit Code: 0 (strict: coverage full)`,
  },
  {
    id: 'impact',
    name: 'autopsy impact',
    cmd: 'autopsy impact -s "src/payment/gateway.ts::processPayment" --profile callers --top-k 5',
    exitCode: 0,
    statusText: 'FULL COVERAGE',
    output: `[2026-10-16T13:00:00Z INFO autopsy_impact] Bounded BFS Traversal (direction: backward, kinds: calls)
Root Symbol: src/payment/gateway.ts::processPayment
Blast Radius Transitive Closure: 42 impacted call-sites across 4 modules

Ranked Impacted Shortlist (Top 5 Callers by Path Weight):
  1. [Rank 1] src/api/checkout.ts::handleCheckout (path len: 1, weight: 1.00)
  2. [Rank 2] src/cron/subscription.ts::renewSubscription (path len: 2, weight: 0.85)
  3. [Rank 3] src/webhooks/stripe.ts::onPaymentIntent (path len: 2, weight: 0.85)
  4. [Rank 4] src/admin/refunds.ts::retryFailedCharge (path len: 3, weight: 0.62)
  5. [Rank 5] tests/e2e/payment.test.ts::testCheckoutFlow (path len: 1, weight: 0.50)

Coverage State: Full (100% deterministic AST call graph)
Recommended Agent Strategy: Run selective test slice (5 tests) before commit.`,
  },
  {
    id: 'diff',
    name: 'autopsy diff',
    cmd: 'autopsy diff --format json',
    exitCode: 0,
    statusText: 'DIFF (Exit 0)',
    output: `{
  "before_snapshot_id": "9f82d1c47a98ef10342ab7168b0c5d2e4f6a8193b7c0d5e2a1f4b8c6d3e2a1f0",
  "after_snapshot_id": "47c1e8b3f6a2d5908c4b7e1f3a6d9c2058e7b4a1f6d3c9e2b8a5f1d4c7e3b9a6",
  "summary": {
    "files_modified": 1,
    "symbols_added": 2,
    "symbols_modified": 1,
    "breaking_contract_changes": 0,
    "edges_added": 3,
    "edges_removed": 1
  },
  "frontier": {
    "affected_files": [
      "src/auth/session.ts",
      "src/api/checkout.ts"
    ]
  },
  "diff_digest": "4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c"
}`,
  },
  {
    id: 'explain',
    name: 'autopsy explain',
    cmd: 'autopsy explain no-circular-deps-FINDING',
    exitCode: 2,
    statusText: 'FAIL (Exit 2)',
    output: `Finding ID: no-circular-deps-FINDING
Policy:     no-circular-deps (no_new_cycle, severity: error)
Exit Code:  2 (invariant violation)

Evidence Trace (import cycle path):
  Step 1: src/domain/order.ts (imports) -> src/services/inventory.ts
  Step 2: src/services/inventory.ts (imports) -> src/events/bus.ts
  Step 3: src/events/bus.ts (imports) -> src/domain/order.ts  <-- [CYCLE CLOSED]

Suggested Agent Repair Action:
  Extract the shared event payload out of 'order.ts' into 'src/domain/types.ts'.
  Re-run 'autopsy verify --strict' to assert the condensation is acyclic.`,
  },
];

export default function TerminalWorkbench() {
  const [activeTab, setActiveTab] = useState<string>('verify');
  const [copied, setCopied] = useState(false);

  const currentTab = TABS.find((t) => t.id === activeTab) || TABS[0];

  const handleCopy = () => {
    navigator.clipboard.writeText(currentTab.cmd);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="w-full max-w-5xl mx-auto rounded-2xl border border-cyan-500/25 bg-[#070b13]/90 backdrop-blur-xl shadow-2xl shadow-cyan-950/40 overflow-hidden">
      {/* Terminal Titlebar */}
      <div className="flex flex-wrap items-center justify-between px-5 py-3.5 border-b border-white/10 bg-[#0c121e]">
        <div className="flex items-center space-x-3">
          <div className="flex space-x-2">
            <span className="w-3 h-3 rounded-full bg-red-500/80 inline-block" />
            <span className="w-3 h-3 rounded-full bg-yellow-500/80 inline-block" />
            <span className="w-3 h-3 rounded-full bg-green-500/80 inline-block" />
          </div>
          <div className="flex items-center text-xs text-gray-400 font-mono pl-2">
            <Terminal className="w-3.5 h-3.5 mr-1.5 text-cyan-400" />
            <span>autopsy // v0.1.0 (offline)</span>
          </div>
        </div>

        {/* Tab Buttons */}
        <div className="flex items-center space-x-1 my-1 sm:my-0">
          {TABS.map((tab) => {
            const isActive = tab.id === activeTab;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`relative px-3 py-1.5 rounded-lg text-xs font-mono transition-all ${
                  isActive
                    ? 'text-cyan-400 bg-cyan-950/40 border border-cyan-500/40 shadow-sm'
                    : 'text-gray-400 hover:text-white hover:bg-white/5 border border-transparent'
                }`}
              >
                {tab.name}
              </button>
            );
          })}
        </div>

        {/* Copy Button */}
        <button
          onClick={handleCopy}
          className="flex items-center space-x-1.5 text-xs text-gray-400 hover:text-cyan-400 bg-white/5 hover:bg-cyan-500/10 px-3 py-1.5 rounded-lg border border-white/10 transition-colors font-mono"
        >
          {copied ? (
            <>
              <Check className="w-3.5 h-3.5 text-green-400" />
              <span className="text-green-400">Copied</span>
            </>
          ) : (
            <>
              <Copy className="w-3.5 h-3.5" />
              <span>Copy Cmd</span>
            </>
          )}
        </button>
      </div>

      {/* Terminal Viewport - Fixed Uniform Box Size with Internal Scroll */}
      <div className="p-6 font-mono text-sm leading-relaxed flex flex-col h-[480px]">
        {/* Command line (Pinned Header) */}
        <div className="flex items-center text-gray-200 mb-4 pb-3 border-b border-white/5 shrink-0">
          <span className="text-green-400 font-semibold mr-2">$</span>
          <span className="text-white font-medium select-all truncate">{currentTab.cmd}</span>
          <span className="ml-auto text-xs px-2.5 py-0.5 rounded border border-white/10 text-gray-400 shrink-0 ml-3">
            {currentTab.statusText}
          </span>
        </div>

        {/* Output Area with Internal Scrolling */}
        <div className="flex-1 overflow-y-auto overflow-x-auto pr-3 terminal-scroll">
          <AnimatePresence mode="wait">
            <motion.pre
              key={activeTab}
              initial={{ opacity: 0, y: 5 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -5 }}
              transition={{ duration: 0.15 }}
              className="text-gray-300 whitespace-pre font-mono text-xs sm:text-sm leading-relaxed"
            >
              {currentTab.output}
            </motion.pre>
          </AnimatePresence>
        </div>
      </div>

      {/* Bottom telemetry badge */}
      <div className="px-5 py-2.5 bg-[#05080e] border-t border-white/5 flex flex-wrap items-center justify-between text-xs font-mono text-gray-500">
        <div className="flex items-center space-x-4">
          <span className="flex items-center text-cyan-400/80">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 mr-1.5 animate-pulse" />
            100/100 BLAKE3 Determinism
          </span>
          <span className="text-gray-600">•</span>
          <span>Zero Network Sockets</span>
          <span className="text-gray-600">•</span>
          <span>Zero Subprocesses</span>
        </div>
        <div className="text-gray-400">Exit Code: {currentTab.exitCode}</div>
      </div>
    </div>
  );
}
