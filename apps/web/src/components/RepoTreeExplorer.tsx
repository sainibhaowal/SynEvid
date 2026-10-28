'use client';

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import {
  Folder,
  FolderOpen,
  ChevronRight,
  ChevronDown,
  FileCode,
  FileText,
  File,
  Search,
  X,
  RotateCcw,
  ChevronsUpDown,
  ChevronsDownUp,
  GitBranch,
} from 'lucide-react';
import { RepoFileItem, api, GitStatusReport } from '@/lib/api';

export interface TreeNode {
  id: string; // full relative path
  name: string;
  isFolder: boolean;
  ext?: string;
  status?: 'verified' | 'impacted';
  gitStatus?: 'modified' | 'untracked' | 'staged' | 'deleted' | 'ignored';
  children?: TreeNode[];
  fileCount?: number;
}

interface RepoTreeExplorerProps {
  files: RepoFileItem[];
  selectedFilePath: string | null;
  onSelectFile: (filePath: string) => void;
  onRefresh: () => void;
  isRefreshing?: boolean;
  gitStatusMap?: Record<string, 'modified' | 'untracked' | 'staged' | 'deleted' | 'ignored'>;
}

// Build hierarchical tree structure from flat file paths
function buildTreeFromFiles(
  files: RepoFileItem[],
  statusMap: Record<string, 'modified' | 'untracked' | 'staged' | 'deleted' | 'ignored'> = {}
): { tree: TreeNode[]; folderCount: number } {
  const root: { [key: string]: any } = {};
  let totalFolders = 0;

  for (const file of files) {
    const parts = file.path.split('/').filter(Boolean);
    let current = root;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isFile = i === parts.length - 1;

      if (isFile) {
        current[part] = {
          __isFile: true,
          file,
        };
      } else {
        if (!current[part]) {
          current[part] = {};
          totalFolders++;
        }
        current = current[part];
      }
    }
  }

  function convert(obj: any, currentPath: string): TreeNode[] {
    const nodes: TreeNode[] = [];

    const keys = Object.keys(obj).sort((a, b) => {
      const aIsFile = !!obj[a].__isFile;
      const bIsFile = !!obj[b].__isFile;
      if (aIsFile !== bIsFile) {
        return aIsFile ? 1 : -1; // folders first
      }
      return a.localeCompare(b);
    });

    for (const key of keys) {
      const val = obj[key];
      const fullPath = currentPath ? `${currentPath}/${key}` : key;

      if (val.__isFile) {
        const f: RepoFileItem = val.file;
        const gitStatus = statusMap[fullPath] || statusMap[f.path];
        nodes.push({
          id: fullPath,
          name: key,
          isFolder: false,
          ext: f.ext,
          status: f.status,
          gitStatus,
        });
      } else {
        const children = convert(val, fullPath);
        const countFiles = (nList: TreeNode[]): number =>
          nList.reduce((acc, n) => acc + (n.isFolder ? countFiles(n.children || []) : 1), 0);
        
        // Propagate folder git status if any child has changes
        const hasModified = children.some(c => c.gitStatus === 'modified');
        const hasUntracked = children.some(c => c.gitStatus === 'untracked');
        const hasStaged = children.some(c => c.gitStatus === 'staged');
        const folderGitStatus = hasStaged ? 'staged' : hasModified ? 'modified' : hasUntracked ? 'untracked' : undefined;

        nodes.push({
          id: fullPath,
          name: key,
          isFolder: true,
          children,
          fileCount: countFiles(children),
          gitStatus: folderGitStatus,
        });
      }
    }

    return nodes;
  }

  return {
    tree: convert(root, ''),
    folderCount: totalFolders,
  };
}

export default function RepoTreeExplorer({
  files,
  selectedFilePath,
  onSelectFile,
  onRefresh,
  isRefreshing = false,
  gitStatusMap: propGitStatusMap,
}: RepoTreeExplorerProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [gitStatus, setGitStatus] = useState<GitStatusReport | null>(null);

  const fetchGitStatus = useCallback(async () => {
    try {
      const data = await api.getGitStatus();
      setGitStatus(data);
    } catch {
      // offline fallback
    }
  }, []);

  useEffect(() => {
    fetchGitStatus();
    const interval = setInterval(fetchGitStatus, 2500);
    return () => clearInterval(interval);
  }, [fetchGitStatus, files]);

  const activeGitStatusMap = useMemo(() => {
    if (propGitStatusMap) return propGitStatusMap;
    const map: Record<string, 'modified' | 'untracked' | 'staged' | 'deleted' | 'ignored'> = {};
    if (gitStatus) {
      for (const s of gitStatus.staged) map[s.path] = 'staged';
      for (const u of gitStatus.unstaged) map[u.path] = 'modified';
      for (const t of gitStatus.untracked) map[t.path] = 'untracked';
      for (const d of gitStatus.deleted) map[d.path] = 'deleted';
      for (const i of gitStatus.ignored) map[i] = 'ignored';
    }
    return map;
  }, [propGitStatusMap, gitStatus]);

  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(() => {
    return new Set(['apps', 'crates', 'src', 'tests', 'docs']);
  });

  const { tree, folderCount } = useMemo(
    () => buildTreeFromFiles(files, activeGitStatusMap),
    [files, activeGitStatusMap]
  );

  // Collect all folder paths for "Expand All"
  const allFolderPaths = useMemo(() => {
    const paths = new Set<string>();
    const walk = (nodes: TreeNode[]) => {
      for (const node of nodes) {
        if (node.isFolder) {
          paths.add(node.id);
          if (node.children) walk(node.children);
        }
      }
    };
    walk(tree);
    return paths;
  }, [tree]);

  // Filter tree and auto-expand matching ancestor folders
  const { filteredTree, autoExpandedMatching } = useMemo(() => {
    if (!searchQuery.trim()) {
      return { filteredTree: tree, autoExpandedMatching: new Set<string>() };
    }

    const q = searchQuery.toLowerCase().trim();
    const expanded = new Set<string>();

    function filterNodes(nodes: TreeNode[], parentPaths: string[]): TreeNode[] {
      const result: TreeNode[] = [];

      for (const node of nodes) {
        if (node.isFolder) {
          const currentParents = [...parentPaths, node.id];
          const filteredChildren = filterNodes(node.children || [], currentParents);
          const folderMatches = node.name.toLowerCase().includes(q);

          if (folderMatches || filteredChildren.length > 0) {
            currentParents.forEach((p) => expanded.add(p));
            result.push({
              ...node,
              children: filteredChildren,
            });
          }
        } else {
          if (node.name.toLowerCase().includes(q) || node.id.toLowerCase().includes(q)) {
            parentPaths.forEach((p) => expanded.add(p));
            result.push(node);
          }
        }
      }

      return result;
    }

    return {
      filteredTree: filterNodes(tree, []),
      autoExpandedMatching: expanded,
    };
  }, [tree, searchQuery]);

  // Combined expanded set
  const effectiveExpanded = useMemo(() => {
    if (searchQuery.trim()) {
      return new Set([...expandedFolders, ...autoExpandedMatching]);
    }
    return expandedFolders;
  }, [expandedFolders, autoExpandedMatching, searchQuery]);

  const toggleFolder = (folderId: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(folderId)) {
        next.delete(folderId);
      } else {
        next.add(folderId);
      }
      return next;
    });
  };

  const handleExpandAll = () => {
    setExpandedFolders(new Set(allFolderPaths));
  };

  const handleCollapseAll = () => {
    setExpandedFolders(new Set());
  };

  // Render file icon based on extension
  const renderFileIcon = (ext?: string) => {
    switch (ext) {
      case 'ts':
      case 'tsx':
      case 'js':
      case 'jsx':
      case 'rs':
      case 'py':
      case 'go':
      case 'java':
      case 'cs':
        return <FileCode className="w-3.5 h-3.5 text-sky-500 shrink-0" />;
      case 'md':
      case 'txt':
      case 'json':
      case 'toml':
      case 'yaml':
      case 'yml':
        return <FileText className="w-3.5 h-3.5 text-amber-500 shrink-0" />;
      default:
        return <File className="w-3.5 h-3.5 text-slate-400 shrink-0" />;
    }
  };

  // Recursive tree node renderer
  const renderTreeNode = (node: TreeNode, depth: number = 0) => {
    if (node.isFolder) {
      const isExpanded = effectiveExpanded.has(node.id);

      return (
        <div key={node.id} className="select-none">
          {/* Folder Row */}
          <div
            onClick={() => toggleFolder(node.id)}
            style={{ paddingLeft: `${depth * 12 + 6}px` }}
            className="flex items-center justify-between py-1 px-1.5 rounded-lg hover:bg-[var(--card-elevated)] cursor-pointer text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors group"
          >
            <div className="flex items-center space-x-1.5 truncate">
              {isExpanded ? (
                <ChevronDown className="w-3.5 h-3.5 text-[var(--text-muted)] group-hover:text-[var(--text-primary)] shrink-0 transition-transform" />
              ) : (
                <ChevronRight className="w-3.5 h-3.5 text-[var(--text-muted)] group-hover:text-[var(--text-primary)] shrink-0 transition-transform" />
              )}
              {isExpanded ? (
                <FolderOpen className="w-3.5 h-3.5 text-amber-500 dark:text-amber-400 shrink-0" />
              ) : (
                <Folder className="w-3.5 h-3.5 text-amber-500 dark:text-amber-400 shrink-0" />
              )}
              <span className={`truncate text-xs font-semibold ${
                node.gitStatus === 'modified' ? 'text-amber-500' :
                node.gitStatus === 'untracked' ? 'text-emerald-500' :
                node.gitStatus === 'staged' ? 'text-emerald-400' : ''
              }`}>
                {node.name}
              </span>
            </div>

            <div className="flex items-center space-x-1 shrink-0 ml-1">
              {node.gitStatus && (
                <span className={`text-[9px] font-mono px-1 py-0.2 rounded font-bold ${
                  node.gitStatus === 'modified'
                    ? 'bg-amber-500/20 text-amber-500 border border-amber-500/30'
                    : node.gitStatus === 'untracked'
                    ? 'bg-emerald-500/20 text-emerald-500 border border-emerald-500/30'
                    : 'bg-emerald-500/30 text-emerald-400 border border-emerald-500/40'
                }`}>
                  {node.gitStatus === 'modified' ? '● M' : node.gitStatus === 'untracked' ? '● U' : '● S'}
                </span>
              )}
              {node.fileCount !== undefined && node.fileCount > 0 && (
                <span className="text-[9px] font-mono text-[var(--text-muted)] px-1 py-0.2 rounded bg-[var(--card-elevated)] border border-[var(--border-subtle)]">
                  {node.fileCount}
                </span>
              )}
            </div>
          </div>

          {/* Folder Children */}
          {isExpanded && node.children && (
            <div className="border-l border-[var(--border-subtle)]/60 ml-[13px] my-0.5">
              {node.children.map((child) => renderTreeNode(child, depth + 1))}
            </div>
          )}
        </div>
      );
    }

    // File Row
    const isSelected = selectedFilePath === node.id;

    return (
      <div
        key={node.id}
        onClick={() => onSelectFile(node.id)}
        style={{ paddingLeft: `${depth * 12 + 18}px` }}
        className={`flex items-center justify-between py-1 px-1.5 rounded-lg cursor-pointer transition-all group ${
          isSelected
            ? 'bg-sky-100/90 dark:bg-cyan-500/15 border border-sky-300 dark:border-cyan-500/40 text-sky-800 dark:text-cyan-300 font-semibold shadow-xs'
            : 'hover:bg-[var(--card-elevated)] border border-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
        } ${node.gitStatus === 'ignored' ? 'opacity-40 italic' : ''}`}
        title={`${node.id} (${node.gitStatus ? `Git: ${node.gitStatus}` : node.status || 'verified'})`}
      >
        <div className="flex items-center space-x-1.5 truncate">
          <span
            className={`w-1.5 h-1.5 rounded-full shrink-0 ${
              node.gitStatus === 'modified'
                ? 'bg-amber-500'
                : node.gitStatus === 'untracked'
                ? 'bg-emerald-500'
                : node.gitStatus === 'staged'
                ? 'bg-emerald-400 ring-2 ring-emerald-500/30'
                : node.gitStatus === 'deleted'
                ? 'bg-rose-500'
                : node.status === 'impacted'
                ? 'bg-rose-500 animate-pulse'
                : 'bg-emerald-500'
            }`}
          />
          {renderFileIcon(node.ext)}
          <span
            className={`truncate text-xs ${
              node.gitStatus === 'modified'
                ? 'text-amber-500 font-semibold'
                : node.gitStatus === 'untracked'
                ? 'text-emerald-500 font-semibold'
                : node.gitStatus === 'staged'
                ? 'text-emerald-400 font-semibold'
                : node.gitStatus === 'deleted'
                ? 'text-rose-500 line-through opacity-70'
                : node.gitStatus === 'ignored'
                ? 'text-[var(--text-muted)]'
                : ''
            }`}
          >
            {node.name}
          </span>
        </div>

        <div className="flex items-center space-x-1 shrink-0 ml-1">
          {/* VS Code-style Git Status Badge */}
          {node.gitStatus === 'modified' && (
            <span
              className="text-[9px] font-mono font-bold px-1 py-0.2 rounded bg-amber-500/15 text-amber-500 border border-amber-500/30"
              title="Modified in working tree"
            >
              M
            </span>
          )}
          {node.gitStatus === 'untracked' && (
            <span
              className="text-[9px] font-mono font-bold px-1 py-0.2 rounded bg-emerald-500/15 text-emerald-500 border border-emerald-500/30"
              title="Untracked new file"
            >
              U
            </span>
          )}
          {node.gitStatus === 'staged' && (
            <span
              className="text-[9px] font-mono font-bold px-1 py-0.2 rounded bg-emerald-500/25 text-emerald-400 border border-emerald-500/40"
              title="Staged for verified commit"
            >
              A
            </span>
          )}
          {node.gitStatus === 'deleted' && (
            <span
              className="text-[9px] font-mono font-bold px-1 py-0.2 rounded bg-rose-500/15 text-rose-500 border border-rose-500/30"
              title="Deleted file"
            >
              D
            </span>
          )}
          {node.gitStatus === 'ignored' && (
            <span
              className="text-[8px] font-mono px-1 py-0.2 rounded bg-[var(--card-elevated)] text-[var(--text-muted)] border border-[var(--border-subtle)]"
              title="Ignored by .gitignore"
            >
              IGN
            </span>
          )}

          <span
            className={`text-[9px] px-1 py-0.2 rounded font-bold uppercase ${
              node.ext === 'ts' || node.ext === 'tsx'
                ? 'bg-sky-500/15 text-sky-600 dark:text-sky-400'
                : node.ext === 'rs'
                ? 'bg-orange-500/15 text-orange-600 dark:text-orange-400'
                : node.ext === 'py'
                ? 'bg-blue-500/15 text-blue-600 dark:text-blue-400'
                : node.ext === 'json' || node.ext === 'toml'
                ? 'bg-amber-500/15 text-amber-600 dark:text-amber-400'
                : 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
            }`}
          >
            {node.ext || 'FILE'}
          </span>
        </div>
      </div>
    );
  };

  return (
    <div className="flex flex-col h-full overflow-hidden font-mono text-xs select-none">
      {/* 1. Header Toolbar */}
      <div className="flex items-center justify-between pb-2.5 border-b border-[var(--border-subtle)] mb-2 shrink-0">
        <div className="flex items-center space-x-1">
          <span className="text-xs uppercase font-mono tracking-wider text-[var(--text-muted)] font-bold">
            Explorer
          </span>
          <button
            onClick={onRefresh}
            title="Rescan repository in real-time"
            className={`p-1 rounded-lg hover:bg-[var(--card-elevated)] text-[var(--text-muted)] hover:text-cyan-500 transition-colors cursor-pointer ${
              isRefreshing ? 'animate-spin text-cyan-500' : ''
            }`}
          >
            <RotateCcw className="w-3.5 h-3.5" />
          </button>
        </div>

        {/* Action Controls & Count Badge */}
        <div className="flex items-center space-x-1.5">
          <button
            onClick={handleExpandAll}
            title="Expand All Folders"
            className="p-1 rounded-lg hover:bg-[var(--card-elevated)] text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          >
            <ChevronsUpDown className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={handleCollapseAll}
            title="Collapse All Folders"
            className="p-1 rounded-lg hover:bg-[var(--card-elevated)] text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors cursor-pointer"
          >
            <ChevronsDownUp className="w-3.5 h-3.5" />
          </button>
          <span
            className="text-[10px] font-mono px-2 py-0.5 rounded-lg bg-sky-50 dark:bg-cyan-500/10 text-sky-700 dark:text-cyan-400 font-semibold border border-sky-200 dark:border-cyan-500/20 shrink-0"
            title={`${files.length} Files in ${folderCount} Folders`}
          >
            {files.length} Files
          </span>
        </div>
      </div>

      {/* 2. Instant Search Filter */}
      <div className="relative mb-2 shrink-0">
        <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Filter repo files & folders..."
          className="w-full pl-8 pr-7 py-1.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-xs font-mono text-[var(--text-primary)] placeholder:text-[var(--text-muted)] outline-none focus:border-cyan-500/50 transition-colors"
        />
        {searchQuery && (
          <button
            onClick={() => setSearchQuery('')}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
          >
            <X className="w-3 h-3" />
          </button>
        )}
      </div>

      {/* 3. Real-Time Hierarchical Folder & File Tree */}
      <div className="flex-1 overflow-y-auto pr-1 space-y-0.5">
        <div className="flex items-center justify-between text-[10px] font-bold text-[var(--text-muted)] uppercase tracking-wider mb-1 px-1">
          <span>Repository Tree</span>
          <span className="text-[9px] text-emerald-600 dark:text-emerald-400 font-semibold">
            &bull; {folderCount} folders
          </span>
        </div>

        {filteredTree.length === 0 ? (
          <div className="p-4 text-center text-xs text-[var(--text-muted)] italic">
            {searchQuery ? 'No matching files or folders found' : 'Scanning repository structure...'}
          </div>
        ) : (
          filteredTree.map((node) => renderTreeNode(node, 0))
        )}
      </div>
    </div>
  );
}
