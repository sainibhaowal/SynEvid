'use client';

import React from 'react';
import {
  ShieldCheck,
  Cpu,
  Layers,
  FolderGit2,
  Radio,
  Sun,
  Moon,
  Plus,
  RefreshCw,
  Sliders,
  CheckCircle2,
  AlertTriangle,
} from 'lucide-react';
import { useTheme } from '@/context/ThemeContext';
import { RepoRecord } from '@/lib/api';

interface CockpitHeaderProps {
  repos: RepoRecord[];
  activeRepoId: string;
  onSelectRepo: (id: string) => void;
  onOpenConnectModal: () => void;
  onOpenCapabilitiesModal: () => void;
  onRefresh: () => void;
  liveSeq: number;
  isLiveConnected: boolean;
  coverageState: string;
  invariantsPass: boolean;
}

export default function CockpitHeader({
  repos,
  activeRepoId,
  onSelectRepo,
  onOpenConnectModal,
  onOpenCapabilitiesModal,
  onRefresh,
  liveSeq,
  isLiveConnected,
  coverageState,
  invariantsPass,
}: CockpitHeaderProps) {
  const { theme, toggleTheme } = useTheme();

  return (
    <header className="sticky top-0 z-50 backdrop-blur-md border-b transition-colors bg-[var(--card)]/90 border-[var(--border-subtle)] px-6 py-3.5 flex items-center justify-between shadow-sm">
      {/* Brand & Repo Selector */}
      <div className="flex items-center space-x-6">
        <div className="flex items-center space-x-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-cyan-500 to-indigo-600 flex items-center justify-center text-white font-bold text-sm shadow-md">
            SE
          </div>
          <div className="flex flex-col">
            <div className="flex items-center space-x-2">
              <span className="font-bold text-base tracking-tight text-[var(--text-primary)] font-brand">
                SYNEVID
              </span>
              <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 font-semibold border border-cyan-500/30">
                v0.2.1
              </span>
            </div>
            <span className="text-[10px] uppercase tracking-wider text-[var(--text-muted)] font-mono font-medium">
              Offline Verification &amp; Reactive Cockpit
            </span>
          </div>
        </div>

        {/* Repository Switcher */}
        <div className="hidden lg:flex items-center space-x-2 pl-4 border-l border-[var(--border-subtle)]">
          <FolderGit2 className="w-4 h-4 text-[var(--text-muted)]" />
          <span className="text-xs font-medium text-[var(--text-secondary)]">Repo:</span>
          <select
            value={activeRepoId}
            onChange={(e) => onSelectRepo(e.target.value)}
            className="text-xs font-mono font-medium rounded-md px-2.5 py-1.5 bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500 cursor-pointer"
          >
            {repos.map((r) => (
              <option key={r.repo_id} value={r.repo_id}>
                {r.display_name} ({r.repo_id.substring(0, 8)})
              </option>
            ))}
          </select>
          <button
            onClick={onOpenConnectModal}
            className="text-xs font-semibold px-2 py-1.5 rounded-md bg-[var(--card-elevated)] hover:bg-cyan-500/10 text-[var(--text-secondary)] hover:text-cyan-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] transition-colors flex items-center space-x-1"
            title="Connect local folder repository"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Connect</span>
          </button>
        </div>
      </div>

      {/* Telemetry Status & Controls */}
      <div className="flex items-center space-x-3">
        {/* Real-time SSE Live Indicator */}
        <div
          className={`flex items-center space-x-2 px-2.5 py-1 rounded-full text-xs font-mono border transition-all ${
            isLiveConnected
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-600 dark:text-emerald-400'
              : 'bg-amber-500/10 border-amber-500/30 text-amber-600 dark:text-amber-400'
          }`}
          title="Bi-directional live event bus connection status"
        >
          <span
            className={`w-2 h-2 rounded-full ${
              isLiveConnected ? 'bg-emerald-500 animate-pulse' : 'bg-amber-500'
            }`}
          />
          <span className="font-semibold">{isLiveConnected ? `LIVE #${liveSeq}` : 'CONNECTING'}</span>
        </div>

        {/* Semantic Coverage Pill */}
        <div className="hidden sm:flex items-center space-x-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-semibold bg-cyan-500/10 border border-cyan-500/30 text-cyan-600 dark:text-cyan-400">
          <ShieldCheck className="w-3.5 h-3.5" />
          <span>COVERAGE: {coverageState.toUpperCase()}</span>
        </div>

        {/* Verification Status Pill */}
        <div
          className={`hidden sm:flex items-center space-x-1.5 px-2.5 py-1 rounded-full text-xs font-mono font-semibold border ${
            invariantsPass
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-600 dark:text-emerald-400'
              : 'bg-rose-500/10 border-rose-500/30 text-rose-600 dark:text-rose-400'
          }`}
        >
          {invariantsPass ? (
            <>
              <CheckCircle2 className="w-3.5 h-3.5" />
              <span>PASS</span>
            </>
          ) : (
            <>
              <AlertTriangle className="w-3.5 h-3.5" />
              <span>FAIL</span>
            </>
          )}
        </div>

        {/* Capabilities Button */}
        <button
          onClick={onOpenCapabilitiesModal}
          className="p-2 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          title="Inspect registered plugins & capabilities"
        >
          <Sliders className="w-4 h-4" />
        </button>

        {/* Refresh Button */}
        <button
          onClick={onRefresh}
          className="p-2 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          title="Re-fetch latest snapshot telemetry"
        >
          <RefreshCw className="w-4 h-4" />
        </button>

        {/* Dark / Light Theme Toggle */}
        <button
          onClick={toggleTheme}
          className="p-2 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          title={`Switch to ${theme === 'dark' ? 'Light' : 'Dark'} Mode`}
        >
          {theme === 'dark' ? <Sun className="w-4 h-4 text-amber-400" /> : <Moon className="w-4 h-4 text-indigo-600" />}
        </button>
      </div>
    </header>
  );
}
