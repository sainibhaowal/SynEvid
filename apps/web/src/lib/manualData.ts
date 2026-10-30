import React from 'react';

export interface ManualDiagramStep {
  label: string;
  sublabel: string;
  badge?: string;
  color: 'cyan' | 'emerald' | 'amber' | 'rose' | 'indigo' | 'sky';
}

export interface ManualTableData {
  headers: string[];
  rows: string[][];
}

export interface ManualFormula {
  formula: string;
  label: string;
  explanation: string;
}

export interface ManualCardBox {
  title: string;
  badge?: string;
  color: 'cyan' | 'emerald' | 'amber' | 'rose' | 'indigo';
  points: string[];
}

export interface ManualSection {
  id: string;
  title: string;
  badge?: string;
  content: {
    summary: string;
    whatIsIt: string;
    howItWorks: string;
    diagramSteps?: ManualDiagramStep[];
    diagramTitle?: string;
    mathFormulas?: ManualFormula[];
    comparisonTable?: ManualTableData;
    infoCards?: ManualCardBox[];
    controlsAndButtons: Array<{
      name: string;
      shortcut?: string;
      action: string;
      details: string;
    }>;
    developerAndArchitecture: string;
    evidenceAndInvariants: string[];
    practicalExamples: Array<{
      scenario: string;
      steps: string[];
      expectedOutcome: string;
    }>;
  };
}

export interface WorkspaceManual {
  workspaceId: string;
  workspaceName: string;
  shortDescription: string;
  sections: ManualSection[];
}

export const WORKSPACE_MANUALS: Record<string, WorkspaceManual> = {
  'ws-repos': {
    workspaceId: 'ws-repos',
    workspaceName: 'Repositories & System Dashboard',
    shortDescription: 'Multi-repository telemetry, universal cross-platform path ingestion, SQLite WAL status, and baseline pinning.',
    sections: [
      {
        id: 'repos-overview',
        title: '1. Architecture & Ingestion Pipeline',
        badge: 'Core Architecture',
        content: {
          summary: 'Universal repository ingestion pipeline transforming arbitrary OS file paths into cryptographic AST snapshots.',
          whatIsIt: 'The Repository Dashboard is the root entry point of SynEvid (Code Autopsy). It provides transparent, mathematically verified metrics regarding the currently attached codebase: active snapshot hash, scanned source files, AST extracted symbols, and dependency edges. SynEvid acts as an honest read-only analyzer: zero files are ever modified, and every metric is persisted in SQLite WAL storage (`.autopsy/storage.db`).',
          howItWorks: 'When a repository path is submitted, the SynEvid daemon executes a strict 4-stage pipeline: (1) Path normalization across Windows, WSL, and POSIX, (2) Symlink containment check to prevent sandbox breakout, (3) Incremental AST symbol extraction via tree-sitter language frontends, and (4) BLAKE3 state digest computation to pin Snapshot A.',
          diagramTitle: 'Deterministic Ingestion & Verification Sequence',
          diagramSteps: [
            { label: '1. Path Ingestion', sublabel: 'C:\\... or //wsl.localhost/...', badge: 'Normalize', color: 'cyan' },
            { label: '2. Sandbox Guard', sublabel: 'follow_links(false) check', badge: 'INV-SANDBOX', color: 'indigo' },
            { label: '3. Tree-Sitter AST', sublabel: 'Functions, Classes, Imports', badge: 'Deterministic', color: 'emerald' },
            { label: '4. BLAKE3 Snapshot', sublabel: 'Pin Baseline Snapshot A', badge: 'Crypto-Proof', color: 'sky' }
          ],
          mathFormulas: [
            {
              formula: 'RepoID = BLAKE3(CanonicalPath) \\quad | \\quad SnapshotId = BLAKE3(AST_{nodes} \\parallel AST_{edges})',
              label: 'Deterministic Snapshot Hashing',
              explanation: 'Ensures identical directory trees produce bit-exact identifiers on Windows, Linux, and macOS without timestamps.'
            }
          ],
          comparisonTable: {
            headers: ['Input Path Format', 'Host OS', 'Normalized Daemon Path', 'Validation Rule'],
            rows: [
              ['C:\\Users\\Ravin\\Project', 'Windows', '/mnt/c/Users/Ravin/Project', 'Drive letter lowercased, slashes inverted'],
              ['\\\\wsl.localhost\\Ubuntu\\home\\...', 'WSL UNC', '/home/...', 'Distro prefix stripped, POSIX canonicalized'],
              ['/home/ravi/project', 'Linux', '/home/ravi/project', 'Canonical path verified with symlink containment'],
              ['/Users/ravi/project', 'macOS', '/Users/ravi/project', 'Case-sensitive UTF-8 path checked']
            ]
          },
          infoCards: [
            {
              title: 'Read-Only Invariant (FR-026)',
              badge: 'Hard Safety',
              color: 'emerald',
              points: [
                'Commands never modify source files or git history.',
                'AST parsing operates on read-only file streams.',
                'No outbound network requests or unauthorized write sockets.'
              ]
            },
            {
              title: 'Honest Semantic Coverage (FR-014)',
              badge: 'No False Passes',
              color: 'amber',
              points: [
                'Dynamic JavaScript eval() emits CoverageState::Unknown.',
                'Refuses to convert UNKNOWN status into a synthetic PASS.',
                'Explicitly logs skipped oversize files (>max_file_bytes).'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Repo Selector Dropdown',
              shortcut: 'Alt+1',
              action: 'Switches the active workspace repository.',
              details: 'Lists connected repositories from SQLite. Switching triggers an atomic memory switch on the daemon without restarting background processes.'
            },
            {
              name: 'Connect Repo Button (+)',
              action: 'Opens the Connect Repository dialog modal.',
              details: 'Accepts Windows paths, WSL UNC network paths, or native Linux paths with automated syntax normalization.'
            },
            {
              name: 'Refresh Telemetry (RefreshCw)',
              action: 'Forces a fresh pull of all telemetry metrics.',
              details: 'Executes parallel fetch of /api/status, /api/v1/repos/list, and /api/v1/files in <50ms.'
            }
          ],
          developerAndArchitecture: 'Adheres to ARCH_NO_CORE_TO_MCP and FR-026. Uses SQLite WAL mode with auto-recovery (ERR-STORAGE-102). In-memory cache invalidation ensures switching repositories immediately purges outdated graph coords.',
          evidenceAndInvariants: [
            'FR-026: Commands must never mutate source code or git history.',
            'INV-CROSS-001: Strict path normalization (forward slashes /) across all operating systems.',
            'NFR-001: 100/100 deterministic repeatability for snapshot digests.'
          ],
          practicalExamples: [
            {
              scenario: 'Attaching an external monorepo without moving directories',
              steps: [
                'Click "+ Connect" in the top bar.',
                'Paste "C:\\Users\\Ravin\\Projects\\Monorepo" into the path input.',
                'Click "Connect Repository".'
              ],
              expectedOutcome: 'Normalized to /mnt/c/Users/Ravin/Projects/Monorepo, registered in SQLite, and active immediately.'
            }
          ]
        }
      }
    ]
  },
  'ws-canvas': {
    workspaceId: 'ws-canvas',
    workspaceName: '3D/2D Code Universe',
    shortDescription: 'Deterministic force-directed 3D/2D WebGL graph visualizer of files, symbols, and cross-module call edges.',
    sections: [
      {
        id: 'universe-mechanics',
        title: '1. Deterministic Force Layout Engine',
        badge: 'WebGL Graph',
        content: {
          summary: 'Celestial WebGL dependency visualization with byte-identical force coordinates and interactive Ego-Network tracing.',
          whatIsIt: 'The Code Universe renders your repository as an interactive 3D celestial cluster. Each sphere node represents a source file or exported symbol, and connecting filaments represent semantic relationships: function calls, imports, class inheritance, and module references. Unlike standard D3 force graphs that drift on every page reload, SynEvid uses a 100% deterministic layout.',
          howItWorks: 'The layout algorithm calculates electrostatic repulsion between nodes and Hooke spring attraction along edges. The simulation is seeded with a 256-bit BLAKE3 hash of the graph structure. Coordinates (x, y, z) are normalized into a [-1.0, 1.0] bounding sphere and rendered at 60 FPS via Three.js with bloom shaders.',
          diagramTitle: 'Deterministic Force-Directed Pipeline',
          diagramSteps: [
            { label: '1. AST Call Graph', sublabel: 'Extract Nodes & Edges', badge: 'Tree-Sitter', color: 'cyan' },
            { label: '2. Seed Generation', sublabel: 'BLAKE3(GraphTopology)', badge: 'Deterministic', color: 'indigo' },
            { label: '3. Force Iterations', sublabel: 'Coulomb Repulsion + Spring', badge: '100 Epochs', color: 'emerald' },
            { label: '4. WebGL Render', sublabel: 'Three.js 60FPS Shaders', badge: 'Interactive', color: 'sky' }
          ],
          mathFormulas: [
            {
              formula: '\\vec{F}_{rep} = \\frac{k_{rep}}{r^2} \\hat{r}_{ij} \\quad , \\quad \\vec{F}_{attr} = k_{attr} (r - r_0) \\hat{r}_{ij}',
              label: 'Force-Directed Equilibrium Equations',
              explanation: 'Simulated over fixed discrete epochs seeded by cryptographic BLAKE3 hashes to guarantee identical node placement.'
            }
          ],
          comparisonTable: {
            headers: ['Dimension Mode', 'Projection', 'Best Use Case', 'Controls'],
            rows: [
              ['3D Constellation', 'Spherical coordinates (x, y, z)', 'Macro architecture & cluster density overview', 'Orbit drag, Pitch, Roll, Zoom'],
              ['2D Planar Map', 'Z-plane clamped to 0.0', 'Layered microservice and DAG flow inspection', 'Pan, Drag, Flat Zoom']
            ]
          },
          infoCards: [
            {
              title: 'Cluster Harmonic Coloring',
              badge: 'Visual Heuristic',
              color: 'cyan',
              points: [
                'Nodes in the same folder share matching chromatic color bands.',
                'Hub nodes with >10 incoming edges glow with cyan pulses.',
                'Impacted or failing nodes illuminate in amber or red.'
              ]
            },
            {
              title: 'Single-File Ego Network',
              badge: 'Deep Inspection',
              color: 'emerald',
              points: [
                'Clicking any node focuses its direct 1-hop neighborhood.',
                'Separates Incoming Callers (who calls me) from Outgoing Callees (who I call).',
                'Displays exact source line and column numbers.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: '3D / 2D Toggle Button',
              action: 'Flattens or expands visual dimensions.',
              details: 'Smoothly interpolates camera between planar 2D top-down view and 3D spherical constellation.'
            },
            {
              name: 'Node Click',
              action: 'Selects symbol and loads Ego-Network.',
              details: 'Pans camera to the node, highlights connected filaments, and populates the right inspection drawer.'
            },
            {
              name: 'Open In Simulator',
              action: 'Jumps directly to Blast Simulator (Alt+3).',
              details: 'Loads the selected function into the in-flight editor for immediate edit simulation.'
            }
          ],
          developerAndArchitecture: 'Zero LLM APIs used for graph placement. Implemented in Rust via `DeterministicForceLayout` trait in `apps/synevid-ui/src/server.rs` matching NFR-001 (Deterministic Repeatability).',
          evidenceAndInvariants: [
            'NFR-001: 100/100 deterministic coordinate repeatability.',
            'INV-CANVAS-001: WebGL headless fail-safe with 2D fallback rendering.'
          ],
          practicalExamples: [
            {
              scenario: 'Locating architectural bottlenecks in a legacy service',
              steps: [
                'Switch to Code Universe (Alt+2).',
                'Rotate the 3D constellation to find the brightest glowing hub node.',
                'Click the node to inspect all incoming callers across the codebase.'
              ],
              expectedOutcome: 'Immediate visualization of critical functions that have extensive blast radius.'
            }
          ]
        }
      }
    ]
  },
  'ws-simulator': {
    workspaceId: 'ws-simulator',
    workspaceName: 'Real-Time Blast Simulator',
    shortDescription: 'In-memory pre-write guardrail evaluating broken callers, arity mismatches, and contract breaks in <50ms.',
    sections: [
      {
        id: 'simulator-guardrails',
        title: '1. In-Flight Edit Simulation & Verification',
        badge: 'Pre-Write Guardrail',
        content: {
          summary: 'Sub-50ms unsaved AST evaluation detecting broken callers and contract drift before files are touched on disk.',
          whatIsIt: 'The Blast Simulator is a pre-write guardrail designed for human engineers and autonomous AI agents. Before you save a file to disk or commit a change, the simulator evaluates the unsaved buffer in memory, immediately calculating which downstream callers break, what lines fail, and whether dead code is created.',
          howItWorks: 'The simulation endpoint (`POST /api/v1/simulate/edit`) receives the unsaved buffer. SynEvid creates a transient AST in memory, diffs modified signatures against the baseline snapshot, queries known incoming call sites, and returns broken callers, arity errors, and dead code diagnostics in <50ms without touching disk.',
          diagramTitle: 'Pre-Write Guardrail Execution Loop',
          diagramSteps: [
            { label: '1. Unsaved Buffer', sublabel: 'In-memory edit patch', badge: 'Zero-Disk-Write', color: 'cyan' },
            { label: '2. Transient AST', sublabel: 'Tree-sitter signature diff', badge: '<20ms', color: 'indigo' },
            { label: '3. Caller Cross-Check', sublabel: 'Arity & type contract test', badge: '<25ms', color: 'amber' },
            { label: '4. Verdict Receipt', sublabel: 'PASS, WARN, or FAIL receipt', badge: 'BLAKE3', color: 'rose' }
          ],
          mathFormulas: [
            {
              formula: '\\Delta_{blast} = \\{ c \\in Callers(f) \\mid Arity(c) \\neq Arity(f\') \\lor \\neg Compatible(Sig(f\'), Sig(f)) \\}',
              label: 'Blast Radius Mathematical Formulation',
              explanation: 'Identifies the exact subset of downstream calling sites invalidated by the proposed signature edit.'
            }
          ],
          comparisonTable: {
            headers: ['Verdict', 'Condition', 'Visual Indicator', 'Agent Action'],
            rows: [
              ['VERIFIED PASS', 'Zero broken callers, zero syntax errors, contracts preserved', 'Green Badge (CheckCircle)', 'Proceed to write file and commit'],
              ['WARNING (WARN)', 'Compatible change with newly dead code or unreferenced exports', 'Amber Badge (AlertTriangle)', 'Review dead code recommendations'],
              ['BREAKING (FAIL)', 'Parameter arity mismatch, missing required argument, syntax break', 'Red Badge (XCircle)', 'Blocked: Auto-repair before saving']
            ]
          },
          infoCards: [
            {
              title: 'Sub-50ms Evaluation Speed',
              badge: 'Ultra Fast',
              color: 'emerald',
              points: [
                'Runs entirely in-memory without compiling or bundling.',
                '100x faster than running Jest, Vitest, or Cargo test suites.',
                'Provides instant feedback while typing.'
              ]
            },
            {
              title: 'Read-Only Integrity (FR-026)',
              badge: 'Zero Risk',
              color: 'cyan',
              points: [
                'Unsaved buffers are never written to disk.',
                'Allows aggressive experimental refactoring with zero risk.',
                'Revert Buffer button restores disk baseline in 1 click.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Simulate Edit Button',
              shortcut: 'Ctrl+Enter',
              action: 'Triggers instant in-memory AST evaluation.',
              details: 'Computes damaged callers and returns arity diagnostics in under 50 milliseconds.'
            },
            {
              name: 'Revert Buffer',
              action: 'Restores buffer to disk baseline.',
              details: 'Discards experimental changes and resets the editor to the verified repository state.'
            },
            {
              name: 'Damaged Callers Panel',
              action: 'Click any caller to spotlight.',
              details: 'Shows exact calling file path, line number, column number, and diagnostic error message.'
            }
          ],
          developerAndArchitecture: 'Enforces FR-026 (Read-Only Analyzer Engine). Implemented in Rust with zero subprocess execution. Supports TypeScript, JavaScript, Rust, Python, Go, Java, and C# signatures.',
          evidenceAndInvariants: [
            'FR-026: Commands must never mutate source code or git history.',
            'NFR-004: Sub-50ms simulation response latency for instant agent feedback.'
          ],
          practicalExamples: [
            {
              scenario: 'Adding a mandatory parameter to a core payment service function',
              steps: [
                'Open Blast Simulator (Alt+3).',
                'Change `chargeCard(amount: number)` to `chargeCard(amount: number, taxRate: number)`.',
                'Press Ctrl+Enter or click "Simulate Edit".'
              ],
              expectedOutcome: 'Instant red FAIL receipt showing: "OrderController.ts:42 — Arity mismatch: expected 2 arguments, caller passes 1".'
            }
          ]
        }
      }
    ]
  },
  'ws-git': {
    workspaceId: 'ws-git',
    workspaceName: 'Native .git Engine & Time-Travel Galaxy',
    shortDescription: 'Zero-subprocess git forensics, commit lanes, semantic diffs, instant undo checkpoints, and automated bisect.',
    sections: [
      {
        id: 'git-galaxy-forensics',
        title: '1. Pure-Rust Git Forensics & Time-Travel',
        badge: 'Native Git',
        content: {
          summary: 'Direct git packfile and object DAG analysis without CLI child processes, featuring semantic blast radius per commit.',
          whatIsIt: 'The Git Galaxy provides visual historical time-travel across your repository. Built on `autopsy-git-engine`, it reads `.git` packfiles and loose objects directly in pure Rust without executing `git` CLI child processes. It visualizes commit lanes, semantic blast radius per commit, active branches, and tags.',
          howItWorks: 'SynEvid reconstructs the commit Directed Acyclic Graph (DAG) directly from `.git/objects`. For each commit, it calculates the semantic symbol diff: which exported symbols were added, modified, or removed, and scores the commit blast radius based on downstream impact.',
          diagramTitle: 'Pure-Rust Git Object Reader Pipeline',
          diagramSteps: [
            { label: '1. .git/objects', sublabel: 'Read packfiles & loose blobs', badge: 'Zero-Subprocess', color: 'cyan' },
            { label: '2. Commit DAG', sublabel: 'Reconstruct ancestry lanes', badge: 'Pure Rust', color: 'indigo' },
            { label: '3. Semantic Differ', sublabel: 'AST symbol diff per commit', badge: 'Deterministic', color: 'emerald' },
            { label: '4. Visual Galaxy', sublabel: 'Color-coded blast radius nodes', badge: 'Interactive', color: 'sky' }
          ],
          mathFormulas: [
            {
              formula: 'BlastScore(C) = \\sum_{s \\in \\Delta(C)} (1.0 \\cdot \\mathbb{I}_{broken}(s) + 0.5 \\cdot \\mathbb{I}_{mod}(s) + 0.2 \\cdot \\mathbb{I}_{add}(s))',
              label: 'Commit Blast Radius Metric',
              explanation: 'Weights breaking contract removals and arity shifts significantly higher than benign additions.'
            }
          ],
          comparisonTable: {
            headers: ['Operation', 'SynEvid Pure-Rust Engine', 'Traditional Git CLI Child Process', 'Performance & Safety Advantage'],
            rows: [
              ['Commit DAG Read', 'Direct memory mapped packfile scan', 'Spawns `git log --graph` subshell', '10x faster, zero process overhead'],
              ['Semantic Diff', 'In-memory AST symbol signature diff', 'Line-based text patch diff', 'Identifies broken functions, not just line edits'],
              ['Undo Safety', 'Cryptographic undo checkpoints in SQLite', 'Dangerous `git reset --hard`', 'Zero risk of permanent unrecoverable data loss']
            ]
          },
          infoCards: [
            {
              title: 'Visual Git Bisect',
              badge: 'Automated Root-Cause',
              color: 'rose',
              points: [
                'Binary search across commit history to pinpoint regressions.',
                'Tests architectural invariants (e.g. no cycles) at each commit step.',
                'Returns the exact breaking commit OID, author, and timestamp.'
              ]
            },
            {
              title: 'Guarded Undo Checkpoints',
              badge: 'Undo Journal',
              color: 'emerald',
              points: [
                'Every staged commit or checkout creates an undo checkpoint.',
                '1-click undo reverts head state without losing untracked files.',
                'Backed by `.autopsy/storage.db` mutation journal.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Commit Node Click',
              action: 'Inspects commit semantic diff report.',
              details: 'Displays author, date, message, added files, modified files, deleted files, and drifted contracts.'
            },
            {
              name: 'Branch Filter Dropdown',
              action: 'Filters commit lanes by branch.',
              details: 'Switches between main, release branches, and feature lanes.'
            },
            {
              name: 'Undo Last Action',
              action: 'Rolls back the latest git operation.',
              details: 'Restores the previous head OID using the SynEvid undo journal.'
            },
            {
              name: 'Run Git Bisect',
              action: 'Launches automated invariant bisect.',
              details: 'Select an invariant rule and let SynEvid automatically find the commit that broke it.'
            }
          ],
          developerAndArchitecture: 'Uses zero outbound network connections and zero sub-processes (INV-SANDBOX-004). Reads git objects directly in pure Rust via `autopsy-git-engine`.',
          evidenceAndInvariants: [
            'FR-028: Offline deterministic git history evaluation.',
            'INV-SANDBOX-004: Zero subprocess execution for git operations.'
          ],
          practicalExamples: [
            {
              scenario: 'Pinpointing which commit broke architectural layering',
              steps: [
                'Open Git Galaxy (Alt+4).',
                'Click "Git Bisect".',
                'Select rule "layer-boundaries" and run.',
                'SynEvid automatically inspects commits in binary search order.'
              ],
              expectedOutcome: 'Pinpoints the exact commit hash and developer who introduced the illegal dependency.'
            }
          ]
        }
      }
    ]
  },
  'ws-trajectory': {
    workspaceId: 'ws-trajectory',
    workspaceName: 'Phase 4 Real-Time Forensic Trajectory Timeline',
    shortDescription: 'Three-tier forensic hierarchy tracking Sessions, Workflows, and Atomic Jobs with BLAKE3 cryptographic receipts.',
    sections: [
      {
        id: 'trajectory-forensics',
        title: '1. Three-Tier Forensic Tracking Model',
        badge: 'Phase 4 Core',
        content: {
          summary: 'Cryptographic tracking of human, CI, and AI agent operations inside SQLite WAL storage with 1-click step rollbacks.',
          whatIsIt: 'The Forensic Trajectory Timeline is the flagship Phase 4 capability of SynEvid. It records every action taken in the repository—by a human engineer in the editor, an automated CI pipeline, or an autonomous AI agent over MCP. Every operation is structured into Sessions, Workflows, and atomic Jobs, signed with a cryptographic BLAKE3 receipt.',
          howItWorks: 'State transitions are written into `.autopsy/storage.db` tables (`forensic_sessions`, `forensic_workflows`, `trajectory_events`). The daemon broadcasts events over Server-Sent Events (`/api/v1/live/events`). Clicking any event node replays the exact blast radius on the 3D Code Universe and opens the side-by-side Before/After diff drawer.',
          diagramTitle: 'Three-Tier Forensic Hierarchy Architecture',
          diagramSteps: [
            { label: 'Tier 1: Session', sublabel: 'Bounded Episode (Actor: Claude/Cursor)', badge: 'SessionID', color: 'cyan' },
            { label: 'Tier 2: Workflow', sublabel: 'Guardrail -> Edit -> Verify Loop', badge: 'WorkflowID', color: 'indigo' },
            { label: 'Tier 3: Atomic Job', sublabel: 'simulate_edit, verify_run, commit', badge: 'EventSeq', color: 'emerald' },
            { label: 'Receipt Drawer', sublabel: 'Diff A vs B + BLAKE3 Digest', badge: 'Cryptographic', color: 'sky' }
          ],
          mathFormulas: [
            {
              formula: 'Receipt = BLAKE3(SessionID \\parallel StepSeq \\parallel Snapshot_A \\parallel Snapshot_B \\parallel Verdict)',
              label: 'Cryptographic Forensic Receipt',
              explanation: 'Provides tamper-evident proof of every transformation executed by human or autonomous agents.'
            }
          ],
          comparisonTable: {
            headers: ['Hierarchy Entity', 'Database Table', 'What You See on Screen', 'Core Purpose'],
            rows: [
              ['1. Forensic Session', 'forensic_sessions', 'Top lane card: Actor badge, Snapshot A & B hashes, active status', 'Isolates the bounded engineering task'],
              ['2. Workflow Pipeline', 'forensic_workflows', 'Grouped horizontal bracket connecting related jobs', 'Measures multi-step duration and aggregate pass/fail'],
              ['3. Atomic Job Event', 'trajectory_events', 'Clickable colored pill node along the timeline track', 'Records individual queries, simulations, edits, and verifications']
            ]
          },
          infoCards: [
            {
              title: 'Interactive Canvas Replay',
              badge: 'Visual Replay',
              color: 'cyan',
              points: [
                'Clicking an event node highlights the exact files and call edges modified.',
                'Rotates the 3D Code Universe camera to the epicenter of the step.',
                'Pulses broken callers in red and repaired callers in green.'
              ]
            },
            {
              title: '1-Click Step Rollback',
              badge: 'Undo Safety',
              color: 'emerald',
              points: [
                'Every step preserves snapshot hashes and modified buffers.',
                'Click "Revert to This Step" to roll working state back instantly.',
                'Allows safe experimentation during multi-step AI agent coding.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Session Selector',
              action: 'Switches between active and historical sessions.',
              details: 'Displays Actor badge (Claude-Code, Cursor, Windsurf, Local-User), Baseline Hash, and Verdict.'
            },
            {
              name: 'Timeline Step Pills',
              action: 'Click any step node to inspect forensic receipt.',
              details: 'Color-coded: Cyan (Info/Query), Green (Pass), Amber (Warning), Red (Breaking Fail).'
            },
            {
              name: 'Replay on Canvas',
              action: 'Spotlights step impact on the 3D Code Universe.',
              details: 'Pans the 3D camera to affected nodes and projects the blast radius.'
            },
            {
              name: 'Revert to This Step',
              action: 'Rolls back working state to that exact step.',
              details: 'Restores the buffer state from SQLite WAL without losing historical audit logs.'
            }
          ],
          developerAndArchitecture: 'Implements SQLite Migration 005 in `crates/autopsy-storage`. Zero LLM used in trajectory recording. All receipts are computed deterministically using BLAKE3.',
          evidenceAndInvariants: [
            'FR-028: 100/100 deterministic BLAKE3 snapshot digests.',
            'AGENT-GATE-1: Forensic verification receipt traceability.'
          ],
          practicalExamples: [
            {
              scenario: 'Auditing an autonomous AI coding agent refactoring session',
              steps: [
                'Open Trajectory & Jobs (Alt+5).',
                'Select the active Claude-Code session.',
                'Click Step #3 (which failed invariant checks).',
                'Inspect the broken callers list in the Forensic Receipt Drawer.'
              ],
              expectedOutcome: 'Cryptographic proof showing exactly what broke at Step #3 and how Step #4 repaired it.'
            }
          ]
        }
      }
    ]
  },
  'ws-gates': {
    workspaceId: 'ws-gates',
    workspaceName: 'Gates, Policy & Invariant Verification',
    shortDescription: 'Offline deterministic rule verification: circular dependency detection, architectural layering, and API contract compliance.',
    sections: [
      {
        id: 'gates-policy',
        title: '1. Invariant Policy & Quality Gates',
        badge: 'Deterministic Gates',
        content: {
          summary: 'Zero-LLM architectural guardrails verifying code integrity against strict structural policies with zero false passes.',
          whatIsIt: 'The Gates & Policy workspace is SynEvid’s automated compliance officer. It evaluates code against strict structural invariants: no circular dependencies, architectural layer boundaries (e.g. presentation cannot depend on storage), and public API contract stability.',
          howItWorks: 'SynEvid executes deterministic graph algorithms (Tarjan Strongly Connected Components for cycle detection, contract signature hashing) directly against the AST snapshot. Findings are categorized into PASS, WARN, or FAIL with exact file and line references.',
          diagramTitle: 'Invariant Gate Evaluation Architecture',
          diagramSteps: [
            { label: '1. Graph Snapshot', sublabel: 'AST nodes & call edges', badge: 'In-Memory', color: 'cyan' },
            { label: '2. Tarjan Cycle Test', sublabel: 'Find Strongly Connected Components', badge: 'O(V+E)', color: 'indigo' },
            { label: '3. Layer Boundary Check', sublabel: 'Enforce ARCH_NO_CORE_TO_MCP', badge: 'Deterministic', color: 'amber' },
            { label: '4. Verdict Gate', sublabel: 'PASS / WARN / FAIL Verdict', badge: 'Honest Coverage', color: 'emerald' }
          ],
          mathFormulas: [
            {
              formula: 'SCC(G) = \\{ C_1, C_2, \\dots, C_k \\} \\quad \\text{where } \\forall u,v \\in C_i, u \\leadsto v \\land v \\leadsto u',
              label: 'Tarjan Strongly Connected Components',
              explanation: 'Any component with $|C_i| > 1$ constitutes an illegal circular dependency loop that fails the quality gate.'
            }
          ],
          comparisonTable: {
            headers: ['Invariant Rule', 'Algorithm', 'Failure Condition', 'Enforcement Level'],
            rows: [
              ['no-circular-deps', 'Tarjan SCC cycle detection', 'Presence of any directed cycle across files', 'Hard Blocking (FAIL)'],
              ['layer-boundaries', 'Directed acyclic reachability check', 'Core crate importing presentation or MCP', 'Hard Blocking (FAIL)'],
              ['api-contract-compatibility', 'Public signature BLAKE3 hashing', 'Exported function changes arity or return type', 'Warning or Breaking FAIL'],
              ['dead-code-detection', 'Root-to-leaf reachability traversal', 'Exported symbol has 0 workspace callers', 'Advisory Warning (WARN)']
            ]
          },
          infoCards: [
            {
              title: 'Honest Semantic Coverage (FR-014)',
              badge: 'No Compromise',
              color: 'emerald',
              points: [
                'SynEvid never turns an UNKNOWN into a false PASS.',
                'Dynamic constructs emit explicit CoverageState::Unknown.',
                'Ensures mission-critical codebases never deploy unverified code.'
              ]
            },
            {
              title: 'Instant Execution Speed',
              badge: '<10ms Evaluation',
              color: 'cyan',
              points: [
                'Runs in pure Rust in under 10 milliseconds.',
                'Can be used as a pre-commit hook or live editor gate.',
                'Produces machine-readable JSON reports for CI/CD.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Run Verification Gates',
              action: 'Evaluates all registered invariant policies.',
              details: 'Re-runs Tarjan cycle analysis and contract checks, updating the header badge.'
            },
            {
              name: 'Rule Finding Filter',
              action: 'Filters findings by status.',
              details: 'Toggles between All, Failing, Warning, and Passing rules.'
            },
            {
              name: 'Jump to Location',
              action: 'Focuses violating file and line.',
              details: 'Navigates Explorer and Code Universe directly to the offending symbol.'
            }
          ],
          developerAndArchitecture: 'Zero LLM APIs used in invariant checks. Adheres to FR-014 and FR-030 (Honest Semantic Coverage). Evaluated via pure Rust graph traversal algorithms in `crates/autopsy-core`.',
          evidenceAndInvariants: [
            'FR-014: Honest semantic coverage states (Verified, Partial, Unknown).',
            'FR-030: Zero false-pass guarantees on unanalyzable dynamic constructs.'
          ],
          practicalExamples: [
            {
              scenario: 'Blocking an accidental circular import before pushing to main',
              steps: [
                'Developer adds an import from `service.ts` back into `controller.ts`.',
                'Open Gates & Policy (Alt+6).',
                'Click "Run Verification Gates".'
              ],
              expectedOutcome: 'Instant red FAIL badge highlighting the cycle: `controller.ts -> service.ts -> controller.ts`.'
            }
          ]
        }
      }
    ]
  },
  'ws-ai': {
    workspaceId: 'ws-ai',
    workspaceName: 'AI Zero-Token Cache & Live MCP Cockpit',
    shortDescription: 'Zero-token-waste working memory cache (60%-91.5% token savings), 1-click MCP configurators, and interactive verification terminal.',
    sections: [
      {
        id: 'ai-token-cache',
        title: '1. AI Zero-Token-Waste Working Memory Cache',
        badge: 'Phase 4 AI Cache',
        content: {
          summary: 'Persistent SQLite working history that returns delta-compressed receipts, eliminating redundant context token waste.',
          whatIsIt: 'AI coding agents (Claude Code, Cursor, Windsurf, Gemini) waste tens of thousands of prompt tokens re-reading whole files and repeating graph searches. Phase 4 introduced the Zero-Token Working History Engine: a persistent local SQLite working cache keyed by `(session_id, snapshot_blake3, symbol_id)`. When an agent queries an impact or simulates an edit, SynEvid returns only the modified delta or a 20-token pointer receipt.',
          howItWorks: 'The daemon checks `ai_working_cache` in `.autopsy/storage.db`. On repeated queries with identical BLAKE3 hashes, it returns `status: UNCHANGED_SINCE_STEP_X`, slashing LLM context consumption by 60% to 91.5% (verified in `phase4_ai_cache_efficiency.rs`).',
          diagramTitle: 'AI Working Memory Cache Loop',
          diagramSteps: [
            { label: 'Step 1: AI Query', sublabel: 'autopsy_impact("chargeCard")', badge: '~180 tokens', color: 'cyan' },
            { label: 'Step 2: Store Digest', sublabel: 'Key: BLAKE3(Session+Snap+Sym)', badge: 'SQLite WAL', color: 'indigo' },
            { label: 'Step 3: In-Flight Edit', sublabel: 'autopsy_simulate_edit(patch)', badge: 'Delta <90 tokens', color: 'amber' },
            { label: 'Step 4: Repeated Query', sublabel: 'UNCHANGED_SINCE_STEP_1', badge: '20 tokens (91.5% saved)', color: 'emerald' }
          ],
          mathFormulas: [
            {
              formula: '\\text{TokenSavings} = 1 - \\frac{\\text{Tokens}(\\text{PointerReceipt})}{\\text{Tokens}(\\text{FullJsonPayload})} = 1 - \\frac{20}{2500} = 99.2\\%',
              label: 'Token Reduction Ratio Formulation',
              explanation: 'Reduces prompt token expenditure from multi-thousand-token file dumps down to compact cryptographic pointers.'
            }
          ],
          comparisonTable: {
            headers: ['Operation', 'Standard AI Workflow (Without SynEvid)', 'SynEvid Phase 4 Working Cache', 'Token Savings'],
            rows: [
              ['Finding Callers', 'AI re-reads 15 entire files (~25,000 tokens)', 'SynEvid returns exact 10 callers (~180 tokens)', '99.2% saved'],
              ['In-Flight Edit Test', 'AI saves to disk and runs test suite (~10,000 tokens)', 'Simulates in memory in <50ms and returns delta (<90 tokens)', '99.1% saved'],
              ['Repeated Query', 'AI re-queries and redumps entire JSON payload (~2,500 tokens)', 'Returns pointer: UNCHANGED_SINCE_STEP_X (~20 tokens)', '99.2% saved'],
              ['Average Refactor Task', 'Consumes 120,000+ context tokens', 'Consumes under 10,200 context tokens', '91.5% overall reduction']
            ]
          },
          infoCards: [
            {
              title: 'Zero Hallucinations Guarantee',
              badge: 'Deterministic Memory',
              color: 'emerald',
              points: [
                'Memory receipts are derived from pure Rust AST parsing.',
                'The agent cannot hallucinate caller locations or line numbers.',
                'Enforces the Baseline -> Impact -> Edit -> Verify -> Repair loop.'
              ]
            },
            {
              title: '21 Deterministic MCP Tools',
              badge: 'Complete Protocol',
              color: 'cyan',
              points: [
                'Includes 4 new Phase 4 tools: autopsy_simulate_edit, autopsy_canvas_focus, autopsy_git_time_travel, autopsy_working_memory.',
                '100% backward compatible with existing v0.1.0 tools.',
                'Runs over local loopback or STDIO with sub-15ms response latency.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Tokens Saved Live Counter',
              action: 'Displays cumulative tokens saved in session.',
              details: 'Tracks token reduction percentage across all agent queries.'
            },
            {
              name: 'Cache Hit / Miss Ratio',
              action: 'Shows percentage of queries served from memory.',
              details: 'Quantifies cache efficiency across multi-step agent coding loops.'
            },
            {
              name: '1-Click MCP Client Configurator',
              action: 'Copy-paste configurations for Claude Code, Cursor, Windsurf, Gemini.',
              details: 'Generates ready-to-use JSON blocks for easy setup.'
            },
            {
              name: 'Embedded Verification Terminal',
              action: 'Interactive native CLI workbench.',
              details: 'Run syn verify, syn impact, syn status, or ls directly inside the app.'
            }
          ],
          developerAndArchitecture: 'Stores ledger in `ai_session_ledger` table. Fully deterministic: identical input trees produce identical cache keys and delta receipts.',
          evidenceAndInvariants: [
            'PHASE-4-v0.5.0: AI Agent Zero-Token-Waste Working Memory specification.',
            'Verified in tests/tests/phase4_ai_cache_efficiency.rs (91.50% token reduction).'
          ],
          practicalExamples: [
            {
              scenario: 'Refactoring a high-impact payment service with Cursor or Claude Code',
              steps: [
                'Copy Cursor MCP configuration from the AI MCP Cockpit (Alt+7).',
                'Prompt agent: "Refactor PaymentService using SynEvid guardrails".',
                'Observe real-time Tokens Saved counter incrementing in the cockpit.'
              ],
              expectedOutcome: 'Agent refactors code with sub-50ms feedback, achieving 91.5% token reduction with zero context overflow.'
            }
          ]
        }
      }
    ]
  },
  'ws-settings': {
    workspaceId: 'ws-settings',
    workspaceName: 'System Configuration & Registered Plugins',
    shortDescription: 'Extensible plugin architecture (autopsy-plugin-api), deterministic layout registries, and blast radius configurations.',
    sections: [
      {
        id: 'settings-plugins',
        title: '1. Plugin API & System Configuration',
        badge: 'Plugin Architecture',
        content: {
          summary: 'Dynamic trait registries allowing custom layout algorithms, language frontends, and policy gates without modifying core engine crates.',
          whatIsIt: 'The Settings & Plugins workspace displays the active plugin registry and configurable runtime parameters. SynEvid features an open plugin architecture (`autopsy-plugin-api`) where layouts, language frontends, and policy gates are decoupled into traits.',
          howItWorks: 'On startup, `DashboardServer` initializes `PluginRegistry` and registers deterministic plugins (such as `DeterministicForceLayout`). Developers can compile custom layout algorithms or domain-specific invariants as dynamic plugins without altering the core verification engine.',
          diagramTitle: 'Extensible Plugin Registry Architecture',
          diagramSteps: [
            { label: '1. Core Verification Crates', sublabel: 'autopsy-core, autopsy-storage', badge: 'Zero-LLM', color: 'cyan' },
            { label: '2. Plugin API Trait Boundary', sublabel: 'autopsy-plugin-api interfaces', badge: 'ARCH_INVERSION', color: 'indigo' },
            { label: '3. Registered Plugins', sublabel: 'Layouts, Language Parsers, Gates', badge: 'Extensible', color: 'emerald' },
            { label: '4. Presentation Layer', sublabel: 'Desktop Shell, MCP, Web Inspector', badge: 'Read-Only', color: 'sky' }
          ],
          mathFormulas: [
            {
              formula: '\\text{TraverseDepth}(k) = \\{ v \\in V \\mid \\text{dist}(u, v) \\le k \\} \\quad \\text{bounded by } max\\_traversal\\_nodes',
              label: 'Configurable Exploration Depth',
              explanation: 'Bounds graph exploration to prevent memory exhaustion on massive enterprise monorepos.'
            }
          ],
          comparisonTable: {
            headers: ['Setting Parameter', 'Default Value', 'Configurable Range', 'Impact on Analysis'],
            rows: [
              ['Blast Radius Depth (k)', '3 hops', '1 to 10 hops', 'Controls how far downstream callers are explored'],
              ['Confidence Threshold', 'Exact (1.0)', 'Exact, Probable, Inferred', 'Filters uncertain dynamic edges'],
              ['Deterministic Layout', 'DeterministicForceLayout', '2D / 3D force simulation', 'Controls coordinate generation for WebGL'],
              ['Max Traversal Nodes', '50,000 nodes', '1,000 to 500,000 nodes', 'Resource bound preventing memory exhaustion']
            ]
          },
          infoCards: [
            {
              title: 'Architectural Inversion Rule',
              badge: 'Strict Invariant',
              color: 'emerald',
              points: [
                'Apps depend on core public interfaces, never the reverse.',
                'Core crates never import presentation, MCP, or GUI layers.',
                'Guarantees modularity and headless testability.'
              ]
            },
            {
              title: 'Permanent Zero-Major Versioning',
              badge: 'Versioning Standard',
              color: 'cyan',
              points: [
                'SynEvid permanently remains under v0.',
                'Strict format: v0.<milestone>.<fix_patch>.',
                'Never increments to v1.0.0; preserves offline deterministic guarantees.'
              ]
            }
          ],
          controlsAndButtons: [
            {
              name: 'Registered Layouts List',
              action: 'Displays active force layout implementations.',
              details: 'Lists DeterministicForceLayout-2D and 3D with verification digests.'
            },
            {
              name: 'Blast Radius Depth Slider',
              action: 'Adjusts caller exploration depth (top_k hops).',
              details: 'Configures how many downstream dependency levels are traversed during impact assessment.'
            },
            {
              name: 'Confidence Threshold',
              action: 'Sets minimum confidence level for dynamic edges.',
              details: 'Filters between Exact, Probable, and Inferred call edges.'
            }
          ],
          developerAndArchitecture: 'Dynamic Trait Registries in `crates/autopsy-plugin-api`. Enforces strict architectural inversion: core crates never depend on presentation or plugin implementations.',
          evidenceAndInvariants: [
            'ARCH_NO_CORE_TO_MCP: Clean architectural boundaries.',
            'Zero LLM API dependencies across all plugins.'
          ],
          practicalExamples: [
            {
              scenario: 'Configuring deep impact analysis for large enterprise monorepos',
              steps: [
                'Open Settings & Plugins (Alt+8).',
                'Adjust Depth from 3 hops to 5 hops.',
                'Run symbol query to inspect distant microservice callers.'
              ],
              expectedOutcome: 'Deep traversal across distant architectural boundaries with full BLAKE3 verification.'
            }
          ]
        }
      }
    ]
  }
};
