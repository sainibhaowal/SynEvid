'use client';

import React, { useState } from 'react';
import { X, FolderGit2, Check } from 'lucide-react';
import { api } from '@/lib/api';

interface ConnectRepoModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConnected: () => void;
}

export default function ConnectRepoModal({ isOpen, onClose, onConnected }: ConnectRepoModalProps) {
  const [rootPath, setRootPath] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState('');

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!rootPath.trim()) return;

    setIsSubmitting(true);
    setErrorMsg('');
    try {
      const res = await api.connectRepo(rootPath.trim(), displayName.trim() || undefined);
      if (res.status === 'connected') {
        onConnected();
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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-fadeIn">
      <div className="w-full max-w-md rounded-xl bg-[var(--card)] border border-[var(--border-strong)] p-6 shadow-2xl space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-[var(--border-subtle)]">
          <div className="flex items-center space-x-2">
            <FolderGit2 className="w-5 h-5 text-cyan-500" />
            <h3 className="text-sm font-bold text-[var(--text-primary)]">Connect Local Repository</h3>
          </div>
          <button onClick={onClose} className="p-1 rounded text-[var(--text-muted)] hover:text-[var(--text-primary)]">
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4 text-xs font-mono">
          <div>
            <label className="block text-[var(--text-secondary)] font-semibold mb-1">
              Folder Root Path (Local absolute path):
            </label>
            <input
              type="text"
              value={rootPath}
              onChange={(e) => setRootPath(e.target.value)}
              placeholder="/home/user/project or C:\project"
              required
              className="w-full p-2.5 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500"
            />
          </div>

          <div>
            <label className="block text-[var(--text-secondary)] font-semibold mb-1">
              Display Name (Optional):
            </label>
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="e.g. Payments Monorepo"
              className="w-full p-2.5 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] focus:outline-none focus:border-cyan-500"
            />
          </div>

          {errorMsg && (
            <div className="p-2.5 rounded bg-rose-500/10 border border-rose-500/30 text-rose-500 text-[11px]">
              {errorMsg}
            </div>
          )}

          <div className="flex items-center justify-end space-x-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-lg bg-[var(--card-elevated)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="px-4 py-2 rounded-lg bg-cyan-600 hover:bg-cyan-500 text-white font-semibold transition-colors disabled:opacity-50"
            >
              {isSubmitting ? 'Validating...' : 'Connect & Index'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
