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
  Terminal,
  BookOpen,
} from 'lucide-react';
import { useTheme } from '@/context/ThemeContext';
import { RepoRecord } from '@/lib/api';

interface CockpitHeaderProps {
  repos: RepoRecord[];
  activeRepoId: string;
  onSelectRepo: (id: string) => void;
  onOpenConnectModal: () => void;
  onOpenCapabilitiesModal: () => void;
  onOpenTerminal?: () => void;
  onOpenManual?: () => void;
  onRefresh: () => void;
  liveSeq: number;
  isLiveConnected: boolean;
  coverageState: string;
  invariantsPass: boolean;
  activeWorkspaceName?: string;
}

export default function CockpitHeader({
  repos,
  activeRepoId,
  onSelectRepo,
  onOpenConnectModal,
  onOpenCapabilitiesModal,
  onOpenTerminal,
  onOpenManual,
  onRefresh,
  liveSeq,
  isLiveConnected,
  coverageState,
  invariantsPass,
  activeWorkspaceName,
}: CockpitHeaderProps) {
  const { theme, toggleTheme } = useTheme();

  return (
    <header className="z-40 backdrop-blur-md transition-colors bg-[var(--card)] border border-[var(--border-subtle)] rounded-2xl px-5 py-2.5 flex items-center justify-between shadow-sm shrink-0 select-none">
      {/* Brand & Repo Selector */}
      <div className="flex items-center space-x-6">
        <div className="flex items-center space-x-3">
          {/* Official SynEvid Brand Logo */}
          <div className="relative group cursor-pointer flex items-center">
            <div className="w-10 h-10 rounded-xl overflow-hidden border border-[var(--border-subtle)] bg-[var(--card-elevated)] p-1.5 shadow-sm transition-all duration-200 group-hover:border-cyan-500/50 group-hover:shadow-[0_0_15px_rgba(0,240,255,0.35)] flex items-center justify-center">
              <img
                src="/assets/logo.svg"
                alt="SynEvid Logo"
                className="w-full h-full object-contain filter drop-shadow-[0_0_4px_rgba(0,240,255,0.4)] transition-transform duration-200 group-hover:scale-105"
              />
            </div>
            <span
              className="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full bg-cyan-400 border-2 border-[var(--card)] animate-pulse shadow-sm"
              title="SynEvid Deterministic Engine Online"
            />
          </div>
          <div className="flex flex-col">
            <div className="flex items-center space-x-2">
              <span className="font-bold text-base tracking-tight text-[var(--text-primary)] font-brand">
                SYNEVID
              </span>
              <span className="text-[10px] font-mono px-2 py-0.5 rounded-lg bg-sky-50 dark:bg-cyan-500/15 text-sky-700 dark:text-cyan-400 font-semibold border border-sky-200 dark:border-cyan-500/30">
                v0.5.0
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
          {(() => {
            const userRepos = repos.filter(
              (r) => !r.root_path?.endsWith('SynEvid') && !r.display_name?.toLowerCase().includes('synevid')
            );
            return (
              <select
                value={activeRepoId}
                onChange={(e) => onSelectRepo(e.target.value)}
                disabled={userRepos.length === 0}
                className="text-xs font-mono font-medium rounded-xl px-3 py-1.5 bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500 cursor-pointer disabled:opacity-50"
              >
                {userRepos.length === 0 ? (
                  <option value="">No repository connected</option>
                ) : (
                  userRepos.map((r) => {
                    const cleanedName =
                      r.display_name && r.display_name !== 'repository'
                        ? r.display_name
                        : r.root_path?.split(/[\/\\]/).filter(Boolean).pop() || 'Repository';
                    return (
                      <option key={r.repo_id} value={r.repo_id}>
                        {cleanedName} ({r.repo_id.substring(0, 8)})
                      </option>
                    );
                  })
                )}
              </select>
            );
          })()}
          <button
            onClick={onOpenConnectModal}
            className="text-xs font-semibold px-2.5 py-1.5 rounded-xl bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/10 text-[var(--text-secondary)] hover:text-sky-700 dark:hover:text-cyan-400 border border-[var(--border-subtle)] transition-colors flex items-center space-x-1 cursor-pointer"
            title="Connect local folder repository"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Connect</span>
          </button>
        </div>
      </div>

      {/* Telemetry Status & Controls (Rounded Square Badges & Buttons) */}
      <div className="flex items-center space-x-2.5">
        {/* Real-time SSE Live Indicator */}
        <div
          className={`flex items-center space-x-2 px-3 py-1.5 rounded-xl text-xs font-mono border transition-all ${
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

        {/* Semantic Coverage Badge */}
        <div className="hidden sm:flex items-center space-x-1.5 px-3 py-1.5 rounded-xl text-xs font-mono font-semibold bg-sky-50 dark:bg-cyan-500/10 border border-sky-200 dark:border-cyan-500/30 text-sky-700 dark:text-cyan-400">
          <ShieldCheck className="w-3.5 h-3.5" />
          <span>COVERAGE: {coverageState.toUpperCase()}</span>
        </div>

        {/* Verification Status Badge */}
        <div
          className={`hidden sm:flex items-center space-x-1.5 px-3 py-1.5 rounded-xl text-xs font-mono font-semibold border ${
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

        {/* Interactive Terminal Workbench Button */}
        {onOpenTerminal && (
          <button
            onClick={onOpenTerminal}
            className="p-2 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-cyan-400 hover:border-cyan-500/30 transition-colors cursor-pointer"
            title="Open Interactive Native Terminal (Alt+7)"
          >
            <Terminal className="w-4 h-4" />
          </button>
        )}

        {/* Workspace Manual / Guide Button */}
        {onOpenManual && (
          <button
            onClick={onOpenManual}
            className="px-2.5 py-1.5 rounded-xl bg-cyan-500/10 hover:bg-cyan-500/20 border border-cyan-500/30 text-cyan-400 font-semibold text-xs font-mono transition-all flex items-center space-x-1.5 cursor-pointer shadow-xs group"
            title={`Open User Manual for ${activeWorkspaceName || 'Current Workspace'}`}
          >
            <BookOpen className="w-3.5 h-3.5 group-hover:scale-110 transition-transform" />
            <span className="hidden md:inline">Manual</span>
          </button>
        )}

        {/* Capabilities Button */}
        <button
          onClick={onOpenCapabilitiesModal}
          className="p-2 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          title="Inspect registered plugins & capabilities"
        >
          <Sliders className="w-4 h-4" />
        </button>

        {/* Refresh Button */}
        <button
          onClick={onRefresh}
          className="p-2 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          title="Re-fetch latest snapshot telemetry"
        >
          <RefreshCw className="w-4 h-4" />
        </button>

        {/* Dark / Light Theme Toggle */}
        <button
          onClick={toggleTheme}
          className="p-2 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          title={`Switch to ${theme === 'dark' ? 'Light' : 'Dark'} Mode`}
        >
          {theme === 'dark' ? <Sun className="w-4 h-4 text-amber-400" /> : <Moon className="w-4 h-4 text-indigo-600" />}
        </button>
      </div>
    </header>
  );
}
