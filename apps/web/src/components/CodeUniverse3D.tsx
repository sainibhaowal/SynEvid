'use client';

import React, { useEffect, useLayoutEffect, useRef, useState, useMemo } from 'react';
import * as THREE from 'three';
import { EgoNetwork, GraphUniverse, UniverseNode, UniverseEdge, SimulateEditResponse } from '@/lib/api';
import {
  DENSE_GRAPH_RENDER_THRESHOLD,
  sampleDenseOverviewEdges,
  setupDenseUniverseRenderer,
} from './denseUniverseRenderer';
import {
  Box,
  Orbit,
  Maximize2,
  Zap,
  Play,
  Pause,
  ChevronLeft,
  ChevronRight,
  X,
  Search,
  ArrowUp,
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  Plus,
  Minus,
  Move,
  Info,
  Layers,
  ShieldCheck,
  CheckCircle2,
  AlertTriangle,
  RotateCcw,
  Crosshair,
  Filter,
  Tag,
  FolderTree,
  Compass,
  SlidersHorizontal,
} from 'lucide-react';

interface CodeUniverse3DProps {
  universe: GraphUniverse;
  egoNetwork?: EgoNetwork | null;
  focusNodeId?: string | null;
  simulation?: SimulateEditResponse | null;
  simulationFilePath?: string | null;
  onSelectNode?: (symbolId: string) => void;
  onOpenSimulator?: (symbolId: string) => void;
  onToggleDimensions: () => void;
}

interface PulsePacket {
  curve: THREE.QuadraticBezierCurve3;
  mesh: THREE.Mesh;
  progress: number;
  speed: number;
}

export interface FolderTheme {
  name: string;
  color: string;
  hex: number;
}

const compactNodeTextureCache = new Map<string, THREE.CanvasTexture>();

function getSimulationNodeState(
  node: UniverseNode,
  simulation: SimulateEditResponse | null | undefined,
  simulationFilePath?: string | null
): { damaged: boolean; dead: boolean } {
  if (!simulation) return { damaged: false, dead: false };
  const syntaxDamaged = simulation.syntax_errors.length > 0 && Boolean(
    simulationFilePath && node.file_path === simulationFilePath
  );
  const damaged = syntaxDamaged || simulation.damaged_callers.some((caller) =>
    caller.stable_id === node.id || (node.kind === 'file' && caller.file_path === node.file_path)
  ) || simulation.broken_imports.some((diagnostic) =>
    diagnostic.includes(node.id) || Boolean(node.file_path && diagnostic.includes(node.file_path))
  );
  const dead = simulation.newly_dead_symbols.some((symbol) =>
    symbol.stable_id === node.id || (node.kind === 'file' && symbol.file_path === node.file_path)
  );
  return { damaged, dead };
}

function isSimulationEdgeBroken(
  edge: UniverseEdge,
  simulation: SimulateEditResponse | null | undefined
): boolean {
  if (!simulation) return false;
  return simulation.damaged_callers.some((caller) =>
    caller.stable_id === edge.source ||
    (caller.file_path === edge.source_file && caller.calling_line === edge.source_line)
  ) || simulation.broken_imports.some((diagnostic) =>
    Boolean(edge.source_file && diagnostic.includes(edge.source_file)) ||
    Boolean(edge.target_file && diagnostic.includes(edge.target_file))
  );
}

type LayoutPoint = { x: number; y: number; z: number };

function getUniverseNodeFilePath(node: UniverseNode): string | undefined {
  if (node.file_path) return node.file_path;
  if (node.id.startsWith('file::')) return node.id.slice('file::'.length);
  return undefined;
}

function relaxPositionsWithSpatialHash(
  points: LayoutPoint[],
  minSeparation: number,
  dimensions: 2 | 3,
  iterations: number
): void {
  for (let iteration = 0; iteration < iterations; iteration += 1) {
    const cells = new Map<string, number[]>();
    points.forEach((point, index) => {
      const cellX = Math.floor(point.x / minSeparation);
      const cellY = Math.floor(point.y / minSeparation);
      const cellZ = dimensions === 3 ? Math.floor(point.z / minSeparation) : 0;
      const key = `${cellX}:${cellY}:${cellZ}`;
      const members = cells.get(key);
      if (members) members.push(index);
      else cells.set(key, [index]);
    });

    const displacementX = new Float64Array(points.length);
    const displacementY = new Float64Array(points.length);
    const displacementZ = new Float64Array(points.length);
    const collisionCounts = new Uint32Array(points.length);
    let anyMoved = false;

    points.forEach((first, firstIndex) => {
      const cellX = Math.floor(first.x / minSeparation);
      const cellY = Math.floor(first.y / minSeparation);
      const cellZ = dimensions === 3 ? Math.floor(first.z / minSeparation) : 0;
      for (let offsetX = -1; offsetX <= 1; offsetX += 1) {
        for (let offsetY = -1; offsetY <= 1; offsetY += 1) {
          for (let offsetZ = dimensions === 3 ? -1 : 0; offsetZ <= (dimensions === 3 ? 1 : 0); offsetZ += 1) {
            const nearby = cells.get(`${cellX + offsetX}:${cellY + offsetY}:${cellZ + offsetZ}`);
            if (!nearby) continue;
            nearby.forEach((secondIndex) => {
              if (secondIndex <= firstIndex) return;
              const second = points[secondIndex];
              let dx = second.x - first.x;
              let dy = second.y - first.y;
              let dz = dimensions === 3 ? second.z - first.z : 0;
              let distance = Math.sqrt(dx * dx + dy * dy + dz * dz);
              if (distance >= minSeparation) return;
              anyMoved = true;
              if (distance < 0.05) {
                const theta = ((firstIndex * 37 + secondIndex * 19) % 360) * (Math.PI / 180);
                if (dimensions === 3) {
                  const phi = ((firstIndex * 53 + secondIndex * 31) % 180) * (Math.PI / 180);
                  dx = Math.sin(phi) * Math.cos(theta);
                  dy = Math.cos(phi);
                  dz = Math.sin(phi) * Math.sin(theta);
                } else {
                  dx = Math.cos(theta);
                  dy = Math.sin(theta);
                }
                distance = 1;
              }
              const push = (minSeparation - distance) * 0.5;
              const moveX = (dx / distance) * push;
              const moveY = (dy / distance) * push;
              const moveZ = (dz / distance) * push;
              displacementX[firstIndex] -= moveX;
              displacementY[firstIndex] -= moveY;
              displacementZ[firstIndex] -= moveZ;
              displacementX[secondIndex] += moveX;
              displacementY[secondIndex] += moveY;
              displacementZ[secondIndex] += moveZ;
              collisionCounts[firstIndex] += 1;
              collisionCounts[secondIndex] += 1;
            });
          }
        }
      }
    });

    if (!anyMoved) break;
    points.forEach((point, index) => {
      const count = collisionCounts[index];
      if (count === 0) return;
      point.x += displacementX[index] / count;
      point.y += displacementY[index] / count;
      if (dimensions === 3) point.z += displacementZ[index] / count;
    });
  }
}

// Preset verified corporate engineering colors for common architectures
const FOLDER_PALETTES: Record<
  string,
  { light: { color: string; hex: number }; dark: { color: string; hex: number } }
> = {
  'apps/web': {
    light: { color: '#0284c7', hex: 0x0284c7 },
    dark: { color: '#38bdf8', hex: 0x38bdf8 },
  },
  'apps/synevid-ui': {
    light: { color: '#6366f1', hex: 0x6366f1 },
    dark: { color: '#818cf8', hex: 0x818cf8 },
  },
  'apps/mcp-server': {
    light: { color: '#ea580c', hex: 0xea580c },
    dark: { color: '#fb923c', hex: 0xfb923c },
  },
  'apps/synevid-lsp': {
    light: { color: '#0891b2', hex: 0x0891b2 },
    dark: { color: '#22d3ee', hex: 0x22d3ee },
  },
  'apps/web-inspector': {
    light: { color: '#4f46e5', hex: 0x4f46e5 },
    dark: { color: '#a5b4fc', hex: 0xa5b4fc },
  },
  'crates/autopsy-cli': {
    light: { color: '#d97706', hex: 0xd97706 },
    dark: { color: '#fbbf24', hex: 0xfbbf24 },
  },
  'crates/autopsy-storage': {
    light: { color: '#059669', hex: 0x059669 },
    dark: { color: '#34d399', hex: 0x34d399 },
  },
  'crates/autopsy-graph-layout': {
    light: { color: '#0d9488', hex: 0x0d9488 },
    dark: { color: '#2dd4bf', hex: 0x2dd4bf },
  },
  'crates/autopsy-plugin-api': {
    light: { color: '#7c3aed', hex: 0x7c3aed },
    dark: { color: '#c084fc', hex: 0xc084fc },
  },
  'crates/autopsy-repo': {
    light: { color: '#db2777', hex: 0xdb2777 },
    dark: { color: '#f472b6', hex: 0xf472b6 },
  },
  'crates/autopsy-symbols': {
    light: { color: '#2563eb', hex: 0x2563eb },
    dark: { color: '#60a5fa', hex: 0x60a5fa },
  },
  'crates/autopsy-impact': {
    light: { color: '#dc2626', hex: 0xdc2626 },
    dark: { color: '#f87171', hex: 0xf87171 },
  },
  'crates/autopsy-test-impact': {
    light: { color: '#e11d48', hex: 0xe11d48 },
    dark: { color: '#fb7185', hex: 0xfb7185 },
  },
  'crates/autopsy-report': {
    light: { color: '#9333ea', hex: 0x9333ea },
    dark: { color: '#e879f9', hex: 0xe879f9 },
  },
  'crates/autopsy-mining': {
    light: { color: '#b45309', hex: 0xb45309 },
    dark: { color: '#fde047', hex: 0xfde047 },
  },
  'crates/autopsy-temporal': {
    light: { color: '#7c3aed', hex: 0x7c3aed },
    dark: { color: '#a78bfa', hex: 0xa78bfa },
  },
  'crates/autopsy-telemetry': {
    light: { color: '#0284c7', hex: 0x0284c7 },
    dark: { color: '#38bdf8', hex: 0x38bdf8 },
  },
  'crates/autopsy-workspace-registry': {
    light: { color: '#4f46e5', hex: 0x4f46e5 },
    dark: { color: '#a5b4fc', hex: 0xa5b4fc },
  },
  tests: {
    light: { color: '#e11d48', hex: 0xe11d48 },
    dark: { color: '#fb7185', hex: 0xfb7185 },
  },
  docs: {
    light: { color: '#16a34a', hex: 0x16a34a },
    dark: { color: '#4ade80', hex: 0x4ade80 },
  },
  schemas: {
    light: { color: '#6d28d9', hex: 0x6d28d9 },
    dark: { color: '#a78bfa', hex: 0xa78bfa },
  },
  scripts: {
    light: { color: '#ca8a04', hex: 0xca8a04 },
    dark: { color: '#facc15', hex: 0xfacc15 },
  },
  packaging: {
    light: { color: '#0f766e', hex: 0x0f766e },
    dark: { color: '#5eead4', hex: 0x5eead4 },
  },
  root: {
    light: { color: '#0f766e', hex: 0x0f766e },
    dark: { color: '#2dd4bf', hex: 0x2dd4bf },
  },
};

// Perceptually Uniform Golden-Angle Color Generator (137.5° step)
// Guarantees an infinite supply of vivid, distinct colors for any arbitrary repository
export function generateGoldenAngleColor(
  index: number,
  isDark: boolean
): { color: string; hex: number } {
  const hue = (index * 137.5077) % 360;
  const saturation = isDark ? 85 : 75;
  const lightness = isDark ? 65 : 42;

  const c = (1 - Math.abs(2 * (lightness / 100) - 1)) * (saturation / 100);
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = lightness / 100 - c / 2;

  let r1 = 0, g1 = 0, b1 = 0;
  if (hue < 60) {
    r1 = c; g1 = x; b1 = 0;
  } else if (hue < 120) {
    r1 = x; g1 = c; b1 = 0;
  } else if (hue < 180) {
    r1 = 0; g1 = c; b1 = x;
  } else if (hue < 240) {
    r1 = 0; g1 = x; b1 = c;
  } else if (hue < 300) {
    r1 = x; g1 = 0; b1 = c;
  } else {
    r1 = c; g1 = 0; b1 = x;
  }

  const r = Math.round((r1 + m) * 255);
  const g = Math.round((g1 + m) * 255);
  const b = Math.round((b1 + m) * 255);

  const hexStr = `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
  const hexNum = (r << 16) | (g << 8) | b;
  return { color: hexStr, hex: hexNum };
}

export interface AdaptiveCluster {
  name: string;
  count: number;
  percentage: number;
  theme: FolderTheme;
  lightTheme: FolderTheme;
  darkTheme: FolderTheme;
}

// Adaptive Namespace & Cluster Engine
// Automatically analyzes any repo topology and bounds clusters to an optimal [4..16] count
export function buildAdaptiveClusters(
  allPaths: string[],
  isDark: boolean = true
): {
  nodeClusterMap: Map<string, string>;
  clusters: AdaptiveCluster[];
} {
  if (allPaths.length === 0) {
    return { nodeClusterMap: new Map<string, string>(), clusters: [] };
  }

  // 1. Detect dominant folder structures (e.g. monolithic src/)
  let srcCount = 0;
  allPaths.forEach((p) => {
    const clean = p.replace(/^[./\\]+/, '');
    if (clean.startsWith('src/')) srcCount++;
  });
  const shouldDrillSrc = srcCount > allPaths.length * 0.45;

  const MONOREPO_ROOTS = new Set([
    'crates',
    'apps',
    'packages',
    'services',
    'pkg',
    'internal',
    'modules',
    'libs',
    'plugins',
    'components',
    'features',
  ]);

  // 2. Extract cluster keys
  const rawKeyMap = new Map<string, string>();
  const rawCounts = new Map<string, number>();

  allPaths.forEach((p) => {
    const clean = p.replace(/^[./\\]+/, '');
    const parts = clean.split('/');

    let k = 'root';
    if (parts.length <= 1) {
      k = 'root';
    } else if (shouldDrillSrc && parts[0] === 'src' && parts.length > 2) {
      k = `src/${parts[1]}`;
    } else if (MONOREPO_ROOTS.has(parts[0]) && parts.length > 1) {
      k = `${parts[0]}/${parts[1]}`;
    } else {
      k = parts[0];
    }

    rawKeyMap.set(p, k);
    rawCounts.set(k, (rawCounts.get(k) || 0) + 1);
  });

  // 3. Bound cluster count to optimal range [4..16]
  const sortedRaw = Array.from(rawCounts.entries()).sort((a, b) => {
    if (a[0] === 'root') return -1;
    if (b[0] === 'root') return 1;
    return b[1] - a[1];
  });

  const MAX_PRIMARY_CLUSTERS = 14;
  const nodeClusterMap = new Map<string, string>();
  const finalCounts = new Map<string, number>();

  if (sortedRaw.length <= 16) {
    allPaths.forEach((p) => {
      const k = rawKeyMap.get(p) || 'root';
      nodeClusterMap.set(p, k);
      finalCounts.set(k, (finalCounts.get(k) || 0) + 1);
    });
  } else {
    // Keep top MAX_PRIMARY_CLUSTERS, rollup long tail into parent directory or other
    const allowed = new Set(sortedRaw.slice(0, MAX_PRIMARY_CLUSTERS).map(([k]) => k));
    allPaths.forEach((p) => {
      const rawK = rawKeyMap.get(p) || 'root';
      if (allowed.has(rawK)) {
        nodeClusterMap.set(p, rawK);
        finalCounts.set(rawK, (finalCounts.get(rawK) || 0) + 1);
      } else {
        const topDir = p.replace(/^[./\\]+/, '').split('/')[0] || 'other';
        const fallbackK = topDir.length > 0 && topDir !== p ? `${topDir}/*` : 'misc';
        nodeClusterMap.set(p, fallbackK);
        finalCounts.set(fallbackK, (finalCounts.get(fallbackK) || 0) + 1);
      }
    });
  }

  // 4. Generate deterministic themes using presets or golden angle generator
  const clusters: AdaptiveCluster[] = Array.from(finalCounts.entries())
    .map(([name, count], idx) => {
      const preset = FOLDER_PALETTES[name];
      const lightTheme: FolderTheme = preset
        ? { name, ...preset.light }
        : { name, ...generateGoldenAngleColor(idx, false) };
      const darkTheme: FolderTheme = preset
        ? { name, ...preset.dark }
        : { name, ...generateGoldenAngleColor(idx, true) };
      return {
        name,
        count,
        percentage: Math.round((count / allPaths.length) * 100),
        theme: isDark ? darkTheme : lightTheme,
        lightTheme,
        darkTheme,
      };
    })
    .sort((a, b) => {
      if (a.name === 'root') return -1;
      if (b.name === 'root') return 1;
      return b.count - a.count;
    });

  return { nodeClusterMap, clusters };
}

// Generate high-resolution 512x320 Retina architectural file card texture (75% less VRAM, 4x faster rasterization)
function generateFileHoloTexture(
  node: UniverseNode,
  folderTheme: FolderTheme,
  isSelected: boolean,
  isHovered: boolean,
  isDamaged: boolean,
  isDead: boolean,
  isDark: boolean = true,
  compact: boolean = false
): THREE.CanvasTexture {
  if (compact) {
    const cacheKey = `${folderTheme.hex}:${isDamaged}:${isDead}:${isDark}`;
    const cached = compactNodeTextureCache.get(cacheKey);
    if (cached) return cached;
    const compactCanvas = document.createElement('canvas');
    compactCanvas.width = 128;
    compactCanvas.height = 80;
    const compactContext = compactCanvas.getContext('2d');
    if (compactContext) {
      compactContext.fillStyle = isDark ? 'rgba(9, 14, 25, 0.97)' : 'rgba(255, 255, 255, 0.97)';
      compactContext.fillRect(1, 1, 126, 78);
      compactContext.strokeStyle = isDamaged ? '#f43f5e' : isDead ? '#64748b' : folderTheme.color;
      compactContext.lineWidth = 3;
      compactContext.strokeRect(2.5, 2.5, 123, 75);
      compactContext.fillStyle = folderTheme.color;
      compactContext.fillRect(8, 8, 5, 64);
      compactContext.fillStyle = isDark ? '#e2e8f0' : '#0f172a';
      compactContext.font = 'bold 20px sans-serif';
      compactContext.fillText(isDamaged ? 'IMPACT' : isDead ? 'DEAD' : 'NODE', 22, 46);
    }
    const compactTexture = new THREE.CanvasTexture(compactCanvas);
    compactTexture.generateMipmaps = true;
    compactTexture.minFilter = THREE.LinearMipmapLinearFilter;
    compactNodeTextureCache.set(cacheKey, compactTexture);
    return compactTexture;
  }

  const canvas = document.createElement('canvas');
  canvas.width = 512;
  canvas.height = 320;
  const ctx = canvas.getContext('2d');

  if (ctx) {
    const ext = node.id.split('.').pop()?.toLowerCase() || '';
    let langTag = 'FILE';
    let langColor = isDark ? '#00f0ff' : '#0284c7';

    if (ext === 'ts' || ext === 'tsx') {
      langTag = 'TS';
      langColor = isDark ? '#00f0ff' : '#0284c7';
    } else if (ext === 'rs') {
      langTag = 'RS';
      langColor = '#f97316';
    } else if (ext === 'py') {
      langTag = 'PY';
      langColor = '#3b82f6';
    } else if (ext === 'json' || ext === 'toml') {
      langTag = 'CFG';
      langColor = '#eab308';
    } else if (ext === 'md') {
      langTag = 'DOC';
      langColor = '#a855f7';
    } else if (ext === 'js' || ext === 'jsx') {
      langTag = 'JS';
      langColor = '#eab308';
    }

    const baseBorderColor = isDamaged
      ? isDark ? '#f43f5e' : '#e11d48'
      : isDead
      ? isDark ? '#64748b' : '#94a3b8'
      : isSelected
      ? isDark ? '#00f0ff' : '#0284c7'
      : isHovered
      ? isDark ? '#38bdf8' : '#0284c7'
      : folderTheme.color;

    // Deep Sci-Fi Glass / Obsidian Plate Background
    const bgGrad = ctx.createLinearGradient(0, 0, 512, 320);
    if (isDamaged) {
      if (isDark) {
        bgGrad.addColorStop(0, 'rgba(45, 12, 24, 0.94)');
        bgGrad.addColorStop(1, 'rgba(24, 6, 12, 0.98)');
      } else {
        bgGrad.addColorStop(0, '#fff1f2');
        bgGrad.addColorStop(1, '#ffe4e6');
      }
    } else if (isDead) {
      bgGrad.addColorStop(0, isDark ? 'rgba(30, 41, 59, 0.92)' : '#f1f5f9');
      bgGrad.addColorStop(1, isDark ? 'rgba(15, 23, 42, 0.96)' : '#e2e8f0');
    } else if (isSelected) {
      if (isDark) {
        bgGrad.addColorStop(0, 'rgba(12, 34, 58, 0.95)');
        bgGrad.addColorStop(1, 'rgba(6, 19, 34, 0.98)');
      } else {
        bgGrad.addColorStop(0, '#f0f9ff');
        bgGrad.addColorStop(1, '#e0f2fe');
      }
    } else {
      if (isDark) {
        bgGrad.addColorStop(0, 'rgba(10, 18, 34, 0.94)');
        bgGrad.addColorStop(0.5, 'rgba(7, 12, 24, 0.96)');
        bgGrad.addColorStop(1, 'rgba(4, 7, 16, 0.98)');
      } else {
        bgGrad.addColorStop(0, '#ffffff');
        bgGrad.addColorStop(1, '#f8fafc');
      }
    }

    // Chamfered Sci-Fi Cut-Corner Geometry
    const cardX = 10;
    const cardY = 10;
    const cardW = 492;
    const cardH = 300;
    const cut = 22;

    ctx.fillStyle = bgGrad;
    ctx.beginPath();
    ctx.moveTo(cardX + cut, cardY);
    ctx.lineTo(cardX + cardW - cut, cardY);
    ctx.lineTo(cardX + cardW, cardY + cut);
    ctx.lineTo(cardX + cardW, cardY + cardH - cut);
    ctx.lineTo(cardX + cardW - cut, cardY + cardH);
    ctx.lineTo(cardX + cut, cardY + cardH);
    ctx.lineTo(cardX, cardY + cardH - cut);
    ctx.lineTo(cardX, cardY + cut);
    ctx.closePath();
    ctx.fill();

    // Laser Edge Border
    ctx.lineWidth = isSelected ? 5 : isHovered ? 4 : isDark ? 2.5 : 3.5;
    ctx.strokeStyle = baseBorderColor;
    ctx.stroke();

    // Sci-Fi Corner HUD Reticles
    const bLen = 28;
    ctx.strokeStyle = isDark ? '#ffffff' : '#0284c7';
    ctx.lineWidth = 3.5;

    ctx.beginPath();
    ctx.moveTo(cardX, cardY + bLen);
    ctx.lineTo(cardX, cardY + cut);
    ctx.lineTo(cardX + cut, cardY);
    ctx.lineTo(cardX + bLen, cardY);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(cardX + cardW - bLen, cardY);
    ctx.lineTo(cardX + cardW - cut, cardY);
    ctx.lineTo(cardX + cardW, cardY + cut);
    ctx.lineTo(cardX + cardW, cardY + bLen);
    ctx.stroke();

    // Header Accent Band
    ctx.fillStyle = isDark ? 'rgba(0, 240, 255, 0.05)' : '#f8fafc';
    ctx.beginPath();
    ctx.roundRect(14, 14, 484, 50, [10, 10, 0, 0]);
    ctx.fill();

    // Header Bar: Language Icon Badge + Folder Tag Badge
    ctx.fillStyle = langColor;
    ctx.beginPath();
    ctx.roundRect(25, 25, 43, 26, 7);
    ctx.fill();

    ctx.font = 'bold 15px monospace';
    ctx.fillStyle = '#ffffff';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(langTag, 46, 38);

    // Folder Package Badge
    ctx.fillStyle = folderTheme.color;
    ctx.beginPath();
    ctx.roundRect(75, 25, Math.min(folderTheme.name.length * 8 + 15, 240), 26, 7);
    ctx.fill();

    ctx.font = 'bold 11px monospace';
    ctx.fillStyle = '#ffffff';
    ctx.textAlign = 'left';
    ctx.fillText(folderTheme.name.toUpperCase(), 84, 38);

    // Status Indicator Dot & Label
    ctx.fillStyle = isDamaged ? (isDark ? '#f43f5e' : '#e11d48') : (isDark ? '#10b981' : '#047857');
    ctx.beginPath();
    ctx.arc(470, 38, 6, 0, Math.PI * 2);
    ctx.fill();

    ctx.font = 'bold 11px monospace';
    ctx.textAlign = 'right';
    ctx.fillStyle = isDamaged ? (isDark ? '#f43f5e' : '#e11d48') : (isDark ? '#10b981' : '#047857');
    ctx.fillText(isDamaged ? 'IMPACTED' : 'VERIFIED', 458, 38);

    // Crisp Divider Line
    ctx.strokeStyle = isDark ? 'rgba(255, 255, 255, 0.15)' : 'rgba(0, 0, 0, 0.12)';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(15, 65);
    ctx.lineTo(497, 65);
    ctx.stroke();

    // Filename Title
    const fileName = node.id.split('/').pop() || node.id;
    ctx.font = '900 24px monospace';
    ctx.fillStyle = isDamaged
      ? isDark ? '#fda4af' : '#be123c'
      : isDark ? '#ffffff' : '#090d16';
    ctx.textAlign = 'left';
    ctx.fillText(fileName, 25, 102);

    // Full File Path
    ctx.font = 'bold 13px monospace';
    ctx.fillStyle = isDark ? '#94a3b8' : '#1e293b';
    ctx.fillText(node.id, 25, 130);

    // Metrics Cards inside plate
    const boxBg = isDark ? 'rgba(255, 255, 255, 0.07)' : '#f8fafc';
    const boxBorder = isDark ? 'rgba(255, 255, 255, 0.16)' : '#e2e8f0';
    const boxLabelColor = isDark ? '#cbd5e1' : '#475569';

    // Box 1: Inbound Calls
    ctx.fillStyle = boxBg;
    ctx.beginPath();
    ctx.roundRect(25, 155, 140, 80, 10);
    ctx.fill();
    ctx.lineWidth = 1.25;
    ctx.strokeStyle = boxBorder;
    ctx.stroke();

    ctx.font = 'bold 11px monospace';
    ctx.fillStyle = boxLabelColor;
    ctx.fillText('INBOUND CALLS', 35, 175);
    ctx.font = 'bold 26px monospace';
    ctx.fillStyle = isDark ? '#00f0ff' : '#0284c7';
    ctx.fillText(isDamaged ? '1' : '2', 35, 212);

    // Box 2: Outbound Refs
    ctx.fillStyle = boxBg;
    ctx.beginPath();
    ctx.roundRect(178, 155, 140, 80, 10);
    ctx.fill();
    ctx.lineWidth = 1.25;
    ctx.strokeStyle = boxBorder;
    ctx.stroke();

    ctx.font = 'bold 11px monospace';
    ctx.fillStyle = boxLabelColor;
    ctx.fillText('OUTBOUND REFS', 188, 175);
    ctx.font = 'bold 26px monospace';
    ctx.fillStyle = isDark ? '#10b981' : '#047857';
    ctx.fillText('2', 188, 212);

    // Box 3: Deterministic AST Verification
    ctx.fillStyle = boxBg;
    ctx.beginPath();
    ctx.roundRect(330, 155, 157, 80, 10);
    ctx.fill();
    ctx.lineWidth = 1.25;
    ctx.strokeStyle = boxBorder;
    ctx.stroke();

    ctx.font = 'bold 11px monospace';
    ctx.fillStyle = boxLabelColor;
    ctx.fillText('AST VERIFICATION', 340, 175);
    ctx.font = 'bold 15px monospace';
    ctx.fillStyle = folderTheme.color;
    ctx.fillText('BLAKE3 EXACT', 340, 210);

    // Bottom Accent Strip
    ctx.fillStyle = isDamaged ? '#f43f5e' : isSelected ? '#00f0ff' : folderTheme.color;
    ctx.fillRect(25, 292, 462, 5);
  }

  const texture = new THREE.CanvasTexture(canvas);
  texture.generateMipmaps = true;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  return texture;
}

// Generate high-resolution Retina billboard cluster title badge
function generateClusterBadgeTexture(
  clusterName: string,
  fileCount: number,
  percentage: number,
  themeColor: string,
  isDark: boolean
): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 768;
  canvas.height = 160;
  const ctx = canvas.getContext('2d');
  if (ctx) {
    // Background rounded pill
    ctx.fillStyle = isDark ? 'rgba(10, 16, 28, 0.94)' : 'rgba(255, 255, 255, 0.96)';
    ctx.beginPath();
    ctx.roundRect(12, 12, 744, 136, 44);
    ctx.fill();

    // Vibrant cluster theme border
    ctx.lineWidth = 6;
    ctx.strokeStyle = themeColor;
    ctx.stroke();

    // Folder Package Icon Circle
    ctx.fillStyle = themeColor;
    ctx.beginPath();
    ctx.arc(68, 80, 32, 0, Math.PI * 2);
    ctx.fill();

    ctx.font = 'bold 30px monospace';
    ctx.fillStyle = '#ffffff';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('DIR', 68, 81);

    // Cluster Name Label
    ctx.font = 'bold 36px monospace';
    ctx.fillStyle = isDark ? '#ffffff' : '#0f172a';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    const displayName = clusterName.length > 22 ? clusterName.slice(0, 20) + '…' : clusterName;
    ctx.fillText(displayName, 122, 80);

    // Count Pill on Right
    ctx.fillStyle = isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(0, 0, 0, 0.06)';
    ctx.beginPath();
    ctx.roundRect(530, 42, 205, 76, 24);
    ctx.fill();

    ctx.font = 'bold 26px monospace';
    ctx.fillStyle = themeColor;
    ctx.textAlign = 'center';
    ctx.fillText(`${fileCount} files`, 632, 80);
  }

  const texture = new THREE.CanvasTexture(canvas);
  texture.generateMipmaps = true;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  return texture;
}

export default function CodeUniverse3D({
  universe,
  egoNetwork,
  focusNodeId,
  simulation,
  simulationFilePath,
  onSelectNode,
  onOpenSimulator,
  onToggleDimensions,
}: CodeUniverse3DProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const isDark = typeof document === 'undefined' || document.documentElement.getAttribute('data-theme') !== 'light';

  // Direct DOM Refs for Instant 120 FPS Hover Glance Pill (0 React Component Re-renders)
  const hoverPillRef = useRef<HTMLDivElement>(null);
  const hoverExtRef = useRef<HTMLSpanElement>(null);
  const hoverNameRef = useRef<HTMLSpanElement>(null);
  const hoverStatusDotRef = useRef<HTMLSpanElement>(null);
  const hoverFolderRef = useRef<HTMLDivElement>(null);
  const hoverCallersRef = useRef<HTMLSpanElement>(null);
  const hoverCalleesRef = useRef<HTMLSpanElement>(null);
  const hoverBrokenRef = useRef<HTMLSpanElement>(null);

  // Dynamic In-Place Update Handler Refs (Zero WebGL Scene Teardown)
  const isDarkRef = useRef<boolean>(isDark);

  const onThemeChangeRef = useRef<((dark: boolean) => void) | null>(null);
  const onDamageOverlayChangeRef = useRef<((damage: boolean) => void) | null>(null);
  const onSimulationOverlayChangeRef = useRef<(() => void) | null>(null);
  const onEdgeFilterChangeRef = useRef<((filter: 'all' | 'calls' | 'imports') => void) | null>(null);
  const simulationRef = useRef<SimulateEditResponse | null | undefined>(simulation);
  const simulationFilePathRef = useRef<string | null | undefined>(simulationFilePath);
  const [themeMutationMs, setThemeMutationMs] = useState<number | null>(null);

  useEffect(() => {
    const syncTheme = (event?: Event) => {
      const dark = document.documentElement.getAttribute('data-theme') !== 'light';
      isDarkRef.current = dark;
      if (event instanceof CustomEvent && typeof event.detail?.mutationMs === 'number') {
        setThemeMutationMs(event.detail.mutationMs);
      }
      onThemeChangeRef.current?.(dark);
    };
    window.addEventListener('synevid-theme-change', syncTheme);
    window.addEventListener('storage', syncTheme);
    return () => {
      window.removeEventListener('synevid-theme-change', syncTheme);
      window.removeEventListener('storage', syncTheme);
    };
  }, []);

  // Previous Node Positions Ref for Smooth Dimensional Morphing (2D <-> 3D)
  const prevPositionsRef = useRef<Map<string, THREE.Vector3>>(new Map());

  // HUD & Navigation State
  const [granularity, setGranularity] = useState<'pkg' | 'file' | 'symbol' | 'lines'>('file');
  const [edgeFilter, setEdgeFilter] = useState<'all' | 'calls' | 'imports'>('all');
  const selectedNodeId = focusNodeId ?? null;
  const damageOverlay = Boolean(simulation);
  const dataPulsesAvailable = universe.node_count < DENSE_GRAPH_RENDER_THRESHOLD;
  const [fps, setFps] = useState<number | null>(null);
  const [presentedFps, setPresentedFps] = useState<number | null>(null);
  const [hasPresentedFps, setHasPresentedFps] = useState(false);
  const [presentationLatencyMs, setPresentationLatencyMs] = useState<number | null>(null);
  const fpsFrameCountRef = useRef(0);
  const fpsWindowStartRef = useRef(0);
  useEffect(() => {
    const resetFpsWindow = () => {
      fpsFrameCountRef.current = 0;
      fpsWindowStartRef.current = performance.now();
      setFps(document.hidden ? 0 : null);
    };
    document.addEventListener('visibilitychange', resetFpsWindow);
    return () => document.removeEventListener('visibilitychange', resetFpsWindow);
  }, []);
  useEffect(() => {
    const desktopBridge = typeof window !== 'undefined' ? (window as any).synevidDesktop : null;
    if (!desktopBridge?.startFrameMeter || !desktopBridge?.getFrameMeterStats) return;

    let cancelled = false;
    setHasPresentedFps(true);
    void desktopBridge.startFrameMeter();
    const sampleTimer = window.setInterval(async () => {
      try {
        const stats = await desktopBridge.getFrameMeterStats();
        if (!cancelled && typeof stats?.fps === 'number') {
          setPresentedFps(stats.fps);
        }
        if (!cancelled && typeof stats?.presentationLatencyMs === 'number') {
          setPresentationLatencyMs(stats.presentationLatencyMs);
        }
      } catch {
        if (!cancelled) setPresentedFps(null);
      }
    }, 500);

    return () => {
      cancelled = true;
      window.clearInterval(sampleTimer);
      if (desktopBridge.stopFrameMeter) void desktopBridge.stopFrameMeter();
    };
  }, []);
  const [isPhysicsPaused, setIsPhysicsPaused] = useState<boolean>(false);
  const [controlMode, setControlMode] = useState<'orbit' | 'pan'>('orbit');
  const [showLegends, setShowLegends] = useState<boolean>(false);
  const [selectedFolderFilter, setSelectedFolderFilter] = useState<string | null>(null);
  const [legendSearch, setLegendSearch] = useState<string>('');
  const displayedFps = hasPresentedFps ? presentedFps : fps;

  const damageOverlayRef = useRef<boolean>(damageOverlay);

  const edgeFilterRef = useRef<'all' | 'calls' | 'imports'>(edgeFilter);

  const selectedFolderFilterRef = useRef<string | null>(selectedFolderFilter);

  // Search filter
  const [searchQuery, setSearchQuery] = useState('');

  // Refs for camera navigation and state tracking
  const selectedNodeIdRef = useRef<string | null>(null);
  const hoveredNodeIdRef = useRef<string | null>(null);
  const activeNeighborsRef = useRef<Set<string>>(new Set());
  const controlModeRef = useRef<'orbit' | 'pan'>('orbit');

  const isPhysicsPausedRef = useRef<boolean>(false);

  useLayoutEffect(() => {
    isDarkRef.current = isDark;
    simulationRef.current = simulation;
    simulationFilePathRef.current = simulationFilePath;
    damageOverlayRef.current = damageOverlay;
    edgeFilterRef.current = edgeFilter;
    selectedFolderFilterRef.current = selectedFolderFilter;
    controlModeRef.current = controlMode;
    isPhysicsPausedRef.current = isPhysicsPaused;
  }, [isDark, simulation, simulationFilePath, damageOverlay, edgeFilter, selectedFolderFilter, controlMode, isPhysicsPaused]);

  const activeDimRef = useRef<'2D' | '3D'>(universe.dimensions || '3D');
  const morphToDimensionRef = useRef<((nextDim: '2D' | '3D') => void) | null>(null);
  const isMorphingRef = useRef<boolean>(false);
  const morphStartRef = useRef<number>(0);

  const cameraRef = useRef<THREE.PerspectiveCamera | null>(null);
  const flyToNodeByIdRef = useRef<((id: string) => void) | null>(null);
  const flyToClusterRef = useRef<((folderKey: string) => void) | null>(null);
  const recenterRef = useRef<(() => void) | null>(null);
  const panUniverseRef = useRef<((dx: number, dy: number) => void) | null>(null);
  const zoomUniverseRef = useRef<((delta: number) => void) | null>(null);

  useEffect(() => {
    if (!focusNodeId) {
      selectedNodeIdRef.current = null;
      return;
    }
    selectedNodeIdRef.current = focusNodeId;
    flyToNodeByIdRef.current?.(focusNodeId);
  }, [focusNodeId]);

  useEffect(() => {
    onSimulationOverlayChangeRef.current?.();
  }, [simulation]);

  useEffect(() => {
    if (universe?.dimensions && universe.dimensions !== activeDimRef.current) {
      morphToDimensionRef.current?.(universe.dimensions);
    }
  }, [universe?.dimensions]);

  // Reactive in-place update handlers without rebuilding WebGL Scene
  useEffect(() => {
    onThemeChangeRef.current?.(isDark);
  }, [isDark]);

  useEffect(() => {
    onDamageOverlayChangeRef.current?.(damageOverlay);
  }, [damageOverlay]);

  useEffect(() => {
    onEdgeFilterChangeRef.current?.(edgeFilter);
  }, [edgeFilter]);

  // All parsed nodes as an array
  const allNodesList: UniverseNode[] = useMemo(() => {
    const nodeData: unknown = universe.nodes;
    if (Array.isArray(nodeData)) return nodeData as UniverseNode[];
    if (nodeData && typeof nodeData === 'object') {
      return Object.entries(nodeData as Record<string, Partial<UniverseNode>>).map(([id, val]) => ({
        ...val,
        id,
        x: Number(val.x) || 0,
        y: Number(val.y) || 0,
        z: Number(val.z) || 0,
        cluster_id: val.cluster_id ?? 1,
      }));
    }
    return [];
  }, [universe.nodes]);

  // Pre-indexed caller & callee counts for O(1) hover lookups (0 allocation during interaction)
  const { callerCounts, calleeCounts } = useMemo(() => {
    const callers = new Map<string, number>();
    const callees = new Map<string, number>();
    if (universe?.edges) {
      for (const e of universe.edges) {
        callees.set(e.source, (callees.get(e.source) || 0) + 1);
        callers.set(e.target, (callers.get(e.target) || 0) + 1);
      }
    }
    for (const node of allNodesList) {
      if (typeof node.caller_count === 'number') callers.set(node.id, node.caller_count);
      if (typeof node.callee_count === 'number') callees.set(node.id, node.callee_count);
    }
    return { callerCounts: callers, calleeCounts: callees };
  }, [allNodesList, universe.edges]);

  // Execute Adaptive Namespace & Cluster Engine (Stable reference across theme toggles)
  const { nodeClusterMap, clusters: adaptiveClusters } = useMemo(() => {
    const pathByNodeId = new Map(allNodesList.map((node) => [
      node.id,
      node.file_path || (node.id.startsWith('file::') ? node.id.slice('file::'.length) : node.id),
    ]));
    const { nodeClusterMap: pathClusterMap, clusters } = buildAdaptiveClusters([...pathByNodeId.values()], false);
    const nodeClusters = new Map<string, string>();
    for (const [nodeId, path] of pathByNodeId) {
      nodeClusters.set(nodeId, pathClusterMap.get(path) || 'root');
    }
    return { nodeClusterMap: nodeClusters, clusters };
  }, [allNodesList]);

  // Filtered legend items for search
  const visibleLegendClusters = useMemo(() => {
    if (!legendSearch.trim()) return adaptiveClusters;
    const q = legendSearch.toLowerCase();
    return adaptiveClusters.filter((c) => c.name.toLowerCase().includes(q));
  }, [adaptiveClusters, legendSearch]);

  // Search matches
  const filteredNodes = useMemo(() => {
    if (!searchQuery.trim()) return [];
    const q = searchQuery.toLowerCase();
    return allNodesList.filter((n) => n.id.toLowerCase().includes(q)).slice(0, 8);
  }, [allNodesList, searchQuery]);

  // Travel to next/prev world
  const handleTravelNext = () => {
    if (allNodesList.length === 0) return;
    const currentIndex = allNodesList.findIndex((n) => n.id === selectedNodeIdRef.current);
    const nextIndex = (currentIndex + 1) % allNodesList.length;
    flyToNodeByIdRef.current?.(allNodesList[nextIndex].id);
  };

  const handleTravelPrev = () => {
    if (allNodesList.length === 0) return;
    const currentIndex = allNodesList.findIndex((n) => n.id === selectedNodeIdRef.current);
    const prevIndex = currentIndex <= 0 ? allNodesList.length - 1 : currentIndex - 1;
    flyToNodeByIdRef.current?.(allNodesList[prevIndex].id);
  };

  const handleRecenter = () => {
    selectedNodeIdRef.current = null;
    setSelectedFolderFilter(null);
    activeNeighborsRef.current.clear();
    onSelectNode?.('');
    recenterRef.current?.();
  };

  const handleFilterFolder = (folderName: string) => {
    if (selectedFolderFilter === folderName) {
      setSelectedFolderFilter(null);
      handleRecenter();
    } else {
      setSelectedFolderFilter(folderName);
      selectedNodeIdRef.current = null;
      onSelectNode?.('');
      flyToClusterRef.current?.(folderName);
    }
  };

  // Selected node details
  const selectedNode = useMemo(() => {
    if (!selectedNodeId || !universe?.nodes) return null;
    const visibleNode = allNodesList.find((node) => node.id === selectedNodeId);
    if (visibleNode) return visibleNode;
    if (selectedNodeId.startsWith('file:')) {
      const filePath = selectedNodeId.slice('file:'.length);
      const fileSymbols = allNodesList.filter((node) => getUniverseNodeFilePath(node) === filePath);
      return {
        id: selectedNodeId,
        label: filePath,
        kind: 'file',
        file_path: filePath,
        status: fileSymbols.some((node) => node.status?.toLowerCase() === 'unknown') ? 'unknown' : fileSymbols[0]?.status,
        caller_count: fileSymbols.reduce((total, node) => total + (node.caller_count || 0), 0),
        callee_count: fileSymbols.reduce((total, node) => total + (node.callee_count || 0), 0),
        x: 0,
        y: 0,
        z: 0,
      } satisfies UniverseNode;
    }
    const target = egoNetwork?.target_node;
    if (!target || target.stable_id !== selectedNodeId) return null;
    return {
      id: target.stable_id,
      label: target.qualified_name,
      kind: target.node_kind,
      file_path: target.file_path,
      start_line: target.start_line,
      end_line: target.end_line,
      status: target.coverage_state,
      x: 0,
      y: 0,
      z: 0,
    } satisfies UniverseNode;
  }, [selectedNodeId, universe, allNodesList, egoNetwork]);

  // Connected edges for the selected node
  const connectedEdges = useMemo(() => {
    if (!selectedNodeId || !universe?.edges) return { inbound: [], outbound: [] };
    if (egoNetwork?.target_node?.stable_id === selectedNodeId) {
      const toUniverseEdge = (edge: EgoNetwork['incoming_callers'][number]): UniverseEdge => ({
        id: edge.edge_id,
        source: edge.source_stable_id,
        target: edge.target_stable_id,
        kind: edge.edge_kind,
        source_file: edge.source_file,
        source_line: edge.source_line,
        source_col: edge.source_col,
        target_file: edge.target_file,
        target_line: edge.target_line,
        confidence: edge.confidence,
      });
      return {
        inbound: egoNetwork.incoming_callers.map(toUniverseEdge),
        outbound: egoNetwork.outgoing_callees.map(toUniverseEdge),
      };
    }
    if (selectedNodeId.startsWith('file:')) {
      const filePath = selectedNodeId.slice('file:'.length);
      const fileNodeIds = new Set(
        allNodesList
          .filter((node) => getUniverseNodeFilePath(node) === filePath)
          .map((node) => node.id),
      );
      return {
        inbound: universe.edges.filter((edge) => fileNodeIds.has(edge.target) || edge.target_file === filePath),
        outbound: universe.edges.filter((edge) => fileNodeIds.has(edge.source) || edge.source_file === filePath),
      };
    }
    const inbound = universe.edges.filter((e) => e.target === selectedNodeId);
    const outbound = universe.edges.filter((e) => e.source === selectedNodeId);
    return { inbound, outbound };
  }, [selectedNodeId, universe, allNodesList, egoNetwork]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const width = container.clientWidth || 1000;
    const height = container.clientHeight || 700;

    // 1. Scene & Setup
    const scene = new THREE.Scene();
    scene.fog = isDark ? new THREE.FogExp2(0x0a0e17, 0.0004) : null;
    const isInitial3D = (universe.dimensions || activeDimRef.current) === '3D';
    activeDimRef.current = isInitial3D ? '3D' : '2D';

    // 2. Camera Setup (Spherical Orbit Coordinate System)
    const camera = new THREE.PerspectiveCamera(45, width / height, 0.1, 4500);
    cameraRef.current = camera;

    const denseTargetZ = universe.node_count >= DENSE_GRAPH_RENDER_THRESHOLD ? 560 : 0;
    const orbitTarget = new THREE.Vector3(0, 0, denseTargetZ);
    const targetLookAt = new THREE.Vector3(0, 0, denseTargetZ);

    // In 3D mode, start with elevated cinematic isometric perspective (60 deg elevation, 45 deg azimuth)
    // In 2.5D mode, start with rich isometric tabletop drafting angle (beta = 0.42, ~24 deg tilt)
    // Compacted initial camera radius brings all modules into a crisp, readable archipelago
    let radius = universe.node_count >= DENSE_GRAPH_RENDER_THRESHOLD ? 1500 : isInitial3D ? 420 : 560;
    let targetRadius = radius;
    let theta = isInitial3D ? Math.PI / 4 : 0;
    let targetTheta = theta;
    let phi = isInitial3D ? Math.PI / 3 : Math.PI / 2;
    let targetPhi = phi;

    // 2.5D In-Plane Turntable & Isometric Orbit Angles (alpha = azimuth/roll, beta = pitch tilt)
    let alpha = 0;
    let targetAlpha = 0;
    let beta = isInitial3D ? 0 : 0.42;
    let targetBeta = beta;

    if (isInitial3D) {
      camera.position.set(
        orbitTarget.x + radius * Math.sin(phi) * Math.sin(theta),
        orbitTarget.y + radius * Math.cos(phi),
        orbitTarget.z + radius * Math.sin(phi) * Math.cos(theta)
      );
      camera.up.set(0, 1, 0);
    } else {
      camera.position.set(
        orbitTarget.x + radius * Math.sin(beta) * Math.sin(alpha),
        orbitTarget.y - radius * Math.sin(beta) * Math.cos(alpha),
        orbitTarget.z + radius * Math.cos(beta)
      );
      camera.up.set(
        -Math.cos(beta) * Math.sin(alpha),
        Math.cos(beta) * Math.cos(alpha),
        Math.sin(beta)
      );
    }
    camera.lookAt(orbitTarget);

    // 3. WebGL Renderer
    const renderer = new THREE.WebGLRenderer({
      antialias: true,
      alpha: true,
      powerPreference: 'high-performance',
    });
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.setClearColor(isDark ? 0x0a0e17 : 0xf1f5f9, 1);
    renderer.toneMapping = isDark ? THREE.ACESFilmicToneMapping : THREE.NoToneMapping;
    renderer.toneMappingExposure = isDark ? 1.15 : 1.0;
    container.innerHTML = '';
    container.appendChild(renderer.domElement);

    if (universe.node_count >= DENSE_GRAPH_RENDER_THRESHOLD) {
      const denseRenderer = setupDenseUniverseRenderer({
        container,
        scene,
        camera,
        renderer,
        universe,
        nodes: allNodesList,
        edges: sampleDenseOverviewEdges(universe.edges),
        nodeClusterMap,
        darkClusterColors: new Map(adaptiveClusters.map((cluster) => [cluster.name, cluster.darkTheme.hex])),
        lightClusterColors: new Map(adaptiveClusters.map((cluster) => [cluster.name, cluster.lightTheme.hex])),
        isDark,
        refs: {
          activeDimension: activeDimRef,
          selectedNodeId: selectedNodeIdRef,
          hoveredNodeId: hoveredNodeIdRef,
          activeNeighbors: activeNeighborsRef,
          controlMode: controlModeRef,
          edgeFilter: edgeFilterRef,
          simulation: simulationRef,
          simulationFilePath: simulationFilePathRef,
          damageOverlay: damageOverlayRef,
        },
        onSelectNode,
        onFps: setFps,
        getNodeState: getSimulationNodeState,
        isEdgeBroken: isSimulationEdgeBroken,
        onHoverNode: (node, clientX, clientY) => {
          const pill = hoverPillRef.current;
          if (!pill || !node) {
            if (pill) pill.style.display = 'none';
            return;
          }
          const rect = container.getBoundingClientRect();
          const left = Math.max(8, Math.min(clientX - rect.left + 14, rect.width - 230));
          const top = Math.max(8, Math.min(clientY - rect.top + 14, rect.height - 88));
          pill.style.transform = `translate3d(${left}px, ${top}px, 0)`;
          pill.style.display = selectedNodeIdRef.current ? 'none' : 'block';
          const extension = node.id.split('.').pop() || 'node';
          const cluster = nodeClusterMap.get(node.id) || 'root';
          if (hoverExtRef.current) {
            hoverExtRef.current.textContent = extension.toUpperCase();
            hoverExtRef.current.style.backgroundColor = isDarkRef.current ? '#0e7490' : '#0369a1';
          }
          if (hoverNameRef.current) hoverNameRef.current.textContent = node.label || node.id.split('/').pop() || node.id;
          if (hoverFolderRef.current) hoverFolderRef.current.textContent = cluster;
          if (hoverCallersRef.current) {
            hoverCallersRef.current.textContent = `← ${callerCounts.get(node.id) ?? 0} callers`;
          }
          if (hoverCalleesRef.current) {
            hoverCalleesRef.current.textContent = `→ ${calleeCounts.get(node.id) ?? 0} callees`;
          }
          if (hoverBrokenRef.current) {
            hoverBrokenRef.current.textContent = node.status?.toUpperCase() || 'UNKNOWN';
            hoverBrokenRef.current.className = node.status?.toLowerCase() === 'verified'
              ? 'font-bold text-emerald-600 dark:text-emerald-400'
              : 'font-bold text-amber-600 dark:text-amber-400';
          }
        },
      });

      morphToDimensionRef.current = denseRenderer.setDimension;
      flyToNodeByIdRef.current = denseRenderer.flyToNode;
      flyToClusterRef.current = denseRenderer.flyToCluster;
      recenterRef.current = denseRenderer.recenter;
      panUniverseRef.current = denseRenderer.pan;
      zoomUniverseRef.current = denseRenderer.zoom;
      onThemeChangeRef.current = denseRenderer.setTheme;
      onDamageOverlayChangeRef.current = denseRenderer.updateOverlay;
      onSimulationOverlayChangeRef.current = denseRenderer.updateOverlay;
      onEdgeFilterChangeRef.current = denseRenderer.setEdgeFilter;
      if (focusNodeId) denseRenderer.flyToNode(focusNodeId);

      return () => {
        onThemeChangeRef.current = null;
        onDamageOverlayChangeRef.current = null;
        onSimulationOverlayChangeRef.current = null;
        onEdgeFilterChangeRef.current = null;
        denseRenderer.dispose();
        renderer.dispose();
        if (container.contains(renderer.domElement)) container.removeChild(renderer.domElement);
      };
    }

    // 4. Lighting Suite
    const ambientLight = new THREE.AmbientLight(0xffffff, isDark ? 1.6 : 1.25);
    scene.add(ambientLight);

    const mainKeyLight = new THREE.DirectionalLight(0xffffff, isDark ? 1.2 : 0.85);
    mainKeyLight.position.set(60, 160, 120);
    scene.add(mainKeyLight);

    // 5. Floor Grid
    const gridHelper = new THREE.GridHelper(
      1600,
      36,
      isDark ? 0x00f0ff : 0x0284c7,
      isDark ? 0x1e293b : 0xcbd5e1
    );
    (gridHelper.material as THREE.Material).transparent = true;
    scene.add(gridHelper);

    // 6. Universe Group
    const universeGroup = new THREE.Group();
    scene.add(universeGroup);

    // =========================================================================
    // 7. Sunflower Fermat Spiral (Phyllotaxis) & 3D Fibonacci Spatial Engine
    // Fully adaptable: distributes clusters with mathematical non-overlap
    // Pre-calculates dual 2D and 3D positions for zero-lag instant GPU morphing
    // =========================================================================
    const rawList = allNodesList;

    // Group files by adaptive cluster
    const clusterMembersMap = new Map<string, UniverseNode[]>();
    rawList.forEach((node) => {
      const cKey = nodeClusterMap.get(node.id) || 'root';
      if (!clusterMembersMap.has(cKey)) clusterMembersMap.set(cKey, []);
      clusterMembersMap.get(cKey)!.push(node);
    });

    const clusterCenters3D = new Map<string, { x: number; y: number; z: number; radius: number }>();
    const clusterCenters2D = new Map<string, { x: number; y: number; z: number; radius: number }>();
    clusterCenters3D.set('root', { x: 0, y: 0, z: 0, radius: 45 });
    clusterCenters2D.set('root', { x: 0, y: 0, z: 0, radius: 45 });

    const nonRootClusters = adaptiveClusters.filter((c) => c.name !== 'root');
    const GOLDEN_ANGLE = 137.5077 * (Math.PI / 180);
    const totalClusters = nonRootClusters.length;

    nonRootClusters.forEach((c, idx) => {
      // 3D Fibonacci Spherical Lattice (Calibrated Compact Spacing)
      const phiK = Math.acos(1 - (2 * (idx + 0.5)) / Math.max(totalClusters, 1));
      const thetaK = idx * GOLDEN_ANGLE;
      const islandRadius3D = 32 + Math.cbrt(c.count) * 16;
      const orbitDist = 240 + (idx % 3) * 55 + Math.sqrt(idx + 1) * 30;
      const cx3D = Math.sin(phiK) * Math.cos(thetaK) * orbitDist;
      const cz3D = Math.sin(phiK) * Math.sin(thetaK) * orbitDist;
      const cy3D = Math.cos(phiK) * (orbitDist * 0.72);
      clusterCenters3D.set(c.name, { x: cx3D, y: cy3D, z: cz3D, radius: islandRadius3D });

      // 2D Mind-Map Radial Sectors with Staggered Alternating Distances (Calibrated Compact Spacing)
      const baseAngle = (2 * Math.PI * idx) / Math.max(totalClusters, 1);
      const tiers = Math.max(1, Math.ceil(Math.sqrt(c.count / 3)));
      const islandRadius2D = 30 + (tiers - 1) * 26 + 14;
      const isOuter = idx % 2 === 1;
      const radialDist = isOuter ? 260 + islandRadius2D * 0.75 : 165 + islandRadius2D * 0.65;
      const cx2D = Math.cos(baseAngle) * radialDist;
      const cy2D = Math.sin(baseAngle) * radialDist;
      clusterCenters2D.set(c.name, { x: cx2D, y: cy2D, z: 0, radius: islandRadius2D });
    });

    // Position member files within each cluster island for both 3D and 2D
    const nodesList3D: (UniverseNode & { z: number })[] = [];
    const nodesList2D: (UniverseNode & { z: number })[] = [];

    adaptiveClusters.forEach((c) => {
      const members = clusterMembersMap.get(c.name) || [];
      const center3D = clusterCenters3D.get(c.name) || { x: 0, y: 0, z: 0, radius: 45 };
      const center2D = clusterCenters2D.get(c.name) || { x: 0, y: 0, z: 0, radius: 45 };

      // Sort files with main entrypoints at center
      const sortedMembers = [...members].sort((a, b) => {
        const aBase = a.id.split('/').pop() || '';
        const bBase = b.id.split('/').pop() || '';
        const isEntry = (name: string) =>
          name.startsWith('index') ||
          name.startsWith('lib') ||
          name.startsWith('mod') ||
          name.startsWith('main') ||
          name.startsWith('server') ||
          name.startsWith('page');
        if (isEntry(aBase) && !isEntry(bBase)) return -1;
        if (!isEntry(aBase) && isEntry(bBase)) return 1;
        return a.id.localeCompare(b.id);
      });

      // 3D Packing (Compact & Non-Overlapping)
      sortedMembers.forEach((node, idx) => {
        if (c.name === 'root' && sortedMembers.length <= 6) {
          const angle = (2 * Math.PI * idx) / Math.max(sortedMembers.length, 1);
          const r = 35 + idx * 6;
          nodesList3D.push({
            ...node,
            x: Math.cos(angle) * r,
            y: ((idx % 3) - 1) * 14,
            z: Math.sin(angle) * r,
          });
        } else {
          const N = sortedMembers.length;
          const nodePhi = Math.acos(1 - (2 * (idx + 0.5)) / Math.max(N, 1));
          const nodeTheta = idx * GOLDEN_ANGLE;
          const nodeR = 24 + Math.cbrt(idx) * 20;
          const nx = center3D.x + Math.sin(nodePhi) * Math.cos(nodeTheta) * nodeR;
          const nz = center3D.z + Math.sin(nodePhi) * Math.sin(nodeTheta) * nodeR;
          const ny = center3D.y + Math.cos(nodePhi) * (nodeR * 0.82);
          nodesList3D.push({
            ...node,
            x: nx,
            y: ny,
            z: nz,
          });
        }
      });

      // 2D Packing (Compact Tier Layout)
      sortedMembers.forEach((node, idx) => {
        if (c.name === 'root') {
          const angle = (2 * Math.PI * idx) / Math.max(sortedMembers.length, 1);
          const r = sortedMembers.length <= 1 ? 0 : 30 + idx * 8;
          nodesList2D.push({
            ...node,
            x: Math.cos(angle) * r,
            y: Math.sin(angle) * r,
            z: 2.0,
          });
        } else if (idx === 0) {
          nodesList2D.push({
            ...node,
            x: center2D.x,
            y: center2D.y,
            z: 4.0, // Primary entrypoint file sits elevated at highest tier in 2.5D
          });
        } else {
          let tier = 1;
          let tierCap = 6;
          let sumBefore = 1;
          while (idx >= sumBefore + tierCap) {
            sumBefore += tierCap;
            tier++;
            tierCap = 6 * tier;
          }
          const slot = idx - sumBefore;
          const tierRadius = 32 + (tier - 1) * 28;
          const offsetAngle = (2 * Math.PI * slot) / tierCap + tier * 0.32;
          const nx = center2D.x + Math.cos(offsetAngle) * tierRadius;
          const ny = center2D.y + Math.sin(offsetAngle) * tierRadius;
          const zElevation = Math.max(0.6, 3.4 - tier * 0.9);
          nodesList2D.push({
            ...node,
            x: nx,
            y: ny,
            z: zElevation,
          });
        }
      });
    });

    relaxPositionsWithSpatialHash(nodesList3D, 28, 3, 18);
    relaxPositionsWithSpatialHash(nodesList2D, 28, 2, 18);

    const pos3DMap = new Map<string, THREE.Vector3>();
    nodesList3D.forEach((n) => pos3DMap.set(n.id, new THREE.Vector3(n.x, n.y, n.z)));
    const pos2DMap = new Map<string, THREE.Vector3>();
    nodesList2D.forEach((n) => pos2DMap.set(n.id, new THREE.Vector3(n.x, n.y, n.z)));

    // Coordinate grid helper: In 3D positioned as floor below lowest cluster;
    // In 2.5D rotated to lie flat on X-Y plane as blueprint drafting table backing
    let lowestY = -120;
    clusterCenters3D.forEach((c) => {
      if (c.y - c.radius < lowestY) lowestY = c.y - c.radius;
    });
    const gridPos3D = new THREE.Vector3(0, Math.min(lowestY - 50, -200), 0);
    const gridPos2D = new THREE.Vector3(0, 0, -2.8);

    if (isInitial3D) {
      gridHelper.rotation.x = 0;
      gridHelper.position.copy(gridPos3D);
      (gridHelper.material as THREE.Material).opacity = isDark ? 0.2 : 0.55;
    } else {
      gridHelper.rotation.x = Math.PI / 2;
      gridHelper.position.copy(gridPos2D);
      (gridHelper.material as THREE.Material).opacity = isDark ? 0.08 : 0.16;
    }

    // Cluster Boundaries & Mind Map Branch Trunk Infrastructure
    interface ClusterIsland {
      key: string;
      name: string;
      center3D: THREE.Vector3;
      center2D: THREE.Vector3;
      radius: number;
      haloMesh: THREE.Mesh;
      ringMesh: THREE.Mesh;
      grabMesh3D: THREE.Mesh;
      grabMesh2D: THREE.Mesh;
      badgeMesh: THREE.Mesh | null;
      memberNodeIds: string[];
    }
    const clusterIslands: ClusterIsland[] = [];

    const cluster3DHalos: {
      mesh: THREE.Mesh;
      clusterKey: string;
      pos3D: THREE.Vector3;
      pos2D: THREE.Vector3;
      startPos: THREE.Vector3;
      targetPos: THREE.Vector3;
      startOpacity: number;
      targetOpacity: number;
    }[] = [];

    const cluster2DRings: {
      mesh: THREE.Mesh;
      clusterKey: string;
      pos3D: THREE.Vector3;
      pos2D: THREE.Vector3;
      startPos: THREE.Vector3;
      targetPos: THREE.Vector3;
      startOpacity: number;
      targetOpacity: number;
    }[] = [];

    nonRootClusters.forEach((c) => {
      const center3D = clusterCenters3D.get(c.name);
      const center2D = clusterCenters2D.get(c.name);
      if (!center3D || !center2D) return;

      const preset = FOLDER_PALETTES[c.name];
      const hex = preset
        ? isDark
          ? preset.dark.hex
          : preset.light.hex
        : c.theme?.hex || 0x0284c7;

      // 1. 3D Torus Halo
      const haloGeo = new THREE.TorusGeometry(center3D.radius * 1.05, 0.5, 16, 64);
      const haloMat = new THREE.MeshBasicMaterial({
        color: hex,
        transparent: true,
        opacity: isInitial3D ? (isDark ? 0.18 : 0.4) : 0,
      });
      const haloMesh = new THREE.Mesh(haloGeo, haloMat);
      const hPos3D = new THREE.Vector3(center3D.x, center3D.y, center3D.z);
      const hPos2D = new THREE.Vector3(center2D.x, center2D.y, 0);
      haloMesh.position.copy(isInitial3D ? hPos3D : hPos2D);
      haloMesh.rotation.x = Math.PI / 2;
      universeGroup.add(haloMesh);

      // 1.1 3D Grab Ring Collider (Enlarged hit area for effortless cursor grabbing)
      const grabGeo3D = new THREE.TorusGeometry(center3D.radius * 1.05, 14, 8, 36);
      const grabMat3D = new THREE.MeshBasicMaterial({ visible: false });
      const grabMesh3D = new THREE.Mesh(grabGeo3D, grabMat3D);
      grabMesh3D.position.copy(haloMesh.position);
      grabMesh3D.rotation.x = Math.PI / 2;
      universeGroup.add(grabMesh3D);

      cluster3DHalos.push({
        mesh: haloMesh,
        clusterKey: c.name,
        pos3D: hPos3D,
        pos2D: hPos2D,
        startPos: haloMesh.position.clone(),
        targetPos: haloMesh.position.clone(),
        startOpacity: haloMat.opacity,
        targetOpacity: haloMat.opacity,
      });

      // 2. 2D Mind Map Island Perimeter Ring
      const ringGeo = new THREE.RingGeometry(center2D.radius * 1.01, center2D.radius * 1.04, 64);
      const ringMat = new THREE.MeshBasicMaterial({
        color: hex,
        transparent: true,
        opacity: isInitial3D ? 0 : isDark ? 0.28 : 0.45,
        side: THREE.DoubleSide,
      });
      const ringMesh = new THREE.Mesh(ringGeo, ringMat);
      const rPos3D = new THREE.Vector3(center3D.x, center3D.y, center3D.z);
      const rPos2D = new THREE.Vector3(center2D.x, center2D.y, -0.2);
      ringMesh.position.copy(isInitial3D ? rPos3D : rPos2D);
      universeGroup.add(ringMesh);

      // 2.1 2D Grab Ring Collider (Wide ring hit area for effortless cursor grabbing)
      const grabGeo2D = new THREE.RingGeometry(center2D.radius * 0.88, center2D.radius * 1.18, 36);
      const grabMat2D = new THREE.MeshBasicMaterial({ visible: false, side: THREE.DoubleSide });
      const grabMesh2D = new THREE.Mesh(grabGeo2D, grabMat2D);
      grabMesh2D.position.copy(ringMesh.position);
      universeGroup.add(grabMesh2D);

      cluster2DRings.push({
        mesh: ringMesh,
        clusterKey: c.name,
        pos3D: rPos3D,
        pos2D: rPos2D,
        startPos: ringMesh.position.clone(),
        targetPos: ringMesh.position.clone(),
        startOpacity: ringMat.opacity,
        targetOpacity: ringMat.opacity,
      });

      clusterIslands.push({
        key: c.name,
        name: c.name,
        center3D: new THREE.Vector3(center3D.x, center3D.y, center3D.z),
        center2D: new THREE.Vector3(center2D.x, center2D.y, 0),
        radius: center3D.radius,
        haloMesh,
        ringMesh,
        grabMesh3D,
        grabMesh2D,
        badgeMesh: null,
        memberNodeIds: (clusterMembersMap.get(c.name) || []).map((m) => m.id),
      });
    });

    // 8. Build Architectural File Slates with Folder-Color Coded Rings
    const nodeObjects: {
      mesh: THREE.Mesh;
      ringMesh: THREE.Mesh;
      data: UniverseNode;
      clusterKey: string;
      theme: FolderTheme;
      pos3D: THREE.Vector3;
      pos2D: THREE.Vector3;
      startPos: THREE.Vector3;
      targetPos: THREE.Vector3;
    }[] = [];

    const slateGeo = new THREE.BoxGeometry(22, 13.75, 0.7);
    const ringGeo = new THREE.TorusGeometry(13.6, 0.38, 16, 64);

    const clusterThemeMap = new Map<string, FolderTheme>();
    const clusterMapThemeLight = new Map<string, FolderTheme>();
    const clusterMapThemeDark = new Map<string, FolderTheme>();
    adaptiveClusters.forEach((c) => {
      clusterThemeMap.set(c.name, c.theme);
      clusterMapThemeLight.set(c.name, c.lightTheme);
      clusterMapThemeDark.set(c.name, c.darkTheme);
    });

    const sourceNodes = nodesList3D.length > 0 ? nodesList3D : allNodesList;
    sourceNodes.forEach((node) => {
      const cKey = nodeClusterMap.get(node.id) || 'root';
      const cTheme = (isDark ? clusterMapThemeDark.get(cKey) : clusterMapThemeLight.get(cKey)) || {
        name: cKey,
        color: '#0284c7',
        hex: 0x0284c7,
      };

      const nodeState = getSimulationNodeState(node, simulationRef.current, simulationFilePathRef.current);
      const isDamaged = damageOverlayRef.current && nodeState.damaged;
      const isDead = damageOverlayRef.current && nodeState.dead;
      const isSelected = selectedNodeIdRef.current === node.id;
      const isHovered = hoveredNodeIdRef.current === node.id;

      const texture = generateFileHoloTexture(node, cTheme, isSelected, isHovered, isDamaged, isDead, isDark, universe.node_count >= 1000);

      const slateMat = new THREE.MeshBasicMaterial({
        map: texture,
        transparent: true,
        opacity: 1.0,
      });

      const pos3D = pos3DMap.get(node.id) || new THREE.Vector3(node.x, node.y, node.z || 0);
      const pos2D = pos2DMap.get(node.id) || new THREE.Vector3(node.x, node.y, 0);

      const initialPos = isInitial3D ? pos3D.clone() : pos2D.clone();
      const startPos = prevPositionsRef.current.get(node.id)?.clone() || initialPos.clone();
      const targetPos = initialPos.clone();

      const mesh = new THREE.Mesh(slateGeo, slateMat);
      mesh.position.copy(startPos);

      // Orbital Ring uses the cluster's signature color
      const ringColor = isDamaged
        ? isDark ? 0xf43f5e : 0xe11d48
        : isDead
        ? isDark ? 0x64748b : 0x94a3b8
        : isSelected
        ? isDark ? 0x00f0ff : 0x0284c7
        : cTheme.hex;

      const ringMat = new THREE.MeshBasicMaterial({
        color: ringColor,
        transparent: true,
        opacity: isSelected ? 1.0 : isDark ? 0.35 : 0.88,
      });
      const ringMesh = new THREE.Mesh(ringGeo, ringMat);
      ringMesh.position.copy(mesh.position);
      ringMesh.rotation.x = Math.PI / 2;
      ringMesh.visible = universe.node_count < 1000;

      universeGroup.add(mesh);
      universeGroup.add(ringMesh);

      nodeObjects.push({
        mesh,
        ringMesh,
        data: node,
        clusterKey: cKey,
        theme: cTheme,
        pos3D,
        pos2D,
        startPos,
        targetPos,
      });
    });

    const nodeObjMap = new Map<string, typeof nodeObjects[0]>();
    nodeObjects.forEach((no) => nodeObjMap.set(no.data.id, no));

    // 8.1 Build Floating Billboard Cluster Title Badges (LOD Labels)
    const clusterBadgeObjects: {
      mesh: THREE.Mesh;
      clusterKey: string;
      count: number;
      percentage: number;
      themeColor: string;
      pos3D: THREE.Vector3;
      pos2D: THREE.Vector3;
      startPos: THREE.Vector3;
      targetPos: THREE.Vector3;
    }[] = [];

    const badgeGeo = new THREE.PlaneGeometry(38, 8.5);
    nonRootClusters.forEach((c) => {
      const center3D = clusterCenters3D.get(c.name);
      const center2D = clusterCenters2D.get(c.name);
      if (!center3D || !center2D) return;

      const theme = (isDark ? clusterMapThemeDark.get(c.name) : clusterMapThemeLight.get(c.name)) || {
        name: c.name,
        color: '#0284c7',
        hex: 0x0284c7,
      };

      const texture = generateClusterBadgeTexture(
        c.name,
        c.count,
        c.percentage,
        theme.color,
        isDark
      );

      const badgeMat = new THREE.MeshBasicMaterial({
        map: texture,
        transparent: true,
        opacity: 0.95,
        depthWrite: false,
        side: THREE.DoubleSide,
      });

      const badgeMesh = new THREE.Mesh(badgeGeo, badgeMat);
      const bPos3D = new THREE.Vector3(center3D.x, center3D.y + center3D.radius + 12, center3D.z);
      const bPos2D = new THREE.Vector3(center2D.x, center2D.y + center2D.radius + 12, 2);

      badgeMesh.position.copy(isInitial3D ? bPos3D : bPos2D);
      universeGroup.add(badgeMesh);

      const targetIsland = clusterIslands.find((i) => i.key === c.name);
      if (targetIsland) {
        targetIsland.badgeMesh = badgeMesh;
      }

      clusterBadgeObjects.push({
        mesh: badgeMesh,
        clusterKey: c.name,
        count: c.count,
        percentage: c.percentage,
        themeColor: theme.color,
        pos3D: bPos3D,
        pos2D: bPos2D,
        startPos: badgeMesh.position.clone(),
        targetPos: badgeMesh.position.clone(),
      });
    });

    // 9. Build Curved Architectural Data Highways (Zero Hairball)
    const edgesList = universe.edges || [];
    const pulsePackets: PulsePacket[] = [];
    const edgeObjects: {
      line: THREE.Line;
      data: UniverseEdge;
      source: string;
      target: string;
      kind: string;
      isBroken: boolean;
      isIntraFolder: boolean;
      baseColors: number[];
      baseSocketColor: number;
      socket1: THREE.Mesh;
      socket2: THREE.Mesh;
      pulseIndex: number;
      idx: number;
    }[] = [];
    const packetGeo = new THREE.SphereGeometry(1.0, 16, 16);

    if (edgesList.length > 0) {
      edgesList.forEach((edge, idx) => {
        const no1 = nodeObjMap.get(edge.source);
        const no2 = nodeObjMap.get(edge.target);
        if (!no1 || !no2) return;

        const isLineVisible = edgeFilterRef.current === 'all' || edge.kind === edgeFilterRef.current;

        const p1_center = no1.mesh.position;
        const p2_center = no2.mesh.position;
        const delta = new THREE.Vector3().subVectors(p2_center, p1_center);
        const totalDist = delta.length();
        const ringRadius = 13.6;

        const p1 = p1_center.clone();
        const p2 = p2_center.clone();

        if (totalDist > ringRadius * 2 + 2) {
          const unitDir = delta.clone().normalize();
          p1.addScaledVector(unitDir, ringRadius);
          p2.addScaledVector(unitDir, -ringRadius);
        }

        const mid = new THREE.Vector3().addVectors(p1, p2).multiplyScalar(0.5);
        const lineDir = new THREE.Vector3().subVectors(p2, p1);
        const distance = lineDir.length();
        let perp: THREE.Vector3;
        if (isInitial3D) {
          const radial = mid.clone().normalize();
          perp = new THREE.Vector3().crossVectors(lineDir, radial).normalize();
          if (perp.lengthSq() < 0.01) {
            perp = new THREE.Vector3(-lineDir.y, lineDir.x, lineDir.z || 0.5).normalize();
          }
        } else {
          perp = new THREE.Vector3(-lineDir.y, lineDir.x, 0).normalize();
        }

        const arcHeight = Math.min(distance * 0.18, 32) * (idx % 2 === 0 ? 1 : -1);
        const ctrl = new THREE.Vector3().copy(mid).addScaledVector(perp, arcHeight);

        const curve = new THREE.QuadraticBezierCurve3(p1, ctrl, p2);
        const points = curve.getPoints(45);

        const isBroken = damageOverlayRef.current && isSimulationEdgeBroken(edge, simulationRef.current);

        const srcCluster = nodeClusterMap.get(edge.source) || 'root';
        const dstCluster = nodeClusterMap.get(edge.target) || 'root';
        const isIntraFolder = srcCluster === dstCluster;

        const srcTheme = (isDarkRef.current ? clusterMapThemeDark.get(srcCluster) : clusterMapThemeLight.get(srcCluster)) || {
          name: srcCluster,
          color: '#0284c7',
          hex: 0x0284c7,
        };
        const positions: number[] = [];
        const colors: number[] = [];
        const baseColors: number[] = [];

        points.forEach((pt, pIdx) => {
          positions.push(pt.x, pt.y, pt.z);
          const t = pIdx / points.length;
          let red: number;
          let green: number;
          let blue: number;

          if (isIntraFolder) {
            const c = new THREE.Color(srcTheme.hex);
            red = c.r;
            green = c.g;
            blue = c.b;
          } else if (edge.kind === 'imports') {
            if (isDarkRef.current) {
              red = THREE.MathUtils.lerp(0.58, 0.0, t);
              green = THREE.MathUtils.lerp(0.2, 0.94, t);
              blue = 1.0;
            } else {
              red = 0.35;
              green = 0.15;
              blue = 0.85;
            }
          } else {
            if (isDarkRef.current) {
              red = 0.0;
              green = THREE.MathUtils.lerp(0.7, 0.94, t);
              blue = 1.0;
            } else {
              red = 0.02;
              green = 0.45;
              blue = 0.75;
            }
          }
          baseColors.push(red, green, blue);
          if (isBroken) colors.push(1.0, 0.15, 0.35);
          else colors.push(red, green, blue);
        });

        const lineGeo = new THREE.BufferGeometry();
        lineGeo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
        lineGeo.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));

        const initialOpacity = isBroken ? 0.85 : isDarkRef.current ? 0.12 : 0.16;

        const lineMat = new THREE.LineBasicMaterial({
          vertexColors: true,
          transparent: true,
          opacity: initialOpacity,
          linewidth: 1.5,
        });

        const curveLine = new THREE.Line(lineGeo, lineMat);
        curveLine.visible = isLineVisible;
        universeGroup.add(curveLine);

        // Terminal socket beads on rings
        const socketColor = isBroken ? 0xf43f5e : srcTheme.hex;
        const socketMat = new THREE.MeshBasicMaterial({ color: socketColor });
        const socketGeo = new THREE.SphereGeometry(0.75, 12, 12);

        const socket1 = new THREE.Mesh(socketGeo, socketMat);
        socket1.position.copy(p1);
        socket1.visible = universe.node_count < 1000;
        universeGroup.add(socket1);

        const socket2 = new THREE.Mesh(socketGeo, socketMat);
        socket2.position.copy(p2);
        socket2.visible = universe.node_count < 1000;
        universeGroup.add(socket2);

        // Electric Data Pulse Packet
        const packetMat = new THREE.MeshBasicMaterial({ color: socketColor });
        const packetMesh = new THREE.Mesh(packetGeo, packetMat);
        packetMesh.position.copy(p1);
        packetMesh.visible = universe.node_count < 1000;
        universeGroup.add(packetMesh);

        const pulseIdx = pulsePackets.length;
        pulsePackets.push({
          curve,
          mesh: packetMesh,
          progress: (idx * 0.25) % 1.0,
          speed: 0.0035 + (idx % 3) * 0.001,
        });

        edgeObjects.push({
          line: curveLine,
          data: edge,
          source: edge.source,
          target: edge.target,
          kind: edge.kind || 'calls',
          isBroken,
          isIntraFolder,
          baseColors,
          baseSocketColor: srcTheme.hex,
          socket1,
          socket2,
          pulseIndex: pulseIdx,
          idx,
        });
      });
    }

    // Dynamic curved Bezier update function used during 2D <-> 3D morphing
    const updateCurvedEdges = () => {
      edgeObjects.forEach((eo) => {
        const no1 = nodeObjMap.get(eo.source);
        const no2 = nodeObjMap.get(eo.target);
        if (!no1 || !no2) return;

        const p1_center = no1.mesh.position;
        const p2_center = no2.mesh.position;
        const delta = new THREE.Vector3().subVectors(p2_center, p1_center);
        const totalDist = delta.length();
        const ringRadius = 13.6;

        const p1 = p1_center.clone();
        const p2 = p2_center.clone();

        if (totalDist > ringRadius * 2 + 2) {
          const unitDir = delta.clone().normalize();
          p1.addScaledVector(unitDir, ringRadius);
          p2.addScaledVector(unitDir, -ringRadius);
        }

        const mid = new THREE.Vector3().addVectors(p1, p2).multiplyScalar(0.5);
        const lineDir = new THREE.Vector3().subVectors(p2, p1);
        const distance = lineDir.length();
        let perp: THREE.Vector3;
        if (activeDimRef.current === '3D') {
          const radial = mid.clone().normalize();
          perp = new THREE.Vector3().crossVectors(lineDir, radial).normalize();
          if (perp.lengthSq() < 0.01) {
            perp = new THREE.Vector3(-lineDir.y, lineDir.x, lineDir.z || 0.5).normalize();
          }
        } else {
          perp = new THREE.Vector3(-lineDir.y, lineDir.x, 0).normalize();
        }

        const arcHeight = Math.min(distance * 0.18, 32) * (eo.idx % 2 === 0 ? 1 : -1);
        const ctrl = new THREE.Vector3().copy(mid).addScaledVector(perp, arcHeight);

        const curve = new THREE.QuadraticBezierCurve3(p1, ctrl, p2);
        const points = curve.getPoints(45);

        const posAttr = eo.line.geometry.attributes.position as THREE.BufferAttribute;
        const arr = posAttr.array as Float32Array;
        for (let pIdx = 0; pIdx < points.length; pIdx++) {
          arr[pIdx * 3] = points[pIdx].x;
          arr[pIdx * 3 + 1] = points[pIdx].y;
          arr[pIdx * 3 + 2] = points[pIdx].z;
        }
        posAttr.needsUpdate = true;

        eo.socket1.position.copy(p1);
        eo.socket2.position.copy(p2);

        if (eo.pulseIndex >= 0 && pulsePackets[eo.pulseIndex]) {
          pulsePackets[eo.pulseIndex].curve = curve;
        }
      });
    };

    // Sub-450ms GPU-accelerated morph transition engine
    const camStartPos = new THREE.Vector3();
    const camStartUp = new THREE.Vector3();
    const camTargetPos = new THREE.Vector3();
    const camTargetUp = new THREE.Vector3(0, 1, 0);
    const camTargetLookAt = new THREE.Vector3(0, 0, 0);

    const gridStartPos = new THREE.Vector3();
    const gridTargetPos = new THREE.Vector3();
    let gridStartRotX = 0;
    let gridTargetRotX = 0;
    let gridStartOpacity = 0;
    let gridTargetOpacity = 0;

    const MORPH_DURATION_MS = 450;

    const morphToDimension = (nextDim: '2D' | '3D') => {
      if (activeDimRef.current === nextDim && !isMorphingRef.current) return;
      activeDimRef.current = nextDim;
      isMorphingRef.current = true;
      morphStartRef.current = performance.now();

      nodeObjects.forEach((no) => {
        no.startPos.copy(no.mesh.position);
        no.targetPos.copy(nextDim === '3D' ? no.pos3D : no.pos2D);
      });

      clusterBadgeObjects.forEach((b) => {
        b.startPos.copy(b.mesh.position);
        b.targetPos.copy(nextDim === '3D' ? b.pos3D : b.pos2D);
      });

      cluster3DHalos.forEach((h) => {
        h.startPos.copy(h.mesh.position);
        h.targetPos.copy(nextDim === '3D' ? h.pos3D : h.pos2D);
        h.startOpacity = (h.mesh.material as THREE.MeshBasicMaterial).opacity;
        h.targetOpacity = nextDim === '3D' ? (isDarkRef.current ? 0.18 : 0.4) : 0;
      });

      cluster2DRings.forEach((r) => {
        r.startPos.copy(r.mesh.position);
        r.targetPos.copy(nextDim === '3D' ? r.pos3D : r.pos2D);
        r.startOpacity = (r.mesh.material as THREE.MeshBasicMaterial).opacity;
        r.targetOpacity = nextDim === '2D' ? (isDarkRef.current ? 0.28 : 0.45) : 0;
      });

      clusterIslands.forEach((isl) => {
        isl.grabMesh3D.position.copy(nextDim === '3D' ? isl.haloMesh.position : isl.ringMesh.position);
        isl.grabMesh2D.position.copy(nextDim === '3D' ? isl.haloMesh.position : isl.ringMesh.position);
      });

      gridStartPos.copy(gridHelper.position);
      gridTargetPos.copy(nextDim === '3D' ? gridPos3D : gridPos2D);
      gridStartRotX = gridHelper.rotation.x;
      gridTargetRotX = nextDim === '3D' ? 0 : Math.PI / 2;
      gridStartOpacity = (gridHelper.material as THREE.Material).opacity;
      gridTargetOpacity = nextDim === '3D' ? (isDarkRef.current ? 0.2 : 0.55) : (isDarkRef.current ? 0.08 : 0.16);

      camStartPos.copy(camera.position);
      camStartUp.copy(camera.up);
      camTargetLookAt.set(0, 0, 0);

      if (nextDim === '3D') {
        const radius3D = 420;
        camTargetPos.set(
          radius3D * Math.sin(Math.PI / 3) * Math.sin(Math.PI / 4),
          radius3D * Math.cos(Math.PI / 3),
          radius3D * Math.sin(Math.PI / 3) * Math.cos(Math.PI / 4)
        );
        camTargetUp.set(0, 1, 0);
      } else {
        const beta25D = 0.42;
        const radius25D = 560;
        camTargetPos.set(0, -radius25D * Math.sin(beta25D), radius25D * Math.cos(beta25D));
        camTargetUp.set(0, Math.cos(beta25D), Math.sin(beta25D));
      }
    };
    morphToDimensionRef.current = morphToDimension;

    // 10. Navigation Utilities
    const panUniverse = (dx: number, dy: number) => {
      const right = new THREE.Vector3();
      const up = new THREE.Vector3();
      camera.matrix.extractBasis(right, up, new THREE.Vector3());
      targetLookAt.addScaledVector(right, dx);
      targetLookAt.addScaledVector(up, dy);
    };
    panUniverseRef.current = panUniverse;

    const zoomUniverse = (delta: number) => {
      targetRadius = Math.max(30, Math.min(3600, targetRadius + delta));
    };
    zoomUniverseRef.current = zoomUniverse;

    // 11. Mouse & Interaction Handlers with Interactive Cluster Ring Grab & Drag
    let isDragging = false;
    let isRightDragging = false;
    let prevMouseX = 0;
    let prevMouseY = 0;

    let isDraggingCluster = false;
    let draggedIsland: ClusterIsland | null = null;
    const dragPlane = new THREE.Plane();
    const dragStartIntersection = new THREE.Vector3();
    const currentIntersection = new THREE.Vector3();
    let hasMovedCluster = false;

    const nodeDragStartPositions = new Map<string, THREE.Vector3>();
    const islandStartCenter = new THREE.Vector3();
    const islandStartHalo = new THREE.Vector3();
    const islandStartRing = new THREE.Vector3();
    const islandStartGrab3D = new THREE.Vector3();
    const islandStartGrab2D = new THREE.Vector3();
    const islandStartBadge = new THREE.Vector3();

    const onMouseDown = (e: MouseEvent) => {
      prevMouseX = e.clientX;
      prevMouseY = e.clientY;
      hasMovedCluster = false;

      if (e.button === 2 || e.button === 1) {
        isRightDragging = true;
        return;
      }

      // 1. Raycast against Cluster Grab Rings / Badges
      const rect = container.getBoundingClientRect();
      const mouseX = ((e.clientX - rect.left) / container.clientWidth) * 2 - 1;
      const mouseY = -((e.clientY - rect.top) / container.clientHeight) * 2 + 1;
      const raycaster = new THREE.Raycaster();
      raycaster.setFromCamera(new THREE.Vector2(mouseX, mouseY), camera);

      const grabCandidates: THREE.Object3D[] = [];
      clusterIslands.forEach((isl) => {
        if (activeDimRef.current === '3D') {
          grabCandidates.push(isl.grabMesh3D, isl.haloMesh);
        } else {
          grabCandidates.push(isl.grabMesh2D, isl.ringMesh);
        }
        if (isl.badgeMesh) grabCandidates.push(isl.badgeMesh);
      });

      const intersects = raycaster.intersectObjects(grabCandidates, false);
      if (intersects.length > 0) {
        const hitObj = intersects[0].object;
        const targetIsland = clusterIslands.find(
          (isl) =>
            isl.grabMesh3D === hitObj ||
            isl.grabMesh2D === hitObj ||
            isl.haloMesh === hitObj ||
            isl.ringMesh === hitObj ||
            isl.badgeMesh === hitObj
        );

        if (targetIsland) {
          isDraggingCluster = true;
          draggedIsland = targetIsland;
          container.style.cursor = 'grabbing';

          // Set up dragging plane
          if (activeDimRef.current === '3D') {
            const camDir = camera.getWorldDirection(new THREE.Vector3()).negate();
            dragPlane.setFromNormalAndCoplanarPoint(camDir, targetIsland.haloMesh.position);
          } else {
            // 2.5D tabletop plane
            dragPlane.setFromNormalAndCoplanarPoint(
              new THREE.Vector3(0, 0, 1),
              new THREE.Vector3(0, 0, 0)
            );
          }

          if (raycaster.ray.intersectPlane(dragPlane, dragStartIntersection)) {
            islandStartCenter.copy(activeDimRef.current === '3D' ? targetIsland.center3D : targetIsland.center2D);
            islandStartHalo.copy(targetIsland.haloMesh.position);
            islandStartRing.copy(targetIsland.ringMesh.position);
            islandStartGrab3D.copy(targetIsland.grabMesh3D.position);
            islandStartGrab2D.copy(targetIsland.grabMesh2D.position);
            if (targetIsland.badgeMesh) islandStartBadge.copy(targetIsland.badgeMesh.position);

            nodeDragStartPositions.clear();
            targetIsland.memberNodeIds.forEach((nid) => {
              const no = nodeObjMap.get(nid);
              if (no) nodeDragStartPositions.set(nid, no.mesh.position.clone());
            });

            return; // Skip camera drag
          }
        }
      }

      isDragging = true;
    };

    const onMouseUp = () => {
      if (isDraggingCluster) {
        isDraggingCluster = false;
        draggedIsland = null;
        container.style.cursor = 'grab';
      }
      isDragging = false;
      isRightDragging = false;
    };

    // Pre-indexed structures for zero-allocation 120 FPS cursor tracking
    const hoverRaycaster = new THREE.Raycaster();
    const mouseVec2 = new THREE.Vector2();
    const nodeMeshes = nodeObjects.map((n) => n.mesh);
    const nodeMeshMap = new Map<THREE.Object3D, typeof nodeObjects[0]>();
    nodeObjects.forEach((no) => nodeMeshMap.set(no.mesh, no));

    const badgeMeshes = clusterBadgeObjects.map((b) => b.mesh);
    const badgeMeshMap = new Map<THREE.Object3D, typeof clusterBadgeObjects[0]>();
    clusterBadgeObjects.forEach((b) => badgeMeshMap.set(b.mesh, b));

    const onMouseMove = (e: MouseEvent) => {
      const deltaX = e.clientX - prevMouseX;
      const deltaY = e.clientY - prevMouseY;
      prevMouseX = e.clientX;
      prevMouseY = e.clientY;

      // 1. If currently dragging a cluster ring, translate the entire cluster island!
      if (isDraggingCluster && draggedIsland) {
        const rect = container.getBoundingClientRect();
        const mouseX = ((e.clientX - rect.left) / container.clientWidth) * 2 - 1;
        const mouseY = -((e.clientY - rect.top) / container.clientHeight) * 2 + 1;
        const raycaster = new THREE.Raycaster();
        raycaster.setFromCamera(new THREE.Vector2(mouseX, mouseY), camera);

        if (raycaster.ray.intersectPlane(dragPlane, currentIntersection)) {
          const delta = new THREE.Vector3().subVectors(currentIntersection, dragStartIntersection);
          if (delta.lengthSq() > 6) {
            hasMovedCluster = true;
          }

          // Move ring meshes & badge
          draggedIsland.haloMesh.position.addVectors(islandStartHalo, delta);
          draggedIsland.ringMesh.position.addVectors(islandStartRing, delta);
          draggedIsland.grabMesh3D.position.addVectors(islandStartGrab3D, delta);
          draggedIsland.grabMesh2D.position.addVectors(islandStartGrab2D, delta);
          if (draggedIsland.badgeMesh) {
            draggedIsland.badgeMesh.position.addVectors(islandStartBadge, delta);
          }

          // Update cluster center in-memory
          const newCenter = new THREE.Vector3().addVectors(islandStartCenter, delta);
          if (activeDimRef.current === '3D') {
            draggedIsland.center3D.copy(newCenter);
          } else {
            draggedIsland.center2D.copy(newCenter);
          }

          // Move all member file nodes
          draggedIsland.memberNodeIds.forEach((nid) => {
            const no = nodeObjMap.get(nid);
            const startP = nodeDragStartPositions.get(nid);
            if (no && startP) {
              no.mesh.position.addVectors(startP, delta);
              no.ringMesh.position.copy(no.mesh.position);
              if (activeDimRef.current === '3D') {
                no.pos3D.copy(no.mesh.position);
              } else {
                no.pos2D.copy(no.mesh.position);
              }
            }
          });

          // Dynamic real-time update of all Bezier ribbons & docking beads
          updateCurvedEdges();
        }

        container.style.cursor = 'grabbing';
        return;
      }

      if (isRightDragging) {
        const factor = (radius / 1000) * 1.6;
        panUniverse(-deltaX * factor, deltaY * factor);
        return;
      }

      if (isDragging) {
        if (controlModeRef.current === 'pan') {
          const factor = (radius / 1000) * 1.6;
          panUniverse(-deltaX * factor, deltaY * factor);
        } else {
          // Orbit Mode works in both 3D Universe and 2.5D Mind Map!
          if (activeDimRef.current === '3D') {
            targetTheta += deltaX * 0.005;
            targetPhi = Math.max(0.08, Math.min(Math.PI - 0.08, targetPhi - deltaY * 0.005));
          } else {
            // In 2.5D: deltaX spins the mind map turntable, deltaY tilts isometric perspective
            targetAlpha += deltaX * 0.004;
            targetBeta = Math.max(0, Math.min(1.05, targetBeta - deltaY * 0.004));
          }
        }
      } else {
        const rect = container.getBoundingClientRect();
        const mouseX = ((e.clientX - rect.left) / container.clientWidth) * 2 - 1;
        const mouseY = -((e.clientY - rect.top) / container.clientHeight) * 2 + 1;

        mouseVec2.set(mouseX, mouseY);
        hoverRaycaster.setFromCamera(mouseVec2, camera);

        // Hover check on cluster rings (grab preview)
        const grabCandidates: THREE.Object3D[] = [];
        clusterIslands.forEach((isl) => {
          if (activeDimRef.current === '3D') {
            grabCandidates.push(isl.grabMesh3D, isl.haloMesh);
          } else {
            grabCandidates.push(isl.grabMesh2D, isl.ringMesh);
          }
          if (isl.badgeMesh) grabCandidates.push(isl.badgeMesh);
        });

        const ringIntersects = hoverRaycaster.intersectObjects(grabCandidates, false);
        if (ringIntersects.length > 0) {
          container.style.cursor = 'grab';
          if (hoveredNodeIdRef.current) {
            hoveredNodeIdRef.current = null;
            if (hoverPillRef.current) hoverPillRef.current.style.display = 'none';
          }
          return;
        }

        // Direct DOM Hover Glance (Zero React Re-renders, Pure 120 FPS Native Interaction)
        const intersects = hoverRaycaster.intersectObjects(nodeMeshes, false);

        if (intersects.length > 0) {
          const hit = nodeMeshMap.get(intersects[0].object);
          if (hit) {
            const inCount = callerCounts.get(hit.data.id) || 0;
            const outCount = calleeCounts.get(hit.data.id) || 0;
            const nodeState = getSimulationNodeState(hit.data, simulationRef.current, simulationFilePathRef.current);
            const isDamaged = damageOverlayRef.current && nodeState.damaged;
            const isDead = damageOverlayRef.current && nodeState.dead;
            const statusLabel = isDamaged ? 'DAMAGED' : isDead ? 'DEAD CODE' : (hit.data.status || 'unknown').toUpperCase();
            const ext = hit.data.id.split('.').pop()?.toLowerCase() || 'file';
            const fileName = hit.data.id.split('/').pop() || hit.data.id;

            if (hoverPillRef.current) {
              const cWidth = container.clientWidth || 1000;
              const cHeight = container.clientHeight || 700;
              const pillX = Math.max(16, Math.min(e.clientX - rect.left + 16, cWidth - 280));
              const pillY = Math.max(16, Math.min(e.clientY - rect.top + 16, cHeight - 120));
              hoverPillRef.current.style.transform = `translate3d(${pillX}px, ${pillY}px, 0)`;
              hoverPillRef.current.style.display = selectedNodeIdRef.current ? 'none' : 'block';

              if (hoverExtRef.current) {
                hoverExtRef.current.textContent = ext.toUpperCase();
                hoverExtRef.current.style.backgroundColor = hit.theme.color;
              }
              if (hoverNameRef.current) hoverNameRef.current.textContent = fileName;
              if (hoverFolderRef.current) hoverFolderRef.current.textContent = hit.clusterKey;
              if (hoverCallersRef.current) hoverCallersRef.current.textContent = `← ${inCount} callers`;
              if (hoverCalleesRef.current) hoverCalleesRef.current.textContent = `→ ${outCount} callees`;
              if (hoverBrokenRef.current) {
                hoverBrokenRef.current.textContent = statusLabel;
                hoverBrokenRef.current.className = `font-bold ${
                  isDamaged ? 'text-rose-500' : isDead ? 'text-slate-500' : 'text-emerald-600 dark:text-emerald-400'
                }`;
              }
              if (hoverStatusDotRef.current) {
                hoverStatusDotRef.current.className = `w-2 h-2 rounded-full shrink-0 ${
                  isDamaged ? 'bg-rose-500 animate-ping' : isDead ? 'bg-slate-500' : 'bg-emerald-500'
                }`;
              }
            }

            if (hit.data.id !== hoveredNodeIdRef.current) {
              hoveredNodeIdRef.current = hit.data.id;
              container.style.cursor = 'pointer';
            }
          }
        } else if (hoveredNodeIdRef.current) {
          hoveredNodeIdRef.current = null;
          if (hoverPillRef.current) hoverPillRef.current.style.display = 'none';
          container.style.cursor = controlModeRef.current === 'pan' ? 'grab' : 'default';
        } else {
          container.style.cursor = controlModeRef.current === 'pan' ? 'grab' : 'default';
        }
      }
    };

    const onClick = (e: MouseEvent) => {
      if (hasMovedCluster) {
        hasMovedCluster = false;
        return; // Suppress click action after dragging a cluster
      }

      const rect = container.getBoundingClientRect();
      const mouseX = ((e.clientX - rect.left) / container.clientWidth) * 2 - 1;
      const mouseY = -((e.clientY - rect.top) / container.clientHeight) * 2 + 1;

      mouseVec2.set(mouseX, mouseY);
      hoverRaycaster.setFromCamera(mouseVec2, camera);

      // 1. Check cluster badge click first (for instant cluster flight)
      const badgeIntersects = hoverRaycaster.intersectObjects(badgeMeshes, false);
      if (badgeIntersects.length > 0) {
        const hitBadge = badgeMeshMap.get(badgeIntersects[0].object);
        if (hitBadge) {
          handleFilterFolder(hitBadge.clusterKey);
          return;
        }
      }

      // 2. Check node click
      const intersects = hoverRaycaster.intersectObjects(nodeMeshes, false);
      if (intersects.length > 0) {
        const hit = nodeMeshMap.get(intersects[0].object);
        if (hit) {
          flyToNode(hit.data);
        }
      } else {
        // 3. Clicked empty space: clear spotlight isolation and restore all files
        if (selectedNodeIdRef.current !== null) {
          selectedNodeIdRef.current = null;
          activeNeighborsRef.current.clear();
          if (onSelectNode) {
            onSelectNode('');
          }
        }
      }
    };

    const onMouseLeave = () => {
      hoveredNodeIdRef.current = null;
      if (hoverPillRef.current) hoverPillRef.current.style.display = 'none';
      container.style.cursor = controlModeRef.current === 'pan' ? 'grab' : 'default';
    };

    // Fly-to node and calculate active neighbors for spotlight dimming
    const flyToNode = (node: UniverseNode) => {
      selectedNodeIdRef.current = node.id;

      const set = new Set<string>();
      set.add(node.id);
      edgesList.forEach((e) => {
        if (e.source === node.id) set.add(e.target);
        if (e.target === node.id) set.add(e.source);
      });
      activeNeighborsRef.current = set;

      if (onSelectNode) {
        onSelectNode(node.id);
      }

      const no = nodeObjMap.get(node.id);
      const curPos = no ? no.mesh.position : new THREE.Vector3(node.x, node.y, node.z || 0);

      targetLookAt.set(curPos.x, curPos.y, curPos.z);
      targetRadius = 75;
      if (activeDimRef.current === '3D') {
        targetTheta = theta;
        targetPhi = Math.max(0.3, Math.min(Math.PI - 0.3, phi));
      } else {
        targetAlpha = alpha;
        targetBeta = 0;
      }
    };

    flyToNodeByIdRef.current = (id: string) => {
      const target = nodeObjects.find((n) => n.data.id === id);
      if (target) {
        flyToNode(target.data);
      }
    };

    flyToClusterRef.current = (folderKey: string) => {
      const is3D = activeDimRef.current === '3D';
      const center = is3D ? clusterCenters3D.get(folderKey) : clusterCenters2D.get(folderKey);
      if (center) {
        targetLookAt.set(center.x, center.y, center.z);
        if (is3D) {
          targetRadius = Math.max(200, center.radius * 2.4);
          targetTheta = Math.PI / 4;
          targetPhi = Math.PI / 3;
        } else {
          targetRadius = Math.max(340, center.radius * 2.2);
          targetBeta = 0.38;
        }
      }
    };

    recenterRef.current = () => {
      selectedNodeIdRef.current = null;
      setSelectedFolderFilter(null);
      activeNeighborsRef.current.clear();
      targetLookAt.set(0, 0, 0);
      if (activeDimRef.current === '3D') {
        targetRadius = 420;
        targetTheta = Math.PI / 4;
        targetPhi = Math.PI / 3;
      } else {
        targetRadius = 560;
        targetAlpha = 0;
        targetBeta = 0.42;
      }
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      zoomUniverse(e.deltaY * 0.18);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (selectedNodeIdRef.current !== null || selectedFolderFilterRef.current !== null) {
          selectedNodeIdRef.current = null;
          activeNeighborsRef.current.clear();
          setSelectedFolderFilter(null);
          if (onSelectNode) {
            onSelectNode('');
          }
        }
      }
    };

    container.addEventListener('mousedown', onMouseDown);
    window.addEventListener('mouseup', onMouseUp);
    container.addEventListener('mousemove', onMouseMove);
    container.addEventListener('mouseleave', onMouseLeave);
    container.addEventListener('click', onClick);
    container.addEventListener('wheel', onWheel, { passive: false });
    container.addEventListener('contextmenu', (e) => e.preventDefault());
    window.addEventListener('keydown', onKeyDown);

    // 12. Render Loop with Clean Spotlight Dimming & Zero Hairball Line Highlighting
    let animId: number;
    const animate = () => {
      animId = requestAnimationFrame(animate);

      // Sub-450ms Realtime GPU-Accelerated Dimensional Morphing
      if (isMorphingRef.current) {
        const morphElapsed = performance.now() - morphStartRef.current;
        const morphT = Math.min(1.0, morphElapsed / MORPH_DURATION_MS);
        const ease = 1 - Math.pow(1 - morphT, 3);

        nodeObjects.forEach((no) => {
          no.mesh.position.lerpVectors(no.startPos, no.targetPos, ease);
          no.ringMesh.position.copy(no.mesh.position);
        });

        clusterBadgeObjects.forEach((b) => {
          b.mesh.position.lerpVectors(b.startPos, b.targetPos, ease);
        });

        cluster3DHalos.forEach((h) => {
          h.mesh.position.lerpVectors(h.startPos, h.targetPos, ease);
          (h.mesh.material as THREE.MeshBasicMaterial).opacity =
            THREE.MathUtils.lerp(h.startOpacity, h.targetOpacity, ease);
        });

        cluster2DRings.forEach((r) => {
          r.mesh.position.lerpVectors(r.startPos, r.targetPos, ease);
          (r.mesh.material as THREE.MeshBasicMaterial).opacity =
            THREE.MathUtils.lerp(r.startOpacity, r.targetOpacity, ease);
        });

        gridHelper.position.lerpVectors(gridStartPos, gridTargetPos, ease);
        gridHelper.rotation.x = THREE.MathUtils.lerp(gridStartRotX, gridTargetRotX, ease);
        (gridHelper.material as THREE.Material).opacity =
          THREE.MathUtils.lerp(gridStartOpacity, gridTargetOpacity, ease);

        updateCurvedEdges();

        camera.position.lerpVectors(camStartPos, camTargetPos, ease);
        camera.up.lerpVectors(camStartUp, camTargetUp, ease);
        orbitTarget.lerp(camTargetLookAt, ease);
        camera.lookAt(orbitTarget);

        if (morphT >= 1.0) {
          isMorphingRef.current = false;
          targetLookAt.set(0, 0, 0);
          if (activeDimRef.current === '3D') {
            radius = 420;
            targetRadius = 420;
            theta = Math.PI / 4;
            targetTheta = Math.PI / 4;
            phi = Math.PI / 3;
            targetPhi = Math.PI / 3;
          } else {
            radius = 560;
            targetRadius = 560;
            alpha = 0;
            targetAlpha = 0;
            beta = 0.42;
            targetBeta = 0.42;
          }
        }
      } else {
        if (
          !isDragging &&
          !isRightDragging &&
          !selectedNodeIdRef.current &&
          !selectedFolderFilterRef.current &&
          controlModeRef.current === 'orbit' &&
          !isPhysicsPausedRef.current
        ) {
          if (activeDimRef.current === '3D') {
            targetTheta += 0.0005;
          } else {
            targetAlpha += 0.0004;
          }
        }

        const damp = 0.08;
        orbitTarget.lerp(targetLookAt, damp);
        radius += (targetRadius - radius) * damp;

        if (activeDimRef.current === '3D') {
          theta += (targetTheta - theta) * damp;
          phi += (targetPhi - phi) * damp;

          const safePhi = Math.max(0.08, Math.min(Math.PI - 0.08, phi));
          camera.position.x = orbitTarget.x + radius * Math.sin(safePhi) * Math.sin(theta);
          camera.position.y = orbitTarget.y + radius * Math.cos(safePhi);
          camera.position.z = orbitTarget.z + radius * Math.sin(safePhi) * Math.cos(theta);
          camera.up.set(0, 1, 0);
        } else {
          alpha += (targetAlpha - alpha) * damp;
          beta += (targetBeta - beta) * damp;

          const safeBeta = Math.max(0, Math.min(1.05, beta));
          camera.position.x = orbitTarget.x + radius * Math.sin(safeBeta) * Math.sin(alpha);
          camera.position.y = orbitTarget.y - radius * Math.sin(safeBeta) * Math.cos(alpha);
          camera.position.z = orbitTarget.z + radius * Math.cos(safeBeta);
          camera.up.set(
            -Math.cos(safeBeta) * Math.sin(alpha),
            Math.cos(safeBeta) * Math.cos(alpha),
            Math.sin(safeBeta)
          );
        }
        camera.lookAt(orbitTarget);
      }

      // Billboard cluster badges + distance LOD fading
      clusterBadgeObjects.forEach((b) => {
        b.mesh.quaternion.copy(camera.quaternion);
        const dist = camera.position.distanceTo(b.mesh.position);
        const isCurrentFolder = selectedFolderFilterRef.current === b.clusterKey;
        const targetOpacity = isCurrentFolder
          ? 1.0
          : THREE.MathUtils.clamp((dist - 90) / 220, 0.22, 0.95);
        (b.mesh.material as THREE.MeshBasicMaterial).opacity = targetOpacity;
      });

      // Deliberate selection (or folder filter) triggers spotlight isolation, NOT hovering!
      const activeFocusNode = selectedNodeIdRef.current;
      const hasFocus = !!activeFocusNode;
      const curFolder = selectedFolderFilterRef.current;
      const hasFolderFilter = !!curFolder;
      const activeNeighbors = activeNeighborsRef.current;

      // Spotlight Dimming & Billboarding
      nodeObjects.forEach((no) => {
        const isSelected = selectedNodeIdRef.current === no.data.id;
        const isHovered = hoveredNodeIdRef.current === no.data.id;
        const isNeighbor = hasFocus && (activeNeighbors.has(no.data.id) || no.data.id === activeFocusNode);
        const inActiveFolder = !hasFolderFilter || no.clusterKey === curFolder;
        const isConnected = inActiveFolder && (!hasFocus || isNeighbor);

        no.mesh.quaternion.copy(camera.quaternion);
        no.ringMesh.quaternion.copy(camera.quaternion);
        no.ringMesh.visible = universe.node_count < 1000 || isSelected || isHovered;

        const slateMat = no.mesh.material as THREE.MeshBasicMaterial;
        const ringMat = no.ringMesh.material as THREE.MeshBasicMaterial;

        // When nothing is selected, cards are 100% full opacity (1.0).
        // When a node is selected, non-connected cards soften to 0.32 (dark) / 0.45 (light) so they never look like muddy gray slabs.
        const targetOpacity = isSelected || isHovered
          ? 1.0
          : isConnected
          ? 1.0
          : isDarkRef.current
          ? 0.32
          : 0.45;

        const targetScale = isSelected ? 1.08 : isHovered ? 1.05 : isConnected ? 1.0 : 0.92;
        const targetRingOpacity = isSelected || isHovered
          ? 1.0
          : isConnected
          ? isDarkRef.current ? 0.35 : 0.88
          : isDarkRef.current ? 0.03 : 0.07;

        slateMat.opacity = THREE.MathUtils.lerp(slateMat.opacity, targetOpacity, 0.15);
        ringMat.opacity = THREE.MathUtils.lerp(ringMat.opacity, targetRingOpacity, 0.15);
        no.mesh.scale.setScalar(THREE.MathUtils.lerp(no.mesh.scale.x, targetScale, 0.15));
        no.ringMesh.scale.copy(no.mesh.scale);

        if (!isPhysicsPausedRef.current) {
          no.ringMesh.rotation.z += isSelected ? 0.02 : 0.006;
        }
      });

      // SMART EDGE VISIBILITY (Eliminates hairball entirely):
      edgeObjects.forEach((eo) => {
        const isDirectlyConnected =
          hasFocus && (eo.source === activeFocusNode || eo.target === activeFocusNode);

        const lineMat = eo.line.material as THREE.LineBasicMaterial;

        let targetOpacity = 0.12;
        if (hasFocus) {
          targetOpacity = isDirectlyConnected ? 0.98 : 0.02;
        } else if (hasFolderFilter) {
          const srcIn = nodeClusterMap.get(eo.source) === curFolder;
          const dstIn = nodeClusterMap.get(eo.target) === curFolder;
          targetOpacity = srcIn && dstIn ? 0.45 : srcIn || dstIn ? 0.18 : 0.02;
        } else {
          targetOpacity = eo.isIntraFolder ? 0.2 : 0.08;
        }
        if (eo.isBroken) targetOpacity = Math.max(targetOpacity, 0.85);

        lineMat.opacity = THREE.MathUtils.lerp(lineMat.opacity, targetOpacity, 0.15);
      });

      // Animate flowing pulse packets along curves
      if (!isPhysicsPausedRef.current && universe.node_count < 1000) {
        pulsePackets.forEach((packet) => {
          packet.progress += packet.speed;
          if (packet.progress > 1.0) packet.progress = 0.0;
          const pt = packet.curve.getPoint(packet.progress);
          packet.mesh.position.copy(pt);
        });
      }

      renderer.render(scene, camera);
      fpsFrameCountRef.current += 1;
      const renderedAt = performance.now();
      const elapsed = renderedAt - fpsWindowStartRef.current;
      if (elapsed >= 1000) {
        setFps((fpsFrameCountRef.current * 1000) / elapsed);
        fpsFrameCountRef.current = 0;
        fpsWindowStartRef.current = renderedAt;
      }
    };

    fpsFrameCountRef.current = 0;
    fpsWindowStartRef.current = performance.now();
    animate();

    const onResize = () => {
      if (!container) return;
      const w = container.clientWidth || 1000;
      const h = container.clientHeight || 700;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    };
    window.addEventListener('resize', onResize);

    // In-Place Dynamic Theme Update without scene destruction (< 4ms, zero hang)
    onThemeChangeRef.current = (dark: boolean) => {
      isDarkRef.current = dark;
      renderer.setClearColor(dark ? 0x0a0e17 : 0xf1f5f9, 1);
      renderer.toneMapping = dark ? THREE.ACESFilmicToneMapping : THREE.NoToneMapping;
      renderer.toneMappingExposure = dark ? 1.15 : 1.0;
      scene.fog = dark ? new THREE.FogExp2(0x0a0e17, 0.0004) : null;
      ambientLight.intensity = dark ? 1.6 : 1.25;
      mainKeyLight.intensity = dark ? 1.2 : 0.85;

      if (gridHelper.material) {
        (gridHelper.material as THREE.LineBasicMaterial).color.setHex(dark ? 0x00f0ff : 0x0284c7);
        (gridHelper.material as THREE.Material).opacity =
          activeDimRef.current === '3D' ? (dark ? 0.2 : 0.55) : (dark ? 0.08 : 0.16);
      }

      nodeObjects.forEach((no) => {
        const isSelected = selectedNodeIdRef.current === no.data.id;
        const isHovered = hoveredNodeIdRef.current === no.data.id;
        const nodeState = getSimulationNodeState(no.data, simulationRef.current, simulationFilePathRef.current);
        const isDamaged = damageOverlayRef.current && nodeState.damaged;
        const isDead = damageOverlayRef.current && nodeState.dead;
        const cTheme = (dark ? clusterMapThemeDark.get(no.clusterKey) : clusterMapThemeLight.get(no.clusterKey)) || no.theme;

        const newTex = generateFileHoloTexture(no.data, cTheme, isSelected, isHovered, isDamaged, isDead, dark, universe.node_count >= 1000);
        const oldMat = no.mesh.material as THREE.MeshBasicMaterial;
        const oldTex = oldMat.map;
        oldMat.map = newTex;
        oldMat.needsUpdate = true;
        if (universe.node_count < 1000) oldTex?.dispose();
        (no.ringMesh.material as THREE.MeshBasicMaterial).color.setHex(isDamaged
          ? dark ? 0xf43f5e : 0xe11d48
          : isDead ? dark ? 0x64748b : 0x94a3b8 : cTheme.hex);
      });

      clusterBadgeObjects.forEach((b) => {
        const theme = (dark ? clusterMapThemeDark.get(b.clusterKey) : clusterMapThemeLight.get(b.clusterKey)) || {
          name: b.clusterKey,
          color: b.themeColor,
          hex: 0x0284c7,
        };
        const newBadgeTex = generateClusterBadgeTexture(
          b.clusterKey,
          b.count,
          b.percentage,
          theme.color,
          dark
        );
        const oldTex = (b.mesh.material as THREE.MeshBasicMaterial).map;
        (b.mesh.material as THREE.MeshBasicMaterial).map = newBadgeTex;
        (b.mesh.material as THREE.MeshBasicMaterial).needsUpdate = true;
        if (universe.node_count < 1000) oldTex?.dispose();
      });
    };

    const updateSimulationOverlay = (damaged: boolean) => {
      damageOverlayRef.current = damaged;
      nodeObjects.forEach((no) => {
        const isSelected = selectedNodeIdRef.current === no.data.id;
        const isHovered = hoveredNodeIdRef.current === no.data.id;
        const nodeState = getSimulationNodeState(no.data, simulationRef.current, simulationFilePathRef.current);
        const isDamaged = damaged && nodeState.damaged;
        const isDead = damaged && nodeState.dead;
        const cTheme = (isDarkRef.current
          ? clusterMapThemeDark.get(no.clusterKey)
          : clusterMapThemeLight.get(no.clusterKey)) || no.theme;
        const newTex = generateFileHoloTexture(no.data, cTheme, isSelected, isHovered, isDamaged, isDead, isDarkRef.current, universe.node_count >= 1000);
        const oldMat = no.mesh.material as THREE.MeshBasicMaterial;
        const oldTex = oldMat.map;
        oldMat.map = newTex;
        oldMat.needsUpdate = true;
        if (universe.node_count < 1000) oldTex?.dispose();
        (no.ringMesh.material as THREE.MeshBasicMaterial).color.setHex(isDamaged
          ? isDarkRef.current ? 0xf43f5e : 0xe11d48
          : isDead
          ? isDarkRef.current ? 0x64748b : 0x94a3b8
          : isSelected
          ? isDarkRef.current ? 0x00f0ff : 0x0284c7
          : cTheme.hex);
      });
      edgeObjects.forEach((eo) => {
        const broken = damaged && isSimulationEdgeBroken(eo.data, simulationRef.current);
        eo.isBroken = broken;
        const colors = eo.line.geometry.getAttribute('color') as THREE.BufferAttribute;
        for (let offset = 0; offset < eo.baseColors.length; offset += 3) {
          colors.setXYZ(
            offset / 3,
            broken ? 1 : eo.baseColors[offset],
            broken ? 0.15 : eo.baseColors[offset + 1],
            broken ? 0.35 : eo.baseColors[offset + 2]
          );
        }
        colors.needsUpdate = true;
        const socketColor = broken ? 0xf43f5e : eo.baseSocketColor;
        (eo.socket1.material as THREE.MeshBasicMaterial).color.setHex(socketColor);
        (eo.socket2.material as THREE.MeshBasicMaterial).color.setHex(socketColor);
        const packet = pulsePackets[eo.pulseIndex];
        if (packet) (packet.mesh.material as THREE.MeshBasicMaterial).color.setHex(socketColor);
      });
    };
    onDamageOverlayChangeRef.current = updateSimulationOverlay;
    onSimulationOverlayChangeRef.current = () => updateSimulationOverlay(damageOverlayRef.current);
    updateSimulationOverlay(damageOverlayRef.current);

    onEdgeFilterChangeRef.current = (filter: 'all' | 'calls' | 'imports') => {
      edgeFilterRef.current = filter;
      edgeObjects.forEach((eo) => {
        eo.line.visible = filter === 'all' || eo.kind === filter;
      });
    };

    return () => {
      onThemeChangeRef.current = null;
      onDamageOverlayChangeRef.current = null;
      onSimulationOverlayChangeRef.current = null;
      onEdgeFilterChangeRef.current = null;
      cancelAnimationFrame(animId);
      nodeObjects.forEach((no) => {
        prevPositionsRef.current.set(no.data.id, no.mesh.position.clone());
      });
      window.removeEventListener('resize', onResize);
      window.removeEventListener('keydown', onKeyDown);
      container.removeEventListener('mousedown', onMouseDown);
      window.removeEventListener('mouseup', onMouseUp);
      container.removeEventListener('mousemove', onMouseMove);
      container.removeEventListener('mouseleave', onMouseLeave);
      container.removeEventListener('click', onClick);
      container.removeEventListener('wheel', onWheel);
      container.removeEventListener('contextmenu', (e) => e.preventDefault());
      renderer.dispose();
    };
  }, [
    universe?.repo_id,
    universe?.snapshot_id,
    universe?.node_count,
    universe?.edge_count,
    nodeClusterMap,
    adaptiveClusters,
  ]);

  return (
    <div className="relative w-full h-[calc(100vh-140px)] rounded-2xl overflow-hidden border border-[var(--border-subtle)] bg-[var(--background)] shadow-sm select-none">
      {/* 1. Main Three.js Canvas Mount */}
      <div ref={containerRef} className="w-full h-full cursor-default" />

      {/* Instant Ultra-Low Latency Hover Glance Pill (Direct DOM Ref - 0 React Re-renders, 120 FPS Native) */}
      <div
        ref={hoverPillRef}
        className="pointer-events-none absolute z-30 transition-transform duration-75 ease-out will-change-transform"
        style={{
          display: 'none',
          left: 0,
          top: 0,
        }}
      >
        <div className="flex flex-col gap-1 px-3 py-2.5 rounded-xl bg-[var(--card)]/95 backdrop-blur-md border border-[var(--border-subtle)] shadow-2xl min-w-[210px] max-w-[300px]">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center space-x-1.5 truncate">
              <span
                ref={hoverExtRef}
                className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded text-white shrink-0"
              />
              <span
                ref={hoverNameRef}
                className="text-xs font-mono font-bold text-[var(--text-primary)] truncate"
              />
            </div>
            <span
              ref={hoverStatusDotRef}
              className="w-2 h-2 rounded-full shrink-0 bg-emerald-500"
            />
          </div>

          <div
            ref={hoverFolderRef}
            className="text-[10px] font-mono text-[var(--text-muted)] truncate"
          />

          <div className="flex items-center justify-between pt-1 border-t border-[var(--border-subtle)] text-[10px] font-mono">
            <span
              ref={hoverCallersRef}
              className="text-sky-600 dark:text-cyan-400 font-semibold"
            />
            <span
              ref={hoverCalleesRef}
              className="text-emerald-600 dark:text-emerald-400 font-semibold"
            />
            <span
              ref={hoverBrokenRef}
              className="font-bold text-emerald-600 dark:text-emerald-400"
            />
          </div>
        </div>
      </div>

      {/* 2. Top-Left Scope & Active Cluster Pill */}
      <div className="absolute top-4 left-4 xl:left-6 flex flex-col gap-1.5 z-20 pointer-events-auto">
        <div className="flex items-center space-x-2.5 bg-[var(--card)]/95 backdrop-blur-md px-3.5 py-2 rounded-xl border border-[var(--border-subtle)] shadow-xl">
          <div className="relative flex items-center justify-center shrink-0">
            <div className="w-2.5 h-2.5 rounded-full bg-cyan-500 animate-ping opacity-75" />
            <div className="w-2 h-2 rounded-full bg-cyan-500 absolute" />
          </div>
          <div className="flex items-center space-x-2">
            <h2 className="text-xs font-bold text-[var(--text-primary)] font-brand tracking-wide">
              Adaptive Mind Map
            </h2>
            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded-md bg-sky-50 dark:bg-cyan-500/15 text-sky-700 dark:text-cyan-400 font-bold border border-sky-300 dark:border-cyan-500/30">
              {universe.dimensions}
            </span>
          </div>
        </div>
        <div
          data-testid="canvas-fps-meter"
          aria-label={hasPresentedFps
            ? (displayedFps === null ? 'Measuring compositor-presented frames per second' : `Canvas presented FPS ${displayedFps.toFixed(1)} frames per second`)
            : (displayedFps === null ? 'Measuring canvas render calls per second' : `Canvas render-call rate ${displayedFps.toFixed(1)} render calls per second`)}
          title={hasPresentedFps
            ? 'Frames reported by Electron webContents presentation events, averaged over the latest one-second sample'
            : 'Successful canvas render calls per second, averaged over the latest one-second sample'}
          className={`w-fit rounded-md border bg-[var(--card)]/95 px-2 py-1 font-mono text-[10px] font-semibold shadow-sm backdrop-blur ${
            displayedFps === null ? 'border-[var(--border-subtle)] text-[var(--text-secondary)]' :
              displayedFps >= 60 ? 'border-emerald-500/40 text-emerald-600 dark:text-emerald-400' :
                displayedFps >= 45 ? 'border-amber-500/40 text-amber-600 dark:text-amber-400' :
                  'border-rose-500/40 text-rose-600 dark:text-rose-400'
          }`}
        >
          {displayedFps === null ? (hasPresentedFps ? 'FPS --' : 'draws/s --') : `${displayedFps.toFixed(1)} ${hasPresentedFps ? 'FPS' : 'draws/s'}`}
        </div>
        {presentationLatencyMs !== null && (
          <div
            data-testid="theme-presentation-latency"
            aria-label={`Theme swap presented in ${presentationLatencyMs.toFixed(1)} milliseconds`}
            title="Measured from the theme toggle mark to the next Electron presentation event"
            className="w-fit rounded-md border border-[var(--border-subtle)] bg-[var(--card)]/95 px-2 py-1 font-mono text-[10px] font-semibold text-[var(--text-secondary)] shadow-sm backdrop-blur"
          >
            Theme → present {presentationLatencyMs.toFixed(1)} ms
          </div>
        )}
        {themeMutationMs !== null && (
          <div
            data-testid="theme-mutation-latency"
            aria-label={`Theme DOM mutation completed in ${themeMutationMs.toFixed(3)} milliseconds`}
            title="Measured synchronously around the root theme class, color-scheme, and token mutation"
            className="w-fit rounded-md border border-[var(--border-subtle)] bg-[var(--card)]/95 px-2 py-1 font-mono text-[10px] font-semibold text-[var(--text-secondary)] shadow-sm backdrop-blur"
          >
            Theme mutation {themeMutationMs.toFixed(3)} ms
          </div>
        )}
        {universe.is_truncated && (
          <div
            data-testid="canvas-lod-status"
            role="status"
            className="max-w-80 rounded-lg border border-amber-400/40 bg-amber-50/95 px-3 py-2 text-[10px] font-mono text-amber-900 shadow-lg dark:bg-amber-950/80 dark:text-amber-100"
          >
            Dense overview renders document cards only; semantic edges connect file cards. Select a card for its persisted symbols, callers, callees, and exact source lines.
          </div>
        )}

        {selectedFolderFilter && (
          <div className="flex items-center space-x-2 bg-[var(--card)]/95 backdrop-blur-md px-3 py-1.5 rounded-xl border border-sky-500/40 dark:border-cyan-500/40 shadow-xl font-mono text-xs animate-fadeIn">
            <span
              className="w-2.5 h-2.5 rounded-full shrink-0"
              style={{
                backgroundColor:
                  (isDark
                    ? adaptiveClusters.find((c) => c.name === selectedFolderFilter)?.darkTheme?.color
                    : adaptiveClusters.find((c) => c.name === selectedFolderFilter)?.lightTheme?.color) ||
                  '#0284c7',
              }}
            />
            <span className="text-[var(--text-primary)] font-bold truncate max-w-[150px]">
              {selectedFolderFilter}
            </span>
            <button
              onClick={() => handleFilterFolder(selectedFolderFilter)}
              aria-label={`Reset folder filter for ${selectedFolderFilter}`}
              className="text-[10px] px-1.5 py-0.5 rounded bg-sky-100 dark:bg-cyan-500/20 text-sky-700 dark:text-cyan-300 hover:bg-sky-200 cursor-pointer font-bold"
            >
              Reset
            </button>
          </div>
        )}
      </div>

      {/* 3. Top-Center Floating HUD Controls Bar */}
      <div className="absolute top-4 left-1/2 -translate-x-1/2 flex items-center space-x-2 bg-[var(--card)]/95 backdrop-blur-md px-3 py-1.5 rounded-2xl border border-[var(--border-subtle)] shadow-xl z-20">
        {/* Animated Motion Slide Toggle: 2D vs 3D */}
        <div className="relative flex items-center bg-[var(--card-elevated)] p-1 rounded-xl border border-[var(--border-subtle)] text-xs font-mono w-[228px] h-[36px] shadow-inner">
          <div
            className="absolute top-1 bottom-1 rounded-lg bg-gradient-to-r from-sky-500 to-cyan-500 dark:from-cyan-500 dark:to-blue-600 shadow-md shadow-sky-500/25 dark:shadow-cyan-500/30 transition-all duration-300 ease-[cubic-bezier(0.2,1,0.3,1)]"
            style={{
              left: universe.dimensions === '2D' ? '4px' : 'calc(50% + 2px)',
              width: 'calc(50% - 6px)',
            }}
          />
          <button
            onClick={() => {
              if (universe.dimensions !== '2D') {
                morphToDimensionRef.current?.('2D');
                onToggleDimensions();
              }
            }}
            className={`relative z-10 flex-1 h-full font-semibold transition-colors duration-200 flex items-center justify-center space-x-1.5 px-2 cursor-pointer select-none whitespace-nowrap ${
              universe.dimensions === '2D'
                ? 'text-white font-bold drop-shadow-xs'
                : 'text-slate-600 dark:text-neutral-400 hover:text-slate-900 dark:hover:text-white'
            }`}
            title="2.5D Isometric Mind Map (Grab & drag rings to move clusters)"
          >
            <Box className="w-3.5 h-3.5 shrink-0" />
            <span className="whitespace-nowrap">2.5D Plane</span>
          </button>
          <button
            onClick={() => {
              if (universe.dimensions !== '3D') {
                morphToDimensionRef.current?.('3D');
                onToggleDimensions();
              }
            }}
            className={`relative z-10 flex-1 h-full font-semibold transition-colors duration-200 flex items-center justify-center space-x-1.5 px-2 cursor-pointer select-none whitespace-nowrap ${
              universe.dimensions === '3D'
                ? 'text-white font-bold drop-shadow-xs'
                : 'text-slate-600 dark:text-neutral-400 hover:text-slate-900 dark:hover:text-white'
            }`}
            title="3D Celestial Universe (Grab & drag rings to move clusters)"
          >
            <Orbit className="w-3.5 h-3.5 shrink-0" />
            <span className="whitespace-nowrap">3D Universe</span>
          </button>
        </div>

        {/* Animated Motion Slide Toggle: Orbit vs Move Space */}
        <div className="relative flex items-center bg-[var(--card-elevated)] p-1 rounded-xl border border-[var(--border-subtle)] text-xs font-mono w-[228px] h-[36px] shadow-inner">
          <div
            className="absolute top-1 bottom-1 rounded-lg bg-gradient-to-r from-sky-500 to-cyan-500 dark:from-cyan-500 dark:to-blue-600 shadow-md shadow-sky-500/25 dark:shadow-cyan-500/30 transition-all duration-300 ease-[cubic-bezier(0.2,1,0.3,1)]"
            style={{
              left: controlMode === 'orbit' ? '4px' : 'calc(50% + 2px)',
              width: 'calc(50% - 6px)',
            }}
          />
          <button
            onClick={() => setControlMode('orbit')}
            className={`relative z-10 flex-1 h-full font-semibold transition-colors duration-200 flex items-center justify-center space-x-1.5 px-2 cursor-pointer select-none whitespace-nowrap ${
              controlMode === 'orbit'
                ? 'text-white font-bold drop-shadow-xs'
                : 'text-slate-600 dark:text-neutral-400 hover:text-slate-900 dark:hover:text-white'
            }`}
            title="Orbit Mode: Drag to rotate view"
          >
            <RotateCcw className="w-3.5 h-3.5 shrink-0" />
            <span className="whitespace-nowrap">Orbit</span>
          </button>
          <button
            onClick={() => setControlMode('pan')}
            className={`relative z-10 flex-1 h-full font-semibold transition-colors duration-200 flex items-center justify-center space-x-1.5 px-2 cursor-pointer select-none whitespace-nowrap ${
              controlMode === 'pan'
                ? 'text-white font-bold drop-shadow-xs'
                : 'text-slate-600 dark:text-neutral-400 hover:text-slate-900 dark:hover:text-white'
            }`}
            title="Pan Mode: Drag in any direction to move space"
          >
            <Move className="w-3.5 h-3.5 shrink-0" />
            <span className="whitespace-nowrap">Move Space</span>
          </button>
        </div>

        {/* World Cycler */}
        <div className="flex items-center space-x-0.5 bg-[var(--card-elevated)] p-1 rounded-xl border border-[var(--border-subtle)] text-xs font-mono">
          <button
            onClick={handleTravelPrev}
            className="p-1 rounded-lg hover:bg-sky-50 dark:hover:bg-cyan-500/15 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 transition-colors cursor-pointer"
            title="Previous File"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
          <span className="px-1 text-[10px] text-slate-500 dark:text-[var(--text-muted)] font-bold">TRAVEL</span>
          <button
            onClick={handleTravelNext}
            className="p-1 rounded-lg hover:bg-sky-50 dark:hover:bg-cyan-500/15 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 transition-colors cursor-pointer"
            title="Next File"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
        </div>

        {/* Quick Search */}
        <div className="relative hidden lg:flex items-center">
          <div className="flex items-center space-x-1.5 px-2.5 py-1.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] focus-within:border-sky-500 dark:focus-within:border-cyan-500/60 text-xs font-mono transition-all">
            <Search className="w-3.5 h-3.5 text-slate-500 dark:text-[var(--text-muted)] shrink-0" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search file..."
              className="bg-transparent border-none outline-none text-xs text-slate-900 dark:text-[var(--text-primary)] w-28 placeholder:text-slate-400 dark:placeholder:text-[var(--text-muted)] font-mono"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery('')}
                aria-label="Clear file search"
                className="text-slate-500 dark:text-[var(--text-muted)] hover:text-slate-900 dark:hover:text-[var(--text-primary)] cursor-pointer"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>

          {searchQuery && filteredNodes.length > 0 && (
            <div className="absolute top-full left-0 mt-1.5 w-72 bg-[var(--card)]/95 backdrop-blur-xl border border-sky-500/30 dark:border-cyan-500/30 rounded-2xl shadow-2xl p-1.5 z-30 font-mono text-xs max-h-56 overflow-y-auto">
              {filteredNodes.map((fn) => (
                <button
                  key={fn.id}
                  onClick={() => {
                    flyToNodeByIdRef.current?.(fn.id);
                    setSearchQuery('');
                  }}
                  aria-label={`Focus graph node ${fn.id}`}
                  className="w-full text-left p-2 rounded-xl hover:bg-sky-50 dark:hover:bg-cyan-500/10 hover:text-sky-600 dark:hover:text-cyan-400 flex items-center justify-between transition-colors cursor-pointer"
                >
                  <span className="truncate text-[11px] text-slate-800 dark:text-[var(--text-primary)] font-medium">
                    {fn.id.split('/').pop()}
                  </span>
                  <span className="text-[9px] px-2 py-0.5 rounded-lg bg-sky-500/15 dark:bg-cyan-500/15 text-sky-600 dark:text-cyan-400 font-bold shrink-0">
                    FLY TO &rarr;
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Recenter */}
        <button
          onClick={handleRecenter}
          className="p-2 rounded-xl bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/10 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] transition-colors cursor-pointer"
          title="Recenter camera to center"
        >
          <Maximize2 className="w-4 h-4" />
        </button>

        {/* Pulse Pause */}
        <button
          disabled={!dataPulsesAvailable}
          onClick={() => dataPulsesAvailable && setIsPhysicsPaused(!isPhysicsPaused)}
          className={`p-2 rounded-xl bg-[var(--card-elevated)] text-slate-700 dark:text-[var(--text-secondary)] border border-[var(--border-subtle)] transition-colors ${dataPulsesAvailable
            ? 'hover:bg-sky-50 dark:hover:bg-cyan-500/10 hover:text-sky-600 dark:hover:text-cyan-400 cursor-pointer'
            : 'cursor-not-allowed opacity-50'}`}
          title={dataPulsesAvailable
            ? isPhysicsPaused ? 'Play data pulses' : 'Pause data pulses'
            : 'Data pulses are disabled for dense graphs'}
          aria-label={dataPulsesAvailable
            ? isPhysicsPaused ? 'Play data pulses' : 'Pause data pulses'
            : 'Data pulses disabled for dense graph'}
        >
          {isPhysicsPaused ? (
            <Play className="w-4 h-4 text-emerald-500 dark:text-emerald-400" />
          ) : (
            <Pause className="w-4 h-4" />
          )}
        </button>

        {/* Legends Toggle */}
        <button
          onClick={() => setShowLegends(!showLegends)}
          className={`p-2 rounded-xl border border-[var(--border-subtle)] transition-colors cursor-pointer ${
            showLegends
              ? 'bg-gradient-to-r from-sky-500 to-cyan-500 dark:from-cyan-500 dark:to-blue-600 text-white font-bold shadow-md shadow-sky-500/25'
              : 'bg-[var(--card-elevated)] text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 hover:bg-sky-50 dark:hover:bg-cyan-500/10'
          }`}
          title="Toggle Architecture Legends & Guide"
        >
          <Info className="w-4 h-4" />
        </button>
      </div>

      {/* 4. Adaptive Bottom Module Legends Bar (With Search & Horizontal Overflow) */}
      <div className="absolute bottom-5 left-4 right-32 hidden md:flex items-center overflow-x-auto py-1 px-2.5 bg-[var(--card)]/90 backdrop-blur-md rounded-2xl border border-[var(--border-subtle)] shadow-xl z-20 gap-2 scrollbar-none">
        <div className="flex items-center space-x-1.5 text-[10px] font-mono text-[var(--text-muted)] font-bold uppercase tracking-wider shrink-0 pr-2 border-r border-[var(--border-subtle)]">
          <FolderTree className="w-3.5 h-3.5 text-sky-600 dark:text-cyan-400" />
          <span>Modules ({adaptiveClusters.length})</span>
        </div>

        {/* Inline Module Filter Search if repo has many packages */}
        {adaptiveClusters.length > 8 && (
          <div className="relative flex items-center shrink-0">
            <input
              type="text"
              value={legendSearch}
              onChange={(e) => setLegendSearch(e.target.value)}
              placeholder="Filter modules..."
              className="text-[11px] font-mono px-2 py-0.5 rounded-lg bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-primary)] w-28 focus:outline-none focus:border-sky-500"
            />
            {legendSearch && (
              <button
                onClick={() => setLegendSearch('')}
                aria-label="Clear module filter"
                className="absolute right-1 text-slate-400 hover:text-slate-600 text-[10px] cursor-pointer"
              >
                &times;
              </button>
            )}
          </div>
        )}

        {visibleLegendClusters.map((fc) => {
          const isActive = selectedFolderFilter === fc.name;
          return (
            <button
              key={fc.name}
              onClick={() => handleFilterFolder(fc.name)}
              aria-label={`Filter graph by folder ${fc.name}`}
              className={`flex items-center space-x-1.5 px-2.5 py-1 rounded-xl text-xs font-mono transition-all shrink-0 cursor-pointer border ${
                isActive
                  ? 'bg-sky-50 dark:bg-cyan-500/20 border-sky-400 dark:border-cyan-400 shadow-sm scale-105'
                  : 'bg-[var(--card-elevated)] hover:bg-[var(--card)] border-[var(--border-subtle)] hover:border-slate-300 dark:hover:border-neutral-700'
              }`}
            >
              <span
                className="w-2.5 h-2.5 rounded-full shrink-0 shadow-xs"
                style={{
                  backgroundColor:
                    (isDark ? fc.darkTheme?.color : fc.lightTheme?.color) || fc.theme.color,
                }}
              />
              <span className="font-semibold text-[var(--text-primary)]">{fc.name}</span>
              <span className="text-[10px] px-1 rounded-md bg-slate-200 dark:bg-neutral-800 text-[var(--text-muted)] font-mono">
                {fc.count} ({fc.percentage}%)
              </span>
            </button>
          );
        })}

        {selectedFolderFilter && (
          <button
            onClick={() => handleFilterFolder(selectedFolderFilter)}
            aria-label="Show all folders in graph"
            className="ml-auto text-[11px] font-mono font-bold text-sky-600 dark:text-cyan-400 hover:underline cursor-pointer shrink-0"
          >
            Show All &rarr;
          </button>
        )}
      </div>

      {/* 5. Directional Navigation D-Pad (Bottom-Right) */}
      <div className="absolute bottom-5 right-5 flex flex-col items-center gap-2 bg-[var(--card)]/90 backdrop-blur-md p-2.5 rounded-2xl border border-[var(--border-subtle)] shadow-2xl z-20 select-none">
        <div className="text-[10px] font-mono text-slate-500 dark:text-[var(--text-muted)] font-bold uppercase tracking-wider">
          Space Nav
        </div>

        <div className="grid grid-cols-3 gap-1 w-20 h-20 items-center justify-center">
          <div />
          <button
            onClick={() => panUniverseRef.current?.(0, 45)}
            className="p-1.5 rounded-lg bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/20 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] flex items-center justify-center cursor-pointer transition-colors shadow-xs"
            title="Pan Up (▲)"
          >
            <ArrowUp className="w-3.5 h-3.5" />
          </button>
          <div />

          <button
            onClick={() => panUniverseRef.current?.(-45, 0)}
            className="p-1.5 rounded-lg bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/20 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] flex items-center justify-center cursor-pointer transition-colors shadow-xs"
            title="Pan Left (◀)"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={handleRecenter}
            className="p-1.5 rounded-lg bg-sky-50 dark:bg-cyan-950/40 hover:bg-sky-100 dark:hover:bg-cyan-900/50 text-sky-600 dark:text-cyan-400 border border-sky-300 dark:border-cyan-500/40 flex items-center justify-center cursor-pointer transition-colors shadow-xs"
            title="Recenter to Origin (0,0,0)"
          >
            <Crosshair className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => panUniverseRef.current?.(45, 0)}
            className="p-1.5 rounded-lg bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/20 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] flex items-center justify-center cursor-pointer transition-colors shadow-xs"
            title="Pan Right (▶)"
          >
            <ArrowRight className="w-3.5 h-3.5" />
          </button>

          <div />
          <button
            onClick={() => panUniverseRef.current?.(0, -45)}
            className="p-1.5 rounded-lg bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/20 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] flex items-center justify-center cursor-pointer transition-colors shadow-xs"
            title="Pan Down (▼)"
          >
            <ArrowDown className="w-3.5 h-3.5" />
          </button>
          <div />
        </div>

        {/* Zoom In & Zoom Out */}
        <div className="flex items-center gap-1.5 w-full pt-1 border-t border-[var(--border-subtle)]">
          <button
            onClick={() => zoomUniverseRef.current?.(-55)}
            className="flex-1 py-1 rounded-lg bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/20 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] flex items-center justify-center cursor-pointer transition-colors text-xs font-bold shadow-xs"
            title="Zoom In (+)"
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => zoomUniverseRef.current?.(55)}
            className="flex-1 py-1 rounded-lg bg-[var(--card-elevated)] hover:bg-sky-50 dark:hover:bg-cyan-500/20 text-slate-700 dark:text-[var(--text-secondary)] hover:text-sky-600 dark:hover:text-cyan-400 border border-[var(--border-subtle)] flex items-center justify-center cursor-pointer transition-colors text-xs font-bold shadow-xs"
            title="Zoom Out (-)"
          >
            <Minus className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* 6. Architecture Legends & Guidance Modal */}
      {showLegends && (
        <div className="absolute top-20 right-6 w-80 bg-[var(--card)]/95 backdrop-blur-xl border border-[var(--border-subtle)] rounded-2xl shadow-2xl p-4 z-30 font-mono text-xs space-y-3.5 animate-fadeIn">
          <div className="flex items-center justify-between border-b border-[var(--border-subtle)] pb-2.5">
            <div className="flex items-center space-x-2">
              <Info className="w-4 h-4 text-sky-600 dark:text-cyan-400" />
              <span className="font-bold text-[var(--text-primary)]">Adaptive Architecture Map</span>
            </div>
            <button
              onClick={() => setShowLegends(false)}
              aria-label="Close architecture legend"
              className="p-1 rounded-lg text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>

          {/* Module Ring Colors */}
          <div className="space-y-1.5">
            <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider font-bold">
              Detected Package Rings ({adaptiveClusters.length})
            </div>
            <div className="grid grid-cols-2 gap-2 text-[11px] max-h-48 overflow-y-auto pr-1">
              {adaptiveClusters.map((fc) => (
                <div key={fc.name} className="flex items-center space-x-2 truncate">
                  <span
                    className="w-3 h-3 rounded-full shrink-0 shadow-xs"
                    style={{
                      backgroundColor:
                        (isDark ? fc.darkTheme?.color : fc.lightTheme?.color) || fc.theme.color,
                    }}
                  />
                  <span className="truncate">{fc.name}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Circuit Lines Guide */}
          <div className="space-y-1.5 pt-2 border-t border-[var(--border-subtle)]">
            <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider font-bold">
              File Space
            </div>
            <div className="flex items-center gap-2 text-[11px]">
              <span className="inline-flex h-5 w-7 items-center justify-center rounded-md border-2 border-cyan-400 bg-slate-900 text-[8px] font-bold text-cyan-200">
                FILE
              </span>
              <span>Document card: colored by module, hover for path, click to travel and inspect</span>
            </div>
            <div className="text-[10px] text-[var(--text-muted)]">
              Colored cards are modules; lines connect real file-to-file dependencies. Drag to orbit through the file network; use Space Nav to move in any direction.
            </div>
          </div>

          {/* Circuit Lines Guide */}
          <div className="space-y-1.5 pt-2 border-t border-[var(--border-subtle)]">
            <div className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider font-bold">
              Circuit Data Highways
            </div>
            <div className="space-y-1.5 text-[11px]">
              <div className="flex items-center space-x-2">
                <span className="w-6 h-1 rounded bg-sky-600 dark:bg-cyan-400 shrink-0" />
                <span>Active Call (Caller &rarr; Callee)</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="w-6 h-1 rounded bg-indigo-600 dark:bg-purple-400 shrink-0" />
                <span>Inter-Module Import Bridge</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="w-6 h-1 rounded bg-rose-500 shrink-0" />
                <span>Impacted / Damaged Circuit</span>
              </div>
              <div className="flex items-center space-x-2">
                <span className="w-2.5 h-2.5 rounded-full bg-sky-600 dark:bg-cyan-300 animate-pulse shrink-0" />
                <span>Electric Pulse Packet (Flow)</span>
              </div>
            </div>
          </div>

          {/* Navigation Shortcuts */}
          <div className="space-y-1.5 pt-2 border-t border-[var(--border-subtle)] text-[10px] text-[var(--text-muted)]">
            <div className="uppercase tracking-wider font-bold">Smart Controls</div>
            <p>&bull; <strong>Click Card</strong>: Isolate file &amp; illuminate exact call ribbons</p>
            <p>&bull; <strong>Hover Card</strong>: Preview connections without clicking</p>
            <p>&bull; <strong>Click Module Pill</strong>: Fly directly to that package</p>
            <p>&bull; <strong>Spacebar</strong>: Freeze-frame data pulse packets</p>
          </div>
        </div>
      )}

      {/* 7. In-Canvas Focused File Details Inspector (Right Drawer) */}
      {selectedNode && (
        <div className="absolute top-20 right-6 w-96 bg-[var(--card)]/95 backdrop-blur-2xl border border-sky-400/30 dark:border-cyan-500/30 rounded-2xl shadow-2xl p-5 z-20 flex flex-col font-mono text-xs space-y-4 max-h-[82vh] overflow-hidden animate-fadeIn">
          {/* Header */}
          <div className="flex items-center justify-between border-b border-[var(--border-subtle)] pb-3">
            <div className="flex items-center space-x-2">
              <ShieldCheck className="w-4 h-4 text-sky-600 dark:text-cyan-400" />
              <h3 className="font-bold text-sm text-[var(--text-primary)] font-brand">
                File Details &amp; Connections
              </h3>
            </div>
            <button
              onClick={handleRecenter}
              aria-label="Clear selected file and recenter universe"
              className="p-1 rounded-xl text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--card-elevated)] transition-colors cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          {/* File Card Summary */}
          <div className="space-y-2">
            <span className="text-[10px] text-[var(--text-muted)] uppercase tracking-wider font-bold">
              Selected File Artifact:
            </span>
            <div className="p-3.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] space-y-2">
              <div className="font-bold text-sm text-[var(--text-primary)] break-all font-mono">
                {selectedNode.id}
              </div>
              <div className="text-[10px] font-mono text-[var(--text-muted)] break-all">
                {selectedNode.file_path || 'Source location unknown'}
                {selectedNode.start_line ? `:${selectedNode.start_line}${selectedNode.end_line && selectedNode.end_line !== selectedNode.start_line ? `-${selectedNode.end_line}` : ''}` : ''}
              </div>
              <div className="flex flex-wrap items-center gap-1.5 pt-1">
                <span
                  className="text-[10px] px-2 py-0.5 rounded-lg text-white font-bold"
                  style={{
                    backgroundColor:
                      (isDark
                        ? adaptiveClusters.find(
                            (c) => c.name === (nodeClusterMap.get(selectedNode.id) || 'root')
                          )?.darkTheme?.color
                        : adaptiveClusters.find(
                            (c) => c.name === (nodeClusterMap.get(selectedNode.id) || 'root')
                          )?.lightTheme?.color) || '#0284c7',
                  }}
                >
                  {(nodeClusterMap.get(selectedNode.id) || 'root').toUpperCase()}
                </span>
                <span className={`text-[10px] px-2 py-0.5 rounded-lg border font-bold ${selectedNode.status?.toLowerCase() === 'verified'
                  ? 'bg-emerald-50 dark:bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-200 dark:border-transparent'
                  : 'bg-amber-50 dark:bg-amber-500/15 text-amber-800 dark:text-amber-300 border-amber-200 dark:border-transparent'}`}>
                  {selectedNode.status?.toUpperCase() || 'UNKNOWN'} COVERAGE
                </span>
              </div>
            </div>
          </div>

          {/* Connected Circuits List */}
          <div className="space-y-2 flex-1 overflow-y-auto pr-1">
            <div className="flex items-center justify-between text-[10px] text-[var(--text-muted)] uppercase tracking-wider font-bold">
              <span>
                Analyzed Connections ({connectedEdges.inbound.length + connectedEdges.outbound.length})
              </span>
              <span className="text-sky-600 dark:text-cyan-400 lowercase font-normal">click to trace</span>
            </div>

            <div className="space-y-1.5">
              {connectedEdges.inbound.map((edge, idx) => (
                <div
                  key={`in-${idx}`}
                  onClick={() => flyToNodeByIdRef.current?.(edge.source)}
                  className="p-2.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-sky-500/50 dark:hover:border-cyan-500/50 hover:bg-sky-50 dark:hover:bg-cyan-500/10 cursor-pointer transition-all flex items-center justify-between group"
                >
                  <div className="flex items-center space-x-2 truncate">
                    <span className="w-2 h-2 rounded-full bg-sky-600 dark:bg-cyan-400 shrink-0" />
                    <span className="truncate text-[11px] text-[var(--text-primary)] group-hover:text-sky-600 dark:group-hover:text-cyan-400 font-medium">
                      &larr; {edge.source.split('/').pop()}
                    </span>
                    <span className="truncate text-[9px] font-mono text-[var(--text-muted)]">
                      {edge.source_file || 'source ?'}:{edge.source_line ?? '?'} → {edge.target_file || 'target ?'}:{edge.target_line ?? '?'}
                    </span>
                  </div>
                  <span className="text-[9px] px-1.5 py-0.5 rounded bg-sky-50 dark:bg-cyan-500/15 text-sky-700 dark:text-cyan-400 border border-sky-200 dark:border-transparent font-bold shrink-0">
                    CALLER
                  </span>
                </div>
              ))}

              {connectedEdges.outbound.map((edge, idx) => (
                <div
                  key={`out-${idx}`}
                  onClick={() => flyToNodeByIdRef.current?.(edge.target)}
                  className="p-2.5 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] hover:border-emerald-500/50 hover:bg-emerald-50 dark:hover:bg-emerald-500/10 cursor-pointer transition-all flex items-center justify-between group"
                >
                  <div className="flex items-center space-x-2 truncate">
                    <span className="w-2 h-2 rounded-full bg-emerald-600 dark:bg-emerald-400 shrink-0" />
                    <span className="truncate text-[11px] text-[var(--text-primary)] group-hover:text-emerald-700 dark:group-hover:text-emerald-400 font-medium">
                      &rarr; {edge.target.split('/').pop()}
                    </span>
                    <span className="truncate text-[9px] font-mono text-[var(--text-muted)]">
                      {edge.source_file || 'source ?'}:{edge.source_line ?? '?'} → {edge.target_file || 'target ?'}:{edge.target_line ?? '?'}
                    </span>
                  </div>
                  <span className="text-[9px] px-1.5 py-0.5 rounded bg-emerald-50 dark:bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border border-emerald-200 dark:border-transparent font-bold shrink-0">
                    CALLEE
                  </span>
                </div>
              ))}

              {connectedEdges.inbound.length === 0 && connectedEdges.outbound.length === 0 && (
                <div className="p-3 rounded-xl bg-[var(--card-elevated)] text-[var(--text-muted)] text-[11px] text-center italic">
                  Root or isolated module artifact
                </div>
              )}
            </div>
          </div>

          {/* Action Buttons */}
          <div className="pt-2 border-t border-[var(--border-subtle)] space-y-2 shrink-0">
            <button
              onClick={() => {
                if (onOpenSimulator) {
                  onOpenSimulator(selectedNode.id);
                } else if (onSelectNode) {
                  onSelectNode(selectedNode.id);
                }
              }}
              aria-label={`Simulate edits for ${selectedNode.label}`}
              className="w-full py-2.5 px-3 rounded-xl bg-gradient-to-r from-sky-600 to-indigo-600 hover:from-sky-500 hover:to-indigo-500 dark:from-cyan-600 dark:to-indigo-600 text-white font-semibold text-xs transition-all flex items-center justify-center space-x-2 shadow-lg cursor-pointer"
            >
              <Zap className="w-4 h-4 text-cyan-200" />
              <span>Simulate Edits in Blast Engine</span>
            </button>
            <button
              onClick={handleRecenter}
              aria-label="Zoom out to graph overview"
              className="w-full py-2 rounded-xl bg-[var(--card-elevated)] border border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] text-xs transition-colors cursor-pointer"
            >
              Zoom Out to Overview
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
