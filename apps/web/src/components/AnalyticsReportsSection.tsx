'use client';

import { useState } from 'react';
import { Download, FileBarChart, Loader2, RefreshCw } from 'lucide-react';
import { api, AnalyticsReportEnvelope, AnalyticsReportFormat } from '@/lib/api';

const FORMATS: AnalyticsReportFormat[] = ['json', 'markdown', 'html', 'sarif'];

function downloadReport(report: AnalyticsReportEnvelope) {
  const blob = new Blob([report.content], { type: report.content_type });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = report.filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export default function AnalyticsReportsSection() {
  const [format, setFormat] = useState<AnalyticsReportFormat>('json');
  const [report, setReport] = useState<AnalyticsReportEnvelope | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async (nextFormat = format) => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.getAnalyticsReport(nextFormat);
      setReport(next);
    } catch (cause) {
      setReport(null);
      setError(cause instanceof Error ? cause.message : 'Analytics report is unavailable');
    } finally {
      setLoading(false);
    }
  };

  return (
    <section className="rounded-2xl border border-[var(--border-subtle)] bg-[var(--card)] p-5 shadow-sm" aria-label="Analytics reports">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2 text-sm font-bold text-[var(--text-primary)] font-brand">
            <FileBarChart className="h-4 w-4 text-cyan-400" />
            Analytics &amp; Reports
          </div>
          <p className="mt-1 text-xs text-[var(--text-muted)] font-mono">
            Structural health, dead code, Git risk velocity, and AI session audit from persisted results.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={() => void load()} disabled={loading} className="rounded-lg border border-[var(--border-subtle)] p-2 text-[var(--text-muted)] hover:text-[var(--text-primary)] disabled:opacity-50" title="Refresh analytics">
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
          </button>
          <button onClick={() => report && downloadReport(report)} disabled={!report} className="flex items-center gap-1.5 rounded-lg border border-cyan-500/30 px-2.5 py-1.5 text-[11px] font-mono text-cyan-300 hover:bg-cyan-500/10 disabled:opacity-40">
            <Download className="h-3.5 w-3.5" /> Export
          </button>
        </div>
      </div>
      <div className="mt-4 flex flex-wrap gap-2">
        {FORMATS.map((candidate) => (
          <button
            key={candidate}
            onClick={() => { setFormat(candidate); void load(candidate); }}
            className={`rounded-lg px-3 py-1.5 text-[11px] font-mono uppercase transition-colors ${format === candidate ? 'bg-cyan-500/15 text-cyan-300 border border-cyan-500/40' : 'border border-[var(--border-subtle)] text-[var(--text-muted)] hover:text-[var(--text-primary)]'}`}
          >
            {candidate}
          </button>
        ))}
      </div>
      {error && <div className="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/5 p-3 text-xs font-mono text-rose-300">{error}</div>}
      {report && (
        <div className="mt-4 space-y-2">
          <div className="flex flex-wrap items-center justify-between gap-2 text-[10px] font-mono text-[var(--text-muted)]">
            <span>{report.filename}</span>
            <span title={report.digest}>{report.digest.slice(0, 24)}…</span>
          </div>
          <pre className="max-h-72 overflow-auto rounded-xl border border-[var(--border-subtle)] bg-[var(--background)] p-3 text-[10px] leading-relaxed text-[var(--text-secondary)]">{report.content}</pre>
        </div>
      )}
      {!report && !error && <p className="mt-4 text-xs font-mono text-[var(--text-muted)]">Choose a deterministic report format to query the active repository.</p>}
    </section>
  );
}
