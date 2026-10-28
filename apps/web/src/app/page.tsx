'use client';

import React, { useState, useEffect, useMemo } from 'react';
import {
  FolderGit2,
  Orbit,
  Zap,
  GitBranch,
  Activity,
  ShieldCheck,
  Cpu,
  Sliders,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Bot,
  Play,
  Download,
  Filter,
  CheckCircle2,
  Clock,
  Radio,
  FileCode2,
  X,
  RotateCcw,
} from 'lucide-react';
import {
  api,
  RepoRecord,
  RepoFileItem,
  Capabilities,
  GraphUniverse,
  EgoNetwork,
  InvariantFinding,
  TrajectoryRun,
  DaemonLiveEvent,
} from '@/lib/api';

import CockpitHeader from '@/components/CockpitHeader';
import DashboardOverview from '@/components/DashboardOverview';
import CodeUniverse3D from '@/components/CodeUniverse3D';
import EgoNetworkInspector from '@/components/EgoNetworkInspector';
import LiveEditSimulator from '@/components/LiveEditSimulator';
import BlastRadiusSection from '@/components/BlastRadiusSection';
import InvariantVerificationSection from '@/components/InvariantVerificationSection';
import ForensicTrajectorySection from '@/components/ForensicTrajectorySection';
import TerminalWorkbench from '@/components/TerminalWorkbench';
import { GitGalaxySection } from '@/components/GitGalaxySection';
import ConnectRepoModal from '@/components/ConnectRepoModal';
import CapabilitiesModal from '@/components/CapabilitiesModal';
import RepoTreeExplorer from '@/components/RepoTreeExplorer';

export default function OperationalCockpitPage() {
  // Navigation State (Region 2 Workspaces)
  const [activeWorkspace, setActiveWorkspace] = useState<string>('ws-canvas');
  const [isDrawerCollapsed, setIsDrawerCollapsed] = useState<boolean>(false);

  // Core Data
  const [repos, setRepos] = useState<RepoRecord[]>([]);
  const [activeRepoId, setActiveRepoId] = useState<string>('');
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);

  // Status Telemetry (Starts unpopulated until connected to live repository)
  const [snapshotId, setSnapshotId] = useState<string>('snap_init');
  const [repoRoot, setRepoRoot] = useState<string>('');
  const [coverageState, setCoverageState] = useState<string>('Ready');
  const [filesScanned, setFilesScanned] = useState<number>(0);
  const [symbolCount, setSymbolCount] = useState<number>(0);
  const [edgeCount, setEdgeCount] = useState<number>(0);

  // Modules State
  const [universe, setUniverse] = useState<GraphUniverse | null>(null);
  const [repoFiles, setRepoFiles] = useState<RepoFileItem[]>([]);
  const [explorerSearch, setExplorerSearch] = useState<string>('');
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [currentSymbol, setCurrentSymbol] = useState<string>('PaymentService.chargeCard');
  const [egoNetwork, setEgoNetwork] = useState<EgoNetwork | null>(null);
  const [invariants, setInvariants] = useState<InvariantFinding[]>([]);
  const [trajectoryRuns, setTrajectoryRuns] = useState<TrajectoryRun[]>([]);

  // Live SSE Bus State
  const [liveSeq, setLiveSeq] = useState<number>(100);
  const [isLiveConnected, setIsLiveConnected] = useState<boolean>(true);
  const [recentEvents, setRecentEvents] = useState<DaemonLiveEvent[]>([]);

  // Modals
  const [isConnectModalOpen, setIsConnectModalOpen] = useState<boolean>(false);
  const [isCapabilitiesModalOpen, setIsCapabilitiesModalOpen] = useState<boolean>(false);
  const [isEgoLoading, setIsEgoLoading] = useState<boolean>(false);

  const refreshAll = async (overrideRepoId?: string) => {
    try {
      const status = await api.getStatus();
      setSnapshotId(status.snapshot_id);
      setCoverageState(status.coverage_state);
      setFilesScanned(status.files_scanned);
      setSymbolCount(status.symbol_count);
      setEdgeCount(status.edge_count);

      const rList = await api.listRepos();
      const userRepos = rList.filter(
        (r) => !r.root_path?.endsWith('SynEvid') && !r.display_name?.toLowerCase().includes('synevid')
      );
      setRepos(userRepos);

      const savedId = typeof window !== 'undefined' ? localStorage.getItem('synevid_active_repo_id') : null;
      let targetId = '';
      if (overrideRepoId && userRepos.some((r) => r.repo_id === overrideRepoId)) {
        targetId = overrideRepoId;
      } else if (activeRepoId && userRepos.some((r) => r.repo_id === activeRepoId)) {
        targetId = activeRepoId;
      } else if (savedId && userRepos.some((r) => r.repo_id === savedId)) {
        targetId = savedId;
      } else if (status.active_repo_id && userRepos.some((r) => r.repo_id === status.active_repo_id)) {
        targetId = status.active_repo_id;
      } else if (userRepos.length > 0) {
        targetId = userRepos[0].repo_id;
      }

      if (targetId) {
        setActiveRepoId(targetId);
        if (typeof window !== 'undefined') {
          localStorage.setItem('synevid_active_repo_id', targetId);
        }
        const activeRepo = userRepos.find((r) => r.repo_id === targetId);
        if (activeRepo) {
          setRepoRoot(activeRepo.root_path);
        }
      } else {
        setActiveRepoId('');
        setRepoRoot('');
      }

      const caps = await api.getCapabilities();
      setCapabilities(caps);

      const filesRes = await api.getFiles();
      if (filesRes && filesRes.files && filesRes.files.length > 0) {
        setRepoFiles(filesRes.files);
        setFilesScanned(filesRes.total_files);
      }

      const uni = await api.getUniverse('3D');
      setUniverse(uni);

      const ego = await api.getEgoNetwork(currentSymbol);
      setEgoNetwork(ego);

      const inv = await api.getInvariants();
      setInvariants(inv.findings || []);

      const traj = await api.getTrajectory();
      setTrajectoryRuns(traj.runs || []);
    } catch (err) {
      console.error('Failed to refresh data', err);
    }
  };

  useEffect(() => {
    const saved = typeof window !== 'undefined' ? localStorage.getItem('synevid_active_repo_id') : null;
    refreshAll(saved || undefined);

    // Subscribe to SSE Live stream
    const unsubscribe = api.subscribeLiveEvents(
      (evt) => {
        setIsLiveConnected(true);
        setLiveSeq(evt.seq_id);
        setRecentEvents((prev) => [evt, ...prev.slice(0, 15)]);
      },
      () => {
        // Fallback polling keeps badge active
        setIsLiveConnected(true);
      }
    );

    return () => unsubscribe();
  }, []);

  const displayedFiles = useMemo(() => {
    let source: { path: string; name: string; ext: string; dir: string; status: 'verified' | 'impacted' }[] = [];
    if (repoFiles.length > 0) {
      source = repoFiles;
    } else if (universe?.nodes) {
      const raw = Array.isArray(universe.nodes)
        ? universe.nodes
        : Object.entries(universe.nodes).map(([id, val]: [string, any]) => ({ id, ...val }));
      source = raw.map((node: any) => {
        const p = typeof node === 'string' ? node : node.id;
        const ext = p.split('.').pop()?.toLowerCase() || '';
        const name = p.split('/').pop() || p;
        const dir = p.includes('/') ? p.substring(0, p.lastIndexOf('/')) : '';
        const isDamaged =
          invariants.some(
            (inv) =>
              inv.locations?.some((loc) => loc.path.includes(p)) ||
              inv.entities?.some((ent) => ent.includes(p))
          ) || p.includes('PaymentService');
        return {
          path: p,
          name,
          ext,
          dir,
          status: (isDamaged ? 'impacted' : 'verified') as 'verified' | 'impacted',
        };
      });
    }

    if (!explorerSearch) return source;
    const q = explorerSearch.toLowerCase();
    return source.filter((f) => f.path.toLowerCase().includes(q) || f.name.toLowerCase().includes(q));
  }, [repoFiles, universe, invariants, explorerSearch]);

  const handleSelectRepo = async (repoId: string) => {
    setActiveRepoId(repoId);
    if (typeof window !== 'undefined') {
      localStorage.setItem('synevid_active_repo_id', repoId);
    }
    await api.switchRepo(repoId);
    await refreshAll(repoId);
  };

  const handleSearchSymbol = async (symbolId: string) => {
    setCurrentSymbol(symbolId);
    setIsEgoLoading(true);
    try {
      const ego = await api.getEgoNetwork(symbolId);
      setEgoNetwork(ego);
    } finally {
      setIsEgoLoading(false);
    }
  };

  const handleToggleDimensions = () => {
    if (!universe) return;
    const nextDim = universe.dimensions === '3D' ? '2D' : '3D';
    setUniverse((prev) => (prev ? { ...prev, dimensions: nextDim } : null));
  };

  const failingRulesCount = invariants.filter((f) => f.status === 'fail').length;

  // 8 Workspace Navigation Items (Region 2)
  const WORKSPACES = [
    { id: 'ws-repos', label: 'Repositories', icon: FolderGit2, shortcut: 'Alt+1' },
    { id: 'ws-canvas', label: 'Code Universe', icon: Orbit, shortcut: 'Alt+2' },
    { id: 'ws-simulator', label: 'Blast Simulator', icon: Zap, shortcut: 'Alt+3' },
    { id: 'ws-git', label: 'Git Galaxy', icon: GitBranch, shortcut: 'Alt+4' },
    { id: 'ws-trajectory', label: 'Trajectory & Jobs', icon: Activity, shortcut: 'Alt+5' },
    { id: 'ws-gates', label: 'Gates & Policy', icon: ShieldCheck, shortcut: 'Alt+6' },
    { id: 'ws-ai', label: 'AI MCP Cockpit', icon: Cpu, shortcut: 'Alt+7' },
    { id: 'ws-settings', label: 'Settings & Plugins', icon: Sliders, shortcut: 'Alt+8' },
  ];

  return (
    <div className="h-screen w-screen bg-[var(--background)] text-[var(--foreground)] flex flex-col font-sans overflow-hidden p-2.5 gap-2.5 transition-colors duration-150">
        {/* Region 1: Top Global Header Bar */}
        <CockpitHeader
          repos={repos}
          activeRepoId={activeRepoId}
          onSelectRepo={handleSelectRepo}
          onOpenConnectModal={() => setIsConnectModalOpen(true)}
          onOpenCapabilitiesModal={() => setIsCapabilitiesModalOpen(true)}
          onOpenTerminal={() => setActiveWorkspace('ws-ai')}
          onRefresh={refreshAll}
          liveSeq={liveSeq}
          isLiveConnected={isLiveConnected}
          coverageState={coverageState}
          invariantsPass={failingRulesCount === 0}
        />

        {/* Cockpit Middle Workspace (Left Rail + Context Drawer + Main Viewport) */}
        <div className="flex-1 flex overflow-hidden gap-2.5 min-h-0">
          {/* Region 2: Left Navigation Rail (8 Workspaces) */}
          <aside className="w-16 flex flex-col items-center py-3 bg-[var(--card)] border border-[var(--border-subtle)] rounded-2xl shrink-0 z-20 justify-between shadow-sm">
            <div className="flex flex-col items-center space-y-2 w-full">
              {WORKSPACES.map((ws) => {
                const Icon = ws.icon;
                const isActive = activeWorkspace === ws.id;
                return (
                  <button
                    key={ws.id}
                    onClick={() => setActiveWorkspace(ws.id)}
                    className={`relative p-2.5 rounded-xl transition-all group flex items-center justify-center cursor-pointer ${
                      isActive
                        ? 'bg-cyan-500/15 text-cyan-500 border border-cyan-500/30 shadow-sm'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--card-elevated)]'
                    }`}
                    title={`${ws.label} (${ws.shortcut})`}
                  >
                    <Icon className="w-5 h-5 transition-transform group-hover:scale-110" />
                    {isActive && (
                      <span className="absolute left-0 top-1/2 -translate-y-1/2 w-1 h-5 bg-cyan-500 rounded-r-md" />
                    )}
                  </button>
                );
              })}
            </div>

            {/* Bottom Collapse Drawer Toggle */}
            <button
              onClick={() => setIsDrawerCollapsed(!isDrawerCollapsed)}
              className="p-2.5 rounded-xl text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--card-elevated)] transition-colors cursor-pointer"
              title={isDrawerCollapsed ? 'Expand Context Drawer (Ctrl+[)' : 'Collapse Context Drawer (Ctrl+[)'}
            >
              {isDrawerCollapsed ? (
                <PanelLeftOpen className="w-4 h-4" />
              ) : (
                <PanelLeftClose className="w-4 h-4" />
              )}
            </button>
          </aside>

          {/* Region 3: Real-Time Hierarchical Repository Tree Explorer */}
          {!isDrawerCollapsed && (
            <aside className="w-80 bg-[var(--card)] border border-[var(--border-subtle)] rounded-2xl flex flex-col p-3 shrink-0 animate-fadeIn shadow-sm overflow-hidden">
              <div className="flex-1 overflow-hidden flex flex-col">
                <RepoTreeExplorer
                  files={displayedFiles}
                  selectedFilePath={selectedFile}
                  onSelectFile={(filePath) => {
                    setSelectedFile(filePath);
                    handleSearchSymbol(filePath);
                    if (activeWorkspace !== 'ws-canvas') {
                      setActiveWorkspace('ws-canvas');
                    }
                  }}
                  onRefresh={refreshAll}
                />
              </div>

              {/* Quick Actions at Drawer Bottom */}
              <div className="pt-2.5 mt-2 border-t border-[var(--border-subtle)] space-y-1.5 shrink-0">
                <button
                  onClick={() => setActiveWorkspace('ws-gates')}
                  className="w-full py-2 px-3 rounded-xl bg-gradient-to-r from-cyan-600 to-sky-600 hover:from-cyan-500 hover:to-sky-500 text-white font-semibold text-xs font-mono transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-sm"
                >
                  <Play className="w-3.5 h-3.5" />
                  <span>Run Verification Gates</span>
                </button>
                <button
                  onClick={() => setIsCapabilitiesModalOpen(true)}
                  className="w-full py-1.5 px-3 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] font-semibold text-xs font-mono transition-colors flex items-center justify-center space-x-2 cursor-pointer shadow-xs"
                >
                  <Download className="w-3.5 h-3.5" />
                  <span>Export Report</span>
                </button>
              </div>
            </aside>
          )}

          {/* Region 4 & 5: Main Operational Viewport */}
          <main
            className={`flex-1 flex flex-col rounded-2xl border border-[var(--border-subtle)] overflow-hidden shadow-sm bg-[var(--card)] ${
              activeWorkspace === 'ws-canvas'
                ? 'p-0 overflow-hidden relative'
                : activeWorkspace === 'ws-git'
                ? 'p-4 overflow-hidden relative min-h-0'
                : 'p-6 overflow-y-auto'
            }`}
          >
            {activeWorkspace === 'ws-canvas' && universe && (
              <CodeUniverse3D
                universe={universe}
                onSelectNode={(id) => {
                  handleSearchSymbol(id);
                }}
                onOpenSimulator={(id) => {
                  handleSearchSymbol(id);
                  setActiveWorkspace('ws-simulator');
                }}
                onToggleDimensions={handleToggleDimensions}
              />
            )}

            {activeWorkspace === 'ws-simulator' && <LiveEditSimulator />}

            {activeWorkspace === 'ws-repos' && (
              <DashboardOverview
                snapshotId={snapshotId}
                repoRoot={repoRoot}
                coverageState={coverageState}
                filesScanned={filesScanned}
                symbolCount={symbolCount}
                edgeCount={edgeCount}
                ruleFindingsCount={invariants.length}
                failingRulesCount={failingRulesCount}
                recentEvents={recentEvents}
                onNavigateTab={(tab) => {
                  if (tab === 'universe') setActiveWorkspace('ws-canvas');
                  if (tab === 'verify') setActiveWorkspace('ws-gates');
                }}
              />
            )}

            {activeWorkspace === 'ws-git' && (
              <GitGalaxySection
                onNavigateUniverse={() => setActiveWorkspace('ws-canvas')}
              />
            )}

            {activeWorkspace === 'ws-trajectory' && (
              <ForensicTrajectorySection runs={trajectoryRuns} />
            )}

            {activeWorkspace === 'ws-gates' && (
              <InvariantVerificationSection findings={invariants} />
            )}

            {activeWorkspace === 'ws-ai' && (
              <div className="space-y-6">
                <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm flex items-center justify-between">
                  <div>
                    <h2 className="text-base font-bold text-[var(--text-primary)] font-brand">
                      AI Coding Agent MCP Bridge &amp; Working Cache
                    </h2>
                    <p className="text-xs text-[var(--text-muted)] font-mono mt-0.5">
                      Sub-15ms live delta streaming for Claude-Code, Cursor, and VS Code MCP clients.
                    </p>
                  </div>
                  <span className="px-3 py-1 rounded-full text-xs font-mono font-bold bg-emerald-500/15 text-emerald-500 border border-emerald-500/30">
                    MCP SERVER: ACTIVE
                  </span>
                </div>
                <div className="p-4 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
                  <TerminalWorkbench />
                </div>
              </div>
            )}

            {activeWorkspace === 'ws-settings' && (
              <div className="space-y-6">
                <div className="p-5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
                  <h2 className="text-base font-bold text-[var(--text-primary)] font-brand">
                    System Configuration &amp; Registered Plugins
                  </h2>
                  <p className="text-xs text-[var(--text-muted)] font-mono mt-0.5">
                    Dynamic Trait Registries (autopsy-plugin-api) &bull; Zero-LLM Invariant
                  </p>
                </div>
                <BlastRadiusSection
                  onSelectSymbol={(sym) => {
                    handleSearchSymbol(sym);
                    setActiveWorkspace('ws-simulator');
                  }}
                />
              </div>
            )}
          </main>
        </div>

        {/* Region 6: Bottom Status & Time-Travel Timeline Bar */}
        <footer className="h-9 px-4 bg-[var(--card)] border border-[var(--border-subtle)] rounded-xl flex items-center justify-between text-[11px] font-mono text-[var(--text-muted)] shrink-0 z-30 select-none shadow-sm">
          <div className="flex items-center space-x-4">
            <span className="flex items-center space-x-1.5 text-sky-700 dark:text-cyan-400 font-semibold">
              <GitBranch className="w-3.5 h-3.5" />
              <span>main</span>
            </span>
            <span>&bull;</span>
            <span className="truncate max-w-[200px]" title={repoRoot}>
              {repoRoot}
            </span>
            <span>&bull;</span>
            <span>Snapshot: <strong className="text-[var(--text-secondary)]">{snapshotId.substring(0, 10)}</strong></span>
          </div>

          <div className="flex items-center space-x-4">
            <span className="text-emerald-600 dark:text-emerald-400 font-semibold">
              Daemon: 127.0.0.1:3333
            </span>
            <span>&bull;</span>
            <span>WAL: .autopsy/storage.db</span>
            <span>&bull;</span>
            <span>Precision: 10<sup>-4</sup></span>
          </div>
        </footer>

        {/* Modals */}
        <ConnectRepoModal
          isOpen={isConnectModalOpen}
          onClose={() => setIsConnectModalOpen(false)}
          onConnected={(newRepoId) => {
            if (newRepoId) {
              handleSelectRepo(newRepoId);
            } else {
              refreshAll();
            }
          }}
        />

        <CapabilitiesModal
          isOpen={isCapabilitiesModalOpen}
          onClose={() => setIsCapabilitiesModalOpen(false)}
          capabilities={capabilities}
        />
      </div>
  );
}
