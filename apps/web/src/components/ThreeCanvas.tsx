'use client';

import React, { useEffect, useRef } from 'react';

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  baseRadius: number;
  currentRadius: number;
  color: string;
  glowColor: string;
  flash: number;
  pulsePhase: number;
  pulseSpeed: number;
}

interface Spark {
  startIdx: number;
  endIdx: number;
  progress: number;
  speed: number;
  sparkColor: string;
}

export default function ThreeCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d', { alpha: true });
    if (!ctx) return;

    let width = (canvas.width = window.innerWidth);
    let height = (canvas.height = window.innerHeight);

    // High-tech Neon Palette
    const palette = [
      { color: '#00f0ff', glow: 'rgba(0, 240, 255, 0.95)' },   // Neon Cyan
      { color: '#a855f7', glow: 'rgba(168, 85, 247, 0.95)' },  // Electric Violet
      { color: '#00e676', glow: 'rgba(0, 230, 118, 0.95)' },  // Emerald Green
      { color: '#38bdf8', glow: 'rgba(56, 189, 248, 0.95)' },  // Sky Electric
      { color: '#ffffff', glow: 'rgba(255, 255, 255, 1.0)' },   // Pure White
    ];

    // 220 Fine Pinpoint Neon Particles spread across entire viewport
    const particleCount = 220;
    const particles: Particle[] = [];

    for (let i = 0; i < particleCount; i++) {
      const pTheme = palette[i % palette.length];
      // Cinematic slow-motion drift
      const speed = 0.04 + Math.random() * 0.08;
      const angle = Math.random() * Math.PI * 2;

      particles.push({
        x: Math.random() * width,
        y: Math.random() * height,
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        baseRadius: 0.6 + Math.random() * 0.9,
        currentRadius: 0.9,
        color: pTheme.color,
        glowColor: pTheme.glow,
        flash: 0,
        pulsePhase: Math.random() * Math.PI * 2,
        pulseSpeed: 0.006 + Math.random() * 0.01,
      });
    }

    // 28 Elegant Slow-Motion Traveling Electric Energy Sparks
    const sparkCount = 28;
    const sparks: Spark[] = [];
    const sparkColorChoices = ['#ffffff', '#00f0ff', '#38bdf8', '#c084fc'];

    for (let s = 0; s < sparkCount; s++) {
      sparks.push({
        startIdx: Math.floor(Math.random() * particleCount),
        endIdx: Math.floor(Math.random() * particleCount),
        progress: Math.random(),
        // Graceful slow-motion transit speed
        speed: 0.0025 + Math.random() * 0.005,
        sparkColor: sparkColorChoices[s % sparkColorChoices.length],
      });
    }

    // Mouse Interaction
    let mouseX = width / 2;
    let mouseY = height / 2;
    let targetMouseX = width / 2;
    let targetMouseY = height / 2;

    const handleMouseMove = (e: MouseEvent) => {
      targetMouseX = e.clientX;
      targetMouseY = e.clientY;
    };
    window.addEventListener('mousemove', handleMouseMove);

    const handleResize = () => {
      if (!canvas) return;
      width = canvas.width = window.innerWidth;
      height = canvas.height = window.innerHeight;
    };
    window.addEventListener('resize', handleResize);

    let animId: number;
    const maxDist = 82; // Refined connection proximity threshold

    const render = () => {
      animId = requestAnimationFrame(render);

      // Smooth mouse tracking
      mouseX += (targetMouseX - mouseX) * 0.06;
      mouseY += (targetMouseY - mouseY) * 0.06;

      ctx.clearRect(0, 0, width, height);

      // 1. Update Particle Kinetic Drift with Viewport Wrap
      for (let i = 0; i < particleCount; i++) {
        const p = particles[i];

        // Smooth independent drift
        p.x += p.vx;
        p.y += p.vy;

        // Ambient breathing pulse
        p.pulsePhase += p.pulseSpeed;
        const breath = Math.sin(p.pulsePhase) * 0.25;

        // Viewport wrapping (ensures full permanent coverage)
        if (p.x < -15) p.x = width + 15;
        else if (p.x > width + 15) p.x = -15;

        if (p.y < -15) p.y = height + 15;
        else if (p.y > height + 15) p.y = -15;

        // Mouse deflection
        const dx = mouseX - p.x;
        const dy = mouseY - p.y;
        const distToMouse = Math.sqrt(dx * dx + dy * dy);
        if (distToMouse < 140 && distToMouse > 0) {
          const force = ((140 - distToMouse) / 140) * 0.35;
          p.x -= (dx / distToMouse) * force * 10;
          p.y -= (dy / distToMouse) * force * 10;
        }

        // Contact flash decay (responsive natural phosphorescent fade)
        if (p.flash > 0.01) {
          p.flash *= 0.90;
          p.currentRadius = p.baseRadius + breath + p.flash * 1.5;
        } else {
          p.currentRadius = p.baseRadius + breath;
        }
      }

      // 2. Real-Time Dynamic Circuit Lines
      const activeConnections: [number, number][] = [];

      for (let i = 0; i < particleCount; i++) {
        for (let j = i + 1; j < particleCount; j++) {
          const p1 = particles[i];
          const p2 = particles[j];

          const dx = p1.x - p2.x;
          const dy = p1.y - p2.y;
          const distSq = dx * dx + dy * dy;

          if (distSq < maxDist * maxDist) {
            const dist = Math.sqrt(distSq);
            const alpha = (1 - dist / maxDist) * 0.12;

            ctx.beginPath();
            ctx.moveTo(p1.x, p1.y);
            ctx.lineTo(p2.x, p2.y);
            ctx.strokeStyle = `rgba(0, 240, 255, ${alpha + (p1.flash + p2.flash) * 0.12})`;
            ctx.lineWidth = 0.45;
            ctx.stroke();

            activeConnections.push([i, j]);
          }
        }
      }

      // 3. Draw High-Density Neon Particles
      for (let i = 0; i < particleCount; i++) {
        const p = particles[i];

        ctx.beginPath();
        ctx.arc(p.x, p.y, Math.max(0.4, p.currentRadius), 0, Math.PI * 2);
        ctx.fillStyle = p.flash > 0.2 ? '#ffffff' : p.color;
        ctx.shadowColor = p.glowColor;
        ctx.shadowBlur = p.flash > 0.2 ? 8 : 4;
        ctx.fill();
        ctx.shadowBlur = 0; // reset
      }

      // 4. Draw Traveling Electric Sparks along Dynamic Circuits (Slow-Motion)
      for (let s = 0; s < sparkCount; s++) {
        const spark = sparks[s];
        spark.progress += spark.speed;

        // If spark arrived at destination or connection severed
        if (spark.progress >= 1.0 || activeConnections.length === 0) {
          // Trigger crisp electrical pulse
          if (particles[spark.endIdx]) {
            particles[spark.endIdx].flash = 0.60;
          }

          if (activeConnections.length > 0) {
            const randPair = activeConnections[Math.floor(Math.random() * activeConnections.length)];
            spark.startIdx = randPair[0];
            spark.endIdx = randPair[1];
          }
          spark.progress = 0;
          spark.speed = 0.010 + Math.random() * 0.012;
        }

        const p1 = particles[spark.startIdx];
        const p2 = particles[spark.endIdx];

        if (p1 && p2) {
          const t = spark.progress;
          const sx = (1 - t) * p1.x + t * p2.x;
          const sy = (1 - t) * p1.y + t * p2.y;

          // Electric bright spark dot in graceful slow motion
          ctx.beginPath();
          ctx.arc(sx, sy, 1.1, 0, Math.PI * 2);
          ctx.fillStyle = spark.sparkColor;
          ctx.shadowColor = '#00f0ff';
          ctx.shadowBlur = 6;
          ctx.fill();
          ctx.shadowBlur = 0;
        }
      }
    };

    render();

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('resize', handleResize);
      cancelAnimationFrame(animId);
    };
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className="fixed inset-0 pointer-events-none z-0 w-full h-full"
      style={{ opacity: 0.98 }}
    />
  );
}
