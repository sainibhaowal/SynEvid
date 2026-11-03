'use client';

import { useEffect, useState } from 'react';
import { LayoutDashboard, Save } from 'lucide-react';

type WorkspacePreset = { id: string; label: string; description: string; workspaceId: string; drawerCollapsed: boolean };

const PRESETS: WorkspacePreset[] = [
  { id: 'architect', label: 'Architect View', description: 'Repository map and explorer', workspaceId: 'ws-repos', drawerCollapsed: false },
  { id: 'coding', label: 'Live Coding & Blast Sim', description: 'Simulator with maximum canvas space', workspaceId: 'ws-simulator', drawerCollapsed: true },
  { id: 'git', label: 'Git Forensics', description: 'Galaxy, history, and time travel', workspaceId: 'ws-git', drawerCollapsed: false },
  { id: 'ai', label: 'AI Supervisor', description: 'Trajectory, audit, and terminal tools', workspaceId: 'ws-ai', drawerCollapsed: false },
];

const STORAGE_KEY = 'synevid_workspace_preset';

export default function WorkspacePresetSection({ onApply }: { onApply: (workspaceId: string, drawerCollapsed: boolean) => void }) {
  const [selected, setSelected] = useState('architect');
  useEffect(() => {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved && PRESETS.some((preset) => preset.id === saved)) setSelected(saved);
  }, []);

  const apply = (preset: WorkspacePreset) => {
    setSelected(preset.id);
    window.localStorage.setItem(STORAGE_KEY, preset.id);
    onApply(preset.workspaceId, preset.drawerCollapsed);
  };

  return (
    <section className="rounded-2xl border border-[var(--border-subtle)] bg-[var(--card)] p-5 shadow-sm" aria-label="Workspace presets">
      <div className="flex items-center gap-2 text-sm font-bold text-[var(--text-primary)] font-brand"><LayoutDashboard className="h-4 w-4 text-cyan-400" /> Workspace presets</div>
      <p className="mt-1 text-xs text-[var(--text-muted)] font-mono">Persisted layouts change navigation only; the canvas renderer and its styling are untouched.</p>
      <div className="mt-4 grid gap-2 sm:grid-cols-2">
        {PRESETS.map((preset) => (
          <button key={preset.id} onClick={() => apply(preset)} className={`rounded-xl border p-3 text-left transition-colors ${selected === preset.id ? 'border-cyan-500/50 bg-cyan-500/10' : 'border-[var(--border-subtle)] hover:bg-[var(--card-elevated)]'}`}>
            <div className="flex items-center justify-between gap-2 text-xs font-bold text-[var(--text-primary)]"><span>{preset.label}</span>{selected === preset.id && <Save className="h-3.5 w-3.5 text-cyan-300" />}</div>
            <div className="mt-1 text-[10px] font-mono text-[var(--text-muted)]">{preset.description}</div>
          </button>
        ))}
      </div>
    </section>
  );
}
