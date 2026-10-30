'use client';

import React, { useState } from 'react';
import {
  BookOpen,
  X,
  ChevronRight,
  ShieldCheck,
  CheckCircle2,
  Terminal,
  Cpu,
  Layers,
  Sparkles,
  Info,
  ExternalLink,
  Code2,
  Sliders,
  Copy,
  Check,
  ArrowRight,
  Calculator,
  Table,
  LayoutGrid,
} from 'lucide-react';
import {
  WORKSPACE_MANUALS,
  WorkspaceManual,
  ManualSection,
  ManualDiagramStep,
  ManualTableData,
  ManualFormula,
  ManualCardBox,
} from '@/lib/manualData';

interface WorkspaceManualModalProps {
  isOpen: boolean;
  onClose: () => void;
  workspaceId: string;
}

export default function WorkspaceManualModal({
  isOpen,
  onClose,
  workspaceId,
}: WorkspaceManualModalProps) {
  const manual: WorkspaceManual = WORKSPACE_MANUALS[workspaceId] || WORKSPACE_MANUALS['ws-repos'];
  const [selectedSectionId, setSelectedSectionId] = useState<string>(
    manual.sections[0]?.id || ''
  );
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  if (!isOpen) return null;

  const currentSection: ManualSection =
    manual.sections.find((s) => s.id === selectedSectionId) || manual.sections[0];

  const handleCopy = (text: string, key: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(key);
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const getStepColorClasses = (color: string) => {
    switch (color) {
      case 'cyan':
        return 'bg-cyan-500/15 border-cyan-500/30 text-cyan-400';
      case 'emerald':
        return 'bg-emerald-500/15 border-emerald-500/30 text-emerald-400';
      case 'amber':
        return 'bg-amber-500/15 border-amber-500/30 text-amber-400';
      case 'rose':
        return 'bg-rose-500/15 border-rose-500/30 text-rose-400';
      case 'indigo':
        return 'bg-indigo-500/15 border-indigo-500/30 text-indigo-400';
      default:
        return 'bg-sky-500/15 border-sky-500/30 text-sky-400';
    }
  };

  const getCardColorClasses = (color: string) => {
    switch (color) {
      case 'cyan':
        return 'bg-cyan-500/5 border-cyan-500/20 text-cyan-400';
      case 'emerald':
        return 'bg-emerald-500/5 border-emerald-500/20 text-emerald-400';
      case 'amber':
        return 'bg-amber-500/5 border-amber-500/20 text-amber-400';
      case 'rose':
        return 'bg-rose-500/5 border-rose-500/20 text-rose-400';
      case 'indigo':
        return 'bg-indigo-500/5 border-indigo-500/20 text-indigo-400';
      default:
        return 'bg-cyan-500/5 border-cyan-500/20 text-cyan-400';
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-5 bg-black/80 backdrop-blur-md animate-fadeIn">
      <div
        className="relative w-full max-w-6xl h-[92vh] bg-[var(--card)] border border-[var(--border-subtle)] rounded-3xl shadow-2xl flex flex-col overflow-hidden text-[var(--foreground)]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Top Header Bar */}
        <header className="px-6 py-4 border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between shrink-0">
          <div className="flex items-center space-x-3">
            <div className="w-10 h-10 rounded-2xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shadow-sm">
              <BookOpen className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center space-x-2">
                <h2 className="text-base font-bold font-brand tracking-tight text-[var(--text-primary)]">
                  {manual.workspaceName} — Production User Manual
                </h2>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded-md bg-cyan-500/15 text-cyan-400 font-semibold border border-cyan-500/30">
                  v0.5.0 Official Spec
                </span>
              </div>
              <p className="text-xs text-[var(--text-muted)] font-mono mt-0.5">
                {manual.shortDescription}
              </p>
            </div>
          </div>

          <div className="flex items-center space-x-2">
            <button
              onClick={onClose}
              className="p-2 rounded-xl text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--card)] border border-transparent hover:border-[var(--border-subtle)] transition-colors cursor-pointer"
              title="Close Manual (Esc)"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </header>

        {/* Modal Body: Left Table of Contents + Right Reading Pane */}
        <div className="flex-1 flex overflow-hidden min-h-0">
          {/* Left Side: Table of Contents & Navigation Bookmarks */}
          <aside className="w-80 border-r border-[var(--border-subtle)] bg-[var(--card)] flex flex-col p-3 shrink-0 overflow-y-auto">
            <div className="px-3 py-2 text-[11px] font-mono uppercase tracking-wider text-[var(--text-muted)] font-bold flex items-center justify-between">
              <span>Table of Contents</span>
              <span className="text-[10px] bg-[var(--card-elevated)] px-2 py-0.5 rounded border border-[var(--border-subtle)]">
                {manual.sections.length} Chapters
              </span>
            </div>

            <nav className="mt-1 space-y-1.5">
              {manual.sections.map((section) => {
                const isSelected = section.id === currentSection.id;
                return (
                  <button
                    key={section.id}
                    onClick={() => setSelectedSectionId(section.id)}
                    className={`w-full text-left p-3.5 rounded-2xl transition-all flex flex-col space-y-1 cursor-pointer border ${
                      isSelected
                        ? 'bg-cyan-500/15 text-cyan-400 border-cyan-500/30 shadow-sm'
                        : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--card-elevated)] border-transparent'
                    }`}
                  >
                    <div className="flex items-center justify-between w-full">
                      <span className="text-xs font-semibold leading-tight font-sans">
                        {section.title}
                      </span>
                      {section.badge && (
                        <span
                          className={`text-[9px] font-mono px-1.5 py-0.5 rounded shrink-0 ml-1.5 ${
                            isSelected
                              ? 'bg-cyan-500/25 text-cyan-300 font-bold'
                              : 'bg-[var(--card-elevated)] text-[var(--text-muted)]'
                          }`}
                        >
                          {section.badge}
                        </span>
                      )}
                    </div>
                    <span className="text-[10px] text-[var(--text-muted)] line-clamp-1 font-mono">
                      {section.content.summary}
                    </span>
                  </button>
                );
              })}
            </nav>

            {/* Invariant & Architecture Notice */}
            <div className="mt-auto pt-4 px-2 text-[10px] font-mono text-[var(--text-muted)] border-t border-[var(--border-subtle)] flex items-center space-x-1.5">
              <ShieldCheck className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span>Offline &bull; Zero LLM in Core &bull; BLAKE3 Verified</span>
            </div>
          </aside>

          {/* Right Side: Rich Visual Reader Pane */}
          <main className="flex-1 overflow-y-auto p-6 lg:p-8 space-y-7 bg-[var(--card-elevated)]/30">
            {currentSection && (
              <div className="max-w-4xl space-y-7 animate-fadeIn">
                {/* Title & Category Badge */}
                <div className="space-y-2 border-b border-[var(--border-subtle)] pb-4">
                  <div className="flex items-center space-x-2">
                    <span className="px-2.5 py-0.5 rounded-lg text-xs font-mono font-semibold bg-cyan-500/15 text-cyan-400 border border-cyan-500/30">
                      {currentSection.badge || 'Official Guide'}
                    </span>
                    <span className="text-xs font-mono text-[var(--text-muted)]">
                      SynEvid Systems Engineering Manual &bull; Phase 4 Specification
                    </span>
                  </div>
                  <h1 className="text-2xl font-bold font-brand tracking-tight text-[var(--text-primary)]">
                    {currentSection.title}
                  </h1>
                  <p className="text-sm text-[var(--text-secondary)] leading-relaxed">
                    {currentSection.content.summary}
                  </p>
                </div>

                {/* VISUAL COMPONENT 1: Architectural Process Flow Diagram */}
                {currentSection.content.diagramSteps &&
                  currentSection.content.diagramSteps.length > 0 && (
                    <div className="p-5 rounded-3xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-sm space-y-3">
                      <div className="text-xs font-mono font-bold uppercase tracking-wider text-cyan-400 flex items-center space-x-2">
                        <LayoutGrid className="w-4 h-4" />
                        <span>{currentSection.content.diagramTitle || 'Process Flow & Architecture'}</span>
                      </div>
                      <div className="grid grid-cols-1 md:grid-cols-4 gap-3 pt-2">
                        {currentSection.content.diagramSteps.map((step, idx) => (
                          <div
                            key={idx}
                            className={`p-3.5 rounded-2xl border flex flex-col justify-between relative ${getStepColorClasses(
                              step.color
                            )}`}
                          >
                            <div className="flex items-center justify-between mb-1">
                              <span className="text-[10px] font-mono uppercase tracking-wider font-bold">
                                Step #{idx + 1}
                              </span>
                              {step.badge && (
                                <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-black/30 font-semibold">
                                  {step.badge}
                                </span>
                              )}
                            </div>
                            <div className="text-xs font-bold font-sans mt-0.5">{step.label}</div>
                            <div className="text-[11px] opacity-80 font-mono mt-1 leading-tight">
                              {step.sublabel}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                {/* Section 1: What is it? */}
                <div className="p-5 rounded-3xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-xs space-y-2">
                  <div className="flex items-center space-x-2 text-xs font-mono font-bold uppercase tracking-wider text-cyan-400">
                    <Info className="w-4 h-4" />
                    <span>What Is It &amp; Why It Matters</span>
                  </div>
                  <p className="text-xs sm:text-sm text-[var(--text-secondary)] leading-relaxed font-sans">
                    {currentSection.content.whatIsIt}
                  </p>
                </div>

                {/* VISUAL COMPONENT 2: Comparison & Specification Table */}
                {currentSection.content.comparisonTable && (
                  <div className="space-y-3">
                    <div className="text-xs font-mono font-bold uppercase tracking-wider text-[var(--text-primary)] flex items-center space-x-2">
                      <Table className="w-4 h-4 text-emerald-400" />
                      <span>Technical Specification &amp; Comparison Matrix</span>
                    </div>
                    <div className="overflow-x-auto rounded-2xl border border-[var(--border-subtle)] bg-[var(--card)] shadow-xs">
                      <table className="w-full text-left border-collapse text-xs">
                        <thead>
                          <tr className="border-b border-[var(--border-subtle)] bg-[var(--card-elevated)] text-[var(--text-primary)] font-mono">
                            {currentSection.content.comparisonTable.headers.map((h, i) => (
                              <th key={i} className="p-3 font-semibold">
                                {h}
                              </th>
                            ))}
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-[var(--border-subtle)] font-sans">
                          {currentSection.content.comparisonTable.rows.map((row, rIdx) => (
                            <tr
                              key={rIdx}
                              className="hover:bg-[var(--card-elevated)]/50 transition-colors"
                            >
                              {row.map((cell, cIdx) => (
                                <td
                                  key={cIdx}
                                  className={`p-3 text-[var(--text-secondary)] ${
                                    cIdx === 0
                                      ? 'font-bold text-[var(--text-primary)] font-mono text-[11px]'
                                      : ''
                                  }`}
                                >
                                  {cell}
                                </td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>
                )}

                {/* VISUAL COMPONENT 3: Mathematical Formulations & KaTeX Blocks */}
                {currentSection.content.mathFormulas &&
                  currentSection.content.mathFormulas.length > 0 && (
                    <div className="space-y-3">
                      <div className="text-xs font-mono font-bold uppercase tracking-wider text-amber-400 flex items-center space-x-2">
                        <Calculator className="w-4 h-4" />
                        <span>Formal Mathematical Guarantees &amp; Formulas</span>
                      </div>
                      <div className="grid grid-cols-1 gap-3">
                        {currentSection.content.mathFormulas.map((math, i) => (
                          <div
                            key={i}
                            className="p-4 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-xs space-y-2"
                          >
                            <div className="flex items-center justify-between">
                              <span className="text-xs font-mono font-bold text-[var(--text-primary)]">
                                {math.label}
                              </span>
                              <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20 font-semibold">
                                Deterministic Formula
                              </span>
                            </div>
                            <div className="p-3 rounded-xl bg-black/40 border border-[var(--border-subtle)] font-mono text-xs sm:text-sm text-cyan-300 overflow-x-auto select-all">
                              <code>{math.formula}</code>
                            </div>
                            <p className="text-[11px] text-[var(--text-muted)] font-sans leading-normal">
                              {math.explanation}
                            </p>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                {/* Section 2: How It Works */}
                <div className="p-5 rounded-3xl bg-[var(--card)] border border-[var(--border-subtle)] shadow-xs space-y-2">
                  <div className="flex items-center space-x-2 text-xs font-mono font-bold uppercase tracking-wider text-emerald-400">
                    <Cpu className="w-4 h-4" />
                    <span>How It Works Under The Hood</span>
                  </div>
                  <p className="text-xs sm:text-sm text-[var(--text-secondary)] leading-relaxed font-sans">
                    {currentSection.content.howItWorks}
                  </p>
                </div>

                {/* VISUAL COMPONENT 4: Infographic Cards & Feature Boxes */}
                {currentSection.content.infoCards && currentSection.content.infoCards.length > 0 && (
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    {currentSection.content.infoCards.map((card, i) => (
                      <div
                        key={i}
                        className={`p-5 rounded-3xl border shadow-xs space-y-2.5 ${getCardColorClasses(
                          card.color
                        )}`}
                      >
                        <div className="flex items-center justify-between">
                          <span className="text-xs font-bold font-mono text-[var(--text-primary)]">
                            {card.title}
                          </span>
                          {card.badge && (
                            <span className="text-[10px] font-mono px-2 py-0.5 rounded font-semibold bg-black/25">
                              {card.badge}
                            </span>
                          )}
                        </div>
                        <ul className="space-y-1.5 text-xs text-[var(--text-secondary)] font-sans">
                          {card.points.map((pt, j) => (
                            <li key={j} className="flex items-start space-x-2">
                              <span className="text-cyan-400 font-bold">&bull;</span>
                              <span className="leading-relaxed">{pt}</span>
                            </li>
                          ))}
                        </ul>
                      </div>
                    ))}
                  </div>
                )}

                {/* Section 3: Buttons, Controls & Keyboard Shortcuts */}
                {currentSection.content.controlsAndButtons.length > 0 && (
                  <div className="space-y-3">
                    <h3 className="text-xs font-mono font-bold uppercase tracking-wider text-[var(--text-primary)] flex items-center space-x-2">
                      <Sliders className="w-4 h-4 text-cyan-400" />
                      <span>Interactive Controls, Buttons &amp; Functions</span>
                    </h3>
                    <div className="grid grid-cols-1 gap-2.5">
                      {currentSection.content.controlsAndButtons.map((btn, i) => (
                        <div
                          key={i}
                          className="p-4 rounded-2xl bg-[var(--card)] border border-[var(--border-subtle)] flex flex-col space-y-1.5 shadow-xs"
                        >
                          <div className="flex items-center justify-between">
                            <span className="text-xs font-bold text-[var(--text-primary)] font-mono">
                              {btn.name}
                            </span>
                            {btn.shortcut && (
                              <kbd className="px-2 py-0.5 rounded bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[10px] font-mono font-semibold text-cyan-400">
                                {btn.shortcut}
                              </kbd>
                            )}
                          </div>
                          <span className="text-xs text-cyan-500 dark:text-cyan-400 font-medium">
                            Action: {btn.action}
                          </span>
                          <p className="text-[11px] text-[var(--text-muted)] font-sans leading-normal">
                            {btn.details}
                          </p>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* Section 4: Practical Step-by-Step Examples */}
                {currentSection.content.practicalExamples.length > 0 && (
                  <div className="space-y-3">
                    <h3 className="text-xs font-mono font-bold uppercase tracking-wider text-[var(--text-primary)] flex items-center space-x-2">
                      <Code2 className="w-4 h-4 text-amber-400" />
                      <span>Practical Production Scenario</span>
                    </h3>
                    <div className="p-5 rounded-3xl bg-[var(--card)] border border-[var(--border-subtle)] space-y-3 shadow-xs">
                      {currentSection.content.practicalExamples.map((ex, i) => (
                        <div key={i} className="space-y-2.5">
                          <div className="text-xs font-bold text-[var(--text-primary)] font-mono">
                            Scenario: {ex.scenario}
                          </div>
                          <ol className="list-decimal list-inside space-y-1 text-xs text-[var(--text-secondary)] font-sans pl-1">
                            {ex.steps.map((st, j) => (
                              <li key={j} className="leading-relaxed">
                                {st}
                              </li>
                            ))}
                          </ol>
                          <div className="p-3 rounded-2xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 text-xs font-mono flex items-center space-x-2">
                            <CheckCircle2 className="w-4 h-4 shrink-0" />
                            <span>Expected Outcome: {ex.expectedOutcome}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* Section 5: Developer Architecture & Cryptographic Invariants */}
                <div className="p-5 rounded-3xl bg-cyan-500/5 border border-cyan-500/20 space-y-3">
                  <div className="flex items-center space-x-2 text-xs font-mono font-bold uppercase tracking-wider text-cyan-400">
                    <ShieldCheck className="w-4 h-4" />
                    <span>Architectural Rules &amp; Non-Negotiable Invariants</span>
                  </div>
                  <p className="text-xs text-[var(--text-secondary)] font-sans leading-relaxed">
                    {currentSection.content.developerAndArchitecture}
                  </p>
                  <div className="space-y-1.5 pt-2 border-t border-cyan-500/20">
                    {currentSection.content.evidenceAndInvariants.map((inv, i) => (
                      <div
                        key={i}
                        className="text-[11px] font-mono text-[var(--text-muted)] flex items-center space-x-2"
                      >
                        <span className="text-cyan-400 font-bold">&bull;</span>
                        <span>{inv}</span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </main>
        </div>

        {/* Modal Footer Bar */}
        <footer className="px-6 py-3 border-t border-[var(--border-subtle)] bg-[var(--card-elevated)] flex items-center justify-between text-xs font-mono text-[var(--text-muted)] shrink-0">
          <div className="flex items-center space-x-3">
            <span>Chapter: {currentSection?.title}</span>
            <span>&bull;</span>
            <span className="text-cyan-400 font-semibold">Offline Verification Spec (v0.5.0)</span>
          </div>
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-xl bg-[var(--card)] hover:bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] font-semibold transition-colors cursor-pointer"
          >
            Close Manual
          </button>
        </footer>
      </div>
    </div>
  );
}
