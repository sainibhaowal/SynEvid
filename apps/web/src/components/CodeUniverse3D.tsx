'use client';

import React, { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { GraphUniverse, UniverseNode } from '@/lib/api';
import { Box, Orbit, ZoomIn, ZoomOut, RotateCcw, Filter, Sparkles } from 'lucide-react';

interface CodeUniverse3DProps {
  universe: GraphUniverse;
  onSelectNode: (symbolId: string) => void;
  onToggleDimensions: () => void;
}

export default function CodeUniverse3D({
  universe,
  onSelectNode,
  onToggleDimensions,
}: CodeUniverse3DProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [hoveredNode, setHoveredNode] = useState<UniverseNode | null>(null);
  const [zoomLevel, setZoomLevel] = useState(1);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const width = container.clientWidth;
    const height = container.clientHeight;

    // Three.js Scene Setup
    const scene = new THREE.Scene();
    const is3D = universe.dimensions === '3D';

    // Camera
    const camera = new THREE.PerspectiveCamera(45, width / height, 0.1, 1000);
    camera.position.z = is3D ? 180 : 160;

    // Renderer
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.innerHTML = '';
    container.appendChild(renderer.domElement);

    // Lighting
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.8);
    scene.add(ambientLight);
    const pointLight = new THREE.PointLight(0x00f0ff, 1.5, 300);
    pointLight.position.set(50, 50, 100);
    scene.add(pointLight);

    // Scale Factor
    const spread = 80;
    const nodeObjects: { mesh: THREE.Mesh; data: UniverseNode }[] = [];

    // Group for entire universe (enables smooth rotation & orbit)
    const universeGroup = new THREE.Group();
    scene.add(universeGroup);

    // Node Colors by cluster
    const clusterColors = [0x00f0ff, 0x10b981, 0x8b5cf6, 0xf59e0b, 0xec4899, 0x3b82f6];

    // Build Node Meshes
    const nodeMap = new Map<string, THREE.Vector3>();
    const sphereGeo = new THREE.SphereGeometry(is3D ? 3.2 : 2.8, 24, 24);

    universe.nodes.forEach((node) => {
      const color = clusterColors[(node.cluster_id || 0) % clusterColors.length];
      const mat = new THREE.MeshStandardMaterial({
        color,
        emissive: color,
        emissiveIntensity: 0.35,
        roughness: 0.2,
        metalness: 0.8,
      });

      const mesh = new THREE.Mesh(sphereGeo, mat);
      const x = node.x * spread;
      const y = node.y * spread;
      const z = is3D ? (node.z || 0) * spread : 0;

      mesh.position.set(x, y, z);
      universeGroup.add(mesh);
      nodeObjects.push({ mesh, data: node });
      nodeMap.set(node.id, mesh.position);
    });

    // Build Edges
    if (universe.edges && universe.edges.length > 0) {
      const linePositions: number[] = [];
      const lineColors: number[] = [];

      universe.edges.forEach((edge) => {
        const p1 = nodeMap.get(edge.source);
        const p2 = nodeMap.get(edge.target);
        if (p1 && p2) {
          linePositions.push(p1.x, p1.y, p1.z);
          linePositions.push(p2.x, p2.y, p2.z);

          // Subtle cyan-to-violet gradient
          lineColors.push(0.0, 0.94, 1.0);
          lineColors.push(0.54, 0.36, 0.96);
        }
      });

      const lineGeo = new THREE.BufferGeometry();
      lineGeo.setAttribute('position', new THREE.Float32BufferAttribute(linePositions, 3));
      lineGeo.setAttribute('color', new THREE.Float32BufferAttribute(lineColors, 3));

      const lineMat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 0.45,
        linewidth: 1.5,
      });

      const lines = new THREE.LineSegments(lineGeo, lineMat);
      universeGroup.add(lines);
    }

    // Interactive Orbit & Rotation State
    let isDragging = false;
    let prevMouseX = 0;
    let prevMouseY = 0;
    let rotY = 0;
    let rotX = 0;

    const onMouseDown = (e: MouseEvent) => {
      isDragging = true;
      prevMouseX = e.clientX;
      prevMouseY = e.clientY;
    };

    const onMouseUp = () => {
      isDragging = false;
    };

    const onMouseMove = (e: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const mouseX = ((e.clientX - rect.left) / width) * 2 - 1;
      const mouseY = -((e.clientY - rect.top) / height) * 2 + 1;

      if (isDragging) {
        const deltaX = e.clientX - prevMouseX;
        const deltaY = e.clientY - prevMouseY;
        prevMouseX = e.clientX;
        prevMouseY = e.clientY;

        rotY += deltaX * 0.008;
        rotX += deltaY * 0.008;
        universeGroup.rotation.y = rotY;
        if (is3D) {
          universeGroup.rotation.x = Math.max(-Math.PI / 3, Math.min(Math.PI / 3, rotX));
        }
      } else {
        // Raycast for Hover
        const raycaster = new THREE.Raycaster();
        raycaster.setFromCamera(new THREE.Vector2(mouseX, mouseY), camera);
        const intersects = raycaster.intersectObjects(nodeObjects.map((n) => n.mesh));

        if (intersects.length > 0) {
          const hit = nodeObjects.find((n) => n.mesh === intersects[0].object);
          if (hit) {
            setHoveredNode(hit.data);
            container.style.cursor = 'pointer';
            return;
          }
        }
        setHoveredNode(null);
        container.style.cursor = isDragging ? 'grabbing' : 'grab';
      }
    };

    const onClick = (e: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const mouseX = ((e.clientX - rect.left) / width) * 2 - 1;
      const mouseY = -((e.clientY - rect.top) / height) * 2 + 1;

      const raycaster = new THREE.Raycaster();
      raycaster.setFromCamera(new THREE.Vector2(mouseX, mouseY), camera);
      const intersects = raycaster.intersectObjects(nodeObjects.map((n) => n.mesh));

      if (intersects.length > 0) {
        const hit = nodeObjects.find((n) => n.mesh === intersects[0].object);
        if (hit) {
          onSelectNode(hit.data.id);
        }
      }
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      camera.position.z = Math.max(60, Math.min(350, camera.position.z + e.deltaY * 0.1));
    };

    container.addEventListener('mousedown', onMouseDown);
    window.addEventListener('mouseup', onMouseUp);
    container.addEventListener('mousemove', onMouseMove);
    container.addEventListener('click', onClick);
    container.addEventListener('wheel', onWheel, { passive: false });

    // Animation Loop
    let animId: number;
    const animate = () => {
      animId = requestAnimationFrame(animate);

      // Subtle slow auto-rotation when idle
      if (!isDragging && is3D) {
        universeGroup.rotation.y += 0.001;
      }

      renderer.render(scene, camera);
    };
    animate();

    const handleResize = () => {
      if (!container) return;
      const newW = container.clientWidth;
      const newH = container.clientHeight;
      camera.aspect = newW / newH;
      camera.updateProjectionMatrix();
      renderer.setSize(newW, newH);
    };
    window.addEventListener('resize', handleResize);

    return () => {
      cancelAnimationFrame(animId);
      container.removeEventListener('mousedown', onMouseDown);
      window.removeEventListener('mouseup', onMouseUp);
      container.removeEventListener('mousemove', onMouseMove);
      container.removeEventListener('click', onClick);
      container.removeEventListener('wheel', onWheel);
      window.removeEventListener('resize', handleResize);
      renderer.dispose();
    };
  }, [universe, onSelectNode]);

  return (
    <div className="relative w-full h-[540px] rounded-xl overflow-hidden border border-[var(--border-subtle)] bg-[#04060b] shadow-inner">
      {/* Three.js Container */}
      <div ref={containerRef} className="w-full h-full cursor-grab active:cursor-grabbing" />

      {/* Floating Controls HUD */}
      <div className="absolute top-4 right-4 flex items-center space-x-2 bg-[var(--card)]/80 backdrop-blur-md px-3 py-1.5 rounded-lg border border-[var(--border-subtle)] shadow-lg">
        <button
          onClick={onToggleDimensions}
          className="text-xs font-mono font-semibold px-2.5 py-1 rounded bg-[var(--card-elevated)] hover:bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border border-[var(--border-subtle)] transition-colors flex items-center space-x-1"
        >
          {universe.dimensions === '3D' ? <Box className="w-3.5 h-3.5" /> : <Orbit className="w-3.5 h-3.5" />}
          <span>{universe.dimensions === '3D' ? '2D Planar' : '3D Spatial Orbit'}</span>
        </button>
      </div>

      {/* Floating Info Badge */}
      <div className="absolute bottom-4 left-4 bg-[var(--card)]/80 backdrop-blur-md px-3 py-2 rounded-lg border border-[var(--border-subtle)] text-xs font-mono text-[var(--text-secondary)] shadow-lg">
        <div className="flex items-center space-x-2">
          <Sparkles className="w-3.5 h-3.5 text-cyan-500" />
          <span className="font-semibold text-[var(--text-primary)]">Deterministic Seeded Layout</span>
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 font-bold">
            100/100 Repeatable
          </span>
        </div>
        <div className="mt-1 text-[11px] text-[var(--text-muted)]">
          {universe.node_count} Symbols &bull; {universe.edge_count} Dependency Edges &bull; Precision: 10<sup>-4</sup>
        </div>
      </div>

      {/* Hover Tooltip */}
      {hoveredNode && (
        <div className="absolute top-4 left-4 bg-[var(--card-elevated)] border border-cyan-500/40 px-3.5 py-2.5 rounded-lg shadow-xl pointer-events-none animate-fadeIn">
          <div className="text-xs font-mono font-bold text-cyan-600 dark:text-cyan-400">{hoveredNode.id}</div>
          <div className="text-[11px] text-[var(--text-muted)] mt-0.5 font-mono">
            Cluster: #{hoveredNode.cluster_id ?? 0} &bull; Click to inspect Ego-Network
          </div>
        </div>
      )}
    </div>
  );
}
