'use client';

import React, { Suspense, startTransition, useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  TimeTravelVisualFrame,
  EgoNetwork,
  SimulateEditResponse,
  InvariantFinding,
  TrajectoryRun,
  DaemonLiveEvent,
} from '@/lib/api';

import CockpitHeader from '@/components/CockpitHeader';
import DashboardOverview from '@/components/DashboardOverview';
import LiveEditSimulator from '@/components/LiveEditSimulator';
import BlastRadiusSection from '@/components/BlastRadiusSection';
import InvariantVerificationSection from '@/components/InvariantVerificationSection';
import ForensicTrajectorySection, { CanvasReplaySelection } from '@/components/ForensicTrajectorySection';
import AiCockpitSection from '@/components/AiCockpitSection';
import TerminalWorkbench from '@/components/TerminalWorkbench';
import { GitGalaxySection } from '@/components/GitGalaxySection';
import ConnectRepoModal from '@/components/ConnectRepoModal';
import CapabilitiesModal from '@/components/CapabilitiesModal';
import RepoTreeExplorer from '@/components/RepoTreeExplorer';
import WorkspaceManualModal from '@/components/WorkspaceManualModal';
import AnalyticsReportsSection from '@/components/AnalyticsReportsSection';
import WorkspacePresetSection from '@/components/WorkspacePresetSection';
import { BookOpen } from 'lucide-react';

const CodeUniverse3D = React.lazy(() => import('@/components/CodeUniverse3D'));

type GraphLoadStage = 'idle' | 'scheduled' | 'requesting' | 'downloading' | 'parsing' | 'preparing' | 'transferring' | 'mounting' | 'ready' | 'error';

const GRAPH_STAGE_LABELS: Record<GraphLoadStage, string> = {
  idle: 'Waiting for a repository',
  scheduled: 'Scheduling canvas work',
  requesting: 'Opening persisted graph',
  downloading: 'Streaming graph independently',
  parsing: 'Parsing graph off the UI thread',
  preparing: 'Preparing deterministic nodes and edges',
  transferring: 'Transferring prepared graph',
  mounting: 'Starting GPU canvas',
  ready: 'Canvas ready',
  error: 'Canvas unavailable',
};

function CanvasLoadingState({ stage, progress }: { stage: GraphLoadStage; progress: number }) {
  return (
    <div className="flex-1 flex flex-col items-center justify-center p-8 text-center" aria-live="polite">
      <div className="relative w-16 h-16 mb-5">
        <div className="absolute inset-0 rounded-2xl border border-cyan-500/20 bg-cyan-500/5 animate-pulse" />
        <div className="absolute inset-2 rounded-xl border border-cyan-400/50 animate-spin" style={{ animationDuration: '2.4s' }} />
        <Orbit className="absolute inset-0 m-auto w-6 h-6 text-cyan-300" />
      </div>
      <h3 className="text-sm font-bold text-[var(--text-primary)] font-mono">Canvas loading independently</h3>
      <p className="text-xs text-[var(--text-muted)] font-mono mt-1.5">{GRAPH_STAGE_LABELS[stage]}</p>
      <div className="w-72 max-w-full h-1.5 rounded-full bg-[var(--card-elevated)] overflow-hidden mt-4 border border-[var(--border-subtle)]">
        <div
          className="h-full rounded-full bg-gradient-to-r from-cyan-500 via-sky-400 to-violet-500 transition-[width] duration-300 ease-out"
          style={{ width: `${Math.max(2, Math.min(progress, 100))}%` }}
        />
      </div>
      <div className="mt-3 flex items-center gap-2 text-[10px] font-mono text-[var(--text-muted)]">
        <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
        Cockpit, explorer, simulator, and navigation remain available
      </div>
    </div>
  );
}

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
  const [universeError, setUniverseError] = useState<string | null>(null);
  const [graphLoadStage, setGraphLoadStage] = useState<GraphLoadStage>('idle');
  const [graphLoadProgress, setGraphLoadProgress] = useState<number>(0);
  const graphWorkerRef = useRef<Worker | null>(null);
  const graphRequestIdRef = useRef(0);
  const graphScheduleRef = useRef<number | null>(null);
  const [repoFiles, setRepoFiles] = useState<RepoFileItem[]>([]);
  const [explorerSearch, setExplorerSearch] = useState<string>('');
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [canvasSelection, setCanvasSelection] = useState<string | null>(null);
  const [simulatorFilePath, setSimulatorFilePath] = useState<string | null>(null);
  const [simulation, setSimulation] = useState<SimulateEditResponse | null>(null);
  const [currentSymbol, setCurrentSymbol] = useState<string>('');
  const [egoNetwork, setEgoNetwork] = useState<EgoNetwork | null>(null);
  const [invariants, setInvariants] = useState<InvariantFinding[]>([]);
  const [trajectoryRuns, setTrajectoryRuns] = useState<TrajectoryRun[]>([]);
  const [activeAiActivity, setActiveAiActivity] = useState<{
    action: string;
    target: string;
    eventSeq?: number;
  } | null>(null);
  const aiActivityTimerRef = useRef<number | null>(null);
  const universeRef = useRef<GraphUniverse | null>(null);

  // Live SSE Bus State
  const [liveSeq, setLiveSeq] = useState<number>(0);
  const [isLiveConnected, setIsLiveConnected] = useState<boolean>(true);
  const [recentEvents, setRecentEvents] = useState<DaemonLiveEvent[]>([]);

  useEffect(() => {
    universeRef.current = universe;
  }, [universe]);

  // Modals
  const [isConnectModalOpen, setIsConnectModalOpen] = useState<boolean>(false);
  const [isCapabilitiesModalOpen, setIsCapabilitiesModalOpen] = useState<boolean>(false);
  const [isManualModalOpen, setIsManualModalOpen] = useState<boolean>(false);
  const [manualWorkspaceId, setManualWorkspaceId] = useState<string>('ws-repos');
  const [isEgoLoading, setIsEgoLoading] = useState<boolean>(false);
  const [isRefreshingGates, setIsRefreshingGates] = useState<boolean>(false);

  const loadUniverse = useCallback((dimension: '2D' | '3D' = '3D') => {
    const worker = graphWorkerRef.current;
    const requestId = graphRequestIdRef.current + 1;
    graphRequestIdRef.current = requestId;
    setUniverseError(null);
    setGraphLoadStage('scheduled');
    setGraphLoadProgress(1);
    const begin = () => {
      if (requestId !== graphRequestIdRef.current) return;
      if (worker) {
        worker.postMessage({ type: 'load', requestId, dimension });
        return;
      }
      setGraphLoadStage('requesting');
      api.getUniverse(dimension).then((nextUniverse) => {
        if (requestId !== graphRequestIdRef.current) return;
        setGraphLoadStage('mounting');
        startTransition(() => setUniverse(nextUniverse));
        setGraphLoadProgress(100);
        setGraphLoadStage('ready');
      }).catch((error: unknown) => {
        if (requestId !== graphRequestIdRef.current) return;
        setUniverseError(error instanceof Error ? error.message : 'Repository graph is unavailable');
        setGraphLoadStage('error');
      });
    };
    const browserWindow = window as Window;
    if (typeof window !== 'undefined' && 'requestIdleCallback' in window) {
      graphScheduleRef.current = window.requestIdleCallback(begin, { timeout: 350 });
    } else {
      graphScheduleRef.current = browserWindow.setTimeout(begin, 32);
    }
  }, []);

  useEffect(() => {
    const worker = new Worker(new URL('../workers/graphUniverse.worker.ts', import.meta.url));
    graphWorkerRef.current = worker;
    worker.onmessage = (event: MessageEvent<Record<string, unknown>>) => {
      const requestId = Number(event.data.requestId);
      if (requestId !== graphRequestIdRef.current) return;
      if (event.data.type === 'progress') {
        setGraphLoadStage(String(event.data.stage) as GraphLoadStage);
        setGraphLoadProgress(Number(event.data.progress) || 0);
        return;
      }
      if (event.data.type === 'complete') {
        setGraphLoadStage('mounting');
        setGraphLoadProgress(99);
        startTransition(() => {
          setUniverse(event.data.universe as GraphUniverse);
          setGraphLoadProgress(100);
          setGraphLoadStage('ready');
        });
        return;
      }
      if (event.data.type === 'error') {
        setUniverse(null);
        setUniverseError(String(event.data.message || 'Repository graph is unavailable'));
        setGraphLoadStage('error');
      }
    };
    return () => {
      graphRequestIdRef.current += 1;
      worker.postMessage({ type: 'cancel', requestId: graphRequestIdRef.current });
      worker.terminate();
      graphWorkerRef.current = null;
      if (graphScheduleRef.current !== null) {
        if ('cancelIdleCallback' in window) window.cancelIdleCallback(graphScheduleRef.current);
        else (window as Window).clearTimeout(graphScheduleRef.current);
      }
    };
  }, []);

  const refreshAll = async (overrideRepoId?: string) => {
    try {
      const [status, rList, caps] = await Promise.all([
        api.getStatus(),
        api.listRepos(),
        api.getCapabilities(),
      ]);

      setSnapshotId(status.snapshot_id || 'active-worktree');
      setCoverageState(status.coverage_state || 'verified');
      setFilesScanned(status.files_scanned);
      setSymbolCount(status.symbol_count);
      setEdgeCount(status.edge_count);
      setCapabilities(caps);

      const userRepos = rList.filter(
        (r) => !r.root_path?.endsWith('SynEvid') && !r.display_name?.toLowerCase().includes('synevid')
      );
      setRepos(userRepos);

      const savedId = typeof window !== 'undefined' ? localStorage.getItem('synevid_active_repo_id') : null;
      let targetId = '';
      if (overrideRepoId && userRepos.some((r) => r.repo_id === overrideRepoId)) {
        targetId = overrideRepoId;
      } else if (status.active_repo_id && userRepos.some((r) => r.repo_id === status.active_repo_id)) {
        targetId = status.active_repo_id;
      } else if (activeRepoId && userRepos.some((r) => r.repo_id === activeRepoId)) {
        targetId = activeRepoId;
      } else if (savedId && userRepos.some((r) => r.repo_id === savedId)) {
        targetId = savedId;
      } else if (userRepos.length > 0) {
        // Prefer fast native Linux repositories over slow /mnt/ mounts on startup
        const nativeRepo = userRepos.find((r) => !r.root_path?.startsWith('/mnt/'));
        targetId = nativeRepo ? nativeRepo.repo_id : userRepos[0].repo_id;
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
        if (status.active_repo_id && status.active_repo_id !== targetId) {
          await api.switchRepo(targetId).catch(() => {});
        }
      } else {
        setActiveRepoId('');
        setRepoRoot('');
      }

      // Fetch files immediately so Explorer renders in <50ms
      api.getFiles().then((filesRes) => {
        setRepoFiles(filesRes.files || []);
        setFilesScanned(filesRes.total_files || 0);
      }).catch(() => {
        setRepoFiles([]);
      });

      loadUniverse('3D');
      if (currentSymbol) {
        api.getEgoNetwork(currentSymbol).then((ego) => setEgoNetwork(ego)).catch(() => setEgoNetwork(null));
      } else {
        setEgoNetwork(null);
      }
      api.getInvariants().then((inv) => setInvariants(inv.findings || [])).catch(() => setInvariants([]));
      api.getTrajectory().then((traj) => setTrajectoryRuns(traj.runs || []));
    } catch (err) {
      console.error('Failed to refresh data', err);
    }
  };

  useEffect(() => {
    refreshAll();

    // Subscribe to SSE Live stream
    const unsubscribe = api.subscribeLiveEvents(
      (evt) => {
        setIsLiveConnected(true);
        setLiveSeq(evt.seq_id);
        setRecentEvents((prev) => [evt, ...prev.slice(0, 15)]);
        if (evt.event_type === 'mcp.live_activity' || evt.event_type === 'mcp.canvas_focus') {
          const focused = evt.payload.target_symbol || evt.payload.target_file || evt.payload.symbol || evt.payload.file;
          const action = String(evt.payload.action || evt.payload.tool || evt.event_type);
          if (typeof focused === 'string' && focused.length > 0) {
            const node = universeRef.current?.nodes.find((candidate) =>
              candidate.id === focused || candidate.file_path === focused || candidate.label === focused
            );
            if (node) setCanvasSelection(node.id);
            setCurrentSymbol(focused);
          }
          setActiveAiActivity({
            action,
            target: typeof focused === 'string' && focused.length > 0 ? focused : 'repository',
            eventSeq: typeof evt.payload.event_seq === 'number' ? evt.payload.event_seq : undefined,
          });
          if (aiActivityTimerRef.current !== null) window.clearTimeout(aiActivityTimerRef.current);
          aiActivityTimerRef.current = window.setTimeout(() => setActiveAiActivity(null), 1600);
        }
        if (evt.event_type === 'mcp.canvas_focus') {
          const focused = evt.payload.target_symbol || evt.payload.target_file;
          if (typeof focused === 'string' && focused.length > 0) {
            setCurrentSymbol(focused);
            setActiveWorkspace('ws-canvas');
          }
        }
      },
      () => {
        setIsLiveConnected(false);
      }
    );

    return () => {
      unsubscribe();
      if (aiActivityTimerRef.current !== null) window.clearTimeout(aiActivityTimerRef.current);
    };
  }, []);

  const displayedFiles = useMemo(() => {
    const source = repoFiles.map((file) => {
      const isDamaged = simulation?.damaged_callers.some((caller) => caller.file_path === file.path) ||
        simulation?.newly_dead_symbols.some((symbol) => symbol.file_path === file.path);
      return { ...file, status: isDamaged ? 'impacted' as const : file.status };
    });

    if (!explorerSearch) return source;
    const q = explorerSearch.toLowerCase();
    return source.filter((f) => f.path.toLowerCase().includes(q) || f.name.toLowerCase().includes(q));
  }, [repoFiles, simulation, explorerSearch]);

  const handleSelectRepo = async (repoId: string) => {
    setUniverse(null);
    setUniverseError(null);
    graphRequestIdRef.current += 1;
    graphWorkerRef.current?.postMessage({ type: 'cancel', requestId: graphRequestIdRef.current });
    setGraphLoadStage('idle');
    setGraphLoadProgress(0);
    setRepoFiles([]);
    setSelectedFile(null);
    setCanvasSelection(null);
    setCurrentSymbol('');
    setEgoNetwork(null);
    setSimulatorFilePath(null);
    setSimulation(null);
    setActiveRepoId(repoId);
    if (typeof window !== 'undefined') {
      localStorage.setItem('synevid_active_repo_id', repoId);
    }
    await api.switchRepo(repoId);
    await refreshAll(repoId);
  };

  const handleRemoveRepo = async (repoId: string) => {
    const repo = repos.find((candidate) => candidate.repo_id === repoId);
    if (!repo || typeof window === 'undefined') return;
    const label = repo.display_name || repo.root_path || 'this repository';
    if (!window.confirm(`Remove the saved registration for ${label}? Source files will not be deleted.`)) return;

    const result = await api.removeRepo(repoId);
    if (result.status !== 'removed') return;
    localStorage.removeItem('synevid_active_repo_id');
    await refreshAll();
  };

  const handleSearchSymbol = async (symbolId: string) => {
    setCurrentSymbol(symbolId);
    if (!symbolId) {
      setEgoNetwork(null);
      setIsEgoLoading(false);
      return;
    }
    setIsEgoLoading(true);
    try {
      const ego = await api.getEgoNetwork(symbolId);
      setEgoNetwork(ego);
    } catch {
      setEgoNetwork(null);
    } finally {
      setIsEgoLoading(false);
    }
  };

  const handleCanvasNodeSelect = (symbolId: string) => {
    setCanvasSelection(symbolId || null);
    if (!symbolId) {
      setSelectedFile(null);
      void handleSearchSymbol('');
      return;
    }
    const node = universe?.nodes.find((candidate) => candidate.id === symbolId);
    const filePath = node?.file_path || (symbolId.startsWith('file:') ? symbolId.slice('file:'.length) : null);
    if (filePath) setSelectedFile(filePath);
    void handleSearchSymbol(symbolId.startsWith('file:') ? '' : symbolId);
  };

  const handleOpenSimulator = (symbolId: string) => {
    const node = universe?.nodes.find((candidate) => candidate.id === symbolId);
    const egoFilePath = egoNetwork?.target_node?.stable_id === symbolId
      ? egoNetwork.target_node.file_path
      : undefined;
    const filePath = node?.file_path || egoFilePath || selectedFile;
    setCanvasSelection(symbolId);
    if (filePath) {
      setSelectedFile(filePath);
      setSimulatorFilePath(filePath);
    } else {
      setSimulatorFilePath(null);
    }
    setSimulation(null);
    setActiveWorkspace('ws-simulator');
  };

  const handleSelectRepositoryFile = (filePath: string) => {
    setSelectedFile(filePath);
    setSimulatorFilePath(filePath);
    setSimulation(null);
    const fileNode = universe?.nodes.find((node) => node.file_path === filePath && node.kind === 'file') ||
      universe?.nodes.find((node) => node.file_path === filePath);
    setCanvasSelection(fileNode?.id || null);
    void handleSearchSymbol(filePath);
    if (activeWorkspace !== 'ws-canvas') setActiveWorkspace('ws-canvas');
  };

  const handleToggleDimensions = () => {
    if (!universe) return;
    const nextDim = universe.dimensions === '3D' ? '2D' : '3D';
    setUniverse((prev) => (prev ? { ...prev, dimensions: nextDim } : null));
  };

  // Git Galaxy historical frames use the same canvas data contract as the live
  // universe. This swaps only the graph data; CodeUniverse3D's renderer,
  // animation loop, camera, and visual styling remain unchanged.
  const handleTimeTravelFrame = useCallback((frame: TimeTravelVisualFrame) => {
    const historicalUniverse: GraphUniverse = {
      repo_id: activeRepoId,
      snapshot_id: frame.oid,
      dimensions: frame.dimensions,
      coverage_state: frame.coverage_state || 'Unknown',
      node_count: frame.node_count,
      edge_count: frame.edge_count,
      nodes: frame.nodes.map((node) => ({
        id: node.id,
        x: node.x,
        y: node.y,
        z: node.z,
        cluster_id: node.cluster_id,
        file_path: node.file_path,
        kind: node.kind,
        start_line: node.start_line,
        end_line: node.end_line,
        status: node.diff_status,
      })),
      edges: frame.edges.map((edge, index) => ({
        id: `historical:${frame.oid}:${index}`,
        source: edge.source,
        target: edge.target,
        kind: edge.kind,
      })),
    };
    startTransition(() => setUniverse(historicalUniverse));
    setSnapshotId(frame.oid);
    setCoverageState(frame.coverage_state || 'Unknown');
    setSymbolCount(frame.node_count);
    setEdgeCount(frame.edge_count);
    setActiveWorkspace('ws-canvas');
  }, [activeRepoId]);

  const handleOpenVerificationGates = async () => {
    setActiveWorkspace('ws-gates');
    setIsRefreshingGates(true);
    try {
      const result = await api.getInvariants();
      setInvariants(result.findings || []);
    } catch (err) {
      console.error('Failed to refresh gate findings', err);
    } finally {
      setIsRefreshingGates(false);
    }
  };

  const handleExportReport = () => {
    const activeRepo = repos.find((repo) => repo.repo_id === activeRepoId);
    const report = {
      schema_version: 'synevid.report.v1',
      application: { name: 'SynEvid', version: '0.6.0' },
      repository: {
        id: activeRepoId || null,
        name: activeRepo?.display_name || null,
        root_path: repoRoot || null,
      },
      snapshot_id: snapshotId,
      coverage_state: coverageState,
      metrics: {
        files_scanned: filesScanned,
        symbol_count: symbolCount,
        edge_count: edgeCount,
      },
      findings: invariants,
    };
    const filenameBase = (activeRepo?.display_name || 'workspace')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '') || 'workspace';
    const downloadUrl = URL.createObjectURL(
      new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
    );
    const link = document.createElement('a');
    link.href = downloadUrl;
    link.download = 'synevid-report-' + filenameBase + '.json';
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(downloadUrl), 1000);
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

  const activeWorkspaceItem = WORKSPACES.find((w) => w.id === activeWorkspace);

  const handleOpenManualForCurrentWorkspace = (targetId?: string) => {
    setManualWorkspaceId(targetId || activeWorkspace);
    setIsManualModalOpen(true);
  };

  return (
    <div className="h-screen w-screen bg-[var(--background)] text-[var(--foreground)] flex flex-col font-sans overflow-hidden p-2.5 gap-2.5 transition-colors duration-150">
        {/* Region 1: Top Global Header Bar */}
        <CockpitHeader
          repos={repos}
          activeRepoId={activeRepoId}
          onSelectRepo={handleSelectRepo}
          onRemoveRepo={handleRemoveRepo}
          onOpenConnectModal={() => setIsConnectModalOpen(true)}
          onOpenCapabilitiesModal={() => setIsCapabilitiesModalOpen(true)}
          onOpenTerminal={() => setActiveWorkspace('ws-ai')}
          onOpenManual={() => handleOpenManualForCurrentWorkspace()}
          activeWorkspaceName={activeWorkspaceItem?.label}
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

            {/* Bottom Collapse Drawer Toggle & Quick Manual Button */}
            <div className="flex flex-col items-center space-y-1.5 w-full">
              <button
                onClick={() => handleOpenManualForCurrentWorkspace()}
                className="p-2.5 rounded-xl text-cyan-400 hover:text-cyan-300 hover:bg-cyan-500/10 border border-transparent hover:border-cyan-500/30 transition-all cursor-pointer group"
                title={`Open Manual for ${activeWorkspaceItem?.label || 'Workspace'}`}
              >
                <BookOpen className="w-4 h-4 group-hover:scale-110 transition-transform" />
              </button>

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
            </div>
          </aside>

          {/* Region 3: Real-Time Hierarchical Repository Tree Explorer */}
          {!isDrawerCollapsed && (
            <aside className="w-80 bg-[var(--card)] border border-[var(--border-subtle)] rounded-2xl flex flex-col p-3 shrink-0 animate-fadeIn shadow-sm overflow-hidden">
              <div className="flex-1 overflow-hidden flex flex-col">
                <RepoTreeExplorer
                  files={displayedFiles}
                  selectedFilePath={selectedFile}
                  onSelectFile={handleSelectRepositoryFile}
                  onRefresh={refreshAll}
                />
              </div>

              {/* Quick Actions at Drawer Bottom */}
              <div className="pt-2.5 mt-2 border-t border-[var(--border-subtle)] space-y-1.5 shrink-0">
                <button
                  onClick={handleOpenVerificationGates}
                  disabled={isRefreshingGates}
                  className="w-full py-2 px-3 rounded-xl bg-gradient-to-r from-cyan-600 to-sky-600 hover:from-cyan-500 hover:to-sky-500 text-white font-semibold text-xs font-mono transition-all flex items-center justify-center space-x-2 cursor-pointer shadow-sm"
                >
                  <Play className="w-3.5 h-3.5" />
                  <span>{isRefreshingGates ? 'Loading Gates...' : 'Open Gates & Policy'}</span>
                </button>
                <button
                  onClick={handleExportReport}
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
            {activeWorkspace === 'ws-canvas' && activeAiActivity && (
              <div
                className="pointer-events-none absolute right-4 top-4 z-20 flex items-center gap-2 rounded-xl border border-cyan-400/60 bg-slate-950/90 px-3 py-2 font-mono text-[11px] text-cyan-200 shadow-lg shadow-cyan-500/10"
                role="status"
                aria-live="polite"
              >
                <span className="h-2 w-2 animate-pulse rounded-full bg-cyan-300" />
                <Bot className="h-3.5 w-3.5 text-cyan-300" />
                <span className="font-bold text-cyan-100">AI Active</span>
                <span className="max-w-[220px] truncate text-cyan-300">
                  {activeAiActivity.action} · {activeAiActivity.target}
                </span>
              </div>
            )}
            {activeWorkspace === 'ws-canvas' && universe && universe.nodes.length > 0 && (
              <Suspense fallback={<CanvasLoadingState stage="mounting" progress={99} />}>
                <CodeUniverse3D
                  universe={universe}
                  egoNetwork={egoNetwork}
                  focusNodeId={canvasSelection}
                  simulation={simulation}
                  simulationFilePath={simulatorFilePath}
                  onSelectNode={handleCanvasNodeSelect}
                  onOpenSimulator={handleOpenSimulator}
                  onToggleDimensions={handleToggleDimensions}
                />
              </Suspense>
            )}

            {activeWorkspace === 'ws-canvas' && (!universe || universe.nodes.length === 0) && (
              graphLoadStage !== 'idle' && graphLoadStage !== 'error'
                ? <CanvasLoadingState stage={graphLoadStage} progress={graphLoadProgress} />
                : <div className="flex-1 flex flex-col items-center justify-center p-8 text-center">
                    <div className="w-12 h-12 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center mb-4 animate-pulse">
                      <Orbit className="w-6 h-6 text-cyan-400" />
                    </div>
                    <h3 className="text-sm font-bold text-[var(--text-primary)] font-mono">
                      {universeError ? 'Repository analysis unavailable' : 'Connect a repository to explore its graph'}
                    </h3>
                    <p className="text-xs text-[var(--text-muted)] font-mono mt-1.5 max-w-md">
                      {universeError || 'The canvas will show persisted symbols and verified dependency edges from the selected repository.'}
                    </p>
                  </div>
            )}

            {activeWorkspace === 'ws-simulator' && (
              <LiveEditSimulator
                filePath={simulatorFilePath}
                repoId={activeRepoId}
                onSimulationChange={setSimulation}
              />
            )}

            {activeWorkspace === 'ws-repos' && (
              <div className="space-y-6">
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
                <AnalyticsReportsSection />
              </div>
            )}

            {activeWorkspace === 'ws-git' && (
              <GitGalaxySection
                onTimeTravelFrame={handleTimeTravelFrame}
                onNavigateUniverse={() => setActiveWorkspace('ws-canvas')}
              />
            )}

            {activeWorkspace === 'ws-trajectory' && (
              <ForensicTrajectorySection
                liveEvents={recentEvents}
                onReplayCanvas={(selection: CanvasReplaySelection) => {
                  const targetNode = universeRef.current?.nodes.find((node) =>
                    selection.affectedSymbols.includes(node.id) ||
                    selection.affectedSymbols.includes(node.label || '') ||
                    selection.targetSymbolOrFile === node.id ||
                    selection.targetSymbolOrFile === node.file_path
                  );
                  const target = targetNode?.id || selection.targetSymbolOrFile;
                  const filePath = targetNode?.file_path || selection.affectedFiles[0] ||
                    (selection.targetSymbolOrFile.includes('/') ? selection.targetSymbolOrFile : null);
                  const edgeNodeIds = selection.affectedEdges.flatMap((edge) => [edge.source, edge.target]);
                  const fileNodeIds = universeRef.current?.nodes
                    .filter((node) => node.file_path && selection.affectedFiles.includes(node.file_path))
                    .map((node) => node.id) || [];
                  const affectedIds = [...new Set([
                    ...selection.affectedSymbols,
                    ...edgeNodeIds,
                    ...fileNodeIds,
                    ...(targetNode ? [targetNode.id] : []),
                  ])];
                  const replaySimulation: SimulateEditResponse = {
                    repo_id: activeRepoId,
                    verdict: 'INFO',
                    coverage_state: 'verified',
                    baseline_snapshot_id: null,
                    candidate_snapshot_id: null,
                    impacted_entities: affectedIds,
                    impact_truncated: false,
                    damaged_callers: affectedIds.map((stable_id) => ({
                      stable_id,
                      file_path: filePath || '',
                      calling_line: 0,
                      calling_col: 0,
                      diagnostic: `Historical event #${selection.eventSeq} affected node`,
                      severity: 'info',
                    })),
                    newly_dead_symbols: [],
                    broken_imports: [],
                    syntax_errors: [],
                    computation_latency_ms: 0,
                  };
                  setSimulation(replaySimulation);
                  setSimulatorFilePath(filePath);
                  setCanvasSelection(targetNode?.id || null);
                  setSelectedFile(filePath || null);
                  setCurrentSymbol(target);
                  setActiveWorkspace('ws-canvas');
                }}
              />
            )}

            {activeWorkspace === 'ws-gates' && (
              <InvariantVerificationSection findings={invariants} />
            )}

            {activeWorkspace === 'ws-ai' && (
              <div className="space-y-6">
                <AiCockpitSection
                  activeSessionId={activeRepoId}
                  onFocusCanvas={(sym) => {
                    setCurrentSymbol(sym);
                    setActiveWorkspace('ws-canvas');
                  }}
                />
                <div className="p-4 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
                  <div className="text-xs font-mono font-bold text-[var(--text-muted)] uppercase mb-3">
                    Embedded Verification Terminal
                  </div>
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
                <WorkspacePresetSection onApply={(workspaceId, drawerCollapsed) => {
                  setActiveWorkspace(workspaceId);
                  setIsDrawerCollapsed(drawerCollapsed);
                }} />
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

        {/* Region 7: End-to-End User Manual Modal */}
        <WorkspaceManualModal
          isOpen={isManualModalOpen}
          onClose={() => setIsManualModalOpen(false)}
          workspaceId={manualWorkspaceId}
        />
      </div>
  );
}
