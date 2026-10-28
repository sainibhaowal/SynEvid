'use client';

import React, { useState, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Terminal as TerminalIcon,
  Copy,
  Check,
  RotateCcw,
  Trash2,
  Send,
  Maximize2,
  Minimize2,
  Square,
  Minus,
  X,
  Play,
  Cpu,
  Shield,
  Activity,
  Code2,
  CheckCircle2,
  Sparkles,
} from 'lucide-react';
import { api } from '@/lib/api';

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
    id: 'interactive',
    name: '● Interactive CLI',
    cmd: 'git status',
    exitCode: 0,
    statusText: 'LIVE REPL',
    output: '',
  },
  {
    id: 'verify',
    name: 'autopsy-verify',
    cmd: 'autopsy verify --strict',
    exitCode: 0,
    statusText: 'PASS (Exit 0)',
    output: `[2026-10-16T13:00:00Z INFO autopsy_repo] Scanning repository at /workspace/project
[2026-10-16T13:00:00Z INFO autopsy_repo] Computed RepoSnapshot: BLAKE3(512 files, 48 configs)
[2026-10-16T13:00:01Z INFO autopsy_adapter_typescript] Parsing AST via tree-sitter (0 errors)
[2026-10-16T13:00:01Z INFO autopsy_graph] Multigraph built: 4,821 nodes, 19,402 edges
[2026-10-16T13:00:02Z INFO autopsy_invariants] Evaluating 4 configured policies (.autopsy/invariants.yml):
  \x1b[32m✔\x1b[0m architecture-layers  (layers): PASS (presentation -> domain -> infrastructure)
  \x1b[32m✔\x1b[0m no-circular-deps     (no_new_cycle): PASS (0 new cycles)
  \x1b[32m✔\x1b[0m api-compatibility    (api_compatibility): PASS (0 breaking changes)
  \x1b[32m✔\x1b[0m forbidden-deps       (forbidden_dependency): PASS (0 matched edges)
[2026-10-16T13:00:02Z INFO autopsy_evidence] Sealed BLAKE3 Evidence Receipt:
  Snapshot: 9f82d1c47a98ef10342ab7168b0c5d2e4f6a8193b7c0d5e2a1f4b8c6d3e2a1f0
  Digest:   b3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
[2026-10-16T13:00:02Z SUCCESS] Verification Gate PASSED. Exit Code: 0 (strict: coverage full)`,
  },
  {
    id: 'impact',
    name: 'autopsy-impact',
    cmd: 'autopsy impact -s "src/services/PaymentService.ts::chargeCard" --profile callers --top-k 5',
    exitCode: 0,
    statusText: 'FULL COVERAGE',
    output: `[2026-10-16T13:00:00Z INFO autopsy_impact] Bounded BFS Traversal (direction: backward, kinds: calls)
Root Symbol: src/services/PaymentService.ts::chargeCard
Blast Radius Transitive Closure: 42 impacted call-sites across 4 modules

Ranked Impacted Shortlist (Top 5 Callers by Path Weight):
  1. [Rank 1] src/api/checkout.ts::handleCheckout (path len: 1, weight: 1.00)
  2. [Rank 2] src/cron/subscription.ts::renewSubscription (path len: 2, weight: 0.85)
  3. [Rank 3] src/webhooks/stripe.ts::onPaymentIntent (path len: 2, weight: 0.85)
  4. [Rank 4] src/admin/refunds.ts::retryFailedCharge (path len: 3, weight: 0.62)
  5. [Rank 5] tests/e2e/payment.test.ts::testCheckoutFlow (path len: 1, weight: 0.50)

Coverage State: Full (100% deterministic AST call graph)
Recommended Strategy: Run targeted test slice before commit.`,
  },
  {
    id: 'diff',
    name: 'autopsy-diff',
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
];

interface CommandLogEntry {
  command: string;
  stdout: string;
  stderr: string;
  exitCode: number;
  timestamp: string;
}

// Convert ANSI escape codes to colored spans
function renderAnsiText(text: string): React.ReactNode[] {
  if (!text) return [];
  const regex = /\x1b\[([0-9;]+)m/g;
  const parts: React.ReactNode[] = [];
  let lastIndex = 0;
  let currentColor = '';
  let match: RegExpExecArray | null;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      const chunk = text.substring(lastIndex, match.index);
      parts.push(
        <span key={parts.length} className={currentColor}>
          {chunk}
        </span>
      );
    }
    const code = match[1];
    switch (code) {
      case '0':
        currentColor = '';
        break;
      case '31':
        currentColor = 'text-rose-400 font-semibold';
        break;
      case '32':
        currentColor = 'text-emerald-400 font-semibold';
        break;
      case '33':
        currentColor = 'text-amber-300 font-semibold';
        break;
      case '34':
        currentColor = 'text-sky-400 font-semibold';
        break;
      case '35':
        currentColor = 'text-purple-400 font-semibold';
        break;
      case '36':
        currentColor = 'text-cyan-300 font-semibold';
        break;
      case '1':
        currentColor += ' font-bold';
        break;
      default:
        break;
    }
    lastIndex = regex.lastIndex;
  }

  if (lastIndex < text.length) {
    const chunk = text.substring(lastIndex);
    parts.push(
      <span key={parts.length} className={currentColor}>
        {chunk}
      </span>
    );
  }

  return parts;
}

const AUTOCOMPLETE_COMMANDS = [
  'git status',
  'git log --oneline',
  'git log',
  'git branch',
  'git tag',
  'git commit -m ""',
  'git undo',
  'autopsy verify',
  'autopsy impact',
  'autopsy diff',
  'ls',
  'pwd',
  'whoami',
  'uname -a',
  'clear',
  'help',
];

export default function TerminalWorkbench() {
  const [activeTab, setActiveTab] = useState<string>('interactive');
  const [copied, setCopied] = useState(false);
  const [isMaximized, setIsMaximized] = useState(false);

  // Dynamic Prompt Context
  const [promptUser, setPromptUser] = useState<string>('developer');
  const [promptPath, setPromptPath] = useState<string>('~');
  const [promptBranch, setPromptBranch] = useState<string>('main');

  // Interactive Terminal State
  const [inputCommand, setInputCommand] = useState<string>('');
  const [commandLogs, setCommandLogs] = useState<CommandLogEntry[]>([]);
  const [isExecuting, setIsExecuting] = useState<boolean>(false);
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number>(-1);

  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const currentTab = TABS.find((t) => t.id === activeTab) || TABS[0];

  // Fetch initial repo status for realistic prompt path
  useEffect(() => {
    api.getStatus()
      .then((st) => {
        if (st.repo_root) {
          const folder = st.repo_name || st.repo_root.split(/[/\\]/).filter(Boolean).pop() || 'repo';
          setPromptPath(`~/${folder}`);
        }
      })
      .catch(() => {});

    // Initial authentic terminal MOTD banner
    setCommandLogs([
      {
        command: 'system::init',
        stdout: `SynEvid Pure-Rust Terminal v0.4.0 (x86_64-linux-gnu / PTY)
Kernel: Deterministic Verification Engine (INV-SANDBOX-003, Zero External Sub-Processes)
Hardware Sandboxing: Strict Read-Only Traversal (FR-026) & In-Memory Git DAG Engine
Type 'help' for command list, or click any quick-run chip below.
Keyboard Shortcuts: Ctrl+L to clear screen, Up/Down for command history, Tab to autocomplete.`,
        stderr: '',
        exitCode: 0,
        timestamp: new Date().toLocaleTimeString(),
      },
    ]);
  }, []);

  useEffect(() => {
    if (activeTab === 'interactive' && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [commandLogs, activeTab, isExecuting]);

  const handleCopy = () => {
    const textToCopy = activeTab === 'interactive' ? inputCommand || 'git status' : currentTab.cmd;
    navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const executeCommand = async (cmdToRun: string) => {
    const cleanCmd = cmdToRun.trim();
    if (!cleanCmd) return;

    if (cleanCmd === 'clear') {
      setCommandLogs([]);
      setInputCommand('');
      return;
    }

    setIsExecuting(true);
    setHistory((prev) => [cleanCmd, ...prev.filter((c) => c !== cleanCmd)]);
    setHistoryIndex(-1);

    try {
      const res = await api.execTerminal(cleanCmd);
      if (res.stdout === '__CLEAR__') {
        setCommandLogs([]);
      } else {
        setCommandLogs((prev) => [
          ...prev,
          {
            command: cleanCmd,
            stdout: res.stdout,
            stderr: res.stderr,
            exitCode: res.exit_code,
            timestamp: new Date().toLocaleTimeString(),
          },
        ]);
      }
    } catch (err: unknown) {
      setCommandLogs((prev) => [
        ...prev,
        {
          command: cleanCmd,
          stdout: '',
          stderr: err instanceof Error ? err.message : 'Execution failed: daemon unreachable',
          exitCode: 1,
          timestamp: new Date().toLocaleTimeString(),
        },
      ]);
    } finally {
      setIsExecuting(false);
      setInputCommand('');
      // Keep focus on input
      setTimeout(() => inputRef.current?.focus(), 20);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Ctrl + L: Clear terminal screen
    if (e.ctrlKey && (e.key === 'l' || e.key === 'L')) {
      e.preventDefault();
      setCommandLogs([]);
      return;
    }

    // Ctrl + C: Abort line
    if (e.ctrlKey && (e.key === 'c' || e.key === 'C')) {
      e.preventDefault();
      setCommandLogs((prev) => [
        ...prev,
        {
          command: inputCommand + ' ^C',
          stdout: '',
          stderr: '',
          exitCode: 130,
          timestamp: new Date().toLocaleTimeString(),
        },
      ]);
      setInputCommand('');
      return;
    }

    // Ctrl + U: Clear line before cursor
    if (e.ctrlKey && (e.key === 'u' || e.key === 'U')) {
      e.preventDefault();
      setInputCommand('');
      return;
    }

    // Tab Autocompletion
    if (e.key === 'Tab') {
      e.preventDefault();
      if (!inputCommand) return;
      const match = AUTOCOMPLETE_COMMANDS.find((c) => c.startsWith(inputCommand));
      if (match) {
        setInputCommand(match);
      }
      return;
    }

    if (e.key === 'Enter') {
      executeCommand(inputCommand);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (history.length > 0 && historyIndex + 1 < history.length) {
        const nextIdx = historyIndex + 1;
        setHistoryIndex(nextIdx);
        setInputCommand(history[nextIdx]);
      }
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyIndex > 0) {
        const prevIdx = historyIndex - 1;
        setHistoryIndex(prevIdx);
        setInputCommand(history[prevIdx]);
      } else if (historyIndex === 0) {
        setHistoryIndex(-1);
        setInputCommand('');
      }
    }
  };

  const handleTerminalAreaClick = () => {
    inputRef.current?.focus();
  };

  const QUICK_BUTTONS = [
    { label: 'git status', desc: 'Scan working tree' },
    { label: 'git log --oneline', desc: 'Topological commit DAG' },
    { label: 'git branch', desc: 'List branches' },
    { label: 'git tag', desc: 'List semantic tags' },
    { label: 'git undo', desc: '1-click rollback' },
    { label: 'autopsy verify', desc: 'Architecture gates' },
    { label: 'autopsy impact', desc: 'Blast radius BFS' },
    { label: 'ls', desc: 'List files' },
    { label: 'pwd', desc: 'Current directory' },
    { label: 'clear', desc: 'Clear screen' },
    { label: 'help', desc: 'Command manual' },
  ];

  return (
    <div
      className={`w-full transition-all duration-200 font-mono text-xs select-text ${
        isMaximized
          ? 'fixed inset-3 z-50 flex flex-col bg-[#070b13] border border-cyan-500/30 rounded-xl shadow-2xl overflow-hidden'
          : 'max-w-5xl mx-auto rounded-xl border border-[#1e293b] bg-[#070b13] shadow-2xl overflow-hidden'
      }`}
    >
      {/* Industrial Native Terminal Header (Strictly NO Apple traffic lights) */}
      <div className="flex items-center justify-between px-3 py-2 bg-[#0d131f] border-b border-[#1e293b] select-none text-xs">
        {/* Left: Terminal Tab & Prompt Metadata */}
        <div className="flex items-center space-x-3">
          <div className="flex items-center space-x-1.5 px-2.5 py-1 rounded bg-[#162032] border border-[#233149] text-cyan-400 font-bold text-[11px]">
            <TerminalIcon className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
            <span className="tracking-tight">syn-sh</span>
          </div>

          <div className="hidden sm:flex items-center space-x-2 text-[11px] text-gray-400 font-mono">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            <span className="text-gray-300 font-semibold">{promptUser}@synevid</span>
            <span className="text-gray-600">:</span>
            <span className="text-sky-400 font-semibold">{promptPath}</span>
            <span className="text-amber-400 font-semibold">({promptBranch})</span>
          </div>
        </div>

        {/* Center: Tabs */}
        <div className="flex items-center space-x-1">
          {TABS.map((tab) => {
            const isActive = tab.id === activeTab;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`px-3 py-1 rounded text-[11px] font-mono transition-colors cursor-pointer border ${
                  isActive
                    ? 'bg-[#1a253a] border-cyan-500/50 text-cyan-300 font-bold shadow-sm'
                    : 'bg-transparent border-transparent text-gray-400 hover:text-gray-200 hover:bg-[#131c2d]'
                }`}
              >
                {tab.name}
              </button>
            );
          })}
        </div>

        {/* Right: Native Controls & Actions */}
        <div className="flex items-center space-x-1 text-gray-400">
          <button
            onClick={() => setCommandLogs([])}
            title="Clear buffer (Ctrl+L)"
            className="p-1 rounded hover:text-rose-400 hover:bg-white/5 transition-colors cursor-pointer"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>

          <button
            onClick={handleCopy}
            title="Copy command"
            className="flex items-center space-x-1 px-2 py-0.5 rounded bg-[#162032] hover:bg-[#1f2d47] border border-[#233149] text-gray-300 hover:text-cyan-300 transition-colors text-[11px] cursor-pointer"
          >
            {copied ? (
              <>
                <Check className="w-3 h-3 text-emerald-400" />
                <span className="text-emerald-400">Copied</span>
              </>
            ) : (
              <>
                <Copy className="w-3 h-3" />
                <span>Copy</span>
              </>
            )}
          </button>

          <div className="h-4 w-px bg-white/10 mx-1" />

          {/* Linux/Native Window Controls */}
          <button
            onClick={() => setIsMaximized(!isMaximized)}
            title={isMaximized ? 'Restore down' : 'Maximize terminal'}
            className="p-1 rounded hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
          >
            {isMaximized ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>

      {/* Terminal Viewport */}
      {activeTab === 'interactive' ? (
        <div
          className={`flex flex-col bg-[#05080e] ${isMaximized ? 'flex-1 min-h-0' : 'h-[500px]'}`}
          onClick={handleTerminalAreaClick}
        >
          {/* Quick Preset Buttons (All original buttons preserved with authentic chip styling) */}
          <div className="flex items-center space-x-1.5 px-3 py-2 bg-[#0a0f19] border-b border-[#162032] overflow-x-auto text-[11px] shrink-0 custom-scrollbar select-none">
            <span className="text-gray-500 font-bold uppercase text-[9px] tracking-wider px-1 shrink-0">
              Quick Run:
            </span>
            {QUICK_BUTTONS.map((btn) => (
              <button
                key={btn.label}
                onClick={(e) => {
                  e.stopPropagation();
                  executeCommand(btn.label);
                }}
                title={btn.desc}
                className="px-2.5 py-1 rounded bg-[#121a29] hover:bg-cyan-950/60 border border-[#1e2c44] hover:border-cyan-500/50 text-gray-300 hover:text-cyan-300 shrink-0 font-mono text-[11px] transition-all cursor-pointer shadow-sm active:scale-95"
              >
                {btn.label}
              </button>
            ))}
          </div>

          {/* Interactive Screen Scroll Output */}
          <div
            ref={scrollRef}
            className="flex-1 overflow-y-auto p-4 space-y-3 font-mono leading-relaxed custom-scrollbar cursor-text"
          >
            {commandLogs.map((log, idx) => (
              <div key={idx} className="space-y-1">
                {/* Command Header line */}
                {log.command !== 'system::init' && (
                  <div className="flex items-center space-x-2 text-[12px] select-text">
                    <span className="text-emerald-400 font-bold">
                      {promptUser}@synevid<span className="text-gray-500">:</span>
                      <span className="text-sky-400">{promptPath}</span>
                      <span className="text-amber-400 font-normal"> ({promptBranch})</span>$
                    </span>
                    <span className="text-white font-bold">{log.command}</span>
                    <span className="text-[10px] text-gray-600 ml-auto select-none font-mono">
                      {log.timestamp}
                    </span>
                  </div>
                )}

                {/* Stdout Output with Real ANSI Formatting */}
                {log.stdout && (
                  <pre className="text-gray-200 whitespace-pre-wrap text-[11px] leading-relaxed pl-2 border-l-2 border-cyan-500/40 select-text font-mono">
                    {renderAnsiText(log.stdout)}
                  </pre>
                )}

                {/* Stderr Output */}
                {log.stderr && (
                  <pre className="text-rose-400 whitespace-pre-wrap text-[11px] leading-relaxed pl-2 border-l-2 border-rose-500/50 select-text font-mono">
                    {renderAnsiText(log.stderr)}
                  </pre>
                )}
              </div>
            ))}

            {isExecuting && (
              <div className="flex items-center space-x-2 text-cyan-400 text-[11px] pl-2 font-mono">
                <span className="inline-block w-2 h-2 bg-cyan-400 rounded-full animate-ping" />
                <span className="animate-pulse">Processing in pure-Rust sandbox (INV-SANDBOX-003)...</span>
              </div>
            )}
          </div>

          {/* Live Prompt Input Bar */}
          <div className="px-4 py-2.5 bg-[#0a0f19] border-t border-[#162032] flex items-center space-x-2 shrink-0">
            <span className="text-emerald-400 font-bold shrink-0 text-xs">
              {promptUser}@synevid<span className="text-gray-500">:</span>
              <span className="text-sky-400">{promptPath}</span>
              <span className="text-amber-400 font-normal"> ({promptBranch})</span>$
            </span>

            <div className="flex-1 relative flex items-center">
              <input
                ref={inputRef}
                type="text"
                value={inputCommand}
                onChange={(e) => setInputCommand(e.target.value)}
                onKeyDown={handleKeyDown}
                disabled={isExecuting}
                placeholder="Type command (git status, git commit -m '...', autopsy verify)..."
                className="w-full bg-transparent text-gray-100 placeholder-gray-600 focus:outline-none font-mono text-xs caret-cyan-400"
                autoFocus
                spellCheck={false}
                autoComplete="off"
                autoCapitalize="off"
              />
            </div>

            <button
              onClick={() => executeCommand(inputCommand)}
              disabled={isExecuting || !inputCommand.trim()}
              title="Execute command (Enter)"
              className="px-3 py-1 rounded bg-cyan-600 hover:bg-cyan-500 text-white font-bold transition-all disabled:opacity-30 cursor-pointer flex items-center space-x-1 shrink-0 text-[11px]"
            >
              <Send className="w-3 h-3" />
              <span>Run</span>
            </button>
          </div>
        </div>
      ) : (
        /* Reference Guide View */
        <div
          className={`p-6 font-mono text-xs leading-relaxed flex flex-col bg-[#05080e] ${
            isMaximized ? 'flex-1 min-h-0' : 'h-[500px]'
          }`}
        >
          <div className="flex items-center text-gray-200 mb-4 pb-3 border-b border-[#162032] shrink-0">
            <span className="text-emerald-400 font-semibold mr-2">$</span>
            <span className="text-white font-medium select-all truncate">{currentTab.cmd}</span>
            <span className="ml-auto text-xs px-2.5 py-0.5 rounded border border-white/10 text-cyan-400 shrink-0 ml-3 bg-cyan-950/20">
              {currentTab.statusText}
            </span>
          </div>

          <div className="flex-1 overflow-y-auto overflow-x-auto pr-3 custom-scrollbar">
            <AnimatePresence mode="wait">
              <motion.pre
                key={activeTab}
                initial={{ opacity: 0, y: 5 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -5 }}
                transition={{ duration: 0.15 }}
                className="text-gray-300 whitespace-pre font-mono text-xs leading-relaxed"
              >
                {renderAnsiText(currentTab.output)}
              </motion.pre>
            </AnimatePresence>
          </div>
        </div>
      )}

      {/* Industrial Bottom Terminal Telemetry Statusline */}
      <div className="px-4 py-2 bg-[#080d16] border-t border-[#162032] flex flex-wrap items-center justify-between text-[11px] font-mono text-gray-400 shrink-0 select-none">
        <div className="flex items-center space-x-4">
          <span className="flex items-center text-cyan-400">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 mr-1.5 animate-pulse" />
            100/100 BLAKE3 Determinism
          </span>
          <span className="text-gray-600">&bull;</span>
          <span>Zero Sockets</span>
          <span className="text-gray-600">&bull;</span>
          <span className="text-emerald-400/90 font-medium">In-Process Sandbox (INV-SANDBOX-003)</span>
          <span className="text-gray-600">&bull;</span>
          <span className="text-gray-500">UTF-8 / PTY</span>
        </div>

        <div className="flex items-center space-x-3 text-gray-400">
          <span>
            Exit Code:{' '}
            <strong
              className={
                (activeTab === 'interactive'
                  ? commandLogs.length > 0
                    ? commandLogs[commandLogs.length - 1].exitCode
                    : 0
                  : currentTab.exitCode) === 0
                  ? 'text-emerald-400'
                  : 'text-rose-400'
              }
            >
              {activeTab === 'interactive'
                ? commandLogs.length > 0
                  ? commandLogs[commandLogs.length - 1].exitCode
                  : 0
                : currentTab.exitCode}
            </strong>
          </span>
        </div>
      </div>
    </div>
  );
}
