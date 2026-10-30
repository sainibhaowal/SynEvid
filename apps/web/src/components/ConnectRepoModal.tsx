'use client';

import React, { useState, useEffect, useRef } from 'react';
import {
  X,
  FolderGit2,
  Folder,
  FolderOpen,
  ArrowUp,
  HardDrive,
  CheckCircle2,
  RefreshCw,
  FolderSearch,
  ExternalLink,
} from 'lucide-react';
import { api, FsBrowseResponse, FsDirectoryEntry, normalizeCrossPlatformPath } from '@/lib/api';

interface ConnectRepoModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConnected: (newRepoId?: string) => void;
}

export default function ConnectRepoModal({ isOpen, onClose, onConnected }: ConnectRepoModalProps) {
  const [rootPath, setRootPath] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState('');

  // Filesystem Explorer state
  const [fsData, setFsData] = useState<FsBrowseResponse | null>(null);
  const [isLoadingFs, setIsLoadingFs] = useState(false);
  const [isBrowserOpen, setIsBrowserOpen] = useState(true);

  const fileInputRef = useRef<HTMLInputElement | null>(null);

  // Load initial filesystem roots/workspace when modal opens
  useEffect(() => {
    if (isOpen) {
      loadDirectory(rootPath || undefined);
    }
  }, [isOpen]);

  const loadDirectory = async (targetPath?: string) => {
    setIsLoadingFs(true);
    setErrorMsg('');
    try {
      const data = await api.browseFilesystem(targetPath);
      setFsData(data);
      // Invariant: NEVER auto-populate rootPath with default paths on open!
      // Keep rootPath strictly controlled by user input or explicit selection.
    } catch (err: any) {
      // Fallback handled inside api.ts
    } finally {
      setIsLoadingFs(false);
    }
  };

  const handleSelectFolder = (path: string, name?: string) => {
    setRootPath(path);
    if (name && !displayName) {
      setDisplayName(name);
    }
    loadDirectory(path);
  };

  const handleNativePicker = async () => {
    // 0. If running inside Electron desktop shell, use native OS folder dialog directly
    if (typeof window !== 'undefined' && (window as any).synevidDesktop?.openDirectory) {
      try {
        const pickedPath = await (window as any).synevidDesktop.openDirectory();
        if (pickedPath) {
          const normalized = normalizeCrossPlatformPath(pickedPath);
          setRootPath(normalized);
          const name = normalized.split(/[/\\]/).filter(Boolean).pop() || normalized;
          if (!displayName) setDisplayName(name);
          loadDirectory(normalized);
          return;
        }
      } catch (e) {
        console.warn('Native desktop picker error:', e);
      }
    }

    // 1. Try modern File System Access API (Chromium, Edge, Chrome, Antigravity IDE)
    if (typeof window !== 'undefined' && 'showDirectoryPicker' in window) {
      try {
        const dirHandle = await (window as any).showDirectoryPicker();
        if (dirHandle && dirHandle.name) {
          // If user picked a directory, query backend for that name under current or parent
          const prospective = fsData?.current_path
            ? `${fsData.current_path.replace(/\/+$/, '')}/${dirHandle.name}`
            : dirHandle.name;
          setRootPath(prospective);
          if (!displayName) setDisplayName(dirHandle.name);
          loadDirectory(prospective);
          return;
        }
      } catch (err: any) {
        // User cancelled or browser rejected
        if (err?.name === 'AbortError') return;
      }
    }

    // 2. Fallback to hidden directory file input
    if (fileInputRef.current) {
      fileInputRef.current.click();
    }
  };

  const handleFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      const firstFile = files[0];
      const relPath = firstFile.webkitRelativePath || '';
      const topDir = relPath.split('/')[0];
      if (topDir) {
        const prospective = fsData?.current_path
          ? `${fsData.current_path.replace(/\/+$/, '')}/${topDir}`
          : topDir;
        setRootPath(prospective);
        if (!displayName) setDisplayName(topDir);
        loadDirectory(prospective);
      }
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!rootPath.trim()) return;

    setIsSubmitting(true);
    setErrorMsg('');
    try {
      const normalized = normalizeCrossPlatformPath(rootPath.trim());
      const res = await api.connectRepo(normalized, displayName.trim() || undefined);
      if (res.status === 'connected') {
        onConnected(res.repo_id);
        onClose();
      } else {
        setErrorMsg(res.error || 'Failed to connect repository');
      }
    } catch (err: any) {
      setErrorMsg(err?.message || 'Error communicating with daemon');
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-fadeIn">
      {/* Hidden native directory input fallback */}
      <input
        type="file"
        ref={fileInputRef}
        onChange={handleFileInputChange}
        // @ts-expect-error standard webkit directory attributes
        webkitdirectory="true"
        directory=""
        className="hidden"
      />

      <div className="w-full max-w-xl rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] p-5 md:p-6 shadow-2xl space-y-4">
        {/* Header */}
        <div className="flex items-center justify-between pb-3 border-b border-[var(--border-subtle)]">
          <div className="flex items-center space-x-2.5">
            <div className="w-8 h-8 rounded-xl overflow-hidden border border-[var(--border-subtle)] bg-[var(--card-elevated)] p-1.5 flex items-center justify-center shadow-sm">
              <FolderGit2 className="w-full h-full text-sky-600 dark:text-cyan-400" />
            </div>
            <div>
              <h3 className="text-sm font-bold text-[var(--text-primary)]">Connect Local Repository</h3>
              <p className="text-[11px] text-[var(--text-muted)]">Select or browse any codebase on this machine</p>
            </div>
          </div>
          <button
            onClick={onClose}
            aria-label="Close connect repository dialog"
            className="p-1.5 rounded-xl text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--card-elevated)] transition-colors cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4 text-xs font-mono">
          {/* Path Input & Native OS Picker Button */}
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label className="text-[var(--text-secondary)] font-semibold flex items-center gap-1.5">
                <FolderOpen className="w-3.5 h-3.5 text-sky-600 dark:text-cyan-400" />
                Folder Root Path (Absolute OS Path):
              </label>
              <button
                type="button"
                onClick={handleNativePicker}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-sky-50 dark:bg-cyan-600/15 hover:bg-sky-100 dark:hover:bg-cyan-600/25 border border-sky-300 dark:border-cyan-500/30 text-sky-700 dark:text-cyan-400 text-[11px] font-semibold transition-colors cursor-pointer"
              >
                <FolderSearch className="w-3.5 h-3.5" />
                <span>Open OS Explorer Picker</span>
              </button>
            </div>
            <div className="flex gap-2">
              <input
                type="text"
                value={rootPath}
                onChange={(e) => {
                  setRootPath(e.target.value);
                  loadDirectory(e.target.value);
                }}
                placeholder="/home/user/project or C:\project"
                required
                className="flex-1 p-2.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-sky-500 dark:focus:border-cyan-500 text-xs"
              />
              <button
                type="button"
                onClick={() => loadDirectory(rootPath)}
                title="Refresh directory contents"
                className="px-3 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${isLoadingFs ? 'animate-spin' : ''}`} />
              </button>
            </div>
          </div>

          {/* Quick Presets */}
          {fsData?.quick_presets && fsData.quick_presets.length > 0 && (
            <div>
              <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider font-semibold mb-1.5">
                Quick Shortcuts:
              </div>
              <div className="flex flex-wrap gap-1.5">
                {fsData.quick_presets
                  .filter((preset) => !preset.label.toLowerCase().includes('synevid'))
                  .map((preset, idx) => (
                    <button
                      key={idx}
                      type="button"
                      onClick={() => handleSelectFolder(preset.path, preset.label)}
                      className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-sky-400 dark:hover:border-cyan-500/50 text-[var(--text-secondary)] hover:text-sky-700 dark:hover:text-cyan-400 hover:bg-sky-50/50 dark:hover:bg-cyan-950/20 transition-colors cursor-pointer text-[11px]"
                    >
                      <HardDrive className="w-3 h-3 text-sky-600 dark:text-cyan-400/80" />
                      <span>{preset.label}</span>
                    </button>
                  ))}
              </div>
            </div>
          )}

          {/* Interactive Filesystem Navigator */}
          <div className="rounded-xl border border-[var(--border-subtle)] bg-[var(--card-elevated)] p-3 space-y-2">
            <div className="flex items-center justify-between border-b border-[var(--border-subtle)] pb-2">
              <div className="flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)] font-semibold truncate mr-2">
                <Folder className="w-3.5 h-3.5 text-amber-500 dark:text-amber-400 shrink-0" />
                <span className="text-[var(--text-muted)]">Browsing:</span>
                <span className="text-[var(--text-primary)] truncate font-semibold">
                  {fsData?.current_path || rootPath || '/'}
                </span>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                {fsData?.is_git_repo && (
                  <span className="flex items-center gap-1 px-2 py-0.5 rounded-lg bg-emerald-50 dark:bg-emerald-500/15 border border-emerald-300 dark:border-emerald-500/30 text-emerald-700 dark:text-emerald-400 text-[10px] font-bold">
                    <CheckCircle2 className="w-3 h-3" />
                    GIT REPO
                  </span>
                )}
                {fsData?.current_path && fsData.current_path !== rootPath && (
                  <button
                    type="button"
                    onClick={() => {
                      setRootPath(fsData.current_path);
                      const name = fsData.current_path.split(/[/\\]/).filter(Boolean).pop();
                      if (name && !displayName) setDisplayName(name);
                    }}
                    className="text-[10px] font-semibold px-2 py-0.5 rounded-lg bg-sky-50 dark:bg-cyan-500/15 text-sky-700 dark:text-cyan-400 border border-sky-300 dark:border-cyan-500/30 hover:bg-sky-100 dark:hover:bg-cyan-500/25 transition-colors cursor-pointer"
                  >
                    Select this folder
                  </button>
                )}
              </div>
            </div>

            {/* Folder list */}
            <div className="max-h-40 overflow-y-auto space-y-1 pr-1 custom-scrollbar">
              {/* Up to Parent button */}
              {fsData?.parent_path && (
                <button
                  type="button"
                  onClick={() => handleSelectFolder(fsData.parent_path!)}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-[var(--card)] hover:bg-sky-50 dark:hover:bg-cyan-600/10 hover:text-sky-700 dark:hover:text-cyan-400 text-[var(--text-secondary)] text-[11px] transition-colors cursor-pointer text-left"
                >
                  <ArrowUp className="w-3.5 h-3.5 text-sky-600 dark:text-cyan-400" />
                  <span className="font-semibold">.. (Parent Folder)</span>
                </button>
              )}

              {/* Subdirectories */}
              {fsData?.directories && fsData.directories.length > 0 ? (
                fsData.directories.map((dir, idx) => (
                  <button
                    key={idx}
                    type="button"
                    onClick={() => handleSelectFolder(dir.path, dir.name)}
                    className="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg hover:bg-sky-50 dark:hover:bg-cyan-600/10 hover:text-sky-700 dark:hover:text-cyan-400 text-[var(--text-secondary)] text-[11px] transition-colors cursor-pointer text-left group"
                  >
                    <div className="flex items-center gap-2 truncate">
                      <Folder className="w-3.5 h-3.5 text-amber-500 dark:text-amber-400 shrink-0" />
                      <span className="truncate">{dir.name}</span>
                    </div>
                    {dir.is_git_repo && (
                      <span className="text-[9px] px-1.5 py-0.5 rounded bg-emerald-50 dark:bg-emerald-500/15 border border-emerald-300 dark:border-emerald-500/30 text-emerald-700 dark:text-emerald-400 font-bold ml-2">
                        GIT
                      </span>
                    )}
                  </button>
                ))
              ) : (
                <div className="p-3 text-center text-[var(--text-muted)] text-[11px]">
                  No subdirectories in this folder
                </div>
              )}
            </div>
          </div>

          {/* Optional Display Name */}
          <div>
            <label className="block text-[var(--text-secondary)] font-semibold mb-1 text-[11px]">
              Display Name (Optional alias):
            </label>
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="e.g. My Next.js Web App"
              className="w-full p-2.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-sky-500 dark:focus:border-cyan-500 text-xs"
            />
          </div>

          {/* Error Message */}
          {errorMsg && (
            <div className="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-500 text-[11px]">
              {errorMsg}
            </div>
          )}

          {/* Modal Actions */}
          <div className="flex items-center justify-end space-x-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-xl bg-[var(--card-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors cursor-pointer text-xs"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || !rootPath.trim()}
              className="flex items-center gap-2 px-5 py-2 rounded-xl bg-sky-600 hover:bg-sky-500 dark:bg-cyan-600 dark:hover:bg-cyan-500 text-white font-semibold transition-colors disabled:opacity-50 cursor-pointer shadow-sm text-xs"
            >
              <FolderGit2 className="w-4 h-4" />
              <span>{isSubmitting ? 'Connecting & Indexing...' : 'Connect & Index'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
