/// <reference lib="webworker" />

import type { GraphUniverse, UniverseEdge, UniverseNode } from '../lib/api';

type GraphWorkerRequest = {
  type: 'load' | 'cancel';
  requestId: number;
  dimension?: '2D' | '3D';
};

type RawGraph = Record<string, unknown> & {
  nodes?: unknown;
  edges?: unknown;
  error?: unknown;
  error_code?: unknown;
};

let activeController: AbortController | null = null;

function postProgress(requestId: number, stage: string, progress: number) {
  self.postMessage({ type: 'progress', requestId, stage, progress });
}

function normalizeCoordinate(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0;
  return Math.abs(value) > 5 ? value / 150 : value;
}

function normalizeUniverse(raw: RawGraph, dimension: '2D' | '3D'): GraphUniverse {
  const rawNodes = raw.nodes;
  const entries: Array<[string, Record<string, unknown>]> = Array.isArray(rawNodes)
    ? rawNodes.map((node: Record<string, unknown>) => [String(node.id ?? ''), node])
    : rawNodes && typeof rawNodes === 'object'
      ? Object.entries(rawNodes as Record<string, Record<string, unknown>>)
      : [];
  const nodes = entries.map(([id, node]) => ({
    ...node,
    id,
    x: normalizeCoordinate(node.x),
    y: normalizeCoordinate(node.y),
    z: normalizeCoordinate(node.z),
    cluster_id: typeof node.cluster_id === 'number' ? node.cluster_id : 1,
    label: typeof node.label === 'string' ? node.label : undefined,
    kind: typeof node.kind === 'string' ? node.kind : undefined,
    file_path: typeof node.file_path === 'string' ? node.file_path : undefined,
    start_line: typeof node.start_line === 'number' ? node.start_line : undefined,
    end_line: typeof node.end_line === 'number' ? node.end_line : undefined,
    status: typeof node.status === 'string' ? node.status : undefined,
    caller_count: typeof node.caller_count === 'number' ? node.caller_count : undefined,
    callee_count: typeof node.callee_count === 'number' ? node.callee_count : undefined,
  })) as UniverseNode[];
  const edges = Array.isArray(raw.edges)
    ? raw.edges.filter((edge: unknown) => {
        if (!edge || typeof edge !== 'object') return false;
        const record = edge as Record<string, unknown>;
        return typeof record.source === 'string' && typeof record.target === 'string';
      }) as UniverseEdge[]
    : [];
  return {
    repo_id: typeof raw.repo_id === 'string' ? raw.repo_id : '',
    snapshot_id: typeof raw.snapshot_id === 'string' ? raw.snapshot_id : undefined,
    dimensions: raw.dimensions === '2D' ? '2D' : dimension,
    coverage_state: typeof raw.coverage_state === 'string' ? raw.coverage_state : 'unknown',
    node_count: nodes.length,
    edge_count: edges.length,
    total_node_count: typeof raw.total_node_count === 'number' ? raw.total_node_count : nodes.length,
    total_edge_count: typeof raw.total_edge_count === 'number' ? raw.total_edge_count : edges.length,
    is_truncated: raw.is_truncated === true,
    nodes,
    edges,
  };
}

async function loadGraph(requestId: number, dimension: '2D' | '3D') {
  activeController?.abort();
  const controller = new AbortController();
  activeController = controller;
  const query = dimension === '2D' ? '?dim=2d' : '?dim=3d';
  postProgress(requestId, 'requesting', 4);
  const response = await fetch(`/api/v1/graph/universe${query}`, { signal: controller.signal });
  if (!response.ok) throw new Error(`Graph request failed (${response.status})`);
  const totalBytes = Number(response.headers.get('content-length')) || 0;
  const reader = response.body?.getReader();
  const chunks: Uint8Array[] = [];
  let receivedBytes = 0;
  if (reader) {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value) {
        chunks.push(value);
        receivedBytes += value.byteLength;
        const ratio = totalBytes > 0 ? receivedBytes / totalBytes : 0;
        postProgress(requestId, 'downloading', Math.min(68, 8 + Math.round(ratio * 60)));
      }
    }
  }
  postProgress(requestId, 'parsing', 74);
  const payload = reader
    ? new TextDecoder().decode(concatenateChunks(chunks, receivedBytes))
    : await response.text();
  const raw = JSON.parse(payload) as RawGraph;
  if (raw.error_code || raw.error) throw new Error(String(raw.error || raw.error_code));
  postProgress(requestId, 'preparing', 86);
  const universe = normalizeUniverse(raw, dimension);
  postProgress(requestId, 'transferring', 96);
  self.postMessage({ type: 'complete', requestId, universe });
}

function concatenateChunks(chunks: Uint8Array[], totalBytes: number): Uint8Array {
  const result = new Uint8Array(totalBytes);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return result;
}

self.onmessage = (event: MessageEvent<GraphWorkerRequest>) => {
  const message = event.data;
  if (message.type === 'cancel') {
    activeController?.abort();
    activeController = null;
    return;
  }
  loadGraph(message.requestId, message.dimension || '3D').catch((error: unknown) => {
    if (error instanceof DOMException && error.name === 'AbortError') return;
    self.postMessage({
      type: 'error',
      requestId: message.requestId,
      message: error instanceof Error ? error.message : 'Repository graph is unavailable',
    });
  });
};

export {};
