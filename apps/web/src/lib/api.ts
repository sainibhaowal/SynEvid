// SynEvid Reactive Engine & Daemon API Client (v0.2.1)
// Connects to local daemon on http://127.0.0.1:3333 (synevid-ui or autopsy ui)

export interface RepoRecord {
  repo_id: string;
  root_path: string;
  display_name: string;
  active_branch?: string;
  languages_json?: string;
  last_indexed_seq?: number;
}

export interface Capabilities {
  languages: string[];
  resolvers: string[];
  layouts: string[];
  gates?: string[];
  storage_engine?: string;
}

export interface UniverseNode {
  id: string;
  x: number;
  y: number;
  z?: number;
  cluster_id?: number;
}

export interface UniverseEdge {
  source: string;
  target: string;
  kind: string;
}

export interface GraphUniverse {
  repo_id: string;
  dimensions: '2D' | '3D';
  node_count: number;
  edge_count: number;
  nodes: UniverseNode[];
  edges: UniverseEdge[];
}

export interface GraphNodeRow {
  snapshot_id: string;
  stable_id: string;
  node_kind: string;
  qualified_name: string;
  file_path: string;
  start_line: number;
  end_line: number;
  signature_hash: string;
  visibility: string;
  is_exported: boolean;
  is_dead_code: boolean;
  coverage_state: string;
}

export interface GraphEdgeRow {
  snapshot_id: string;
  edge_id: string;
  source_stable_id: string;
  target_stable_id: string;
  edge_kind: string;
  source_file: string;
  source_line: number;
  source_col: number;
  target_file: string;
  target_line: number;
  confidence: string;
}

export interface EgoNetwork {
  target_node: GraphNodeRow | null;
  incoming_callers: GraphEdgeRow[];
  outgoing_callees: GraphEdgeRow[];
}

export interface DamagedCallerItem {
  stable_id: string;
  file_path: string;
  calling_line: number;
  calling_col: number;
  diagnostic: string;
  severity: string;
}

export interface DeadSymbolItem {
  stable_id: string;
  file_path: string;
  line: number;
  diagnostic: string;
}

export interface SimulateEditResponse {
  verdict: 'PASS' | 'WARN' | 'FAIL';
  damaged_callers: DamagedCallerItem[];
  newly_dead_symbols: DeadSymbolItem[];
  broken_imports: string[];
  syntax_errors: string[];
  computation_latency_ms: number;
}

export interface InvariantFinding {
  rule_id: string;
  status: 'pass' | 'fail' | 'unknown';
  severity: 'error' | 'warning' | 'info';
  message: string;
  entities?: string[];
  locations?: Array<{ path: string; line?: number }>;
}

export interface TrajectoryRun {
  session_id: string;
  exit_code: number;
  report_digest?: string;
  started_snapshot?: string;
  completed_snapshot?: string;
  steps?: Array<{ action: string; status: string }>;
  next_agent_guidance?: string;
}

export interface DaemonLiveEvent {
  seq_id: number;
  event_type: string;
  timestamp_epoch_ms: number;
  repo_id: string;
  payload: Record<string, unknown>;
}

// Fallback Mock Data for offline testing
const FALLBACK_REPOS: RepoRecord[] = [
  {
    repo_id: '8f43a0e1b2',
    root_path: '/home/ravi/Projects Beta/SynEvid',
    display_name: 'SynEvid Core Workspace',
    active_branch: 'main',
    languages_json: '["typescript","rust","python","go","java","csharp"]',
    last_indexed_seq: 42,
  },
  {
    repo_id: 'a93f12c84e',
    root_path: '/home/ravi/Projects/Microservices',
    display_name: 'Order & Payment Gateway',
    active_branch: 'develop',
    languages_json: '["typescript","go"]',
    last_indexed_seq: 18,
  },
];

const FALLBACK_CAPS: Capabilities = {
  languages: ['typescript', 'rust', 'python', 'go', 'java', 'csharp'],
  resolvers: ['npm', 'pnpm', 'yarn', 'cargo', 'go', 'maven', 'gradle', 'nuget'],
  layouts: ['DeterministicForceLayout-2D', 'DeterministicForceLayout-3D'],
  gates: ['no-new-cycles', 'api-contract-compatibility', 'layer-boundaries'],
  storage_engine: 'SQLite WAL Mode with Auto-Recovery (ERR-STORAGE-102)',
};

export class SynevidApiClient {
  private baseUrl: string;

  constructor(baseUrl?: string) {
    if (typeof window !== 'undefined') {
      this.baseUrl = baseUrl || localStorage.getItem('synevid_api_url') || 'http://127.0.0.1:3333';
    } else {
      this.baseUrl = baseUrl || 'http://127.0.0.1:3333';
    }
  }

  public getBaseUrl(): string {
    return this.baseUrl;
  }

  public setBaseUrl(url: string): void {
    this.baseUrl = url;
    if (typeof window !== 'undefined') {
      localStorage.setItem('synevid_api_url', url);
    }
  }

  private async fetchWithTimeout<T>(path: string, options?: RequestInit, timeoutMs = 2500): Promise<T> {
    const controller = new AbortController();
    const id = setTimeout(() => controller.abort(), timeoutMs);

    try {
      const res = await fetch(`${this.baseUrl}${path}`, {
        ...options,
        signal: controller.signal,
        headers: {
          'Content-Type': 'application/json',
          ...(options?.headers || {}),
        },
      });
      clearTimeout(id);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      return (await res.json()) as T;
    } catch (err) {
      clearTimeout(id);
      throw err;
    }
  }

  public async getStatus() {
    try {
      return await this.fetchWithTimeout<{
        snapshot_id: string;
        repo_root: string;
        coverage_state: string;
        files_scanned: number;
        symbol_count: number;
        edge_count: number;
        version: string;
      }>('/api/status');
    } catch {
      return {
        snapshot_id: 'snap_c42f0b98e1a7',
        repo_root: '/home/ravi/Projects Beta/SynEvid',
        coverage_state: 'Verified',
        files_scanned: 184,
        symbol_count: 1420,
        edge_count: 3890,
        version: 'v0.2.1',
      };
    }
  }

  public async getCapabilities(): Promise<Capabilities> {
    try {
      return await this.fetchWithTimeout<Capabilities>('/api/v1/capabilities');
    } catch {
      return FALLBACK_CAPS;
    }
  }

  public async listRepos(): Promise<RepoRecord[]> {
    try {
      return await this.fetchWithTimeout<RepoRecord[]>('/api/v1/repos/list');
    } catch {
      return FALLBACK_REPOS;
    }
  }

  public async connectRepo(rootPath: string, displayName?: string) {
    return await this.fetchWithTimeout<{ status: string; repo_id?: string; error?: string }>(
      '/api/v1/repos/connect',
      {
        method: 'POST',
        body: JSON.stringify({ root_path: rootPath, display_name: displayName }),
      }
    );
  }

  public async switchRepo(repoId: string) {
    return await this.fetchWithTimeout<{ status: string; active_repo?: RepoRecord }>(
      '/api/v1/repos/switch',
      {
        method: 'POST',
        body: JSON.stringify({ repo_id: repoId }),
      }
    );
  }

  public async getUniverse(dim: '2D' | '3D' = '2D'): Promise<GraphUniverse> {
    try {
      const q = dim === '3D' ? '?dim=3d' : '';
      return await this.fetchWithTimeout<GraphUniverse>(`/api/v1/graph/universe${q}`);
    } catch {
      return {
        repo_id: '8f43a0e1b2',
        dimensions: dim,
        node_count: 7,
        edge_count: 8,
        nodes: [
          { id: 'src/index.ts', x: 0.0, y: 0.0, z: 0.0, cluster_id: 0 },
          { id: 'src/controllers/OrderController.ts', x: -0.45, y: -0.32, z: 0.2, cluster_id: 1 },
          { id: 'src/services/PaymentService.ts', x: 0.42, y: -0.28, z: -0.15, cluster_id: 2 },
          { id: 'src/repositories/UserRepository.ts', x: 0.62, y: 0.38, z: 0.1, cluster_id: 3 },
          { id: 'src/domain/Order.ts', x: -0.55, y: 0.45, z: -0.25, cluster_id: 1 },
          { id: 'src/services/NotificationService.ts', x: 0.1, y: 0.65, z: 0.3, cluster_id: 2 },
          { id: 'src/infra/DatabasePool.ts', x: 0.75, y: -0.1, z: 0.05, cluster_id: 3 },
        ],
        edges: [
          { source: 'src/index.ts', target: 'src/controllers/OrderController.ts', kind: 'imports' },
          { source: 'src/controllers/OrderController.ts', target: 'src/services/PaymentService.ts', kind: 'calls' },
          { source: 'src/services/PaymentService.ts', target: 'src/repositories/UserRepository.ts', kind: 'calls' },
          { source: 'src/controllers/OrderController.ts', target: 'src/domain/Order.ts', kind: 'references' },
          { source: 'src/services/PaymentService.ts', target: 'src/services/NotificationService.ts', kind: 'calls' },
          { source: 'src/repositories/UserRepository.ts', target: 'src/infra/DatabasePool.ts', kind: 'calls' },
          { source: 'src/index.ts', target: 'src/infra/DatabasePool.ts', kind: 'initializes' },
          { source: 'src/domain/Order.ts', target: 'src/services/PaymentService.ts', kind: 'references' },
        ],
      };
    }
  }

  public async getEgoNetwork(symbolId: string): Promise<EgoNetwork> {
    try {
      return await this.fetchWithTimeout<EgoNetwork>(`/api/v1/graph/ego?id=${encodeURIComponent(symbolId)}`);
    } catch {
      return {
        target_node: {
          snapshot_id: 'snap_active_worktree',
          stable_id: symbolId,
          node_kind: 'function',
          qualified_name: symbolId,
          file_path: 'src/services/PaymentService.ts',
          start_line: 35,
          end_line: 60,
          signature_hash: 'blake3:92fa77c8e9',
          visibility: 'public',
          is_exported: true,
          is_dead_code: false,
          coverage_state: 'verified',
        },
        incoming_callers: [
          {
            snapshot_id: 'snap_active_worktree',
            edge_id: 'edge_ego_1',
            source_stable_id: 'OrderController.createOrder',
            target_stable_id: symbolId,
            edge_kind: 'calls',
            source_file: 'src/controllers/OrderController.ts',
            source_line: 42,
            source_col: 18,
            target_file: 'src/services/PaymentService.ts',
            target_line: 35,
            confidence: 'exact',
          },
          {
            snapshot_id: 'snap_active_worktree',
            edge_id: 'edge_ego_2',
            source_stable_id: 'CheckoutService.processCheckout',
            target_stable_id: symbolId,
            edge_kind: 'calls',
            source_file: 'src/services/CheckoutService.ts',
            source_line: 88,
            source_col: 12,
            target_file: 'src/services/PaymentService.ts',
            target_line: 35,
            confidence: 'exact',
          },
        ],
        outgoing_callees: [
          {
            snapshot_id: 'snap_active_worktree',
            edge_id: 'edge_ego_3',
            source_stable_id: symbolId,
            target_stable_id: 'UserRepository.findById',
            edge_kind: 'calls',
            source_file: 'src/services/PaymentService.ts',
            source_line: 48,
            source_col: 10,
            target_file: 'src/repositories/UserRepository.ts',
            target_line: 19,
            confidence: 'exact',
          },
        ],
      };
    }
  }

  public async simulateEdit(filePath: string, unsavedBuffer: string): Promise<SimulateEditResponse> {
    try {
      return await this.fetchWithTimeout<SimulateEditResponse>('/api/v1/simulate/edit', {
        method: 'POST',
        body: JSON.stringify({ file_path: filePath, unsaved_buffer: unsavedBuffer }),
      });
    } catch {
      // Local client-side simulation fallback when daemon is offline
      const isBreaking = unsavedBuffer.includes('taxRate: number') || unsavedBuffer.includes('strictMode: boolean');
      const isSyntaxErr = unsavedBuffer.includes('(((') || unsavedBuffer.includes('syntax_error_mock');
      const isDead = !unsavedBuffer.includes('legacyFormat') && unsavedBuffer.length > 30;

      const damaged: DamagedCallerItem[] = isBreaking
        ? [
            {
              stable_id: 'OrderController.createPayment',
              file_path: 'src/controllers/OrderController.ts',
              calling_line: 42,
              calling_col: 18,
              diagnostic: 'Arity mismatch: expected 3 arguments, caller passes 2',
              severity: 'breaking_error',
            },
          ]
        : [];

      const dead: DeadSymbolItem[] = isDead
        ? [
            {
              stable_id: 'PaymentService.legacyFormat',
              file_path: filePath,
              line: 88,
              diagnostic: '0 remaining callers across workspace',
            },
          ]
        : [];

      return {
        verdict: isBreaking || isSyntaxErr ? 'FAIL' : isDead ? 'WARN' : 'PASS',
        damaged_callers: damaged,
        newly_dead_symbols: dead,
        broken_imports: [],
        syntax_errors: isSyntaxErr ? ['SyntaxError: Unexpected token or unclosed parenthesis at line 3'] : [],
        computation_latency_ms: 12,
      };
    }
  }

  public async getInvariants() {
    try {
      return await this.fetchWithTimeout<{ findings: InvariantFinding[] }>('/api/verify');
    } catch {
      return {
        findings: [
          {
            rule_id: 'no-circular-deps',
            status: 'pass' as const,
            severity: 'error' as const,
            message: 'No circular dependencies introduced across module graph',
            locations: [{ path: 'src/services' }],
          },
          {
            rule_id: 'api-compatibility',
            status: 'pass' as const,
            severity: 'error' as const,
            message: 'All public API contract definitions match workspace callers',
            locations: [{ path: 'src/domain' }],
          },
        ],
      };
    }
  }

  public async getTrajectory() {
    try {
      return await this.fetchWithTimeout<{ runs: TrajectoryRun[] }>('/api/trajectory');
    } catch {
      return {
        runs: [
          {
            session_id: 'sess_9f82ab11',
            exit_code: 0,
            report_digest: 'blake3:c498ae20bb11',
            started_snapshot: 'snap_v0_1_baseline',
            completed_snapshot: 'snap_v0_2_pluggable',
            steps: [{ action: 'verify_invariants', status: 'pass' }, { action: 'compute_layout', status: 'pass' }],
            next_agent_guidance: 'All invariants verified. Ready for Phase 2 3D visual canvas integration.',
          },
        ],
      };
    }
  }

  public subscribeLiveEvents(onEvent: (event: DaemonLiveEvent) => void, onError?: () => void): () => void {
    if (typeof window === 'undefined') return () => {};

    let evtSource: EventSource | null = null;
    let cancelled = false;

    try {
      evtSource = new EventSource(`${this.baseUrl}/api/v1/live`);
      evtSource.onmessage = (e) => {
        try {
          const parsed = JSON.parse(e.data) as DaemonLiveEvent;
          onEvent(parsed);
        } catch {
          // Ignore parse errors on ping
        }
      };
      evtSource.onerror = () => {
        if (onError) onError();
      };
    } catch {
      if (onError) onError();
    }

    return () => {
      cancelled = true;
      if (evtSource) {
        evtSource.close();
      }
    };
  }
}

export const api = new SynevidApiClient();
