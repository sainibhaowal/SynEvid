'use client';

import { useState } from 'react';
import { Download, FileBarChart, Loader2, RefreshCw } from 'lucide-react';
import { api, AnalyticsReportEnvelope, AnalyticsReportFormat, AnalyticsReportHistoryEntry, AnalyticsReportReceipt } from '@/lib/api';

const FORMATS: AnalyticsReportFormat[] = ['json', 'markdown', 'html', 'sarif', 'bundle'];
const REPORT_SECTIONS = [
  'structural_health', 'static_unreferenced', 'runtime_unobserved', 'newly_dead_after_edit',
  'git_velocity', 'git_evolution', 'ai_session_audit', 'coverage_and_unknowns',
  'performance_and_resources', 'errors_and_recovery',
] as const;

function objectValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

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

export default function AnalyticsReportsSection({ onSelectFinding }: { onSelectFinding: (filePath: string, stableId: string) => void }) {
  const [format, setFormat] = useState<AnalyticsReportFormat>('json');
  const [report, setReport] = useState<AnalyticsReportEnvelope | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<AnalyticsReportReceipt | null>(null);
  const [receiptError, setReceiptError] = useState<string | null>(null);
  const [history, setHistory] = useState<AnalyticsReportHistoryEntry[]>([]);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const reportData = objectValue(report?.report);

  const load = async (nextFormat = format) => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.getAnalyticsReport(nextFormat);
      setReport(next);
      setReceipt(null);
      setReceiptError(null);
    } catch (cause) {
      setReport(null);
      setError(cause instanceof Error ? cause.message : 'Analytics report is unavailable');
    } finally {
      setLoading(false);
    }
  };

  const openReceipt = async (reportId: string) => {
    setReceiptError(null);
    try {
      setReceipt(await api.getAnalyticsReportReceipt(reportId));
    } catch (cause) {
      setReceipt(null);
      setReceiptError(cause instanceof Error ? cause.message : 'Persisted evidence receipt is unavailable');
    }
  };

  const loadHistory = async () => {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      setHistory(await api.getAnalyticsReportHistory());
      setHistoryLoaded(true);
    } catch (cause) {
      setHistory([]);
      setHistoryLoaded(true);
      setHistoryError(cause instanceof Error ? cause.message : 'Report history is unavailable');
    } finally {
      setHistoryLoading(false);
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
            Evidence-scoped structural, static-reference, Git, and session reports. Unsupported data is labeled unavailable.
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
          {reportData && (
            <>
              <p className="text-[10px] font-mono text-[var(--text-muted)]">
                Scope: snapshot {String(objectValue(reportData.scope)?.snapshot_id ?? 'unavailable')} · report {String(reportData.report_id ?? report.digest)}
              </p>
              <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                {REPORT_SECTIONS.map((key) => {
                  const section = objectValue(reportData[key]);
                  if (!section) return null;
                  return (
                    <div key={key} className="rounded-lg border border-[var(--border-subtle)] p-2">
                      <div className="text-[10px] font-mono text-[var(--text-muted)]">{key.replaceAll('_', ' ')}</div>
                      <div className="mt-1 text-xs font-semibold text-[var(--text-primary)]">{String(section.status ?? 'UNAVAILABLE')}</div>
                      {typeof section.reason === 'string' && <p className="mt-1 text-[10px] text-[var(--text-muted)]">{section.reason}</p>}
                      <dl className="mt-2 space-y-1 border-t border-[var(--border-subtle)] pt-2 text-[9px] font-mono text-[var(--text-muted)]">
                        <div><dt className="inline">Evidence basis: </dt><dd className="inline break-words">{String(section.evidence_basis ?? section.evidence_kind ?? 'Not emitted by this report section')}</dd></div>
                        <div><dt className="inline">Coverage: </dt><dd className="inline">{String(section.coverage_state ?? 'Not emitted')}</dd></div>
                        <div><dt className="inline">Evidence digest: </dt><dd className="inline break-all">{String(section.evidence_digest ?? section.result_digest ?? section.graph_edge_digest ?? 'Not emitted')}</dd></div>
                      </dl>
                      {typeof reportData.report_id === 'string' && (
                        <a href="#analytics-evidence-receipt" className="mt-2 inline-block text-[9px] font-mono text-cyan-300 underline underline-offset-2">
                          Report receipt: {reportData.report_id.slice(0, 18)}…
                        </a>
                      )}
                    </div>
                  );
                })}
              </div>
              {Array.isArray(objectValue(reportData.static_unreferenced)?.candidates) && (
                <div className="space-y-1">
                  {(objectValue(reportData.static_unreferenced)?.candidates as unknown[]).map((value) => {
                    const finding = objectValue(value);
                    if (!finding || typeof finding.file_path !== 'string' || typeof finding.stable_id !== 'string') return null;
                    return (
                      <button key={finding.stable_id} onClick={() => onSelectFinding(finding.file_path as string, finding.stable_id as string)} className="block w-full rounded-md border border-amber-500/20 px-2 py-1.5 text-left text-[10px] font-mono text-amber-200 hover:bg-amber-500/10">
                        Candidate only · {String(finding.qualified_name)} — {finding.file_path}:{String(finding.line)}
                      </button>
                    );
                  })}
                </div>
              )}
            </>
          )}
          <div id="analytics-evidence-receipt" className="rounded-lg border border-cyan-500/20 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="text-[10px] font-mono text-[var(--text-muted)]">Durable report evidence receipt · {String(reportData?.report_id ?? report.digest)}</div>
              <button onClick={() => typeof reportData?.report_id === 'string' && void openReceipt(reportData.report_id)} className="rounded-md border border-cyan-500/30 px-2 py-1 text-[10px] font-mono text-cyan-300 hover:bg-cyan-500/10">
                Load persisted receipt
              </button>
              <button onClick={() => void loadHistory()} disabled={historyLoading} className="rounded-md border border-[var(--border-subtle)] px-2 py-1 text-[10px] font-mono text-[var(--text-secondary)] hover:text-[var(--text-primary)] disabled:opacity-50">
                {historyLoading ? 'Loading report history…' : 'Load report history'}
              </button>
            </div>
            {receiptError && <p role="alert" className="mt-2 text-[10px] font-mono text-rose-300">{receiptError}</p>}
            {historyError && <p role="alert" className="mt-2 text-[10px] font-mono text-rose-300">{historyError}</p>}
            {history.length > 0 && <div className="mt-2 space-y-1">
              {history.map((entry) => <button key={entry.report_id} onClick={() => void openReceipt(entry.report_id)} className="flex w-full flex-wrap justify-between gap-2 rounded border border-[var(--border-subtle)] px-2 py-1 text-left text-[9px] font-mono text-[var(--text-muted)] hover:text-cyan-300">
                <span>Snapshot {entry.snapshot_id} · {new Date(entry.created_at * 1000).toISOString()}</span>
                <span>{entry.report_id.slice(0, 20)}…</span>
              </button>)}
            </div>}
            {historyLoaded && history.length === 0 && !historyError && <p className="mt-2 text-[9px] font-mono text-[var(--text-muted)]">No reports are stored for this repository yet.</p>}
            {receipt && <pre className="mt-2 max-h-56 overflow-auto text-[9px] leading-relaxed text-[var(--text-secondary)]">{JSON.stringify(receipt, null, 2)}</pre>}
          </div>
          <pre className="max-h-72 overflow-auto rounded-xl border border-[var(--border-subtle)] bg-[var(--background)] p-3 text-[10px] leading-relaxed text-[var(--text-secondary)]">{report.content}</pre>
        </div>
      )}
      {!report && !error && <p className="mt-4 text-xs font-mono text-[var(--text-muted)]">Choose a deterministic report format to query the active repository.</p>}
    </section>
  );
}
