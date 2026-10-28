'use client';

import React, { useState, useEffect, useMemo, useCallback } from 'react';
import {
  GitBranch,
  GitCommit,
  GitPullRequest,
  Clock,
  RotateCcw,
  Plus,
  Play,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  ShieldCheck,
  Tag,
  ArrowRight,
  FileCode,
  FilePlus,
  FileDiff,
  Sliders,
  ChevronRight,
  RefreshCw,
  Eye,
  Layers,
  FileText,
  Trash2,
  Search,
  Check,
  Split,
  FastForward,
  CornerDownRight,
  Sparkles,
} from 'lucide-react';
import {
  api,
  GitGalaxy,
  CommitGalaxyNode,
  TimeTravelVisualFrame,
  SemanticDiffReport,
  GitMutationResult,
  GitStatusReport,
  FileStatusItem,
  FileDiffReport,
  BisectReport,
} from '@/lib/api';

interface GitGalaxySectionProps {
  onTimeTravelFrame?: (frame: TimeTravelVisualFrame) => void;
  onNavigateUniverse?: () => void;
}

export function GitGalaxySection({ onTimeTravelFrame, onNavigateUniverse }: GitGalaxySectionProps) {
  // Navigation Tabs inside Git Galaxy
  const [activeTab, setActiveTab] = useState<'dag' | 'status' | 'branches' | 'bisect'>('dag');

  // Galaxy Data
  const [galaxy, setGalaxy] = useState<GitGalaxy | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  // Selected Commit & Time-Travel State
  const [selectedCommitOid, setSelectedCommitOid] = useState<string | null>(null);
  const [activeFrame, setActiveFrame] = useState<TimeTravelVisualFrame | null>(null);
  const [isFrameLoading, setIsFrameLoading] = useState<boolean>(false);

  // Range Comparison Mode (Commit A <-> Commit B)
  const [isRangeMode, setIsRangeMode] = useState<boolean>(false);
  const [pinA, setPinA] = useState<string | null>(null);
  const [pinB, setPinB] = useState<string | null>(null);
  const [diffReport, setDiffReport] = useState<SemanticDiffReport | null>(null);
  const [isDiffLoading, setIsDiffLoading] = useState<boolean>(false);

  // Source Control & Working Tree Git Status State
  const [gitStatus, setGitStatus] = useState<GitStatusReport | null>(null);
  const [statusLoading, setStatusLoading] = useState<boolean>(false);
  const [activeDiffPath, setActiveDiffPath] = useState<string | null>(null);
  const [fileDiff, setFileDiff] = useState<FileDiffReport | null>(null);
  const [diffFileLoading, setDiffFileLoading] = useState<boolean>(false);

  // Commit Form State
  const [commitType, setCommitType] = useState<string>('feat');
  const [commitMessage, setCommitMessage] = useState<string>('');
  const [commitTag, setCommitTag] = useState<string>('');
  const [newBranchName, setNewBranchName] = useState<string>('');
  const [mergeSourceBranch, setMergeSourceBranch] = useState<string>('');
  const [newTagName, setNewTagName] = useState<string>('');

  // Bisect State
  const [bisectStartOid, setBisectStartOid] = useState<string>('');
  const [bisectEndOid, setBisectEndOid] = useState<string>('');
  const [bisectReport, setBisectReport] = useState<BisectReport | null>(null);
  const [isBisecting, setIsBisecting] = useState<boolean>(false);

  // Feedback & Mutation Lock
  const [mutationFeedback, setMutationFeedback] = useState<GitMutationResult | null>(null);
  const [isMutating, setIsMutating] = useState<boolean>(false);

  // Time-Travel Scrubbing Playhead Index
  const [sliderIndex, setSliderIndex] = useState<number>(0);

  // 1. Load Git Galaxy
  const loadGalaxy = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.getGitGalaxy(200);
      setGalaxy(data);
      if (data.commits && data.commits.length > 0) {
        const headCommit = data.commits.find((c) => c.is_head) || data.commits[0];
        setSelectedCommitOid(headCommit.oid);
        setPinB(headCommit.oid);
        setBisectEndOid(headCommit.oid);
        if (data.commits.length > 1) {
          setPinA(data.commits[1].oid);
          setBisectStartOid(data.commits[data.commits.length - 1].oid);
        } else {
          setPinA(headCommit.oid);
          setBisectStartOid(headCommit.oid);
        }
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to read Git Galaxy');
    } finally {
      setLoading(false);
    }
  }, []);

  // 2. Load Real-time Working Tree Status
  const loadStatus = useCallback(async () => {
    setStatusLoading(true);
    try {
      const data = await api.getGitStatus();
      setGitStatus(data);
      // Auto-select first modified/untracked file for diff inspection
      const firstUncommitted =
        data.staged[0]?.path || data.unstaged[0]?.path || data.untracked[0]?.path;
      if (firstUncommitted && !activeDiffPath) {
        setActiveDiffPath(firstUncommitted);
      }
    } catch {
      // offline fallback
    } finally {
      setStatusLoading(false);
    }
  }, [activeDiffPath]);

  useEffect(() => {
    loadGalaxy();
    loadStatus();
  }, [loadGalaxy, loadStatus]);

  // 3. Fetch Line Diff when active file changes
  useEffect(() => {
    if (!activeDiffPath) return;
    let cancelled = false;
    setDiffFileLoading(true);

    api
      .getFileDiff(activeDiffPath)
      .then((diff) => {
        if (!cancelled) setFileDiff(diff);
      })
      .catch((err) => {
        console.warn('File diff failed:', err);
      })
      .finally(() => {
        if (!cancelled) setDiffFileLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [activeDiffPath]);

  // 4. Fetch Time-Travel Visual Frame when commit is selected
  useEffect(() => {
    if (!selectedCommitOid) return;
    let cancelled = false;
    setIsFrameLoading(true);

    api
      .getTimeTravelFrame(selectedCommitOid, '3d')
      .then((frame) => {
        if (!cancelled) {
          setActiveFrame(frame);
          if (onTimeTravelFrame) {
            onTimeTravelFrame(frame);
          }
        }
      })
      .catch((err) => {
        console.warn('Time-travel frame lookup failed:', err);
      })
      .finally(() => {
        if (!cancelled) setIsFrameLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedCommitOid, onTimeTravelFrame]);

  // 5. Fetch Range Diff when in Range Mode
  useEffect(() => {
    if (!isRangeMode || !pinA || !pinB) return;
    let cancelled = false;
    setIsDiffLoading(true);

    api
      .getGitDiff(pinA, pinB)
      .then((report) => {
        if (!cancelled) setDiffReport(report);
      })
      .catch((err) => {
        console.warn('Git range diff failed:', err);
      })
      .finally(() => {
        if (!cancelled) setIsDiffLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [isRangeMode, pinA, pinB]);

  // Synchronize Slider Index
  const commitsList = useMemo(() => galaxy?.commits || [], [galaxy]);
  useEffect(() => {
    if (commitsList.length > 0 && selectedCommitOid) {
      const idx = commitsList.findIndex((c) => c.oid === selectedCommitOid);
      if (idx !== -1) {
        setSliderIndex(commitsList.length - 1 - idx);
      }
    }
  }, [commitsList, selectedCommitOid]);

  const handleSliderChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = parseInt(e.target.value, 10);
    setSliderIndex(val);
    const targetCommit = commitsList[commitsList.length - 1 - val];
    if (targetCommit) {
      setSelectedCommitOid(targetCommit.oid);
    }
  };

  // --- ACTIONS ---

  // Stage File
  const handleStageFile = async (path: string) => {
    setIsMutating(true);
    try {
      const currentStaged = (gitStatus?.staged || []).map((s) => s.path);
      const updated = Array.from(new Set([...currentStaged, path]));
      const res = await api.stageGit(updated);
      setMutationFeedback(res);
      await loadStatus();
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'stage',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Stage failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Unstage File
  const handleUnstageFile = async (path: string) => {
    setIsMutating(true);
    try {
      const res = await api.unstageGit([path]);
      setMutationFeedback(res);
      await loadStatus();
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'unstage',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Unstage failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Stage All
  const handleStageAll = async () => {
    if (!gitStatus) return;
    setIsMutating(true);
    try {
      const allPaths = [
        ...(gitStatus?.staged || []).map((s) => s.path),
        ...(gitStatus?.unstaged || []).map((u) => u.path),
        ...(gitStatus?.untracked || []).map((t) => t.path),
      ];
      const res = await api.stageGit(Array.from(new Set(allPaths)));
      setMutationFeedback(res);
      await loadStatus();
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'stage_all',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Stage all failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Unstage All
  const handleUnstageAll = async () => {
    setIsMutating(true);
    try {
      const res = await api.unstageGit();
      setMutationFeedback(res);
      await loadStatus();
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'unstage_all',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Unstage all failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Commit Changes
  const handleCommit = async () => {
    if (!commitMessage.trim()) return;
    setIsMutating(true);
    try {
      const fullMsg = `${commitType}: ${commitMessage.trim()}`;
      const res = await api.commitGit(fullMsg);
      setMutationFeedback(res);
      setCommitMessage('');

      // If user supplied a release tag, create it simultaneously
      if (commitTag.trim() && res.new_commit_oid) {
        await api.tagGit({
          tag_name: commitTag.trim(),
          target_oid: res.new_commit_oid,
          action: 'create',
        });
        setCommitTag('');
      }

      await loadGalaxy();
      await loadStatus();
      setActiveTab('dag');
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'commit',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Commit failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Branch Operations
  const handleBranchAction = async (branchName: string, action: 'create' | 'switch' | 'delete') => {
    if (!branchName.trim()) return;
    setIsMutating(true);
    try {
      const res = await api.branchGit(branchName.trim(), action);
      setMutationFeedback(res);
      if (action === 'create') setNewBranchName('');
      await loadGalaxy();
      await loadStatus();
      setActiveTab('dag');
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: `branch_${action}`,
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : `Branch ${action} failed`,
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Branch Merge
  const handleMergeBranch = async () => {
    if (!mergeSourceBranch) return;
    setIsMutating(true);
    try {
      const res = await api.mergeGit({ source_branch: mergeSourceBranch });
      setMutationFeedback(res);
      setMergeSourceBranch('');
      await loadGalaxy();
      await loadStatus();
      setActiveTab('dag');
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'merge',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Merge failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Tag Creation
  const handleCreateTag = async () => {
    if (!newTagName.trim()) return;
    setIsMutating(true);
    try {
      const res = await api.tagGit({
        tag_name: newTagName.trim(),
        target_oid: selectedCommitOid || undefined,
        action: 'create',
      });
      setMutationFeedback(res);
      setNewTagName('');
      await loadGalaxy();
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'tag_create',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Tag creation failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Delete Tag
  const handleDeleteTag = async (tagName: string) => {
    setIsMutating(true);
    try {
      const res = await api.tagGit({
        tag_name: tagName,
        action: 'delete',
      });
      setMutationFeedback(res);
      await loadGalaxy();
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'tag_delete',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Tag delete failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Time-Travel Checkout / Detach
  const handleCheckoutCommit = async (oid: string) => {
    setIsMutating(true);
    try {
      const res = await api.checkoutGit({ target: oid, detach: true });
      setMutationFeedback(res);
      await loadGalaxy();
      await loadStatus();
      setActiveTab('dag');
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'checkout',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Checkout failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Revert Commit
  const handleRevertCommit = async (oid: string) => {
    setIsMutating(true);
    try {
      const res = await api.revertGit(oid);
      setMutationFeedback(res);
      await loadGalaxy();
      await loadStatus();
      setActiveTab('dag');
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'revert',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Revert failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  // Autopsy Bisect
  const handleRunBisect = async () => {
    if (!bisectStartOid || !bisectEndOid) return;
    setIsBisecting(true);
    try {
      const report = await api.bisectGit({
        start_oid: bisectStartOid,
        end_oid: bisectEndOid,
      });
      setBisectReport(report);
      if (report.breaking_commit_oid) {
        setSelectedCommitOid(report.breaking_commit_oid);
      }
    } catch (err: unknown) {
      console.warn('Bisect failed:', err);
    } finally {
      setIsBisecting(false);
    }
  };

  // Undo (Cmd+Z)
  const handleUndo = async () => {
    setIsMutating(true);
    try {
      const res = await api.undoGit();
      setMutationFeedback(res);
      await loadGalaxy();
      await loadStatus();
      setActiveTab('dag');
    } catch (err: unknown) {
      setMutationFeedback({
        success: false,
        status: 'error',
        action: 'undo',
        new_commit_oid: null,
        undo_checkpoint_id: null,
        message: err instanceof Error ? err.message : 'Undo rollback failed',
        error: String(err),
      });
    } finally {
      setIsMutating(false);
    }
  };

  const formatDate = (epochSec: number) => {
    const d = new Date(epochSec * 1000);
    return d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const selectedCommit = useMemo(() => {
    return commitsList.find((c) => c.oid === selectedCommitOid) || commitsList[0];
  }, [commitsList, selectedCommitOid]);

  const branchesList = useMemo(() => galaxy?.branches || [], [galaxy]);
  const tagsList = useMemo(() => galaxy?.tags || [], [galaxy]);

  return (
    <div className="flex flex-col h-full min-h-0 gap-3 font-mono text-xs select-none">
      {/* 1. Header Navigation Toolbar */}
      <div className="shrink-0 flex flex-wrap items-center justify-between gap-3 p-3.5 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm">
        <div className="flex items-center space-x-3">
          <div className="w-8 h-8 rounded-lg bg-violet-600/15 text-violet-500 border border-violet-500/30 flex items-center justify-center font-bold">
            <GitBranch className="w-4 h-4" />
          </div>
          <div>
            <div className="flex items-center space-x-2">
              <h2 className="text-sm font-bold text-[var(--text-primary)] font-brand">
                Git Time-Travel Galaxy &amp; Source Control
              </h2>
              <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-violet-600/15 text-violet-500 border border-violet-500/25">
                v0.4.0 &bull; PURE RUST OBJECT ENGINE
              </span>
            </div>
            <p className="text-xs text-[var(--text-muted)] font-mono mt-0.5">
              Active branch: <span className="text-violet-500 font-bold">{galaxy?.active_branch || 'main'}</span> &bull; Uncommitted changes:{' '}
              <span className={`font-bold ${gitStatus?.total_uncommitted ? 'text-amber-500' : 'text-emerald-500'}`}>
                {gitStatus?.total_uncommitted || 0} files
              </span>
            </p>
          </div>
        </div>

        {/* View Mode Toggle Pill Tabs */}
        <div className="flex items-center space-x-1.5 p-1 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)]">
          <button
            onClick={() => setActiveTab('dag')}
            className={`px-3 py-1.5 rounded-lg text-xs font-mono font-semibold transition-all cursor-pointer flex items-center space-x-1.5 ${
              activeTab === 'dag'
                ? 'bg-violet-600 text-white shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
            }`}
          >
            <Layers className="w-3.5 h-3.5" />
            <span>Galaxy DAG</span>
          </button>
          <button
            onClick={() => setActiveTab('status')}
            className={`px-3 py-1.5 rounded-lg text-xs font-mono font-semibold transition-all cursor-pointer flex items-center space-x-1.5 ${
              activeTab === 'status'
                ? 'bg-violet-600 text-white shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
            }`}
          >
            <FileDiff className="w-3.5 h-3.5" />
            <span>Working Tree &amp; Diff</span>
            {gitStatus && gitStatus.total_uncommitted > 0 && (
              <span className="px-1.5 py-0.2 rounded-full text-[9px] bg-amber-500 text-black font-bold">
                {gitStatus.total_uncommitted}
              </span>
            )}
          </button>
          <button
            onClick={() => setActiveTab('branches')}
            className={`px-3 py-1.5 rounded-lg text-xs font-mono font-semibold transition-all cursor-pointer flex items-center space-x-1.5 ${
              activeTab === 'branches'
                ? 'bg-violet-600 text-white shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
            }`}
          >
            <GitBranch className="w-3.5 h-3.5" />
            <span>Branches &amp; Tags</span>
          </button>
          <button
            onClick={() => setActiveTab('bisect')}
            className={`px-3 py-1.5 rounded-lg text-xs font-mono font-semibold transition-all cursor-pointer flex items-center space-x-1.5 ${
              activeTab === 'bisect'
                ? 'bg-violet-600 text-white shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
            }`}
          >
            <Search className="w-3.5 h-3.5" />
            <span>Autopsy Bisect</span>
          </button>
        </div>

        {/* Global Action Toolbar */}
        <div className="flex items-center space-x-2">
          {activeTab === 'dag' && (
            <button
              onClick={() => setIsRangeMode(!isRangeMode)}
              className={`px-3 py-1.5 rounded-lg text-xs font-mono font-semibold border transition-all flex items-center space-x-1.5 cursor-pointer ${
                isRangeMode
                  ? 'bg-amber-500/15 text-amber-500 border-amber-500/30'
                  : 'bg-[var(--card-elevated)] text-[var(--text-secondary)] border-[var(--border-subtle)] hover:text-[var(--text-primary)]'
              }`}
            >
              <Sliders className="w-3.5 h-3.5" />
              <span>{isRangeMode ? 'Range Active' : 'Range Mode (A ↔ B)'}</span>
            </button>
          )}

          {/* 1-Click Undo Button */}
          <button
            onClick={handleUndo}
            disabled={isMutating}
            title="Instant rollback via refs/autopsy/undo/ stack (Cmd+Z)"
            className="px-3 py-1.5 rounded-lg text-xs font-mono font-semibold bg-[var(--card-elevated)] text-[var(--text-secondary)] border border-[var(--border-subtle)] hover:text-emerald-500 hover:border-emerald-500/30 transition-all flex items-center space-x-1.5 cursor-pointer disabled:opacity-50"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>Undo (Cmd+Z)</span>
          </button>

          <button
            onClick={() => {
              loadGalaxy();
              loadStatus();
            }}
            disabled={loading || statusLoading}
            className="p-1.5 rounded-lg bg-[var(--card-elevated)] text-[var(--text-secondary)] border border-[var(--border-subtle)] hover:text-[var(--text-primary)] transition-all cursor-pointer"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading || statusLoading ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {/* Mutation Feedback Banner */}
      {mutationFeedback && (
        <div
          className={`shrink-0 p-3 rounded-xl border flex items-center justify-between text-xs font-mono ${
            mutationFeedback.success
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-500'
              : 'bg-rose-500/10 border-rose-500/30 text-rose-500'
          }`}
        >
          <div className="flex items-center space-x-2">
            {mutationFeedback.success ? (
              <CheckCircle2 className="w-4 h-4 shrink-0" />
            ) : (
              <XCircle className="w-4 h-4 shrink-0" />
            )}
            <span>
              <strong>{mutationFeedback.action.toUpperCase()}</strong>: {mutationFeedback.message}
              {mutationFeedback.new_commit_oid && ` (OID: ${mutationFeedback.new_commit_oid.slice(0, 7)})`}
            </span>
          </div>
          <button onClick={() => setMutationFeedback(null)} className="text-xs hover:underline cursor-pointer">
            Dismiss
          </button>
        </div>
      )}

      {/* ======================================================== */}
      {/* TAB 1: COMMIT DAG & TIME-TRAVEL SCRUBBING GALAXY           */}
      {/* ======================================================== */}
      {activeTab === 'dag' && (
        <>
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-3 flex-1 min-h-0 overflow-y-auto lg:overflow-hidden">
            {/* Left Column: Interactive Commit & Branch DAG (7 Cols) */}
            <div className="lg:col-span-7 flex flex-col rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] overflow-hidden shadow-sm min-h-[320px] lg:min-h-0">
              <div className="shrink-0 p-3 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between">
                <div className="flex items-center space-x-2 text-xs font-mono font-semibold text-[var(--text-secondary)]">
                  <Layers className="w-3.5 h-3.5 text-violet-500" />
                  <span>
                    Evolutionary Topology ({commitsList.length} Commits from <code>git init</code>)
                  </span>
                </div>
                {isRangeMode && (
                  <span className="text-[11px] font-mono text-amber-500 font-semibold">
                    Pin A: {pinA?.slice(0, 7)} &bull; Pin B: {pinB?.slice(0, 7)}
                  </span>
                )}
              </div>

              {/* Commit DAG Stream */}
              <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-2">
                {loading ? (
                  <div className="flex flex-col items-center justify-center h-48 text-xs font-mono text-[var(--text-muted)] space-y-2">
                    <RefreshCw className="w-5 h-5 animate-spin text-violet-500" />
                    <span>Reading pure-Rust git objects directly from .git/objects...</span>
                  </div>
                ) : error ? (
                  <div className="p-4 rounded-lg bg-rose-500/10 border border-rose-500/20 text-xs font-mono text-rose-500">
                    {error}
                  </div>
                ) : commitsList.length === 0 ? (
                  <div className="flex flex-col items-center justify-center h-48 text-xs font-mono text-[var(--text-muted)]">
                    No commits discovered in this repository yet.
                  </div>
                ) : (
                  commitsList.map((commit) => {
                    const isSelected = commit.oid === selectedCommitOid;
                    const isPinA = isRangeMode && pinA === commit.oid;
                    const isPinB = isRangeMode && pinB === commit.oid;

                    return (
                      <div
                        key={commit.oid}
                        onClick={() => {
                          setSelectedCommitOid(commit.oid);
                          if (isRangeMode) {
                            if (!pinA || (pinA && pinB)) {
                              setPinA(commit.oid);
                              setPinB(null);
                            } else {
                              setPinB(commit.oid);
                            }
                          }
                        }}
                        className={`p-3 rounded-xl border transition-all cursor-pointer relative group ${
                          isSelected
                            ? 'bg-violet-600/10 border-violet-500/50 shadow-sm'
                            : isPinA || isPinB
                            ? 'bg-amber-500/10 border-amber-500/40'
                            : 'bg-[var(--card)] hover:bg-[var(--card-elevated)] border-[var(--border-subtle)]'
                        }`}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="flex items-start space-x-3">
                            <div className="flex flex-col items-center mt-0.5">
                              <div
                                className={`w-3 h-3 rounded-full border-2 ${
                                  isSelected
                                    ? 'bg-violet-500 border-white'
                                    : commit.is_head
                                    ? 'bg-emerald-500 border-white'
                                    : 'bg-[var(--card-elevated)] border-violet-400'
                                }`}
                              />
                              <div className="w-0.5 h-6 bg-[var(--border-subtle)] my-1" />
                            </div>

                            <div>
                              <div className="flex flex-wrap items-center gap-1.5">
                                <span className="font-bold text-xs text-[var(--text-primary)] font-mono">
                                  {commit.short_oid}
                                </span>
                                {commit.is_head && (
                                  <span className="px-1.5 py-0.2 rounded text-[10px] font-mono font-bold bg-emerald-500/15 text-emerald-500 border border-emerald-500/30">
                                    HEAD
                                  </span>
                                )}
                                {(commit.branches || []).map((b) => (
                                  <span
                                    key={b}
                                    className="px-1.5 py-0.2 rounded text-[10px] font-mono font-bold bg-sky-500/15 text-sky-500 border border-sky-500/30 flex items-center space-x-1"
                                  >
                                    <GitBranch className="w-2.5 h-2.5 inline" />
                                    <span>{b}</span>
                                  </span>
                                ))}
                                {(commit.tags || []).map((t) => (
                                  <span
                                    key={t}
                                    className="px-1.5 py-0.2 rounded text-[10px] font-mono font-bold bg-purple-500/15 text-purple-500 border border-purple-500/30 flex items-center space-x-1"
                                  >
                                    <Tag className="w-2.5 h-2.5 inline" />
                                    <span>{t}</span>
                                  </span>
                                ))}
                                {isPinA && (
                                  <span className="px-1.5 py-0.2 rounded text-[10px] font-mono font-bold bg-amber-500 text-black">
                                    PIN A (Base)
                                  </span>
                                )}
                                {isPinB && (
                                  <span className="px-1.5 py-0.2 rounded text-[10px] font-mono font-bold bg-amber-500 text-black">
                                    PIN B (Target)
                                  </span>
                                )}
                              </div>

                              <p className="text-xs font-medium text-[var(--text-primary)] mt-1 line-clamp-1">
                                {commit.summary}
                              </p>

                              <div className="flex items-center space-x-3 text-[11px] font-mono text-[var(--text-muted)] mt-1">
                                <span>{commit.author_name}</span>
                                <span>&bull;</span>
                                <span>{formatDate(commit.timestamp)}</span>
                              </div>
                            </div>
                          </div>

                          <div className="flex flex-col items-end space-y-1 shrink-0">
                            <span
                              className={`px-2 py-0.5 rounded text-[10px] font-mono font-bold border ${
                                commit.verdict === 'PASS'
                                  ? 'bg-emerald-500/15 text-emerald-500 border-emerald-500/30'
                                  : commit.verdict === 'WARN'
                                  ? 'bg-amber-500/15 text-amber-500 border-amber-500/30'
                                  : 'bg-rose-500/15 text-rose-500 border-rose-500/30'
                              }`}
                            >
                              {commit.verdict || 'PASS'}
                            </span>

                            <div className="flex items-center space-x-1 text-[10px] font-mono text-[var(--text-muted)]">
                              <span>
                                <strong>{commit.symbols_modified}</strong> syms
                              </span>
                              <span>&bull;</span>
                              <span>
                                Blast: <strong>{commit.blast_radius_score}</strong>
                              </span>
                            </div>
                          </div>
                        </div>
                      </div>
                    );
                  })
                )}
              </div>
            </div>

            {/* Right Column: Time-Travel Virtual Canvas & Range Diff Report (5 Cols) */}
            <div className="lg:col-span-5 flex flex-col gap-3 min-h-0 lg:overflow-y-auto pr-1">
              {/* Virtual Time-Travel Inspector Card */}
              <div className="p-4 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm space-y-3">
                <div className="flex items-center justify-between border-b border-[var(--border-subtle)] pb-2">
                  <div className="flex items-center space-x-2 text-xs font-mono font-semibold text-[var(--text-secondary)]">
                    <Clock className="w-3.5 h-3.5 text-violet-500" />
                    <span>Virtual Time-Travel Snapshot</span>
                  </div>
                  {onNavigateUniverse && (
                    <button
                      onClick={onNavigateUniverse}
                      className="text-xs font-mono text-violet-500 hover:underline flex items-center space-x-1 cursor-pointer"
                    >
                      <span>Open 3D Universe</span>
                      <ChevronRight className="w-3 h-3" />
                    </button>
                  )}
                </div>

                {selectedCommit ? (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between text-xs font-mono">
                      <span className="text-[var(--text-muted)]">Commit:</span>
                      <span className="font-bold text-[var(--text-primary)]">
                        {selectedCommit.oid.slice(0, 10)}...
                      </span>
                    </div>
                    <div className="flex items-center justify-between text-xs font-mono">
                      <span className="text-[var(--text-muted)]">Summary:</span>
                      <span className="font-semibold text-[var(--text-primary)] line-clamp-1">
                        {selectedCommit.summary}
                      </span>
                    </div>

                    {/* Quick Historical Actions */}
                    <div className="flex items-center space-x-2 pt-2 border-t border-[var(--border-subtle)]">
                      <button
                        onClick={() => handleCheckoutCommit(selectedCommit.oid)}
                        disabled={isMutating}
                        title="Detach working tree at this commit with undo safety stack"
                        className="flex-1 py-1 px-2 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-violet-500/40 text-violet-400 hover:text-violet-300 transition-all font-mono text-xs flex items-center justify-center space-x-1 cursor-pointer disabled:opacity-50"
                      >
                        <FastForward className="w-3 h-3" />
                        <span>Checkout / Detach</span>
                      </button>

                      <button
                        onClick={() => handleRevertCommit(selectedCommit.oid)}
                        disabled={isMutating}
                        title="Cleanly revert this commit with blast radius verification"
                        className="py-1 px-2.5 rounded-lg bg-rose-500/10 border border-rose-500/30 hover:bg-rose-500/20 text-rose-400 transition-all font-mono text-xs flex items-center space-x-1 cursor-pointer disabled:opacity-50"
                      >
                        <RotateCcw className="w-3 h-3" />
                        <span>Revert</span>
                      </button>
                    </div>

                    {activeFrame && (
                      <div className="grid grid-cols-3 gap-2 pt-2 border-t border-[var(--border-subtle)]">
                        <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-center">
                          <div className="text-xs font-mono font-bold text-emerald-500">
                            +{activeFrame.added_files.length}
                          </div>
                          <div className="text-[10px] font-mono text-[var(--text-muted)]">Added</div>
                        </div>
                        <div className="p-2 rounded-lg bg-sky-500/10 border border-sky-500/20 text-center">
                          <div className="text-xs font-mono font-bold text-sky-500">
                            ~{activeFrame.modified_files.length}
                          </div>
                          <div className="text-[10px] font-mono text-[var(--text-muted)]">Modified</div>
                        </div>
                        <div className="p-2 rounded-lg bg-rose-500/10 border border-rose-500/20 text-center">
                          <div className="text-xs font-mono font-bold text-rose-500">
                            -{activeFrame.deleted_files.length}
                          </div>
                          <div className="text-[10px] font-mono text-[var(--text-muted)]">Deleted</div>
                        </div>
                      </div>
                    )}
                  </div>
                ) : (
                  <div className="text-xs font-mono text-[var(--text-muted)] text-center py-4">
                    Select any commit in the DAG to preview in time-travel
                  </div>
                )}
              </div>

              {/* Range Diff Report Card */}
              {isRangeMode && (
                <div className="p-4 rounded-xl bg-amber-500/5 border border-amber-500/25 shadow-sm space-y-3">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-mono font-bold text-amber-500">
                      Range Structural Blast (A ↔ B)
                    </span>
                    <span
                      className={`px-2 py-0.5 rounded text-[10px] font-mono font-bold ${
                        diffReport?.verdict === 'PASS'
                          ? 'bg-emerald-500/15 text-emerald-500 border border-emerald-500/30'
                          : 'bg-rose-500/15 text-rose-500 border border-rose-500/30'
                      }`}
                    >
                      {diffReport?.verdict || 'CALCULATING...'}
                    </span>
                  </div>

                  {diffReport && (
                    <div className="text-xs font-mono space-y-1 text-[var(--text-secondary)]">
                      <div>
                        Changed Files: <strong>{diffReport.total_changed_files}</strong> &bull; Blast Radius Score:{' '}
                        <strong>{diffReport.blast_radius_score}</strong>
                      </div>
                      {diffReport.drifted_contracts && diffReport.drifted_contracts.length > 0 && (
                        <div className="pt-2 text-rose-500">
                          <strong>Drifted Contracts:</strong>
                          <ul className="list-disc list-inside mt-1 text-[11px]">
                            {(diffReport.drifted_contracts || []).map((c, i) => (
                              <li key={i}>{c}</li>
                            ))}
                          </ul>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>

          {/* 3. Bottom Time-Travel Scrubbing Slider Bar */}
          {commitsList.length > 0 && (
            <div className="shrink-0 p-3 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] shadow-md space-y-2 z-10">
              <div className="flex items-center justify-between text-xs font-mono">
                <div className="flex items-center space-x-2 text-[var(--text-secondary)] min-w-0">
                  <Clock className="w-3.5 h-3.5 text-violet-500 shrink-0" />
                  <span className="font-semibold text-violet-400 shrink-0">Time-Travel Timeline Playhead:</span>
                  <span className="font-bold text-[var(--text-primary)] truncate max-w-[380px]">
                    {selectedCommit?.short_oid} &bull; {selectedCommit?.summary}
                  </span>
                </div>
                <div className="text-[11px] text-[var(--text-muted)] font-mono shrink-0 ml-2">
                  Scrubbing position <strong className="text-[var(--text-primary)]">{sliderIndex + 1}</strong> of <strong className="text-[var(--text-primary)]">{commitsList.length}</strong>
                </div>
              </div>

              <div className="flex items-center space-x-3">
                <span className="text-[10px] font-mono text-[var(--text-muted)] font-semibold shrink-0">git init</span>
                <input
                  type="range"
                  min={0}
                  max={Math.max(commitsList.length - 1, 0)}
                  value={sliderIndex}
                  onChange={handleSliderChange}
                  className="flex-1 accent-violet-500 cursor-pointer h-1.5 rounded-lg bg-[var(--card)]"
                />
                <span className="text-[10px] font-mono text-emerald-500 font-bold shrink-0">HEAD</span>
              </div>
            </div>
          )}
        </>
      )}

      {/* ======================================================== */}
      {/* TAB 2: WORKING TREE, UNCOMMITTED CHANGES & STAGING STUDIO */}
      {/* ======================================================== */}
      {activeTab === 'status' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-3 flex-1 min-h-0 overflow-y-auto lg:overflow-hidden">
          {/* Left Column: Uncommitted Files & Stage Controls (5 Cols) */}
          <div className="lg:col-span-5 flex flex-col rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] overflow-hidden shadow-sm min-h-0">
            {/* Header with Stage All / Unstage All */}
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between shrink-0">
              <div className="flex items-center space-x-2 text-xs font-mono font-semibold text-[var(--text-secondary)]">
                <FileDiff className="w-3.5 h-3.5 text-violet-500" />
                <span>Working Tree Changes ({gitStatus?.total_uncommitted || 0})</span>
              </div>

              <div className="flex items-center space-x-1">
                <button
                  onClick={handleStageAll}
                  disabled={isMutating || !gitStatus?.total_uncommitted}
                  className="px-2 py-1 rounded text-[11px] font-mono font-semibold bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-400 border border-emerald-500/30 transition-all cursor-pointer disabled:opacity-40"
                >
                  Stage All
                </button>
                <button
                  onClick={handleUnstageAll}
                  disabled={isMutating || !gitStatus?.staged.length}
                  className="px-2 py-1 rounded text-[11px] font-mono font-semibold bg-white/5 hover:bg-white/10 text-[var(--text-muted)] hover:text-white border border-[var(--border-subtle)] transition-all cursor-pointer disabled:opacity-40"
                >
                  Unstage All
                </button>
              </div>
            </div>

            {/* Scrollable List of Files */}
            <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
              {/* STAGED SECTION */}
              <div>
                <div className="flex items-center justify-between text-[11px] font-bold text-emerald-400 uppercase tracking-wider mb-1.5">
                  <span>Staged Changes ({(gitStatus?.staged || []).length})</span>
                </div>
                {(gitStatus?.staged || []).length === 0 ? (
                  <div className="text-[11px] text-[var(--text-muted)] italic pl-2">No files currently staged</div>
                ) : (
                  <div className="space-y-1">
                    {(gitStatus?.staged || []).map((f) => (
                      <div
                        key={f.path}
                        onClick={() => setActiveDiffPath(f.path)}
                        className={`flex items-center justify-between p-2 rounded-lg border transition-all cursor-pointer ${
                          activeDiffPath === f.path
                            ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300 font-semibold'
                            : 'bg-[var(--card)] hover:bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-primary)]'
                        }`}
                      >
                        <div className="flex items-center space-x-2 truncate">
                          <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                          <span className="truncate">{f.path}</span>
                        </div>
                        <div className="flex items-center space-x-2 shrink-0 ml-2">
                          <span className="text-[10px] text-emerald-400 font-bold">+{f.added_lines}</span>
                          <span className="text-[10px] text-rose-400 font-bold">-{f.deleted_lines}</span>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              handleUnstageFile(f.path);
                            }}
                            title="Unstage file"
                            className="p-1 rounded hover:bg-rose-500/20 text-[var(--text-muted)] hover:text-rose-400"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* UNSTAGED MODIFIED SECTION */}
              <div className="pt-2 border-t border-[var(--border-subtle)]">
                <div className="flex items-center justify-between text-[11px] font-bold text-amber-400 uppercase tracking-wider mb-1.5">
                  <span>Modified Working Files ({(gitStatus?.unstaged || []).length})</span>
                </div>
                {(gitStatus?.unstaged || []).length === 0 ? (
                  <div className="text-[11px] text-[var(--text-muted)] italic pl-2">No unstaged modifications</div>
                ) : (
                  <div className="space-y-1">
                    {(gitStatus?.unstaged || []).map((f) => (
                      <div
                        key={f.path}
                        onClick={() => setActiveDiffPath(f.path)}
                        className={`flex items-center justify-between p-2 rounded-lg border transition-all cursor-pointer ${
                          activeDiffPath === f.path
                            ? 'bg-amber-500/15 border-amber-500/40 text-amber-300 font-semibold'
                            : 'bg-[var(--card)] hover:bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-primary)]'
                        }`}
                      >
                        <div className="flex items-center space-x-2 truncate">
                          <span className="w-1.5 h-1.5 rounded-full bg-amber-500" />
                          <span className="truncate">{f.path}</span>
                        </div>
                        <div className="flex items-center space-x-2 shrink-0 ml-2">
                          <span className="text-[10px] text-emerald-400 font-bold">+{f.added_lines}</span>
                          <span className="text-[10px] text-rose-400 font-bold">-{f.deleted_lines}</span>
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              handleStageFile(f.path);
                            }}
                            title="Stage file"
                            className="p-1 rounded hover:bg-emerald-500/20 text-emerald-400"
                          >
                            <Plus className="w-3 h-3" />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* UNTRACKED SECTION */}
              {gitStatus && (gitStatus.untracked || []).length > 0 && (
                <div className="pt-2 border-t border-[var(--border-subtle)]">
                  <div className="flex items-center justify-between text-[11px] font-bold text-emerald-500 uppercase tracking-wider mb-1.5">
                    <span>Untracked Files ({(gitStatus.untracked || []).length})</span>
                  </div>
                  <div className="space-y-1">
                    {(gitStatus.untracked || []).map((f) => (
                      <div
                        key={f.path}
                        onClick={() => setActiveDiffPath(f.path)}
                        className={`flex items-center justify-between p-2 rounded-lg border transition-all cursor-pointer ${
                          activeDiffPath === f.path
                            ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300 font-semibold'
                            : 'bg-[var(--card)] hover:bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-primary)]'
                        }`}
                      >
                        <div className="flex items-center space-x-2 truncate">
                          <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                          <span className="truncate">{f.path}</span>
                        </div>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleStageFile(f.path);
                          }}
                          title="Stage file"
                          className="p-1 rounded hover:bg-emerald-500/20 text-emerald-400"
                        >
                          <Plus className="w-3 h-3" />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Commit Box at Bottom of Left Column */}
            <div className="p-3 border-t border-[var(--border-subtle)] bg-[var(--card-elevated)] space-y-2 shrink-0">
              <div className="flex items-center space-x-2">
                <select
                  value={commitType}
                  onChange={(e) => setCommitType(e.target.value)}
                  className="px-2 py-1.5 rounded-lg text-xs font-mono font-bold bg-[var(--card)] border border-[var(--border-subtle)] text-violet-400 focus:outline-none"
                >
                  <option value="feat">feat</option>
                  <option value="fix">fix</option>
                  <option value="refactor">refactor</option>
                  <option value="chore">chore</option>
                  <option value="docs">docs</option>
                  <option value="perf">perf</option>
                  <option value="test">test</option>
                </select>

                <input
                  type="text"
                  placeholder="Commit message summary..."
                  value={commitMessage}
                  onChange={(e) => setCommitMessage(e.target.value)}
                  className="flex-1 px-3 py-1.5 rounded-lg text-xs font-mono bg-[var(--card)] border border-[var(--border-subtle)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-violet-500"
                />
              </div>

              <div className="flex items-center space-x-2">
                <input
                  type="text"
                  placeholder="Optional Release Tag (e.g. v0.4.1)..."
                  value={commitTag}
                  onChange={(e) => setCommitTag(e.target.value)}
                  className="w-48 px-2.5 py-1.5 rounded-lg text-xs font-mono bg-[var(--card)] border border-[var(--border-subtle)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-purple-500"
                />

                <button
                  onClick={handleCommit}
                  disabled={isMutating || !commitMessage.trim()}
                  className="flex-1 py-1.5 px-3 rounded-lg text-xs font-mono font-bold bg-violet-600 hover:bg-violet-500 text-white transition-all cursor-pointer disabled:opacity-50 flex items-center justify-center space-x-1.5 shadow-sm"
                >
                  <ShieldCheck className="w-3.5 h-3.5" />
                  <span>Verify &amp; Commit</span>
                </button>
              </div>
            </div>
          </div>

          {/* Right Column: Unified Line-by-Line Code Diff Inspector (7 Cols) */}
          <div className="lg:col-span-7 flex flex-col rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] overflow-hidden shadow-sm min-h-0">
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between shrink-0">
              <div className="flex items-center space-x-2 truncate">
                <FileCode className="w-3.5 h-3.5 text-violet-500 shrink-0" />
                <span className="font-semibold text-xs text-[var(--text-primary)] truncate">
                  {activeDiffPath || 'No file selected for diff'}
                </span>
                {fileDiff && (
                  <span className="text-[10px] text-[var(--text-muted)] shrink-0 ml-2">
                    +{fileDiff.total_additions} / -{fileDiff.total_deletions}
                  </span>
                )}
              </div>

              {activeDiffPath && (
                <div className="flex items-center space-x-2 shrink-0">
                  <button
                    onClick={() => handleStageFile(activeDiffPath)}
                    className="px-2.5 py-1 rounded text-xs font-mono font-semibold bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-400 border border-emerald-500/30 transition-all cursor-pointer"
                  >
                    Stage File
                  </button>
                </div>
              )}
            </div>

            {/* Diff Lines Scroll View */}
            <div className="flex-1 min-h-0 overflow-y-auto p-3 font-mono text-[11px] leading-relaxed bg-[#05080e]">
              {diffFileLoading ? (
                <div className="flex items-center justify-center h-48 text-[var(--text-muted)] space-x-2">
                  <RefreshCw className="w-4 h-4 animate-spin text-violet-500" />
                  <span>Evaluating pure-Rust line diff...</span>
                </div>
              ) : !fileDiff || !fileDiff.diff_lines || fileDiff.diff_lines.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-48 text-[var(--text-muted)] italic">
                  Select any modified or untracked file to inspect line-level diffs
                </div>
              ) : (
                <div className="space-y-0.5">
                  {(fileDiff.diff_lines || []).map((line, idx) => (
                    <div
                      key={idx}
                      className={`flex items-start px-2 py-0.5 rounded ${
                        line.kind === 'add'
                          ? 'bg-emerald-500/15 text-emerald-300'
                          : line.kind === 'del'
                          ? 'bg-rose-500/15 text-rose-300 line-through opacity-80'
                          : 'text-gray-400'
                      }`}
                    >
                      <span className="w-8 shrink-0 text-gray-600 text-right select-none pr-2">
                        {line.old_line || ''}
                      </span>
                      <span className="w-8 shrink-0 text-gray-600 text-right select-none pr-3">
                        {line.new_line || ''}
                      </span>
                      <span className="w-4 shrink-0 font-bold select-none">
                        {line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}
                      </span>
                      <pre className="flex-1 whitespace-pre-wrap break-all font-mono">{line.text}</pre>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* ======================================================== */}
      {/* TAB 3: BRANCHES & TAGS STUDIO                            */}
      {/* ======================================================== */}
      {activeTab === 'branches' && (
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-3 flex-1 min-h-0 overflow-y-auto lg:overflow-hidden">
          {/* Left Column: Branch Operations & List (6 Cols) */}
          <div className="lg:col-span-6 flex flex-col rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] overflow-hidden shadow-sm min-h-0">
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between shrink-0">
              <div className="flex items-center space-x-2 text-xs font-mono font-semibold text-[var(--text-secondary)]">
                <GitBranch className="w-3.5 h-3.5 text-violet-500" />
                <span>Local Branches ({branchesList.length})</span>
              </div>
            </div>

            {/* Create Branch Row */}
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card)] flex items-center space-x-2 shrink-0">
              <input
                type="text"
                placeholder="new-feature-branch..."
                value={newBranchName}
                onChange={(e) => setNewBranchName(e.target.value)}
                className="flex-1 px-3 py-1.5 rounded-lg text-xs font-mono bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-violet-500"
              />
              <button
                onClick={() => handleBranchAction(newBranchName, 'create')}
                disabled={isMutating || !newBranchName.trim()}
                className="px-3 py-1.5 rounded-lg text-xs font-mono font-bold bg-violet-600 hover:bg-violet-500 text-white transition-all cursor-pointer disabled:opacity-50"
              >
                Create Branch
              </button>
            </div>

            {/* Merge Branch Section */}
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center space-x-2 shrink-0">
              <span className="text-[11px] text-[var(--text-muted)] shrink-0">Merge into {galaxy?.active_branch}:</span>
              <select
                value={mergeSourceBranch}
                onChange={(e) => setMergeSourceBranch(e.target.value)}
                className="flex-1 px-2.5 py-1 rounded-lg text-xs font-mono bg-[var(--card)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none"
              >
                <option value="">Select branch to merge...</option>
                {branchesList
                  .filter((b) => b.name !== galaxy?.active_branch)
                  .map((b) => (
                    <option key={b.name} value={b.name}>
                      {b.name} ({b.target_oid.slice(0, 7)})
                    </option>
                  ))}
              </select>
              <button
                onClick={handleMergeBranch}
                disabled={isMutating || !mergeSourceBranch}
                className="px-3 py-1 rounded-lg text-xs font-mono font-bold bg-emerald-600 hover:bg-emerald-500 text-white transition-all cursor-pointer disabled:opacity-50"
              >
                Merge
              </button>
            </div>

            {/* Branches List */}
            <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-1.5">
              {branchesList.map((branch) => {
                const isActive = branch.name === galaxy?.active_branch;
                return (
                  <div
                    key={branch.name}
                    className={`flex items-center justify-between p-2.5 rounded-xl border transition-all ${
                      isActive
                        ? 'bg-violet-600/15 border-violet-500/40 text-violet-300 font-semibold'
                        : 'bg-[var(--card)] hover:bg-[var(--card-elevated)] border-[var(--border-subtle)] text-[var(--text-primary)]'
                    }`}
                  >
                    <div className="flex items-center space-x-2">
                      <GitBranch className={`w-3.5 h-3.5 ${isActive ? 'text-violet-400' : 'text-gray-500'}`} />
                      <span>{branch.name}</span>
                      {isActive && (
                        <span className="text-[9px] font-bold px-1.5 py-0.2 rounded bg-violet-600 text-white">
                          CURRENT
                        </span>
                      )}
                    </div>

                    <div className="flex items-center space-x-2">
                      <span className="text-[10px] font-mono text-[var(--text-muted)]">
                        {branch.target_oid.slice(0, 7)}
                      </span>
                      {!isActive && (
                        <>
                          <button
                            onClick={() => handleBranchAction(branch.name, 'switch')}
                            disabled={isMutating}
                            className="px-2 py-0.5 rounded text-[11px] font-semibold bg-white/5 hover:bg-sky-500/20 text-sky-400 border border-sky-500/30 cursor-pointer"
                          >
                            Switch
                          </button>
                          <button
                            onClick={() => handleBranchAction(branch.name, 'delete')}
                            disabled={isMutating}
                            className="p-1 rounded text-rose-400 hover:bg-rose-500/20 cursor-pointer"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Right Column: Release Tags Studio (6 Cols) */}
          <div className="lg:col-span-6 flex flex-col rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] overflow-hidden shadow-sm min-h-0">
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between shrink-0">
              <div className="flex items-center space-x-2 text-xs font-mono font-semibold text-[var(--text-secondary)]">
                <Tag className="w-3.5 h-3.5 text-purple-400" />
                <span>Semantic &amp; Release Tags ({tagsList.length})</span>
              </div>
            </div>

            {/* Create Tag Form */}
            <div className="p-3 border-b border-[var(--border-subtle)] bg-[var(--card)] flex items-center space-x-2 shrink-0">
              <input
                type="text"
                placeholder="v1.0.0..."
                value={newTagName}
                onChange={(e) => setNewTagName(e.target.value)}
                className="flex-1 px-3 py-1.5 rounded-lg text-xs font-mono bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-purple-500"
              />
              <button
                onClick={handleCreateTag}
                disabled={isMutating || !newTagName.trim()}
                className="px-3 py-1.5 rounded-lg text-xs font-mono font-bold bg-purple-600 hover:bg-purple-500 text-white transition-all cursor-pointer disabled:opacity-50"
              >
                Tag Commit
              </button>
            </div>

            {/* Tags List */}
            <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-1.5">
              {tagsList.length === 0 ? (
                <div className="p-4 text-center text-[var(--text-muted)] italic">
                  No release tags created yet. Tag your HEAD commit above!
                </div>
              ) : (
                tagsList.map((tag) => (
                  <div
                    key={tag.name}
                    className="flex items-center justify-between p-2.5 rounded-xl bg-[var(--card)] hover:bg-[var(--card-elevated)] border border-[var(--border-subtle)]"
                  >
                    <div className="flex items-center space-x-2 text-purple-400 font-bold">
                      <Tag className="w-3.5 h-3.5" />
                      <span>{tag.name}</span>
                    </div>

                    <div className="flex items-center space-x-2">
                      <span className="text-[10px] font-mono text-[var(--text-muted)]">
                        {tag.target_oid.slice(0, 7)}
                      </span>
                      <button
                        onClick={() => handleDeleteTag(tag.name)}
                        disabled={isMutating}
                        className="p-1 rounded text-rose-400 hover:bg-rose-500/20 cursor-pointer"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* ======================================================== */}
      {/* TAB 4: AUTOPSY BISECT ASSISTANT (FR-P3-007)              */}
      {/* ======================================================== */}
      {activeTab === 'bisect' && (
        <div className="flex flex-col flex-1 min-h-0 rounded-xl bg-[var(--card)] border border-[var(--border-subtle)] overflow-hidden shadow-sm p-4 space-y-4">
          <div className="flex items-center space-x-3 border-b border-[var(--border-subtle)] pb-3 shrink-0">
            <div className="w-8 h-8 rounded-lg bg-amber-500/15 text-amber-500 border border-amber-500/30 flex items-center justify-center font-bold">
              <Search className="w-4 h-4" />
            </div>
            <div>
              <h3 className="text-sm font-bold text-[var(--text-primary)]">
                Autopsy Visual Bisect Engine (FR-P3-007)
              </h3>
              <p className="text-xs text-[var(--text-muted)]">
                Automatically walk commit history to pinpoint the exact commit where invariants or contract policies drifted from PASS to FAIL.
              </p>
            </div>
          </div>

          {/* Bisect Inputs */}
          <div className="grid grid-cols-1 md:grid-cols-12 gap-3 shrink-0">
            <div className="md:col-span-5 space-y-1">
              <label className="text-[11px] text-[var(--text-muted)] block">Good Base Commit (Start OID):</label>
              <input
                type="text"
                value={bisectStartOid}
                onChange={(e) => setBisectStartOid(e.target.value)}
                placeholder="Good baseline commit OID..."
                className="w-full px-3 py-1.5 rounded-lg text-xs font-mono bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)]"
              />
            </div>

            <div className="md:col-span-5 space-y-1">
              <label className="text-[11px] text-[var(--text-muted)] block">Breaking Target Commit (End OID / HEAD):</label>
              <input
                type="text"
                value={bisectEndOid}
                onChange={(e) => setBisectEndOid(e.target.value)}
                placeholder="Breaking target commit OID..."
                className="w-full px-3 py-1.5 rounded-lg text-xs font-mono bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)]"
              />
            </div>

            <div className="md:col-span-2 flex items-end">
              <button
                onClick={handleRunBisect}
                disabled={isBisecting || !bisectStartOid || !bisectEndOid}
                className="w-full py-2 px-3 rounded-lg text-xs font-mono font-bold bg-amber-600 hover:bg-amber-500 text-black transition-all cursor-pointer disabled:opacity-50 flex items-center justify-center space-x-1"
              >
                <Play className="w-3.5 h-3.5 fill-black" />
                <span>{isBisecting ? 'Bisecting...' : 'Run Bisect'}</span>
              </button>
            </div>
          </div>

          {/* Bisect Results Display */}
          <div className="flex-1 min-h-0 overflow-y-auto rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] p-4 font-mono">
            {isBisecting ? (
              <div className="flex flex-col items-center justify-center h-48 text-[var(--text-muted)] space-y-2">
                <RefreshCw className="w-6 h-6 animate-spin text-amber-500" />
                <span>Evaluating invariant rules across commit range...</span>
              </div>
            ) : !bisectReport ? (
              <div className="flex flex-col items-center justify-center h-48 text-[var(--text-muted)] italic">
                Set Start OID and End OID above and click &quot;Run Bisect&quot; to locate breaking contract regressions.
              </div>
            ) : (
              <div className="space-y-4">
                <div className="flex items-center justify-between p-3 rounded-xl border bg-[var(--card)]">
                  <div className="flex items-center space-x-2">
                    {bisectReport.verdict === 'PASS' ? (
                      <CheckCircle2 className="w-5 h-5 text-emerald-500" />
                    ) : (
                      <AlertTriangle className="w-5 h-5 text-rose-500" />
                    )}
                    <div>
                      <div className="font-bold text-sm text-[var(--text-primary)]">
                        {bisectReport.verdict === 'FAIL' ? 'Breaking Commit Identified' : 'All Commits Passed Invariants'}
                      </div>
                      <div className="text-xs text-[var(--text-muted)]">
                        Scanned {bisectReport.commits_evaluated} commits in &lt;1.8s
                      </div>
                    </div>
                  </div>

                  <span
                    className={`px-3 py-1 rounded text-xs font-bold ${
                      bisectReport.verdict === 'PASS'
                        ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/40'
                        : 'bg-rose-500/20 text-rose-400 border border-rose-500/40'
                    }`}
                  >
                    {bisectReport.verdict}
                  </span>
                </div>

                {bisectReport.breaking_commit_oid && (
                  <div className="p-4 rounded-xl border border-rose-500/30 bg-rose-500/5 space-y-2">
                    <div className="text-xs font-bold text-rose-400 uppercase tracking-wider">
                      First Breaking Commit:
                    </div>
                    <div className="flex items-center space-x-2 text-sm font-bold text-[var(--text-primary)]">
                      <span>{bisectReport.breaking_commit_oid}</span>
                    </div>
                    <p className="text-xs text-[var(--text-secondary)]">{bisectReport.breaking_summary}</p>
                    <div className="text-[11px] text-[var(--text-muted)]">
                      Author: {bisectReport.breaking_author || 'Unknown'} &bull; Date:{' '}
                      {formatDate(bisectReport.breaking_timestamp)}
                    </div>
                    <div className="pt-2">
                      <button
                        onClick={() => handleRevertCommit(bisectReport.breaking_commit_oid!)}
                        className="px-3 py-1.5 rounded-lg bg-rose-600 hover:bg-rose-500 text-white font-bold text-xs cursor-pointer transition-all"
                      >
                        Revert This Commit With SynEvid Verification
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
