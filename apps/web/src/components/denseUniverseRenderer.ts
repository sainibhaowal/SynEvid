import * as THREE from 'three';
import { GraphUniverse, SimulateEditResponse, UniverseEdge, UniverseNode } from '@/lib/api';

type MutableRef<T> = { current: T };
type Dimension = '2D' | '3D';
type EdgeFilter = 'all' | 'calls' | 'imports';
type LinearRgb = readonly [number, number, number];

interface LinearColorPalette {
  clusters: Map<string, LinearRgb>;
  fallback: LinearRgb;
  damaged: LinearRgb;
  dead: LinearRgb;
  selected: LinearRgb;
  calls: LinearRgb;
  imports: LinearRgb;
}

interface DenseRendererRefs {
  activeDimension: MutableRef<Dimension>;
  selectedNodeId: MutableRef<string | null>;
  hoveredNodeId: MutableRef<string | null>;
  activeNeighbors: MutableRef<Set<string>>;
  controlMode: MutableRef<'orbit' | 'pan'>;
  edgeFilter: MutableRef<EdgeFilter>;
  simulation: MutableRef<SimulateEditResponse | null | undefined>;
  simulationFilePath: MutableRef<string | null | undefined>;
  damageOverlay: MutableRef<boolean>;
}

interface DenseRendererOptions {
  container: HTMLElement;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer;
  universe: GraphUniverse;
  nodes: UniverseNode[];
  edges: UniverseEdge[];
  nodeClusterMap: Map<string, string>;
  darkClusterColors: Map<string, number>;
  lightClusterColors: Map<string, number>;
  isDark: boolean;
  refs: DenseRendererRefs;
  onSelectNode?: (nodeId: string) => void;
  onHoverNode?: (node: UniverseNode | null, clientX: number, clientY: number) => void;
  onFps?: (fps: number) => void;
  getNodeState: (
    node: UniverseNode,
    simulation: SimulateEditResponse | null | undefined,
    filePath?: string | null
  ) => { damaged: boolean; dead: boolean };
  isEdgeBroken: (edge: UniverseEdge, simulation: SimulateEditResponse | null | undefined) => boolean;
}

interface DenseRendererHandle {
  dispose: () => void;
  setDimension: (dimension: Dimension) => void;
  setTheme: (dark: boolean) => void;
  updateOverlay: () => void;
  setEdgeFilter: (filter: EdgeFilter) => void;
  flyToNode: (nodeId: string) => void;
  flyToCluster: (clusterId: string) => void;
  recenter: () => void;
  pan: (dx: number, dy: number) => void;
  zoom: (delta: number) => void;
}

interface PointCloud {
  points: THREE.Points;
  nodes: UniverseNode[];
  colors: THREE.BufferAttribute;
  texture: THREE.CanvasTexture;
  isFileCard: boolean;
}

interface EdgeSegment {
  edge: UniverseEdge;
  sourceIndex: number;
  targetIndex: number;
  colorIndex: number;
  category: 'calls' | 'imports';
}

interface EdgeCloud {
  lines: THREE.LineSegments;
  segments: EdgeSegment[];
  positions: THREE.BufferAttribute;
  colors: THREE.BufferAttribute;
}

const DENSE_FILE_SIZE_PX = 68;
const MAX_RENDER_PIXEL_RATIO = 2;
export const DENSE_GRAPH_RENDER_THRESHOLD = 1_000;
export const MAX_DENSE_OVERVIEW_EDGES = 2_000;

type DenseLayoutPoint = { x: number; y: number; z: number };

function denseFilePath(node: UniverseNode): string | null {
  if (node.file_path) return node.file_path;
  if (node.id.startsWith('file::')) return node.id.slice('file::'.length);
  return null;
}

function denseFileCardId(node: UniverseNode): string | null {
  const filePath = denseFilePath(node);
  return filePath ? `file:${filePath}` : null;
}

function buildDenseRenderNodes(sourceNodes: UniverseNode[]): UniverseNode[] {
  const renderNodes = [...sourceNodes];
  const syntheticFiles = new Map<string, UniverseNode>();
  const canonicalFilePaths = new Set<string>();

  for (const node of sourceNodes) {
    const filePath = denseFilePath(node);
    const fileCardId = denseFileCardId(node);
    if (!filePath || !fileCardId) continue;
    if (node.kind === 'file' && node.id === fileCardId) {
      canonicalFilePaths.add(filePath);
      syntheticFiles.delete(filePath);
      continue;
    }
    if (canonicalFilePaths.has(filePath) || syntheticFiles.has(filePath)) continue;
    syntheticFiles.set(filePath, {
      id: fileCardId,
      x: 0,
      y: 0,
      z: 0,
      label: filePath,
      kind: 'file',
      file_path: filePath,
      status: node.status,
      caller_count: 0,
      callee_count: 0,
    });
  }

  return renderNodes.concat(
    [...syntheticFiles.entries()]
      .filter(([filePath]) => !canonicalFilePaths.has(filePath))
      .map(([, node]) => node),
  );
}

function buildDenseMindMapPositions(
  nodes: UniverseNode[],
  nodeClusterMap: Map<string, string>,
): Map<string, DenseLayoutPoint> {
  const positions = new Map<string, DenseLayoutPoint>();
  const sortedNodes = [...nodes].sort((left, right) => left.id.localeCompare(right.id));
  const clusters = [...new Set(sortedNodes.map((node) => nodeClusterMap.get(node.id) || 'root'))].sort();
  const nodesByCluster = new Map<string, UniverseNode[]>();
  const symbolsByFile = new Map<string, UniverseNode[]>();

  for (const node of sortedNodes) {
    const cluster = nodeClusterMap.get(node.id) || 'root';
    const clusterNodes = nodesByCluster.get(cluster) || [];
    clusterNodes.push(node);
    nodesByCluster.set(cluster, clusterNodes);
    if (node.kind !== 'file' && node.file_path) {
      const fileSymbols = symbolsByFile.get(node.file_path) || [];
      fileSymbols.push(node);
      symbolsByFile.set(node.file_path, fileSymbols);
    }
  }

  const filePositions = new Map<string, DenseLayoutPoint>();
  const clusterColumns = 3;
  const clustersPerShelf = clusterColumns * 2;
  const clusterRowsPerShelf = 2;
  clusters.forEach((cluster, clusterIndex) => {
    const column = clusterIndex % clusterColumns;
    const shelfIndex = Math.floor(clusterIndex / clustersPerShelf);
    const shelfOffset = clusterIndex % clustersPerShelf;
    const row = Math.floor(shelfOffset / clusterColumns);
    const center = {
      x: (column - (clusterColumns - 1) / 2) * 1100,
      y: ((clusterRowsPerShelf - 1) / 2 - row) * 850,
      z: shelfIndex * 900 + (column % 2) * 160 + row * 90,
    };
    const clusterFiles = (nodesByCluster.get(cluster) || [])
      .filter((node) => node.kind === 'file')
      .sort((left, right) => left.id.localeCompare(right.id));
    const fileColumns = Math.max(4, Math.min(10, Math.ceil(Math.sqrt(Math.min(clusterFiles.length || 1, 100)))));
    const fileRowsPerLayer = 6;
    const filesPerLayer = fileColumns * fileRowsPerLayer;
    const fileLayerCount = Math.max(1, Math.ceil(clusterFiles.length / filesPerLayer));
    clusterFiles.forEach((node, fileIndex) => {
      const layerIndex = Math.floor(fileIndex / filesPerLayer);
      const layerOffset = fileIndex % filesPerLayer;
      const fileColumn = layerOffset % fileColumns;
      const fileRow = Math.floor(layerOffset / fileColumns);
      // Sci-Fi honeycomb / staggered offset to prevent line-of-sight occlusion
      const staggerX = (fileRow % 2) * 115;
      const staggerY = (fileColumn % 2) * 35;
      const position = {
        x: center.x + (fileColumn - (fileColumns - 1) / 2) * 230 + staggerX,
        y: center.y + ((fileRowsPerLayer - 1) / 2 - fileRow) * 175 + staggerY,
        z: center.z + (layerIndex - (fileLayerCount - 1) / 2) * 240,
      };
      positions.set(node.id, position);
      filePositions.set(node.file_path || node.label || node.id, position);
    });
  });

  for (const node of sortedNodes) {
    if (node.kind === 'file' || positions.has(node.id)) continue;
    const anchor = node.file_path ? filePositions.get(node.file_path) : undefined;
    const siblings = node.file_path ? symbolsByFile.get(node.file_path) || [] : [];
    const siblingIndex = Math.max(0, siblings.findIndex((sibling) => sibling.id === node.id));
    if (anchor) {
      const ring = Math.floor(siblingIndex / 10) + 1;
      const angle = (siblingIndex % 10) * (Math.PI * 2 / 10);
      const radius = 28 + ring * 22;
      positions.set(node.id, {
        x: anchor.x + Math.cos(angle) * radius,
        y: anchor.y + Math.sin(angle) * radius,
        z: anchor.z + ((siblingIndex % 3) - 1) * 18,
      });
      continue;
    }
    const cluster = nodeClusterMap.get(node.id) || 'root';
    const clusterIndex = Math.max(0, clusters.indexOf(cluster));
    const column = clusterIndex % clusterColumns;
    const shelfIndex = Math.floor(clusterIndex / clustersPerShelf);
    const shelfOffset = clusterIndex % clustersPerShelf;
    const row = Math.floor(shelfOffset / clusterColumns);
    positions.set(node.id, {
      x: (column - (clusterColumns - 1) / 2) * 720 + (siblingIndex % 8) * 36,
      y: ((clusterRowsPerShelf - 1) / 2 - row) * 520 + Math.floor(siblingIndex / 8) * 36,
      z: shelfIndex * 560 + (column % 2) * 110 + row * 65,
    });
  }

  return positions;
}

export function sampleDenseOverviewEdges(
  edges: UniverseEdge[],
  maximum = MAX_DENSE_OVERVIEW_EDGES
): UniverseEdge[] {
  const sampleCount = Math.max(0, Math.min(edges.length, Math.floor(maximum)));
  if (sampleCount === edges.length) return edges;
  if (sampleCount === 0) return [];
  return Array.from(
    { length: sampleCount },
    (_, index) => edges[Math.floor((index * edges.length) / sampleCount)]
  );
}

export function setupDenseUniverseRenderer(options: DenseRendererOptions): DenseRendererHandle {
  const {
    container,
    scene,
    camera,
    renderer,
    universe,
    nodes: sourceNodes,
    edges,
    nodeClusterMap: sourceNodeClusterMap,
    refs,
    getNodeState,
    isEdgeBroken,
  } = options;
  const nodes = buildDenseRenderNodes(sourceNodes);
  const nodeClusterMap = new Map(sourceNodeClusterMap);
  for (const node of nodes) {
    const filePath = denseFilePath(node);
    if (node.kind !== 'file' || nodeClusterMap.has(node.id) || !filePath) continue;
    const sourceNode = sourceNodes.find((candidate) => denseFilePath(candidate) === filePath);
    nodeClusterMap.set(node.id, sourceNode ? sourceNodeClusterMap.get(sourceNode.id) || 'root' : 'root');
  }
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, MAX_RENDER_PIXEL_RATIO));
  renderer.setSize(container.clientWidth || 1000, container.clientHeight || 700);
  renderer.setClearColor(options.isDark ? 0x090a0f : 0xf1f5f9, 1);
  scene.background = new THREE.Color(options.isDark ? 0x090a0f : 0xf1f5f9);

  const nodeIndexById = new Map<string, number>();
  nodes.forEach((node, index) => nodeIndexById.set(node.id, index));
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  const fileNodeIndexById = new Map(
    nodes
      .filter((node) => node.kind === 'file' && denseFileCardId(node) === node.id)
      .map((node) => [node.id, nodeIndexById.get(node.id)!])
  );
  const mindMapPositions = buildDenseMindMapPositions(nodes, nodeClusterMap);
  const currentPositions = new Float32Array(nodes.length * 3);
  const positions3D = new Float32Array(nodes.length * 3);
  const positions2D = new Float32Array(nodes.length * 3);
  nodes.forEach((node, index) => {
    const offset = index * 3;
    const projected = mindMapPositions.get(node.id);
    const x = projected?.x ?? (Number.isFinite(node.x) ? node.x : 0);
    const y = projected?.y ?? (Number.isFinite(node.y) ? node.y : 0);
    const z = projected?.z ?? (Number.isFinite(node.z) ? node.z || 0 : 0);
    positions3D.set([x, y, z], offset);
    positions2D.set([x, y, 0], offset);
    currentPositions.set(refs.activeDimension.current === '3D' ? [x, y, z] : [x, y, 0], offset);
  });

  const nodeGroups = [{
    nodes: nodes.filter((node) => node.kind === 'file' && denseFileCardId(node) === node.id),
    isFileCard: true,
  }];
  const pointClouds = nodeGroups
    .map(({ nodes: groupNodes, isFileCard }) => createPointCloud(
      groupNodes,
      DENSE_FILE_SIZE_PX,
      createFileCardTexture(renderer, options.isDark),
      isFileCard,
      mindMapPositions,
    ))
    .filter((cloud): cloud is PointCloud => cloud !== null);
  pointClouds.forEach((cloud) => scene.add(cloud.points));

  const focusedFileMaterial = new THREE.SpriteMaterial({
    transparent: true,
    depthTest: false,
    depthWrite: false,
  });
  const focusedFileCard = new THREE.Sprite(focusedFileMaterial);
  focusedFileCard.visible = false;
  focusedFileCard.scale.set(78, 46, 1);
  focusedFileCard.renderOrder = 20;
  scene.add(focusedFileCard);

  const callsSegments: EdgeSegment[] = [];
  const importSegments: EdgeSegment[] = [];
  const callsPositions: number[] = [];
  const importPositions: number[] = [];
  const seenFileEdges = new Set<string>();
  edges.forEach((edge) => {
    const sourceNode = nodeById.get(edge.source);
    const targetNode = nodeById.get(edge.target);
    const sourceFileId = sourceNode ? denseFileCardId(sourceNode) : null;
    const targetFileId = targetNode ? denseFileCardId(targetNode) : null;
    if (!sourceFileId || !targetFileId || sourceFileId === targetFileId) return;
    const edgeKey = `${edge.kind}:${sourceFileId}:${targetFileId}`;
    if (seenFileEdges.has(edgeKey)) return;
    seenFileEdges.add(edgeKey);
    const sourceIndex = fileNodeIndexById.get(sourceFileId);
    const targetIndex = fileNodeIndexById.get(targetFileId);
    if (sourceIndex === undefined || targetIndex === undefined) return;
    const isImport = edge.kind.toLowerCase().includes('import');
    const segments = isImport ? importSegments : callsSegments;
    const positionList = isImport ? importPositions : callsPositions;
    const colorIndex = segments.length * 2;
    segments.push({
      edge,
      sourceIndex,
      targetIndex,
      colorIndex,
      category: isImport ? 'imports' : 'calls',
    });
    positionList.push(
      currentPositions[sourceIndex * 3],
      currentPositions[sourceIndex * 3 + 1],
      currentPositions[sourceIndex * 3 + 2],
      currentPositions[targetIndex * 3],
      currentPositions[targetIndex * 3 + 1],
      currentPositions[targetIndex * 3 + 2]
    );
  });

  const callsCloud = createEdgeCloud(callsSegments, callsPositions, options.isDark);
  const importsCloud = createEdgeCloud(importSegments, importPositions, options.isDark);
  if (callsCloud) scene.add(callsCloud.lines);
  if (importsCloud) scene.add(importsCloud.lines);

  const grid = new THREE.GridHelper(2400, 48, options.isDark ? 0x00f0ff : 0x0284c7, options.isDark ? 0x0d2238 : 0xcbd5e1);
  const gridMaterial = grid.material as THREE.Material;
  gridMaterial.transparent = true;
  gridMaterial.opacity = options.isDark ? 0.22 : 0.28;
  grid.rotation.x = refs.activeDimension.current === '2D' ? Math.PI / 2 : 0;
  scene.add(grid);

  // Volumetric Sci-Fi Deep Space Starfield Particle System
  const starfieldCount = 2400;
  const starfieldGeo = new THREE.BufferGeometry();
  const starfieldPos = new Float32Array(starfieldCount * 3);
  const starfieldCols = new Float32Array(starfieldCount * 3);
  for (let i = 0; i < starfieldCount; i += 1) {
    // Distribute in a spherical cosmos radius around the universe
    const r = 1200 + Math.random() * 4000;
    const theta = Math.random() * Math.PI * 2;
    const phi = Math.acos(2 * Math.random() - 1);
    starfieldPos[i * 3] = r * Math.sin(phi) * Math.cos(theta);
    starfieldPos[i * 3 + 1] = r * Math.sin(phi) * Math.sin(theta);
    starfieldPos[i * 3 + 2] = r * Math.cos(phi);

    // Deep space palette: Cyan, Electric Blue, Pure White, and Subtle Violet
    const tintChoice = Math.random();
    if (tintChoice < 0.45) {
      // Electric Cyan
      starfieldCols[i * 3] = 0.0;
      starfieldCols[i * 3 + 1] = 0.94;
      starfieldCols[i * 3 + 2] = 1.0;
    } else if (tintChoice < 0.75) {
      // Cosmic Violet
      starfieldCols[i * 3] = 0.65;
      starfieldCols[i * 3 + 1] = 0.45;
      starfieldCols[i * 3 + 2] = 1.0;
    } else {
      // Stellar White
      starfieldCols[i * 3] = 0.95;
      starfieldCols[i * 3 + 1] = 0.98;
      starfieldCols[i * 3 + 2] = 1.0;
    }
  }
  starfieldGeo.setAttribute('position', new THREE.BufferAttribute(starfieldPos, 3));
  starfieldGeo.setAttribute('color', new THREE.BufferAttribute(starfieldCols, 3));
  const starfieldMaterial = new THREE.PointsMaterial({
    size: 4.5,
    vertexColors: true,
    transparent: true,
    opacity: options.isDark ? 0.75 : 0.25,
    depthWrite: false,
    blending: options.isDark ? THREE.AdditiveBlending : THREE.NormalBlending,
  });
  const starfield = new THREE.Points(starfieldGeo, starfieldMaterial);
  scene.add(starfield);

  let maximumExtent = 0;
  for (let index = 0; index < nodes.length; index += 1) {
    maximumExtent = Math.max(
      maximumExtent,
      Math.hypot(currentPositions[index * 3], currentPositions[index * 3 + 1], currentPositions[index * 3 + 2])
    );
  }
  const initialRadius = Math.max(180, maximumExtent * 2.7);
  let radius = initialRadius;
  let targetRadius = initialRadius;
  let azimuth = Math.PI / 4;
  let elevation = Math.PI / 3;
  const target = new THREE.Vector3();
  const cameraRight = new THREE.Vector3();
  const cameraUp = new THREE.Vector3();
  const cameraBackward = new THREE.Vector3();
  const raycaster = new THREE.Raycaster();
  const pointer = new THREE.Vector2();
  raycaster.params.Points = { threshold: Math.max(maximumExtent * 0.012, 2.5) };

  let isDragging = false;
  let dragMoved = false;
  let dragButton = 0;
  let lastPointerX = 0;
  let lastPointerY = 0;
  let hoverFrame = 0;
  let animationFrame = 0;
  let frameCount = 0;
  let fpsWindowStart = performance.now();
  let morphStart = 0;
  let morphFrom: Float32Array | null = null;
  let morphTo: Float32Array | null = null;
  let activeTheme = options.isDark;
  const toLinearRgb = (hex: number, soften = 0): LinearRgb => {
    const color = new THREE.Color(hex);
    if (soften > 0) color.lerp(new THREE.Color(0xffffff), soften);
    return [color.r, color.g, color.b];
  };
  const createColorPalette = (dark: boolean, clusters: Map<string, number>): LinearColorPalette => ({
    clusters: new Map([...clusters].map(([name, hex]) => [name, toLinearRgb(hex, dark ? 0.04 : 0.28)])),
    fallback: toLinearRgb(dark ? 0x00f0ff : 0x0284c7),
    damaged: toLinearRgb(dark ? 0xff0055 : 0xbe123c),
    dead: toLinearRgb(dark ? 0x475569 : 0x64748b),
    selected: toLinearRgb(dark ? 0x00ffff : 0x0284c7),
    calls: toLinearRgb(dark ? 0x00f0ff : 0x0284c7),
    imports: toLinearRgb(dark ? 0xa855f7 : 0x7c3aed),
  });
  const colorPalettes = {
    dark: createColorPalette(true, options.darkClusterColors),
    light: createColorPalette(false, options.lightClusterColors),
  };
  const activePalette = () => (activeTheme ? colorPalettes.dark : colorPalettes.light);

  const writePointColor = (
    cloud: PointCloud,
    index: number,
    palette: LinearColorPalette,
    selectedNodeId: string | null,
    hasOverlay: boolean
  ) => {
    const node = cloud.nodes[index];
    const selectedNode = selectedNodeId ? nodeById.get(selectedNodeId) : undefined;
    const selectedFilePath = selectedNode ? denseFilePath(selectedNode) : undefined;
    const isSelected = selectedNodeId === node.id ||
      (cloud.isFileCard && selectedFilePath !== undefined && node.file_path === selectedFilePath);
    const state = hasOverlay
      ? getNodeState(node, refs.simulation.current, refs.simulationFilePath.current)
      : null;
    const color = state?.damaged
      ? palette.damaged
      : state?.dead
        ? palette.dead
        : isSelected
          ? palette.selected
          : palette.clusters.get(nodeClusterMap.get(node.id) || 'root') || palette.fallback;
    let red = color[0];
    let green = color[1];
    let blue = color[2];
    if (!state?.damaged && !state?.dead && !isSelected) {
      if (refs.activeNeighbors.current.has(node.id)) {
        red += (1 - red) * 0.22;
        green += (1 - green) * 0.22;
        blue += (1 - blue) * 0.22;
      } else if (selectedNodeId) {
        red *= 0.28;
        green *= 0.28;
        blue *= 0.28;
      }
    }
    const offset = index * 3;
    const colorValues = cloud.colors.array as Float32Array;
    colorValues[offset] = red;
    colorValues[offset + 1] = green;
    colorValues[offset + 2] = blue;
  };

  const writeEdgeColor = (
    cloud: EdgeCloud,
    index: number,
    palette: LinearColorPalette,
    selectedNodeId: string | null,
    hasOverlay: boolean
  ) => {
    const segment = cloud.segments[index];
    const broken = hasOverlay && isEdgeBroken(segment.edge, refs.simulation.current);
    const selectedNode = selectedNodeId ? nodeById.get(selectedNodeId) : undefined;
    const selectedFilePath = selectedNode ? denseFilePath(selectedNode) : undefined;
    const focused = selectedNodeId !== null && (
      segment.edge.source === selectedNodeId ||
      segment.edge.target === selectedNodeId ||
      (selectedFilePath !== undefined && (
        (nodeById.get(segment.edge.source) && denseFilePath(nodeById.get(segment.edge.source)!) === selectedFilePath) ||
        (nodeById.get(segment.edge.target) && denseFilePath(nodeById.get(segment.edge.target)!) === selectedFilePath)
      ))
    );
    const color = broken ? palette.damaged : segment.category === 'imports' ? palette.imports : palette.calls;
    const opacity = selectedNodeId && !focused ? 0.12 : 1;
    const offset = segment.colorIndex * 3;
    const colorValues = cloud.colors.array as Float32Array;
    const red = color[0] * opacity;
    const green = color[1] * opacity;
    const blue = color[2] * opacity;
    colorValues[offset] = red;
    colorValues[offset + 1] = green;
    colorValues[offset + 2] = blue;
    colorValues[offset + 3] = red;
    colorValues[offset + 4] = green;
    colorValues[offset + 5] = blue;
  };

  const refreshPointColors = () => {
    const palette = activePalette();
    const selectedNodeId = refs.selectedNodeId.current;
    const hasOverlay = refs.damageOverlay.current;
    for (const cloud of pointClouds) {
      for (let index = 0; index < cloud.nodes.length; index += 1) {
        writePointColor(cloud, index, palette, selectedNodeId, hasOverlay);
      }
      cloud.colors.needsUpdate = true;
    }
  };

  const refreshEdgeColors = (cloud: EdgeCloud | null) => {
    if (!cloud) return;
    const palette = activePalette();
    const selectedNodeId = refs.selectedNodeId.current;
    const hasOverlay = refs.damageOverlay.current;
    for (let index = 0; index < cloud.segments.length; index += 1) {
      writeEdgeColor(cloud, index, palette, selectedNodeId, hasOverlay);
    }
    cloud.colors.needsUpdate = true;
  };

  let themeRefreshToken = 0;
  const refreshThemeColorsInFrames = () => {
    const token = ++themeRefreshToken;
    const edgeClouds = [callsCloud, importsCloud].filter((cloud): cloud is EdgeCloud => cloud !== null);
    let pointCloudIndex = 0;
    let pointIndex = 0;
    let edgeCloudIndex = 0;
    let edgeIndex = 0;

    const updateSlice = () => {
      if (token !== themeRefreshToken) return;
      const palette = activePalette();
      const selectedNodeId = refs.selectedNodeId.current;
      const hasOverlay = refs.damageOverlay.current;
      const pointCloudChanged = Array.from({ length: pointClouds.length }, () => false);
      const edgeCloudChanged = Array.from({ length: edgeClouds.length }, () => false);
      const deadline = performance.now() + 2;

      while (performance.now() < deadline) {
        if (pointCloudIndex < pointClouds.length) {
          const cloud = pointClouds[pointCloudIndex];
          if (pointIndex >= cloud.nodes.length) {
            pointCloudIndex += 1;
            pointIndex = 0;
            continue;
          }
          writePointColor(cloud, pointIndex, palette, selectedNodeId, hasOverlay);
          pointCloudChanged[pointCloudIndex] = true;
          pointIndex += 1;
          continue;
        }
        if (edgeCloudIndex < edgeClouds.length) {
          const cloud = edgeClouds[edgeCloudIndex];
          if (edgeIndex >= cloud.segments.length) {
            edgeCloudIndex += 1;
            edgeIndex = 0;
            continue;
          }
          writeEdgeColor(cloud, edgeIndex, palette, selectedNodeId, hasOverlay);
          edgeCloudChanged[edgeCloudIndex] = true;
          edgeIndex += 1;
          continue;
        }
        break;
      }

      pointClouds.forEach((cloud, index) => {
        if (pointCloudChanged[index]) cloud.colors.needsUpdate = true;
      });
      edgeClouds.forEach((cloud, index) => {
        if (edgeCloudChanged[index]) cloud.colors.needsUpdate = true;
      });

      if (pointCloudIndex < pointClouds.length || edgeCloudIndex < edgeClouds.length) {
        requestAnimationFrame(updateSlice);
      }
    };

    requestAnimationFrame(() => requestAnimationFrame(updateSlice));
  };

  const refreshColors = () => {
    themeRefreshToken += 1;
    refreshPointColors();
    refreshEdgeColors(callsCloud);
    refreshEdgeColors(importsCloud);
  };

  const updateEdgePositions = () => {
    [callsCloud, importsCloud].forEach((cloud) => {
      if (!cloud) return;
      cloud.segments.forEach((segment) => {
        const sourceOffset = segment.sourceIndex * 3;
        const targetOffset = segment.targetIndex * 3;
        const positionOffset = segment.colorIndex * 3;
        cloud.positions.setXYZ(
          positionOffset / 3,
          currentPositions[sourceOffset],
          currentPositions[sourceOffset + 1],
          currentPositions[sourceOffset + 2]
        );
        cloud.positions.setXYZ(
          positionOffset / 3 + 1,
          currentPositions[targetOffset],
          currentPositions[targetOffset + 1],
          currentPositions[targetOffset + 2]
        );
      });
      cloud.positions.needsUpdate = true;
      cloud.lines.geometry.computeBoundingSphere();
    });
  };

  const updateCamera = () => {
    radius += (targetRadius - radius) * 0.16;
    if (refs.activeDimension.current === '3D') {
      camera.position.set(
        target.x + radius * Math.sin(elevation) * Math.sin(azimuth),
        target.y + radius * Math.cos(elevation),
        target.z + radius * Math.sin(elevation) * Math.cos(azimuth)
      );
      camera.up.set(0, 1, 0);
      grid.rotation.x = 0;
    } else {
      camera.position.set(target.x, target.y, target.z + radius);
      camera.up.set(0, 1, 0);
      grid.rotation.x = Math.PI / 2;
    }
    camera.lookAt(target);
  };

  const nodePosition = (nodeId: string): THREE.Vector3 | null => {
    const index = nodeIndexById.get(nodeId);
    if (index === undefined) return null;
    return new THREE.Vector3(
      currentPositions[index * 3],
      currentPositions[index * 3 + 1],
      currentPositions[index * 3 + 2]
    );
  };

  const showFocusedFileCard = (node: UniverseNode | undefined, position: THREE.Vector3 | null) => {
    const filePath = node ? denseFilePath(node) : null;
    focusedFileMaterial.map?.dispose();
    focusedFileMaterial.map = null;
    focusedFileCard.visible = false;
    if (!node || !filePath || !position) return;
    const cluster = nodeClusterMap.get(node.id) || 'root';
    const clusterColors = activeTheme ? options.darkClusterColors : options.lightClusterColors;
    const accentHex = clusterColors.get(cluster) ?? (activeTheme ? 0x22d3ee : 0x0284c7);
    focusedFileMaterial.map = createFocusedFileTexture(node, filePath, activeTheme, accentHex);
    focusedFileMaterial.needsUpdate = true;
    focusedFileCard.position.copy(position);
    focusedFileCard.visible = true;
  };

  const positionFocusedFileCard = (position: THREE.Vector3) => {
    focusedFileCard.position.copy(position);
    if (refs.activeDimension.current === '3D') {
      focusedFileCard.position.x -= Math.cos(azimuth) * 42;
      focusedFileCard.position.z += Math.sin(azimuth) * 42;
    } else {
      focusedFileCard.position.x -= 42;
    }
  };

  const setFocus = (nodeId: string | null) => {
    refs.selectedNodeId.current = nodeId;
    refs.activeNeighbors.current.clear();
    if (nodeId) {
      refs.activeNeighbors.current.add(nodeId);
      const selectedNode = nodeById.get(nodeId);
      const selectedFilePath = selectedNode ? denseFilePath(selectedNode) : null;
      if (selectedFilePath) {
        for (const node of nodes) {
          if (denseFilePath(node) === selectedFilePath) refs.activeNeighbors.current.add(node.id);
        }
        targetRadius = Math.max(90, Math.min(220, initialRadius * 0.18));
      }
      for (const edge of edges) {
        if (edge.source === nodeId) refs.activeNeighbors.current.add(edge.target);
        if (edge.target === nodeId) refs.activeNeighbors.current.add(edge.source);
      }
      const position = nodePosition(nodeId);
      if (position) {
        target.copy(position);
        targetRadius = selectedFilePath ? 150 : Math.max(72, Math.min(targetRadius, initialRadius * 0.62));
      }
      showFocusedFileCard(selectedNode, position);
      if (position && selectedFilePath) positionFocusedFileCard(position);
    } else {
      target.set(0, 0, 0);
      showFocusedFileCard(undefined, null);
    }
    refreshColors();
  };

  const flyToNode = (nodeId: string) => {
    if (nodeById.has(nodeId)) setFocus(nodeId);
    options.onSelectNode?.(nodeId);
  };

  const flyToCluster = (clusterId: string) => {
    const members = nodes.filter((node) => nodeClusterMap.get(node.id) === clusterId);
    if (members.length === 0) return;
    target.set(0, 0, 0);
    for (const node of members) {
      const index = nodeIndexById.get(node.id)!;
      target.x += currentPositions[index * 3];
      target.y += currentPositions[index * 3 + 1];
      target.z += currentPositions[index * 3 + 2];
    }
    target.multiplyScalar(1 / members.length);
    targetRadius = Math.max(90, Math.min(initialRadius, Math.sqrt(members.length) * 34));
  };

  const recenter = () => {
    refs.selectedNodeId.current = null;
    refs.activeNeighbors.current.clear();
    target.set(0, 0, 0);
    targetRadius = initialRadius;
    showFocusedFileCard(undefined, null);
    refreshColors();
  };

  const setDimension = (dimension: Dimension) => {
    if (refs.activeDimension.current === dimension || nodes.length === 0) return;
    refs.activeDimension.current = dimension;
    morphFrom = new Float32Array(currentPositions);
    morphTo = dimension === '3D' ? positions3D : positions2D;
    morphStart = performance.now();
    grid.rotation.x = dimension === '2D' ? Math.PI / 2 : 0;
  };

  const setTheme = (dark: boolean) => {
    activeTheme = dark;
    scene.background = new THREE.Color(dark ? 0x05070e : 0xf1f5f9);
    renderer.setClearColor(dark ? 0x05070e : 0xf1f5f9, 1);
    (grid.material as THREE.LineBasicMaterial).color.setHex(dark ? 0x00f0ff : 0x0284c7);
    starfieldMaterial.opacity = dark ? 0.75 : 0.25;
    starfieldMaterial.blending = dark ? THREE.AdditiveBlending : THREE.NormalBlending;
    const fileCloud = pointClouds.find((cloud) => cloud.isFileCard);
    if (fileCloud) {
      const nextTexture = createFileCardTexture(renderer, dark);
      const previousTexture = fileCloud.texture;
      fileCloud.texture = nextTexture;
      const material = fileCloud.points.material as THREE.PointsMaterial;
      material.map = nextTexture;
      material.needsUpdate = true;
      previousTexture.dispose();
    }
    const selectedNode = refs.selectedNodeId.current ? nodeById.get(refs.selectedNodeId.current) : undefined;
    showFocusedFileCard(selectedNode, selectedNode ? nodePosition(selectedNode.id) : null);
    refreshThemeColorsInFrames();
  };

  const setEdgeFilter = (filter: EdgeFilter) => {
    refs.edgeFilter.current = filter;
    if (callsCloud) callsCloud.lines.visible = filter !== 'imports';
    if (importsCloud) importsCloud.lines.visible = filter !== 'calls';
  };

  const pan = (dx: number, dy: number) => {
    camera.updateMatrixWorld();
    camera.matrix.extractBasis(cameraRight, cameraUp, cameraBackward);
    const scale = Math.max(radius / Math.max(container.clientHeight, 1), 0.1);
    target.addScaledVector(cameraRight, dx * scale);
    target.addScaledVector(cameraUp, -dy * scale);
  };

  const zoom = (delta: number) => {
    targetRadius = Math.max(36, Math.min(initialRadius * 8, targetRadius + delta));
  };

  const hitNode = (event: MouseEvent): UniverseNode | null => {
    const rect = renderer.domElement.getBoundingClientRect();
    pointer.set(
      ((event.clientX - rect.left) / Math.max(rect.width, 1)) * 2 - 1,
      -((event.clientY - rect.top) / Math.max(rect.height, 1)) * 2 + 1
    );
    raycaster.params.Points = { threshold: Math.max(radius * 0.012, 2.5) };
    raycaster.setFromCamera(pointer, camera);
    const intersections = raycaster.intersectObjects(pointClouds.map((cloud) => cloud.points), false);
    const hit = intersections[0];
    if (!hit || hit.index === undefined) return null;
    const cloud = pointClouds.find((candidate) => candidate.points === hit.object);
    return cloud?.nodes[hit.index] ?? null;
  };

  const onPointerDown = (event: MouseEvent) => {
    isDragging = true;
    dragMoved = false;
    dragButton = event.button;
    lastPointerX = event.clientX;
    lastPointerY = event.clientY;
  };

  const onPointerMove = (event: MouseEvent) => {
    if (isDragging) {
      const deltaX = event.clientX - lastPointerX;
      const deltaY = event.clientY - lastPointerY;
      lastPointerX = event.clientX;
      lastPointerY = event.clientY;
      if (Math.abs(deltaX) + Math.abs(deltaY) > 2) dragMoved = true;
      if (dragButton !== 0 || refs.controlMode.current === 'pan') {
        pan(-deltaX, -deltaY);
      } else if (refs.activeDimension.current === '3D') {
        azimuth += deltaX * 0.005;
        elevation = THREE.MathUtils.clamp(elevation - deltaY * 0.005, 0.12, Math.PI - 0.12);
      } else {
        azimuth += deltaX * 0.005;
      }
      return;
    }
    if (hoverFrame) return;
    hoverFrame = requestAnimationFrame(() => {
      hoverFrame = 0;
      const node = hitNode(event);
      refs.hoveredNodeId.current = node?.id ?? null;
      options.onHoverNode?.(node, event.clientX, event.clientY);
    });
  };

  const onPointerUp = () => {
    isDragging = false;
    dragButton = 0;
  };

  const onClick = (event: MouseEvent) => {
    if (dragMoved || event.button !== 0) return;
    const node = hitNode(event);
    if (node) {
      flyToNode(node.id);
    } else {
      setFocus(null);
      options.onSelectNode?.('');
    }
  };

  const onWheel = (event: WheelEvent) => {
    event.preventDefault();
    zoom(event.deltaY * 0.45);
  };

  const onMouseLeave = () => {
    isDragging = false;
    refs.hoveredNodeId.current = null;
    options.onHoverNode?.(null, 0, 0);
  };

  const onResize = () => {
    const width = container.clientWidth || 1000;
    const height = container.clientHeight || 700;
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    renderer.setSize(width, height);
  };

  const animate = (now: number) => {
    animationFrame = requestAnimationFrame(animate);
    if (morphFrom && morphTo) {
      const t = THREE.MathUtils.clamp((now - morphStart) / 450, 0, 1);
      const eased = 1 - Math.pow(1 - t, 3);
      for (let index = 0; index < currentPositions.length; index += 1) {
        currentPositions[index] = THREE.MathUtils.lerp(morphFrom[index], morphTo[index], eased);
      }
      pointClouds.forEach((cloud) => {
        cloud.nodes.forEach((node, localIndex) => {
          const nodeIndex = nodeIndexById.get(node.id)!;
          const offset = nodeIndex * 3;
          cloud.points.geometry.attributes.position.setXYZ(
            localIndex,
            currentPositions[offset],
            currentPositions[offset + 1],
            currentPositions[offset + 2]
          );
        });
        const attribute = cloud.points.geometry.attributes.position as THREE.BufferAttribute;
        attribute.needsUpdate = true;
        cloud.points.geometry.computeBoundingSphere();
      });
      updateEdgePositions();
      if (t >= 1) {
        morphFrom = null;
        morphTo = null;
      }
    }
    updateCamera();
    if (focusedFileCard.visible && refs.selectedNodeId.current) {
      const focusedPosition = nodePosition(refs.selectedNodeId.current);
      if (focusedPosition) positionFocusedFileCard(focusedPosition);
    }
    renderer.render(scene, camera);
    frameCount += 1;
    const renderedAt = performance.now();
    const elapsed = renderedAt - fpsWindowStart;
    if (elapsed >= 1000) {
      options.onFps?.((frameCount * 1000) / elapsed);
      frameCount = 0;
      fpsWindowStart = renderedAt;
    }
  };

  refs.activeDimension.current = universe.dimensions;
  cameraRefocus(camera, target, initialRadius, universe.dimensions);
  refreshColors();
  setEdgeFilter(refs.edgeFilter.current);
  container.addEventListener('mousedown', onPointerDown);
  container.addEventListener('mousemove', onPointerMove);
  container.addEventListener('click', onClick);
  container.addEventListener('mouseleave', onMouseLeave);
  container.addEventListener('wheel', onWheel, { passive: false });
  container.addEventListener('contextmenu', preventContextMenu);
  window.addEventListener('mouseup', onPointerUp);
  window.addEventListener('resize', onResize);
  animationFrame = requestAnimationFrame(animate);

  const dispose = () => {
    themeRefreshToken += 1;
    cancelAnimationFrame(animationFrame);
    if (hoverFrame) cancelAnimationFrame(hoverFrame);
    container.removeEventListener('mousedown', onPointerDown);
    container.removeEventListener('mousemove', onPointerMove);
    container.removeEventListener('click', onClick);
    container.removeEventListener('mouseleave', onMouseLeave);
    container.removeEventListener('wheel', onWheel);
    container.removeEventListener('contextmenu', preventContextMenu);
    window.removeEventListener('mouseup', onPointerUp);
    window.removeEventListener('resize', onResize);
    pointClouds.forEach((cloud) => {
      scene.remove(cloud.points);
      cloud.points.geometry.dispose();
      (cloud.points.material as THREE.PointsMaterial).map?.dispose();
      (cloud.points.material as THREE.Material).dispose();
    });
    scene.remove(focusedFileCard);
    focusedFileMaterial.map?.dispose();
    focusedFileMaterial.dispose();
    [callsCloud, importsCloud].forEach((cloud) => {
      if (!cloud) return;
      scene.remove(cloud.lines);
      cloud.lines.geometry.dispose();
      (cloud.lines.material as THREE.Material).dispose();
    });
    scene.remove(grid);
    grid.geometry.dispose();
    (grid.material as THREE.Material).dispose();
    scene.remove(starfield);
    starfieldGeo.dispose();
    starfieldMaterial.dispose();
    refs.activeNeighbors.current.clear();
  };

  return {
    dispose,
    setDimension,
    setTheme,
    updateOverlay: refreshColors,
    setEdgeFilter,
    flyToNode,
    flyToCluster,
    recenter,
    pan,
    zoom,
  };
}

function createPointCloud(
  nodes: UniverseNode[],
  size: number,
  texture: THREE.CanvasTexture,
  isFileCard: boolean,
  positionById: Map<string, DenseLayoutPoint>,
): PointCloud | null {
  if (nodes.length === 0) return null;
  const positions = new Float32Array(nodes.length * 3);
  const colors = new Float32Array(nodes.length * 3);
  nodes.forEach((node, index) => {
    const position = positionById.get(node.id);
    positions.set(
      [position?.x ?? node.x, position?.y ?? node.y, position?.z ?? node.z ?? 0],
      index * 3,
    );
    colors.set([0.2, 0.6, 0.9], index * 3);
  });
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  geometry.computeBoundingSphere();
  const material = new THREE.PointsMaterial({
    size,
    sizeAttenuation: false,
    map: texture,
    alphaTest: 0.12,
    transparent: true,
    vertexColors: true,
    depthWrite: false,
  });
  return {
    points: new THREE.Points(geometry, material),
    nodes,
    colors: geometry.getAttribute('color') as THREE.BufferAttribute,
    texture,
    isFileCard,
  };
}

function createFileCardTexture(renderer: THREE.WebGLRenderer, isDark: boolean): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 512;
  canvas.height = 512;
  const context = canvas.getContext('2d');
  if (context) {
    context.clearRect(0, 0, 512, 512);

    // Deep Sci-Fi Obsidian/Smoked Glass Hologram Plate
    const plateGrad = context.createLinearGradient(0, 40, 512, 460);
    if (isDark) {
      plateGrad.addColorStop(0, 'rgba(10, 18, 32, 0.88)');
      plateGrad.addColorStop(0.5, 'rgba(7, 12, 22, 0.94)');
      plateGrad.addColorStop(1, 'rgba(4, 8, 16, 0.96)');
    } else {
      plateGrad.addColorStop(0, 'rgba(240, 246, 255, 0.92)');
      plateGrad.addColorStop(0.5, 'rgba(230, 238, 250, 0.95)');
      plateGrad.addColorStop(1, 'rgba(218, 228, 244, 0.97)');
    }
    context.fillStyle = plateGrad;

    // Angular Chamfered Sci-Fi Border Path (cut corners at 45 degrees)
    const x = 20;
    const y = 60;
    const w = 472;
    const h = 392;
    const cut = 32;

    context.beginPath();
    context.moveTo(x + cut, y);
    context.lineTo(x + w - cut, y);
    context.lineTo(x + w, y + cut);
    context.lineTo(x + w, y + h - cut);
    context.lineTo(x + w - cut, y + h);
    context.lineTo(x + cut, y + h);
    context.lineTo(x, y + h - cut);
    context.lineTo(x, y + cut);
    context.closePath();
    context.fill();

    // Laser Accent Border with Additive Glow Feel
    context.strokeStyle = isDark ? 'rgba(0, 240, 255, 0.85)' : 'rgba(2, 132, 199, 0.9)';
    context.lineWidth = 4;
    context.stroke();

    // Sci-Fi Corner Bracket Reticles (High-Tech HUD styling)
    const bracketLen = 42;
    context.strokeStyle = isDark ? '#ffffff' : '#0284c7';
    context.lineWidth = 5;

    // Top-left bracket
    context.beginPath();
    context.moveTo(x, y + bracketLen);
    context.lineTo(x, y + cut);
    context.lineTo(x + cut, y);
    context.lineTo(x + bracketLen, y);
    context.stroke();

    // Top-right bracket
    context.beginPath();
    context.moveTo(x + w - bracketLen, y);
    context.lineTo(x + w - cut, y);
    context.lineTo(x + w, y + cut);
    context.lineTo(x + w, y + bracketLen);
    context.stroke();

    // Bottom-left bracket
    context.beginPath();
    context.moveTo(x, y + h - bracketLen);
    context.lineTo(x, y + h - cut);
    context.lineTo(x + cut, y + h);
    context.lineTo(x + bracketLen, y + h);
    context.stroke();

    // Bottom-right bracket
    context.beginPath();
    context.moveTo(x + w - bracketLen, y + h);
    context.lineTo(x + w - cut, y + h);
    context.lineTo(x + w, y + h - cut);
    context.lineTo(x + w, y + h - bracketLen);
    context.stroke();

    // Inner Sci-Fi Circuit / Scan Lines
    context.strokeStyle = isDark ? 'rgba(0, 240, 255, 0.15)' : 'rgba(2, 132, 199, 0.15)';
    context.lineWidth = 1.5;
    context.beginPath();
    context.moveTo(x + 24, y + 80);
    context.lineTo(x + w - 24, y + 80);
    context.moveTo(x + 24, y + h - 50);
    context.lineTo(x + w - 24, y + h - 50);
    context.stroke();

    // High-Tech Holographic Chip Icon Area
    context.fillStyle = isDark ? 'rgba(0, 240, 255, 0.12)' : 'rgba(2, 132, 199, 0.12)';
    context.strokeStyle = isDark ? 'rgba(0, 240, 255, 0.45)' : 'rgba(2, 132, 199, 0.45)';
    context.lineWidth = 2;
    context.beginPath();
    context.roundRect(50, 110, 140, 230, 12);
    context.fill();
    context.stroke();

    // Geometric Sci-Fi File Glyphs
    context.fillStyle = isDark ? '#00f0ff' : '#0284c7';
    context.beginPath();
    context.moveTo(85, 145);
    context.lineTo(145, 145);
    context.lineTo(175, 175);
    context.lineTo(175, 305);
    context.lineTo(85, 305);
    context.closePath();
    context.fill();

    context.fillStyle = isDark ? '#0a1220' : '#ffffff';
    context.beginPath();
    context.moveTo(145, 145);
    context.lineTo(145, 175);
    context.lineTo(175, 175);
    context.closePath();
    context.fill();

    // Holographic Monospace Telemetry Typography
    context.fillStyle = isDark ? '#ffffff' : '#0f172a';
    context.font = '900 48px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.textAlign = 'left';
    context.textBaseline = 'middle';
    context.fillText('FILE', 225, 170);

    context.font = '700 24px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.fillStyle = isDark ? '#38bdf8' : '#0369a1';
    context.fillText('HOVER FOR PATH', 225, 245);

    // High-tech status telemetry indicator
    context.fillStyle = isDark ? '#00f0ff' : '#0284c7';
    context.font = '700 16px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.fillText('SYS // NODE-LOCK', 225, 305);

    // Glowing Neon Laser Energy Bar
    const barGrad = context.createLinearGradient(225, 325, 450, 325);
    barGrad.addColorStop(0, isDark ? '#00f0ff' : '#0284c7');
    barGrad.addColorStop(0.7, isDark ? '#38bdf8' : '#38bdf8');
    barGrad.addColorStop(1, isDark ? '#a855f7' : '#7c3aed');
    context.fillStyle = barGrad;
    context.fillRect(225, 325, 230, 8);
  }
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.generateMipmaps = true;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  texture.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
  return texture;
}

function createFocusedFileTexture(
  node: UniverseNode,
  filePath: string,
  isDark: boolean,
  accentHex: number,
): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 800;
  canvas.height = 460;
  const context = canvas.getContext('2d');
  if (context) {
    const fileName = filePath.split('/').pop() || node.label || filePath;
    const parentPath = filePath.includes('/') ? filePath.slice(0, filePath.lastIndexOf('/')) : 'repository root';
    const status = (node.status || 'ANALYZED').toUpperCase();
    const accent = new THREE.Color(accentHex);
    const accentCss = `#${accent.getHexString()}`;

    context.clearRect(0, 0, canvas.width, canvas.height);

    // Deep Sci-Fi Glass / Obsidian Plate with Chamfered Edges
    const bgGrad = context.createLinearGradient(0, 0, canvas.width, canvas.height);
    if (isDark) {
      bgGrad.addColorStop(0, 'rgba(10, 18, 34, 0.96)');
      bgGrad.addColorStop(0.6, 'rgba(6, 12, 22, 0.98)');
      bgGrad.addColorStop(1, 'rgba(3, 6, 14, 0.99)');
    } else {
      bgGrad.addColorStop(0, 'rgba(255, 255, 255, 0.98)');
      bgGrad.addColorStop(1, 'rgba(241, 245, 249, 0.98)');
    }
    context.fillStyle = bgGrad;

    const x = 14;
    const y = 14;
    const w = 772;
    const h = 432;
    const cut = 28;

    context.beginPath();
    context.moveTo(x + cut, y);
    context.lineTo(x + w - cut, y);
    context.lineTo(x + w, y + cut);
    context.lineTo(x + w, y + h - cut);
    context.lineTo(x + w - cut, y + h);
    context.lineTo(x + cut, y + h);
    context.lineTo(x, y + h - cut);
    context.lineTo(x, y + cut);
    context.closePath();
    context.fill();

    // Laser Edge Border
    context.strokeStyle = accentCss;
    context.lineWidth = 4;
    context.stroke();

    // Sci-Fi Reticle Brackets
    const bLen = 36;
    context.strokeStyle = isDark ? '#ffffff' : '#0284c7';
    context.lineWidth = 5;

    context.beginPath();
    context.moveTo(x, y + bLen);
    context.lineTo(x, y + cut);
    context.lineTo(x + cut, y);
    context.lineTo(x + bLen, y);
    context.stroke();

    context.beginPath();
    context.moveTo(x + w - bLen, y);
    context.lineTo(x + w - cut, y);
    context.lineTo(x + w, y + cut);
    context.lineTo(x + w, y + bLen);
    context.stroke();

    // File Chip Holographic Plate
    context.fillStyle = isDark ? 'rgba(0, 240, 255, 0.12)' : 'rgba(2, 132, 199, 0.12)';
    context.strokeStyle = accentCss;
    context.lineWidth = 2;
    context.beginPath();
    context.roundRect(44, 52, 136, 184, 14);
    context.fill();
    context.stroke();

    // Sci-Fi Folder/File Icon
    context.fillStyle = isDark ? '#ffffff' : '#0f172a';
    context.beginPath();
    context.moveTo(74, 82);
    context.lineTo(136, 82);
    context.lineTo(158, 104);
    context.lineTo(158, 206);
    context.lineTo(74, 206);
    context.closePath();
    context.fill();

    context.fillStyle = isDark ? '#080d1a' : '#f8fafc';
    context.beginPath();
    context.moveTo(136, 82);
    context.lineTo(136, 104);
    context.lineTo(158, 104);
    context.closePath();
    context.fill();

    // Filename & Path
    context.textAlign = 'left';
    context.textBaseline = 'middle';
    context.fillStyle = isDark ? '#ffffff' : '#0f172a';
    context.font = '900 38px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.fillText(shortenCanvasText(context, fileName, 510), 210, 92);

    context.fillStyle = isDark ? '#38bdf8' : '#0284c7';
    context.font = '600 21px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.fillText(shortenCanvasText(context, parentPath, 510), 210, 142);

    // Status Pill
    context.fillStyle = isDark ? 'rgba(0, 240, 255, 0.18)' : 'rgba(2, 132, 199, 0.15)';
    context.strokeStyle = accentCss;
    context.lineWidth = 1.5;
    context.beginPath();
    context.roundRect(210, 180, 200, 44, 8);
    context.fill();
    context.stroke();

    context.fillStyle = accentCss;
    context.font = '800 18px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.fillText(shortenCanvasText(context, status, 170), 226, 203);

    // Divider Line
    context.strokeStyle = isDark ? 'rgba(255, 255, 255, 0.14)' : 'rgba(0, 0, 0, 0.12)';
    context.lineWidth = 1.5;
    context.beginPath();
    context.moveTo(44, 268);
    context.lineTo(756, 268);
    context.stroke();

    // Telemetry Telemetry Metrics
    context.fillStyle = isDark ? '#cbd5e1' : '#334155';
    context.font = '700 22px ui-monospace, SFMono-Regular, "Roboto Mono", monospace';
    context.fillText(`${node.caller_count || 0} callers`, 48, 320);
    context.fillText(`${node.callee_count || 0} callees`, 280, 320);
    const lineRange = node.start_line
      ? `lines ${node.start_line}${node.end_line && node.end_line !== node.start_line ? `–${node.end_line}` : ''}`
      : 'file overview';
    context.fillText(lineRange, 510, 320);

    // Laser Energy Bar along Bottom
    const barGrad = context.createLinearGradient(48, 388, 752, 388);
    barGrad.addColorStop(0, accentCss);
    barGrad.addColorStop(0.5, isDark ? '#00f0ff' : '#0284c7');
    barGrad.addColorStop(1, isDark ? '#a855f7' : '#7c3aed');
    context.fillStyle = barGrad;
    context.fillRect(48, 388, 704, 8);
  }
  const texture = new THREE.CanvasTexture(canvas);
  texture.generateMipmaps = true;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  return texture;
}

function shortenCanvasText(
  context: CanvasRenderingContext2D,
  value: string,
  maximumWidth: number,
): string {
  if (context.measureText(value).width <= maximumWidth) return value;
  let shortened = value;
  while (shortened.length > 1 && context.measureText(`${shortened}…`).width > maximumWidth) {
    shortened = shortened.slice(0, -1);
  }
  return `${shortened}…`;
}

function createEdgeCloud(
  segments: EdgeSegment[],
  positions: number[],
  isDark: boolean
): EdgeCloud | null {
  if (segments.length === 0) return null;
  const colors = new Float32Array(segments.length * 6);
  const baseColor = new THREE.Color(isDark ? 0x00f0ff : 0x0284c7);
  for (let index = 0; index < segments.length * 2; index += 1) {
    colors[index * 3] = baseColor.r;
    colors[index * 3 + 1] = baseColor.g;
    colors[index * 3 + 2] = baseColor.b;
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  geometry.computeBoundingSphere();
  const material = new THREE.LineBasicMaterial({
    vertexColors: true,
    transparent: true,
    opacity: isDark ? 0.65 : 0.45,
    depthWrite: false,
    blending: isDark ? THREE.AdditiveBlending : THREE.NormalBlending,
  });
  return {
    lines: new THREE.LineSegments(geometry, material),
    segments,
    positions: geometry.getAttribute('position') as THREE.BufferAttribute,
    colors: geometry.getAttribute('color') as THREE.BufferAttribute,
  };
}

function cameraRefocus(
  camera: THREE.PerspectiveCamera,
  target: THREE.Vector3,
  radius: number,
  dimension: Dimension
) {
  if (dimension === '3D') {
    camera.position.set(
      target.x + radius * Math.sin(Math.PI / 3) * Math.sin(Math.PI / 4),
      target.y + radius * Math.cos(Math.PI / 3),
      target.z + radius * Math.sin(Math.PI / 3) * Math.cos(Math.PI / 4)
    );
  } else {
    camera.position.set(target.x, target.y, target.z + radius);
  }
  camera.lookAt(target);
}

function preventContextMenu(event: Event) {
  event.preventDefault();
}
