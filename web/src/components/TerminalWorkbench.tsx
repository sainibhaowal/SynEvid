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
    cmd: 'autopsy verify --strict --invariants-file .autopsy/invariants.yml',
    exitCode: 0,
    statusText: 'PASS (Exit 0)',
    output: `[2026-10-16T13:00:00Z INFO autopsy_repo] Scanning repository at /workspace/SynEvid
[2026-10-16T13:00:00Z INFO autopsy_repo] Computed RepoSnapshot: BLAKE3(512 files, 48 configs)
[2026-10-16T13:00:01Z INFO autopsy_adapter_typescript] Parsing AST via tree-sitter (0 errors)
[2026-10-16T13:00:01Z INFO autopsy_graph] Multigraph built: 4,821 nodes, 19,402 edges
[2026-10-16T13:00:02Z INFO autopsy_invariants] Evaluating 6 architectural policies:
  ✔ INV-LAYERS-001 (Core -> Adapter inversion): PASS
  ✔ INV-CYCLE-002  (Tarjan SCC cycle check): PASS (0 acyclic cycles)
  ✔ INV-FORBIDDEN  (Zero LLM in crates/*): PASS
  ✔ INV-CONTRACTS  (Callable signature stability): PASS
  ✔ INV-CROSS-001  (CRLF/LF & Path normalization): PASS
[2026-10-16T13:00:02Z INFO autopsy_evidence] Generating BLAKE3 Tamper-Proof Evidence Receipt:
  Receipt ID: receipt-b3-9f82d1c47a98ef10342ab716
  Digest:     b3:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
[2026-10-16T13:00:02Z SUCCESS] Verification Gate PASSED. Exit Code: 0 (Strict PASS)`,
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
    cmd: 'autopsy diff --before snap-b3-8a410b --format json',
    exitCode: 0,
    statusText: 'DIFF (Exit 0)',
    output: `{
  "schema_version": "1.0.0",
  "baseline_snapshot": "snap-b3-8a410b2f",
  "target_worktree_hash": "b3:478a9c31fa7801be",
  "symbol_deltas": {
    "added": 2,
    "modified": 1,
    "removed": 0
  },
  "contracts": {
    "breaking_changes": 0,
    "compatible_extensions": 1,
    "details": [
      {
        "symbol": "src/auth/session.ts::verifyToken",
        "kind": "InterfaceExtension",
        "is_breaking": false,
        "note": "Optional parameter 'tenantId?: string' added"
      }
    ]
  },
  "exit_code": 0
}`,
  },
  {
    id: 'explain',
    name: 'autopsy explain',
    cmd: 'autopsy explain finding-cyc-004',
    exitCode: 2,
    statusText: 'FAIL (Exit 2)',
    output: `Finding ID: finding-cyc-004
Policy:     INV-CYCLE-002 (Prohibited Circular Dependency)
Severity:   CRITICAL_REGRESSION (Exit Code: 2)

Evidence Trace (Path Sequence):
  Step 1: src/domain/order.ts (imports) -> src/services/inventory.ts
  Step 2: src/services/inventory.ts (imports) -> src/events/bus.ts
  Step 3: src/events/bus.ts (imports) -> src/domain/order.ts  <-- [CYCLE CLOSURE]

Suggested Agent Repair Action:
  Extract shared event payload interface from 'order.ts' into 'src/domain/types.ts'.
  Re-run 'autopsy verify' to assert acyclic condensation.`,
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
            <span>synevid-core // v0.1.0-alpha (offline)</span>
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

      {/* Terminal Viewport */}
      <div className="p-6 font-mono text-sm leading-relaxed overflow-x-auto min-h-[380px]">
        {/* Command line */}
        <div className="flex items-center text-gray-200 mb-4 pb-3 border-b border-white/5">
          <span className="text-green-400 font-semibold mr-2">$</span>
          <span className="text-white font-medium select-all">{currentTab.cmd}</span>
          <span className="ml-auto text-xs px-2 py-0.5 rounded border border-white/10 text-gray-400">
            {currentTab.statusText}
          </span>
        </div>

        {/* Output Area */}
        <AnimatePresence mode="wait">
          <motion.pre
            key={activeTab}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -5 }}
            transition={{ duration: 0.18 }}
            className="text-gray-300 whitespace-pre font-mono text-xs sm:text-sm leading-relaxed"
          >
            {currentTab.output}
          </motion.pre>
        </AnimatePresence>
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
