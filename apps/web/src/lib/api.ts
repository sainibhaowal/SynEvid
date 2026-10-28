// SynEvid Reactive Engine & Daemon API Client (v0.2.1)
// Connects to local daemon on http://127.0.0.1:3333 (synevid-ui or autopsy ui)

export interface GitMutationResult {
  success: boolean;
  status: string;
  action: string;
  new_commit_oid?: string | null;
  undo_checkpoint_id?: string | null;
  message: string;
  error?: string | null;
}

export interface CommitGalaxyNode {
  oid: string;
  short_oid: string;
  parents: string[];
  author_name: string;
  author_email: string;
  timestamp: number;
  summary: string;
  lane: number;
  is_head: boolean;
  branches?: string[];
  branch_names?: string[];
  tags?: string[];
  symbols_modified: number;
  symbols_added?: number;
  symbols_deleted?: number;
  blast_radius_score: number;
  verdict?: 'PASS' | 'WARN' | 'BREAKING' | string;
  verification_verdict?: 'PASS' | 'WARN' | 'BREAKING' | string;
}

export interface BranchRef {
  name: string;
  target_oid: string;
  is_active: boolean;
}

export interface TagRef {
  name: string;
  target_oid: string;
}

export interface GitGalaxy {
  repo_id: string;
  head_oid: string | null;
  active_branch: string | null;
  total_commits: number;
  commits: CommitGalaxyNode[];
  branches: BranchRef[];
  tags: TagRef[];
}

export interface TimeTravelNode {
  id: string;
  x: number;
  y: number;
  z: number;
  cluster_id: number;
  diff_status: 'added' | 'modified' | 'unchanged' | 'deleted';
}

export interface TimeTravelEdge {
  source: string;
  target: string;
  kind: string;
}

export interface TimeTravelVisualFrame {
  oid: string;
  short_oid: string;
  summary: string;
  timestamp: number;
  dimensions: '2D' | '3D';
  node_count: number;
  edge_count: number;
  nodes: TimeTravelNode[];
  edges: TimeTravelEdge[];
  added_files: string[];
  modified_files: string[];
  deleted_files: string[];
}

export interface SemanticDiffReport {
  base_oid: string;
  head_oid: string;
  added_files: string[];
  modified_files: string[];
  deleted_files: string[];
  total_changed_files: number;
  blast_radius_score: number;
  verdict: 'PASS' | 'WARN' | 'BREAKING';
  drifted_contracts: string[];
}

export interface FileStatusItem {
  path: string;
  status: 'modified' | 'untracked' | 'staged' | 'deleted' | 'ignored';
  added_lines: number;
  deleted_lines: number;
}

export interface GitStatusReport {
  branch: string;
  head_oid: string | null;
  staged: FileStatusItem[];
  unstaged: FileStatusItem[];
  untracked: FileStatusItem[];
  deleted: FileStatusItem[];
  ignored: string[];
  is_clean: boolean;
  total_uncommitted: number;
}

export interface DiffLine {
  kind: 'add' | 'del' | 'ctx';
  old_line?: number;
  new_line?: number;
  text: string;
}

export interface ForensicSession {
  session_id: string;
  repo_id: string;
  actor_kind: 'AI_AGENT' | 'HUMAN_USER' | 'CI_WORKFLOW' | string;
  actor_name: string;
  baseline_snap_id: string;
  current_snap_id: string;
  files_touched_count: number;
  total_events_count: number;
  status: 'ACTIVE' | 'VERIFIED_PASS' | 'BLOCKED_FAIL' | 'CLOSED' | string;
}

export interface ForensicWorkflow {
  workflow_id: string;
  session_id: string;
  name: string;
  workflow_type: string;
  status: 'RUNNING' | 'COMPLETED' | 'FAILED' | string;
  start_event_seq: number;
  end_event_seq?: number | null;
}

export interface TrajectoryEvent {
  event_seq: number;
  session_id: string;
  job_id?: string | null;
  workflow_id?: string | null;
  event_type: string;
  target_symbol?: string | null;
  target_file?: string | null;
  before_snap_id?: string | null;
  after_snap_id?: string | null;
  verdict: 'PASS' | 'FAIL' | 'WARN' | 'INFO' | string;
  summary: string;
  evidence_json: string;
  blake3_receipt: string;
  tokens_spent: number;
  tokens_saved: number;
}

export interface AiSessionLedger {
  session_id: string;
  queries_count: number;
  tokens_spent: number;
  tokens_saved: number;
  cache_hits: number;
  cache_misses: number;
}

export interface TimelineResponse {
  session: ForensicSession;
  workflows: ForensicWorkflow[];
  events: TrajectoryEvent[];
  ledger?: AiSessionLedger;
}

export interface AiConfigSnippetResponse {
  client: string;
  file_name: string;
  config_json: string;
  instruction: string;
}

export interface FileDiffReport {
  path: string;
  status: string;
  old_content: string;
  new_content: string;
  diff_lines: DiffLine[];
  total_additions: number;
  total_deletions: number;
}

export interface TagRequest {
  tag_name: string;
  target_oid?: string;
  message?: string;
  action?: 'create' | 'delete';
}

export interface MergeRequest {
  source_branch: string;
  strategy?: string;
}

export interface CheckoutRequest {
  target: string;
  detach?: boolean;
}

export interface RevertRequest {
  commit_oid: string;
}

export interface BisectRequest {
  start_oid: string;
  end_oid: string;
  rule_id?: string;
}

export interface BisectReport {
  breaking_commit_oid: string | null;
  breaking_summary: string;
  breaking_author: string;
  breaking_timestamp: number;
  verdict: string;
  drifted_symbols: string[];
  commits_evaluated: number;
  total_scanned: number;
}

export interface TerminalExecResponse {
  command: string;
  stdout: string;
  stderr: string;
  exit_code: number;
}

export interface RepoRecord {
  repo_id: string;
  root_path: string;
  display_name: string;
  active_branch?: string;
  languages_json?: string;
  last_indexed_seq?: number;
}

export interface RepoFileItem {
  path: string;
  name: string;
  ext: string;
  dir: string;
  status: 'verified' | 'impacted';
}

export interface RepoFilesResponse {
  repo_id: string;
  total_files: number;
  files: RepoFileItem[];
}

export interface Capabilities {
  languages: string[];
  resolvers: string[];
  layouts: string[];
  gates?: string[];
  storage_engine?: string;
}

export interface FsDirectoryEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_git_repo: boolean;
}

export interface FsPreset {
  label: string;
  path: string;
}

export interface FsBrowseResponse {
  current_path: string;
  parent_path: string | null;
  is_git_repo: boolean;
  directories: FsDirectoryEntry[];
  quick_presets: FsPreset[];
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

export function normalizeCrossPlatformPath(rawPath: string): string {
  if (!rawPath) return '';
  let p = rawPath.trim();
  if ((p.startsWith('"') && p.endsWith('"')) || (p.startsWith("'") && p.endsWith("'"))) {
    p = p.slice(1, -1).trim();
  }

  const normalized = p.replace(/\\/g, '/');

  // Convert Windows WSL UNC paths: //wsl.localhost/<distro>/... or //wsl$/<distro>/...
  const wslMatch = normalized.match(/^\/\/(?:wsl\.localhost|wsl\$)\/[^/]+(\/.*)$/i);
  if (wslMatch) {
    return wslMatch[1];
  }

  // Convert Windows drive letters C:/... to /mnt/c/...
  const driveMatch = normalized.match(/^([a-zA-Z]):(\/.*)$/);
  if (driveMatch) {
    const driveLetter = driveMatch[1].toLowerCase();
    return `/mnt/${driveLetter}${driveMatch[2]}`;
  }

  return normalized;
}

// Fallback Mock Data for offline testing (Zero hardcoded repository paths)
const FALLBACK_REPOS: RepoRecord[] = [];

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

  private async fetchWithTimeout<T>(path: string, options?: RequestInit, timeoutMs = 15000): Promise<T> {
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
        repo_name?: string;
        active_repo_id?: string;
        coverage_state: string;
        files_scanned: number;
        symbol_count: number;
        edge_count: number;
        version: string;
      }>('/api/status');
    } catch {
      return {
        snapshot_id: 'no-active-snapshot',
        repo_root: '',
        repo_name: '',
        active_repo_id: '',
        coverage_state: 'Unconnected',
        files_scanned: 0,
        symbol_count: 0,
        edge_count: 0,
        version: 'v0.2.6',
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
    const normalized = normalizeCrossPlatformPath(rootPath);
    return await this.fetchWithTimeout<{ status: string; repo_id?: string; display_name?: string; error?: string }>(
      '/api/v1/repos/connect',
      {
        method: 'POST',
        body: JSON.stringify({ root_path: normalized, display_name: displayName }),
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

  public async getFiles(): Promise<RepoFilesResponse> {
    try {
      return await this.fetchWithTimeout<RepoFilesResponse>('/api/v1/files');
    } catch {
      return {
        repo_id: 'default',
        total_files: 0,
        files: [],
      };
    }
  }

  public async browseFilesystem(targetPath?: string): Promise<FsBrowseResponse> {
    try {
      const normalized = targetPath ? normalizeCrossPlatformPath(targetPath) : '';
      const q = normalized ? `?path=${encodeURIComponent(normalized)}` : '';
      return await this.fetchWithTimeout<FsBrowseResponse>(`/api/v1/fs/browse${q}`);
    } catch {
      return {
        current_path: targetPath || '',
        parent_path: '',
        is_git_repo: false,
        directories: [],
        quick_presets: [
          { label: 'User Home', path: '~' },
          { label: 'Root Filesystem', path: '/' },
        ],
      };
    }
  }

  public async getUniverse(dim: '2D' | '3D' = '3D'): Promise<GraphUniverse> {
    try {
      const q = dim === '2D' ? '?dim=2d' : '?dim=3d';
      const raw = await this.fetchWithTimeout<any>(`/api/v1/graph/universe${q}`, undefined, 20000);
      const rawNodes = raw.nodes;
      const normalizedNodes: UniverseNode[] = Array.isArray(rawNodes)
        ? rawNodes
        : rawNodes && typeof rawNodes === 'object'
        ? Object.entries(rawNodes).map(([id, val]: [string, any]) => ({
            id,
            x: typeof val.x === 'number' ? (Math.abs(val.x) > 5 ? val.x / 150 : val.x) : 0,
            y: typeof val.y === 'number' ? (Math.abs(val.y) > 5 ? val.y / 150 : val.y) : 0,
            z: typeof val.z === 'number' ? (Math.abs(val.z) > 5 ? val.z / 150 : val.z) : 0,
            cluster_id: val.cluster_id ?? 1,
          }))
        : [];
      return {
        repo_id: raw.repo_id || 'active',
        dimensions: raw.dimensions || dim,
        node_count: normalizedNodes.length,
        edge_count: (raw.edges || []).length,
        nodes: normalizedNodes,
        edges: raw.edges || [],
      };
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

  public async getGitGalaxy(limit = 200): Promise<GitGalaxy> {
    try {
      return await this.fetchWithTimeout<GitGalaxy>(`/api/v1/git/galaxy?limit=${limit}`);
    } catch {
      return {
        repo_id: 'local_git_repo',
        head_oid: 'a0382e3bab199cab4a2cb7912dab85f46f178317',
        active_branch: 'main',
        total_commits: 1,
        commits: [
          {
            oid: 'a0382e3bab199cab4a2cb7912dab85f46f178317',
            short_oid: 'a0382e3',
            parents: [],
            author_name: 'SynEvid Engineer',
            author_email: 'engineer@synevid.dev',
            timestamp: Math.floor(Date.now() / 1000),
            summary: 'Initial repo commit',
            lane: 0,
            is_head: true,
            branch_names: ['main'],
            tags: ['v0.4.0'],
            symbols_modified: 4,
            blast_radius_score: 2,
            verification_verdict: 'PASS',
          },
        ],
        branches: [{ name: 'main', target_oid: 'a0382e3bab199cab4a2cb7912dab85f46f178317', is_active: true }],
        tags: [{ name: 'v0.4.0', target_oid: 'a0382e3bab199cab4a2cb7912dab85f46f178317' }],
      };
    }
  }

  public async getTimeTravelFrame(oid: string, dim: '2d' | '3d' = '3d'): Promise<TimeTravelVisualFrame> {
    try {
      return await this.fetchWithTimeout<TimeTravelVisualFrame>(`/api/v1/git/time-travel?oid=${oid}&dim=${dim}`);
    } catch {
      return {
        oid,
        short_oid: oid.slice(0, 7),
        summary: 'Historical commit frame',
        timestamp: Math.floor(Date.now() / 1000),
        dimensions: dim === '3d' ? '3D' : '2D',
        node_count: 3,
        edge_count: 2,
        nodes: [
          { id: 'src/index.ts', x: 0, y: 0, z: 0, cluster_id: 0, diff_status: 'unchanged' },
          { id: 'src/engine.ts', x: 50, y: 20, z: 10, cluster_id: 1, diff_status: 'modified' },
          { id: 'src/utils.ts', x: -40, y: 30, z: -10, cluster_id: 2, diff_status: 'added' },
        ],
        edges: [
          { source: 'src/index.ts', target: 'src/engine.ts', kind: 'calls' },
          { source: 'src/engine.ts', target: 'src/utils.ts', kind: 'imports' },
        ],
        added_files: ['src/utils.ts'],
        modified_files: ['src/engine.ts'],
        deleted_files: [],
      };
    }
  }

  public async getGitDiff(baseOid: string, headOid: string): Promise<SemanticDiffReport> {
    try {
      return await this.fetchWithTimeout<SemanticDiffReport>(`/api/v1/git/diff?base=${baseOid}&head=${headOid}`);
    } catch {
      return {
        base_oid: baseOid,
        head_oid: headOid,
        added_files: [],
        modified_files: ['src/engine.ts'],
        deleted_files: [],
        total_changed_files: 1,
        blast_radius_score: 2,
        verdict: 'PASS',
        drifted_contracts: [],
      };
    }
  }

  public async stageGit(paths: string[]): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/stage', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ paths }),
    });
  }

  public async commitGit(message: string, authorName?: string, authorEmail?: string): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/commit', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message, author_name: authorName, author_email: authorEmail }),
    });
  }

  public async branchGit(branchName: string, action: 'create' | 'switch' | 'delete'): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/branch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ branch_name: branchName, action }),
    });
  }

  public async undoGit(): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/undo', {
      method: 'POST',
    });
  }

  public async getGitStatus(): Promise<GitStatusReport> {
    try {
      return await this.fetchWithTimeout<GitStatusReport>('/api/v1/git/status');
    } catch {
      return {
        branch: 'main',
        head_oid: null,
        staged: [],
        unstaged: [],
        untracked: [],
        deleted: [],
        ignored: [],
        is_clean: true,
        total_uncommitted: 0,
      };
    }
  }

  public async getFileDiff(path: string): Promise<FileDiffReport> {
    try {
      const res = await this.fetchWithTimeout<FileDiffReport>(`/api/v1/git/diff-file?path=${encodeURIComponent(path)}`);
      return {
        path: res?.path || path,
        status: res?.status || 'clean',
        old_content: res?.old_content || '',
        new_content: res?.new_content || '',
        diff_lines: res?.diff_lines || [],
        total_additions: res?.total_additions || 0,
        total_deletions: res?.total_deletions || 0,
      };
    } catch {
      return {
        path,
        status: 'clean',
        old_content: '',
        new_content: '',
        diff_lines: [],
        total_additions: 0,
        total_deletions: 0,
      };
    }
  }

  public async unstageGit(paths?: string[]): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/unstage', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(paths ? { paths } : {}),
    });
  }

  public async tagGit(req: TagRequest): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/tag', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
  }

  public async mergeGit(req: MergeRequest): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/merge', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
  }

  public async checkoutGit(req: CheckoutRequest): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/checkout', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
  }

  public async revertGit(commitOid: string): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/git/revert', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ commit_oid: commitOid }),
    });
  }

  public async bisectGit(req: BisectRequest): Promise<BisectReport> {
    return await this.fetchWithTimeout<BisectReport>('/api/v1/git/bisect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
    });
  }

  public async execTerminal(command: string): Promise<TerminalExecResponse> {
    return await this.fetchWithTimeout<TerminalExecResponse>('/api/v1/terminal/exec', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ command }),
    });
  }

  public async listForensicSessions(): Promise<ForensicSession[]> {
    return await this.fetchWithTimeout<ForensicSession[]>('/api/v1/forensics/sessions');
  }

  public async startForensicSession(actorName: string = 'Claude-Code', actorKind: string = 'AI_AGENT'): Promise<{ session_id: string; status: string }> {
    return await this.fetchWithTimeout<{ session_id: string; status: string }>('/api/v1/forensics/sessions/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ actor_name: actorName, actor_kind: actorKind }),
    });
  }

  public async getForensicTimeline(sessionId?: string): Promise<TimelineResponse> {
    const q = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : '';
    return await this.fetchWithTimeout<TimelineResponse>(`/api/v1/forensics/timeline${q}`);
  }

  public async revertForensicStep(sessionId: string, eventSeq: number): Promise<GitMutationResult> {
    return await this.fetchWithTimeout<GitMutationResult>('/api/v1/forensics/revert', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ session_id: sessionId, event_seq: eventSeq }),
    });
  }

  public async exportForensicTrace(sessionId: string): Promise<any> {
    return await this.fetchWithTimeout<any>(`/api/v1/forensics/export?session_id=${encodeURIComponent(sessionId)}`);
  }

  public async importForensicTrace(bundle: any): Promise<any> {
    return await this.fetchWithTimeout<any>('/api/v1/forensics/import', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ bundle }),
    });
  }

  public async getAiLedger(sessionId: string): Promise<AiSessionLedger> {
    return await this.fetchWithTimeout<AiSessionLedger>(`/api/v1/ai/ledger?session_id=${encodeURIComponent(sessionId)}`);
  }

  public async getAiConfigSnippet(client: string): Promise<AiConfigSnippetResponse> {
    return await this.fetchWithTimeout<AiConfigSnippetResponse>(`/api/v1/ai/config-snippet?client=${encodeURIComponent(client)}`);
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
