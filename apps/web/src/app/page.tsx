'use client';

import React, { useState, useEffect } from 'react';
import {
  LayoutDashboard,
  Orbit,
  ArrowDownLeft,
  Zap,
  Radio,
  ShieldCheck,
  History,
  Terminal as TerminalIcon,
} from 'lucide-react';
import { ThemeProvider } from '@/context/ThemeContext';
import {
  api,
  RepoRecord,
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
import ConnectRepoModal from '@/components/ConnectRepoModal';
import CapabilitiesModal from '@/components/CapabilitiesModal';

export default function OperationalCockpitPage() {
  const [activeTab, setActiveTab] = useState<string>('overview');
  const [repos, setRepos] = useState<RepoRecord[]>([]);
  const [activeRepoId, setActiveRepoId] = useState<string>('');
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);

  // Status Telemetry
  const [snapshotId, setSnapshotId] = useState<string>('-');
  const [repoRoot, setRepoRoot] = useState<string>('.');
  const [coverageState, setCoverageState] = useState<string>('Verified');
  const [filesScanned, setFilesScanned] = useState<number>(0);
  const [symbolCount, setSymbolCount] = useState<number>(0);
  const [edgeCount, setEdgeCount] = useState<number>(0);

  // Modules State
  const [universe, setUniverse] = useState<GraphUniverse | null>(null);
  const [currentSymbol, setCurrentSymbol] = useState<string>('PaymentService.chargeCard');
  const [egoNetwork, setEgoNetwork] = useState<EgoNetwork | null>(null);
  const [invariants, setInvariants] = useState<InvariantFinding[]>([]);
  const [trajectoryRuns, setTrajectoryRuns] = useState<TrajectoryRun[]>([]);

  // Live SSE Bus State
  const [liveSeq, setLiveSeq] = useState<number>(100);
  const [isLiveConnected, setIsLiveConnected] = useState<boolean>(false);
  const [recentEvents, setRecentEvents] = useState<DaemonLiveEvent[]>([]);

  // Modals
  const [isConnectModalOpen, setIsConnectModalOpen] = useState<boolean>(false);
  const [isCapabilitiesModalOpen, setIsCapabilitiesModalOpen] = useState<boolean>(false);
  const [isEgoLoading, setIsEgoLoading] = useState<boolean>(false);

  const refreshAll = async () => {
    try {
      const status = await api.getStatus();
      setSnapshotId(status.snapshot_id);
      setRepoRoot(status.repo_root);
      setCoverageState(status.coverage_state);
      setFilesScanned(status.files_scanned);
      setSymbolCount(status.symbol_count);
      setEdgeCount(status.edge_count);

      const rList = await api.listRepos();
      setRepos(rList);
      if (rList.length > 0 && !activeRepoId) {
        setActiveRepoId(rList[0].repo_id);
      }

      const caps = await api.getCapabilities();
      setCapabilities(caps);

      const uni = await api.getUniverse('2D');
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
    refreshAll();

    // Subscribe to SSE Live stream
    const unsubscribe = api.subscribeLiveEvents(
      (evt) => {
        setIsLiveConnected(true);
        setLiveSeq(evt.seq_id);
        setRecentEvents((prev) => [evt, ...prev.slice(0, 15)]);
      },
      () => {
        setIsLiveConnected(false);
      }
    );

    return () => unsubscribe();
  }, []);

  const handleSelectRepo = async (repoId: string) => {
    setActiveRepoId(repoId);
    await api.switchRepo(repoId);
    refreshAll();
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

  const handleToggleDimensions = async () => {
    if (!universe) return;
    const nextDim = universe.dimensions === '3D' ? '2D' : '3D';
    const uni = await api.getUniverse(nextDim);
    setUniverse(uni);
  };

  const failingRulesCount = invariants.filter((f) => f.status === 'fail').length;

  const TABS = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'universe', label: 'Code Universe (3D/2D)', icon: Orbit },
    { id: 'ego', label: 'Ego Network & Lines', icon: ArrowDownLeft },
    { id: 'simulator', label: 'Live Simulator (<50ms)', icon: Zap },
    { id: 'blast', label: 'Blast Radius', icon: Radio },
    { id: 'verify', label: 'Invariants', icon: ShieldCheck },
    { id: 'trajectory', label: 'Forensic Trajectory', icon: History },
    { id: 'workbench', label: 'Terminal / CLI', icon: TerminalIcon },
  ];

  return (
    <ThemeProvider>
      <div className="min-h-screen bg-[var(--background)] text-[var(--foreground)] flex flex-col font-sans transition-colors duration-150">
        {/* Cockpit Command Bar Header */}
        <CockpitHeader
          repos={repos}
          activeRepoId={activeRepoId}
          onSelectRepo={handleSelectRepo}
          onOpenConnectModal={() => setIsConnectModalOpen(true)}
          onOpenCapabilitiesModal={() => setIsCapabilitiesModalOpen(true)}
          onRefresh={refreshAll}
          liveSeq={liveSeq}
          isLiveConnected={isLiveConnected}
          coverageState={coverageState}
          invariantsPass={failingRulesCount === 0}
        />

        {/* Tabbed Navigation Bar */}
        <nav className="bg-[var(--card)] border-b border-[var(--border-subtle)] px-6 flex space-x-6 overflow-x-auto shadow-sm">
          {TABS.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center space-x-2 py-3.5 text-xs font-semibold whitespace-nowrap border-b-2 transition-all ${
                  isActive
                    ? 'border-cyan-500 text-cyan-600 dark:text-cyan-400'
                    : 'border-transparent text-[var(--text-muted)] hover:text-[var(--text-primary)]'
                }`}
              >
                <Icon className="w-4 h-4" />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </nav>

        {/* Cockpit Main Module Surface */}
        <main className="flex-1 max-w-7xl mx-auto w-full p-6">
          {activeTab === 'overview' && (
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
              onNavigateTab={(tab) => setActiveTab(tab)}
            />
          )}

          {activeTab === 'universe' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-base font-bold text-[var(--text-primary)] font-brand">
                    Seeded Graph Universe
                  </h2>
                  <p className="text-xs text-[var(--text-muted)] mt-0.5">
                    Barnes-Hut layout seeded by BLAKE3(snapshot_digest). Click any node to drill into line-to-line ego network.
                  </p>
                </div>
              </div>
              {universe && (
                <CodeUniverse3D
                  universe={universe}
                  onSelectNode={(id) => {
                    handleSearchSymbol(id);
                    setActiveTab('ego');
                  }}
                  onToggleDimensions={handleToggleDimensions}
                />
              )}
            </div>
          )}

          {activeTab === 'ego' && (
            <EgoNetworkInspector
              ego={egoNetwork}
              currentSymbol={currentSymbol}
              onSearchSymbol={handleSearchSymbol}
              isLoading={isEgoLoading}
            />
          )}

          {activeTab === 'simulator' && <LiveEditSimulator />}

          {activeTab === 'blast' && (
            <BlastRadiusSection
              onSelectSymbol={(sym) => {
                handleSearchSymbol(sym);
                setActiveTab('ego');
              }}
            />
          )}

          {activeTab === 'verify' && <InvariantVerificationSection findings={invariants} />}

          {activeTab === 'trajectory' && <ForensicTrajectorySection runs={trajectoryRuns} />}

          {activeTab === 'workbench' && (
            <div className="p-4 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
              <div className="mb-4">
                <h3 className="text-sm font-bold text-[var(--text-primary)] font-brand">
                  Terminal &amp; CLI Agent Workbench
                </h3>
                <p className="text-xs text-[var(--text-muted)] mt-0.5">
                  Execute live SynEvid commands, inspect JSON envelopes and test impact profiles.
                </p>
              </div>
              <TerminalWorkbench />
            </div>
          )}
        </main>

        {/* Modals */}
        <ConnectRepoModal
          isOpen={isConnectModalOpen}
          onClose={() => setIsConnectModalOpen(false)}
          onConnected={refreshAll}
        />

        <CapabilitiesModal
          isOpen={isCapabilitiesModalOpen}
          onClose={() => setIsCapabilitiesModalOpen(false)}
          capabilities={capabilities}
        />
      </div>
    </ThemeProvider>
  );
}
