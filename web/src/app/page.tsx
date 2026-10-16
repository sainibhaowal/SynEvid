'use client';

import React, { useState, useEffect } from 'react';
import Image from 'next/image';
import { motion, useScroll, useSpring } from 'framer-motion';
import {
  ShieldCheck,
  Cpu,
  GitBranch,
  Layers,
  Terminal,
  Lock,
  ArrowRight,
  ExternalLink,
  Code2,
  FileCheck,
  Eye,
  Server,
  ChevronUp
} from 'lucide-react';

import ThreeCanvas from '@/components/ThreeCanvas';
import TerminalWorkbench from '@/components/TerminalWorkbench';
import LifecycleSection from '@/components/LifecycleSection';
import BenchmarkSection from '@/components/BenchmarkSection';

export default function HomePage() {
  const { scrollYProgress, scrollY } = useScroll();
  const scaleX = useSpring(scrollYProgress, {
    stiffness: 120,
    damping: 24,
    restDelta: 0.001,
  });

  const [showScrollTop, setShowScrollTop] = useState(false);

  useEffect(() => {
    return scrollY.on('change', (latest) => {
      setShowScrollTop(latest > 400);
    });
  }, [scrollY]);

  const scrollToTop = () => {
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  return (
    <div className="min-h-screen bg-[#05080e] text-[#f0f4fc] relative selection:bg-cyan-500 selection:text-black">
      {/* Dynamic Scroll Progress Indicator */}
      <motion.div
        style={{ scaleX }}
        className="fixed top-0 left-0 right-0 h-1 bg-gradient-to-r from-cyan-400 via-purple-500 to-green-400 origin-left z-[100] shadow-[0_0_15px_rgba(0,240,255,0.8)]"
      />

      {/* Permanent Global Neon Particle Electric Circuit Background */}
      <ThreeCanvas />

      {/* Background Cyber Grid */}
      <div className="cyber-grid fixed inset-0 pointer-events-none z-0 opacity-30" />

      {/* Navigation Header */}
      <header className="sticky top-0 z-50 backdrop-blur-xl bg-[#060910]/85 border-b border-white/10 transition-colors">
        <div className="max-w-7xl mx-auto px-6 h-20 flex items-center justify-between">
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.5 }}
            className="flex items-center space-x-3.5"
          >
            {/* Accurate SynEvid Crystal Logo in Clean White Box */}
            <div className="relative group flex items-center justify-center w-11 h-11 md:w-12 md:h-12 rounded-xl bg-white p-1 shadow-[0_0_18px_rgba(255,255,255,0.35)] hover:shadow-[0_0_26px_rgba(255,255,255,0.6)] transition-all shrink-0 overflow-hidden">
              <img
                src="/assets/logo_clean_emblem.png"
                alt="SynEvid Crystal Logo"
                className="w-full h-full object-contain"
              />
            </div>
            <div className="flex flex-col justify-center">
              <span className="font-brand font-bold text-xl md:text-2xl tracking-[0.16em] text-white uppercase neon-title-glow leading-tight">
                SYNEVID
              </span>
              <span className="text-[9px] md:text-[10px] tracking-[0.24em] text-cyan-300 font-brand font-semibold uppercase opacity-95">
                CODE AUTOPSY
              </span>
            </div>
          </motion.div>

          <nav className="hidden md:flex items-center space-x-8 text-sm font-medium text-gray-300">
            <a href="#engineering" className="hover:text-cyan-400 transition-colors">
              Engineering
            </a>
            <a href="#lifecycle" className="hover:text-cyan-400 transition-colors">
              Agent Protocol
            </a>
            <a href="#workbench" className="hover:text-cyan-400 transition-colors">
              CLI Workbench
            </a>
            <a href="#benchmarks" className="hover:text-cyan-400 transition-colors">
              Benchmarks
            </a>
            <a href="#tooling" className="hover:text-cyan-400 transition-colors">
              Inspector &amp; MCP
            </a>
          </nav>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.5 }}
            className="flex items-center space-x-3"
          >
            <a
              href="https://github.com/sainibhaowal/SynEvid"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center space-x-2 text-xs font-mono font-semibold px-4 py-2.5 rounded-xl border border-cyan-500/30 bg-cyan-950/30 hover:bg-cyan-500/15 text-cyan-300 hover:border-cyan-400 transition-all shadow-sm shadow-cyan-950 hover:shadow-cyan-500/20"
            >
              <GitBranch className="w-4 h-4" />
              <span>GitHub Repo</span>
            </a>
          </motion.div>
        </div>
      </header>

      {/* Hero Section */}
      <section className="relative pt-20 pb-28 overflow-hidden">
        <div className="max-w-7xl mx-auto px-6 relative z-10 text-center">
          {/* Status Badge */}
          <motion.div
            initial={{ opacity: 0, y: -20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.3 }}
            transition={{ duration: 0.6 }}
            className="inline-flex items-center space-x-2 px-4 py-1.5 rounded-full border border-cyan-500/30 bg-cyan-950/40 text-cyan-300 text-xs font-mono uppercase tracking-wider mb-10 backdrop-blur-md"
          >
            <span className="w-2 h-2 rounded-full bg-cyan-400 animate-ping mr-1" />
            <span>100/100 Deterministic Repeatability • Zero LLM in Core • Rust 2024</span>
          </motion.div>

          {/* Official SynEvid Brand Banner Title (Matching Exact Screenshot) */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.3 }}
            transition={{ duration: 0.7 }}
            className="mb-8"
          >
            <h1 className="font-brand text-5xl sm:text-7xl lg:text-8xl font-bold uppercase tracking-[0.16em] text-white leading-none mb-4 neon-banner-glow">
              <span className="bg-gradient-to-b from-white via-cyan-100 to-cyan-300 bg-clip-text text-transparent">
                SYNEVID
              </span>
            </h1>
            <div className="font-brand text-xs sm:text-sm lg:text-base font-semibold uppercase tracking-[0.22em] text-cyan-200/90 max-w-4xl mx-auto neon-sub-glow">
              DETERMINISTIC VERIFICATION &amp; CHANGE-IMPACT ENGINE
            </div>
          </motion.div>

          {/* Hero Motto */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.3 }}
            transition={{ duration: 0.7, delay: 0.15 }}
            className="mb-6"
          >
            <h2 className="font-brand text-xl sm:text-3xl lg:text-4xl font-bold uppercase tracking-[0.14em] text-gray-200">
              <span className="bg-gradient-to-r from-cyan-300 via-white to-cyan-400 bg-clip-text text-transparent drop-shadow-[0_0_20px_rgba(0,240,255,0.7)]">
                Evidence
              </span>{' '}
              Is All You Need.
            </h2>
          </motion.div>

          {/* Hero Subtitle */}
          <motion.p
            initial={{ opacity: 0, y: 25 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.3 }}
            transition={{ duration: 0.7, delay: 0.2 }}
            className="text-base sm:text-xl text-gray-300 max-w-3xl mx-auto font-normal leading-relaxed mb-10"
          >
            Offline ground-truth verification for autonomous AI coding agents and safety-critical engineering.
            Cryptographic BLAKE3 receipts, honest semantic coverage, and zero LLM in correctness crates.
          </motion.p>

          {/* Hero CTAs */}
          <motion.div
            initial={{ opacity: 0, y: 25 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.3 }}
            transition={{ duration: 0.7, delay: 0.3 }}
            className="flex flex-wrap items-center justify-center gap-4 mb-16"
          >
            <motion.a
              href="#workbench"
              whileHover={{ scale: 1.03 }}
              whileTap={{ scale: 0.98 }}
              className="flex items-center space-x-2.5 px-6 py-3.5 rounded-xl font-semibold text-black bg-cyan-400 hover:bg-cyan-300 transition-all shadow-lg shadow-cyan-500/25 text-sm font-sans"
            >
              <Terminal className="w-4 h-4" />
              <span>Explore Interactive CLI</span>
              <ArrowRight className="w-4 h-4 ml-1" />
            </motion.a>

            <motion.a
              href="#engineering"
              whileHover={{ scale: 1.03 }}
              whileTap={{ scale: 0.98 }}
              className="flex items-center space-x-2 px-6 py-3.5 rounded-xl font-semibold text-white bg-white/5 hover:bg-white/10 border border-white/10 hover:border-cyan-500/30 transition-all text-sm backdrop-blur-md"
            >
              <ShieldCheck className="w-4 h-4 text-cyan-400" />
              <span>Forensic Architecture</span>
            </motion.a>
          </motion.div>

          {/* Banner Showcase with Scroll Parallax */}
          <motion.div
            initial={{ opacity: 0, y: 40, scale: 0.96 }}
            whileInView={{ opacity: 1, y: 0, scale: 1 }}
            viewport={{ once: false, amount: 0.2 }}
            transition={{ duration: 0.8, ease: 'easeOut' }}
            className="relative max-w-5xl mx-auto rounded-3xl p-1.5 bg-gradient-to-br from-cyan-500/40 via-purple-500/30 to-green-500/20 shadow-2xl shadow-cyan-950/60"
          >
            <div className="rounded-[22px] overflow-hidden bg-[#070b13] border border-white/10">
              <img
                src="/assets/banner.jpg"
                alt="SynEvid Banner — Deterministic Software Verification"
                className="w-full h-auto object-cover opacity-90 hover:opacity-100 transition-opacity duration-300"
              />
            </div>
          </motion.div>
        </div>
      </section>

      {/* Proof & Telemetry Numbers Bar with Staggered Scroll Animation */}
      <section className="border-t border-b border-white/10 bg-[#060a12]/80 py-12">
        <div className="max-w-7xl mx-auto px-6 grid grid-cols-2 md:grid-cols-4 gap-6 text-center">
          {[
            { val: '100/100', label: 'Deterministic Repeatability (BLAKE3)', color: 'text-cyan-400' },
            { val: '+31.73pp', label: 'Benchmark Recall Advantage', color: 'text-green-400' },
            { val: '0 LLM', label: 'In Correctness Crates (Math Only)', color: 'text-purple-400' },
            { val: '161/161', label: 'Strict Verification Tests Passing', color: 'text-cyan-300' }
          ].map((item, idx) => (
            <motion.div
              key={idx}
              initial={{ opacity: 0, y: 30 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: false, amount: 0.2 }}
              transition={{ duration: 0.5, delay: idx * 0.1 }}
              whileHover={{ y: -4 }}
              className="p-5 rounded-2xl bg-white/[0.02] border border-white/5 hover:border-cyan-500/25 transition-all shadow-lg"
            >
              <div className={`text-3xl sm:text-4xl font-extrabold ${item.color} font-mono mb-1.5`}>
                {item.val}
              </div>
              <div className="text-xs uppercase tracking-wider text-gray-400 font-medium">
                {item.label}
              </div>
            </motion.div>
          ))}
        </div>
      </section>

      {/* What Kind of Engineering Section with Scroll Reveals */}
      <section id="engineering" className="py-28 max-w-7xl mx-auto px-6">
        <motion.div
          initial={{ opacity: 0, y: 30 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: false, amount: 0.2 }}
          transition={{ duration: 0.6 }}
          className="text-center max-w-3xl mx-auto mb-16"
        >
          <div className="inline-flex items-center space-x-2 px-3 py-1 rounded-full border border-cyan-500/30 bg-cyan-950/20 text-cyan-400 text-xs font-mono uppercase tracking-wider mb-4">
            <Cpu className="w-3.5 h-3.5 mr-1" />
            <span>Systems Engineering Truth</span>
          </div>
          <h2 className="text-3xl sm:text-5xl font-extrabold text-white tracking-tight mb-4">
            What Kind of Engineering Is SynEvid?
          </h2>
          <p className="text-gray-400 text-base sm:text-lg">
            SynEvid is <strong>not</strong> a chatbot, code synthesizer, or LLM wrapper. It is a mathematical forensic verification oracle engineered in Rust to compute bounded semantic blast radius and seal cryptographic evidence.
          </p>
        </motion.div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {[
            {
              icon: <ShieldCheck className="w-6 h-6" />,
              title: 'Zero LLM in Correctness Crates',
              desc: 'Core verification crates (crates/*) never import or call LLM APIs. Invariants, graph traversals, and contract checks are 100% deterministic code.',
              badgeColor: 'bg-cyan-500/10 text-cyan-400 border-cyan-500/25'
            },
            {
              icon: <GitBranch className="w-6 h-6" />,
              title: 'Bounded Blast Radius',
              desc: 'Calculates transitive dependency closures via Petgraph multigraphs, Tarjan SCC cycle condensations, and ranked shortest paths across symbol identities.',
              badgeColor: 'bg-purple-500/10 text-purple-400 border-purple-500/25'
            },
            {
              icon: <FileCheck className="w-6 h-6" />,
              title: 'BLAKE3 Cryptographic Receipts',
              desc: 'Generates tamper-proof evidence receipts with byte-identical digests across identical repository states, enabling mathematical CI/CD and agent PR gates.',
              badgeColor: 'bg-green-500/10 text-green-400 border-green-500/25'
            },
            {
              icon: <Layers className="w-6 h-6" />,
              title: 'Honest Semantic Coverage',
              desc: 'Unsupported or dynamic constructs emit explicit CoverageState::Unknown. UNKNOWN never collapses into PASS. Over-approximation is disclosed, not faked.',
              badgeColor: 'bg-amber-500/10 text-amber-400 border-amber-500/25'
            },
            {
              icon: <Lock className="w-6 h-6" />,
              title: 'Read-Only Analyzer Invariant',
              desc: 'Commands strictly inspect repository state and change deltas; they never mutate source code or git history. Safe to run in any critical environment.',
              badgeColor: 'bg-blue-500/10 text-blue-400 border-blue-500/25'
            },
            {
              icon: <Code2 className="w-6 h-6" />,
              title: 'Enterprise Sandboxing',
              desc: 'Zero outbound network sockets. Zero sub-process execution. Symlink escape containment and path-traversal sanitization across Linux, macOS, and Windows.',
              badgeColor: 'bg-cyan-500/10 text-cyan-400 border-cyan-500/25'
            }
          ].map((item, idx) => (
            <motion.div
              key={idx}
              initial={{ opacity: 0, y: 40 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: false, amount: 0.15 }}
              transition={{ duration: 0.5, delay: (idx % 3) * 0.1 }}
              whileHover={{ y: -6, transition: { duration: 0.2 } }}
              className="p-8 rounded-2xl border border-white/10 bg-[#090e18]/80 backdrop-blur-xl relative overflow-hidden group shadow-lg hover:border-cyan-500/40 hover:shadow-cyan-950/40"
            >
              <div className={`w-12 h-12 rounded-xl border flex items-center justify-center mb-6 group-hover:scale-110 transition-transform ${item.badgeColor}`}>
                {item.icon}
              </div>
              <h3 className="text-xl font-bold text-white mb-2">{item.title}</h3>
              <p className="text-sm text-gray-400 leading-relaxed font-sans">{item.desc}</p>
            </motion.div>
          ))}
        </div>
      </section>

      {/* In-Session Lifecycle Flow Section */}
      <div id="lifecycle">
        <LifecycleSection />
      </div>

      {/* Terminal Workbench Section */}
      <section id="workbench" className="py-28 max-w-7xl mx-auto px-6">
        <motion.div
          initial={{ opacity: 0, y: 30 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: false, amount: 0.2 }}
          transition={{ duration: 0.6 }}
          className="text-center max-w-3xl mx-auto mb-14"
        >
          <div className="inline-flex items-center space-x-2 px-3 py-1 rounded-full border border-cyan-500/30 bg-cyan-950/20 text-cyan-400 text-xs font-mono uppercase tracking-wider mb-4">
            <Terminal className="w-3.5 h-3.5 mr-1" />
            <span>Developer Workbench</span>
          </div>
          <h2 className="text-3xl sm:text-5xl font-extrabold text-white tracking-tight mb-4">
            Deterministic CLI in Action
          </h2>
          <p className="text-gray-400 text-base sm:text-lg">
            Inspect real commands executed by the compiled <code>autopsy</code> binary across verification, impact assessment, deltas, and evidence explainability.
          </p>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 40 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: false, amount: 0.15 }}
          transition={{ duration: 0.7 }}
        >
          <TerminalWorkbench />
        </motion.div>
      </section>

      {/* Benchmarks Section */}
      <div id="benchmarks">
        <BenchmarkSection />
      </div>

      {/* Web Inspector & MCP Agent Tooling */}
      <section id="tooling" className="py-28 border-t border-white/10 bg-[#070b14]/70">
        <div className="max-w-7xl mx-auto px-6">
          <motion.div
            initial={{ opacity: 0, y: 30 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: false, amount: 0.2 }}
            transition={{ duration: 0.6 }}
            className="text-center max-w-3xl mx-auto mb-16"
          >
            <div className="inline-flex items-center space-x-2 px-3 py-1 rounded-full border border-purple-500/30 bg-purple-950/20 text-purple-400 text-xs font-mono uppercase tracking-wider mb-4">
              <Server className="w-3.5 h-3.5 mr-1" />
              <span>Ecosystem Interfaces</span>
            </div>
            <h2 className="text-3xl sm:text-5xl font-extrabold text-white tracking-tight mb-4">
              Zero-Backend Web Inspector & MCP Server
            </h2>
            <p className="text-gray-400 text-base sm:text-lg">
              SynEvid integrates with human reviewers and frontier AI agents alike through read-only, time-free visualizers and protocol bridges.
            </p>
          </motion.div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
            <motion.div
              initial={{ opacity: 0, x: -30 }}
              whileInView={{ opacity: 1, x: 0 }}
              viewport={{ once: false, amount: 0.2 }}
              transition={{ duration: 0.6 }}
              whileHover={{ y: -4 }}
              className="p-8 rounded-2xl border border-white/10 bg-[#0a0f1b]/80 backdrop-blur-xl hover:border-cyan-500/30 transition-all shadow-xl"
            >
              <div className="flex items-center space-x-3 mb-4">
                <div className="p-3 rounded-xl bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
                  <Eye className="w-6 h-6" />
                </div>
                <div>
                  <h3 className="text-xl font-bold text-white">apps/web-inspector</h3>
                  <div className="text-xs font-mono text-cyan-400">Offline Forensic Evidence Board</div>
                </div>
              </div>
              <p className="text-sm text-gray-300 leading-relaxed mb-6 font-sans">
                A lightweight, zero-backend, read-only visual evidence board. Displays cryptographic verification reports, temporal revision journals, and multigraph blast radius paths directly from <code>autopsy</code> JSON artifacts.
              </p>
              <div className="bg-[#05080e] p-4 rounded-xl border border-white/5 font-mono text-xs text-gray-400">
                <span className="text-green-400">$</span> cargo run -p autopsy-cli -- --format json temporal &gt; journal.json
              </div>
            </motion.div>

            <motion.div
              initial={{ opacity: 0, x: 30 }}
              whileInView={{ opacity: 1, x: 0 }}
              viewport={{ once: false, amount: 0.2 }}
              transition={{ duration: 0.6 }}
              whileHover={{ y: -4 }}
              className="p-8 rounded-2xl border border-white/10 bg-[#0a0f1b]/80 backdrop-blur-xl hover:border-purple-500/30 transition-all shadow-xl"
            >
              <div className="flex items-center space-x-3 mb-4">
                <div className="p-3 rounded-xl bg-purple-500/10 text-purple-400 border border-purple-500/20">
                  <Server className="w-6 h-6" />
                </div>
                <div>
                  <h3 className="text-xl font-bold text-white">apps/mcp-server</h3>
                  <div className="text-xs font-mono text-purple-400">Model Context Protocol Bridge</div>
                </div>
              </div>
              <p className="text-sm text-gray-300 leading-relaxed mb-6 font-sans">
                Enables frontier coding agents (Claude, Codex, Gemini) to invoke SynEvid natively during conversation sessions. Gives agents deterministic ground truth before they propose code edits.
              </p>
              <div className="bg-[#05080e] p-4 rounded-xl border border-white/5 font-mono text-xs text-gray-400">
                <span className="text-purple-400">tools:</span> [autopsy_baseline, autopsy_impact, autopsy_verify, autopsy_explain]
              </div>
            </motion.div>
          </div>
        </div>
      </section>

      {/* Floating Scroll-to-Top Button */}
      {showScrollTop && (
        <motion.button
          initial={{ opacity: 0, scale: 0.8 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.8 }}
          whileHover={{ scale: 1.1, backgroundColor: 'rgba(0, 240, 255, 0.25)' }}
          whileTap={{ scale: 0.95 }}
          onClick={scrollToTop}
          aria-label="Scroll to top"
          className="fixed bottom-8 right-8 z-50 p-3.5 rounded-full bg-[#0a0f1b]/90 border border-cyan-500/40 text-cyan-400 shadow-xl shadow-cyan-950/60 backdrop-blur-md transition-colors"
        >
          <ChevronUp className="w-5 h-5" />
        </motion.button>
      )}

      {/* Footer */}
      <footer className="border-t border-white/10 bg-[#04060a] py-16">
        <div className="max-w-7xl mx-auto px-6">
          <div className="grid grid-cols-1 md:grid-cols-4 gap-10 mb-12">
            <div className="md:col-span-1">
              <div className="flex items-center space-x-3 mb-4">
                <div className="relative group flex items-center justify-center w-10 h-10 rounded-lg bg-white p-1 shadow-[0_0_12px_rgba(255,255,255,0.3)] shrink-0 overflow-hidden">
                  <img src="/assets/logo_clean_emblem.png" alt="SynEvid Logo" className="w-full h-full object-contain" />
                </div>
                <div className="flex flex-col">
                  <span className="font-brand font-bold text-lg tracking-[0.16em] text-white uppercase neon-title-glow leading-tight">
                    SYNEVID
                  </span>
                  <span className="text-[8px] tracking-[0.2em] text-cyan-300 font-brand font-medium uppercase opacity-90">
                    CODE AUTOPSY
                  </span>
                </div>
              </div>
              <p className="text-xs text-gray-400 leading-relaxed mb-4">
                Evidence Is All You Need. Offline, mathematically deterministic software verification & change-impact engine.
              </p>
              <div className="text-[11px] font-mono text-cyan-400/80">
                BLAKE3 Content-Addressed Proofs
              </div>
            </div>

            <div>
              <div className="text-xs font-mono uppercase text-gray-300 tracking-wider font-bold mb-4">
                Core Engine
              </div>
              <ul className="space-y-2 text-xs text-gray-400">
                <li><a href="#engineering" className="hover:text-cyan-400 transition-colors">Forensic Architecture</a></li>
                <li><a href="#lifecycle" className="hover:text-cyan-400 transition-colors">Agent Lifecycle Protocol</a></li>
                <li><a href="#workbench" className="hover:text-cyan-400 transition-colors">CLI Commands</a></li>
                <li><a href="#benchmarks" className="hover:text-cyan-400 transition-colors">Benchmark Gate</a></li>
              </ul>
            </div>

            <div>
              <div className="text-xs font-mono uppercase text-gray-300 tracking-wider font-bold mb-4">
                Monorepo
              </div>
              <ul className="space-y-2 text-xs text-gray-400">
                <li><a href="https://github.com/sainibhaowal/SynEvid/tree/main/crates" target="_blank" className="hover:text-cyan-400 transition-colors">Rust Crates</a></li>
                <li><a href="https://github.com/sainibhaowal/SynEvid/tree/main/apps/web-inspector" target="_blank" className="hover:text-cyan-400 transition-colors">Web Inspector</a></li>
                <li><a href="https://github.com/sainibhaowal/SynEvid/tree/main/apps/mcp-server" target="_blank" className="hover:text-cyan-400 transition-colors">MCP Server</a></li>
                <li><a href="https://github.com/sainibhaowal/SynEvid/tree/main/docs" target="_blank" className="hover:text-cyan-400 transition-colors">Documentation</a></li>
              </ul>
            </div>

            <div>
              <div className="text-xs font-mono uppercase text-gray-300 tracking-wider font-bold mb-4">
                Governance
              </div>
              <ul className="space-y-2 text-xs text-gray-400">
                <li><a href="https://github.com/sainibhaowal/SynEvid/blob/main/LICENSE" target="_blank" className="hover:text-cyan-400 transition-colors">Apache 2.0 License</a></li>
                <li><a href="https://github.com/sainibhaowal/SynEvid/blob/main/SECURITY.md" target="_blank" className="hover:text-cyan-400 transition-colors">Security Policy</a></li>
                <li><a href="https://github.com/sainibhaowal/SynEvid/blob/main/CODE_OF_CONDUCT.md" target="_blank" className="hover:text-cyan-400 transition-colors">Code of Conduct</a></li>
                <li><a href="https://github.com/sainibhaowal/SynEvid" target="_blank" className="hover:text-cyan-400 transition-colors">GitHub Stars & CI</a></li>
              </ul>
            </div>
          </div>

          <div className="pt-8 border-t border-white/5 flex flex-wrap items-center justify-between text-xs text-gray-500 font-mono">
            <div>&copy; 2026 SynEvid (Code Autopsy) Project. Released under Apache 2.0.</div>
            <div className="flex items-center space-x-2 mt-2 sm:mt-0">
              <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
              <span className="text-gray-400">100/100 Repeatable Determinism Invariant Verified</span>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
