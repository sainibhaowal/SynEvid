// Headless tests for the Web Inspector page.
//
// The inspector is a single zero-build HTML file, so the only way to test its logic is to
// extract the script the browser would run and run *that* — the same bytes, via `vm`. A DOM
// stub is enough because every renderer is pure string building: no framework, no fetch, no
// build step, and nothing that could reach the network.
//
// The two artifacts exercised here are the Phase 4.2 ones (Step 45): a revision journal and
// an organisational blast radius. The assertions that matter are the honesty ones — an
// unresolved reference must not be drawn as an edge, a missing component must not render as
// an empty answer, and a bare fact list must not be given an envelope's guarantees.

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const html = fs.readFileSync(path.join(here, '..', 'index.html'), 'utf8');
const script = html.match(/<script>([\s\S]*)<\/script>/)[1];

function fakeElement(id) {
  return {
    id,
    html: '',
    textContent: '',
    style: {},
    classList: { add() {}, remove() {} },
    set innerHTML(value) {
      this.html = String(value);
    },
    get innerHTML() {
      return this.html;
    },
    addEventListener() {},
    appendChild() {},
    querySelector() {
      return fakeElement(id + ':child');
    },
    querySelectorAll() {
      return [];
    },
  };
}

function loadPage() {
  const elements = new Map();
  const document = {
    getElementById(id) {
      if (!elements.has(id)) {
        elements.set(id, fakeElement(id));
      }
      return elements.get(id);
    },
    querySelectorAll() {
      return [];
    },
    createElement(tag) {
      return fakeElement(tag);
    },
  };
  const sandbox = {
    document,
    navigator: {},
    alert() {},
    JSON,
    Object,
    Array,
    String,
    Number,
    Math,
    console,
  };
  sandbox.window = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(script, sandbox);
  return sandbox;
}

const fact = (id, predicate, subject, object, from, to, coverage) => ({
  fact_id: id,
  subject,
  predicate,
  object,
  valid_from: from,
  valid_to: to || null,
  evidence_digest: 'a'.repeat(64),
  coverage: coverage || 'verified',
  provenance: 'git:tree',
});

const JOURNAL = {
  journal: [
    fact('f1', 'file_present', 'src/a.ts', 'blob:1', 'r1', null),
    fact('f2', 'file_present', 'src/b.ts', 'blob:2', 'r1', 'r2'),
    fact('f3', 'file_present', 'src/c.ts', 'blob:3', 'r2', null, 'partial'),
  ],
  stats: { total: 3, active: 2, retired: 1, partial: 1, unknown: 0 },
  coverage: 'partial',
  notes: ['facts come from `TreeExtractor`, which records file presence per revision; symbol-level extraction is not wired to the language adapters, so this journal says which files existed, not which symbols were defined'],
};

const RADIUS = {
  catalog: {
    files_scanned: 9,
    entities: [{ reference: 'component:default/checkout' }, { reference: 'component:default/cart' }],
    manifests: [],
    dependencies: [],
    unmodelled_kinds: { Group: 1 },
    refusals: { 'missing kind': 1 },
    duplicate_entities: [],
    coverage: 'partial',
    notes: ['1 reference(s) name a modelled kind that is not in this catalog'],
    digest: 'b'.repeat(64),
  },
  blast_radius: {
    component: 'component:default/checkout',
    found: true,
    owners: ['group:default/team-x'],
    owner_is_team: true,
    co_owned_components: ['component:default/cart'],
    siblings: [],
    system: 'system:default/checkout-system',
    domain: null,
    provided_apis: ['api:default/checkout-api'],
    consumed_apis: [],
    dependents: ['component:default/web'],
    unresolved: ['api:default/ghost is referenced by component:default/checkout but is not in the catalog'],
    coverage: 'partial',
    notes: ['1 reference(s) do not resolve'],
  },
};

test('the page refuses an artifact it cannot render, and says what it can', () => {
  const page = loadPage();
  assert.equal(page.classifyArtifact({ hello: 'world' }), 'unknown');
  assert.throws(() => page.loadArtifact({ hello: 'world' }), /blast-radius/);
  assert.equal(page.classifyArtifact([{ nope: true }]), 'unknown');
});

test('the page stays read-only and offline', () => {
  // A single static file with no backend: no network API, no storage write, no clock. A
  // timeline that invented a date would be the one thing this page must never do, so the
  // absence of a clock API is an invariant rather than a coincidence.
  for (const forbidden of ['fetch(', 'XMLHttpRequest', 'localStorage.setItem', 'Date.now(', 'new Date(']) {
    assert.ok(
      !script.includes(forbidden),
      `the inspector must stay read-only and time-free: found ${forbidden}`
    );
  }
  assert.ok(!script.includes('innerHTML = esc(parsed'), 'the report is rendered as text, not as HTML');
});

test('a temporal envelope renders the journal, its coverage and its granularity note', () => {
  const page = loadPage();
  page.loadArtifact(JOURNAL);
  const sub = page.document.getElementById('timeline-sub');
  assert.match(sub.innerHTML, /2 revision\(s\)/);
  assert.match(sub.innerHTML, /3 fact\(s\)/);
  assert.match(sub.innerHTML, /partial/, 'the envelope coverage is shown, not assumed');
  const caption = page.document.getElementById('timeline-caption').innerHTML;
  assert.match(caption, /not which symbols/, 'the extractor granularity note is carried through');
  assert.match(caption, /carries commit ids, not\s+timestamps/);
  assert.match(page.document.getElementById('meta-coverage').textContent, /PARTIAL/);
});

test('the slider shows what is active at a revision, and moves', () => {
  const page = loadPage();
  page.loadArtifact(JOURNAL);
  // At the newest revision: `f2` was retired by r2, so the active set is f1 and f3.
  assert.match(page.document.getElementById('tl-count').textContent, /^1 added here · 1 retired here$/);
  assert.match(page.document.getElementById('timeline-stats').innerHTML, /Active at this revision<\/div><div class="meta-val"[^>]*>2</);
  const facts = page.document.getElementById('timeline-facts').innerHTML;
  assert.match(facts, /file_present/);
  assert.match(facts, /src\/c\.ts/);
  assert.match(facts, /partial/, 'a fact that did not fully resolve is badged');
  // At the first revision: f2 is still active, f3 does not exist yet.
  page.paintTimeline(0);
  assert.match(page.document.getElementById('tl-count').textContent, /^2 added here · 0 retired here$/);
  assert.match(page.document.getElementById('timeline-stats').innerHTML, /Active at this revision<\/div><div class="meta-val"[^>]*>2</);
  assert.ok(
    !page.document.getElementById('timeline-facts').innerHTML.includes('src/c.ts'),
    'a fact introduced later is not shown as active earlier'
  );
});

test('a bare fact list is rendered without the guarantees of an envelope', () => {
  const page = loadPage();
  page.loadArtifact(JOURNAL.journal);
  assert.match(page.document.getElementById('timeline-sub').innerHTML, /bare fact list/i);
  assert.match(
    page.document.getElementById('timeline-caption').innerHTML,
    /are not in this document/,
    'the page must not invent the coverage the envelope would have stated'
  );
  assert.match(page.document.getElementById('meta-coverage').textContent, /NOT STATED/);
});

test('a blast radius draws only stated references, and never an unresolved one as an edge', () => {
  const page = loadPage();
  page.loadArtifact(RADIUS);
  const graph = page.document.getElementById('distributed-graph').innerHTML;
  // Six stated references: one owner, one co-owned component, one provided API, one
  // dependent — the unresolved reference is not among them.
  const lines = (graph.match(/<line /g) || []).length;
  assert.equal(lines, 4, `one line per stated reference, got ${lines}`);
  assert.match(graph, /marker-end="url\(#arrow-in\)"/, 'a dependent points at the component it depends on');
  assert.ok(
    !graph.includes('ghost'),
    'an unresolved reference must not be drawn in the graph'
  );
  const refs = page.document.getElementById('distributed-refs').innerHTML;
  assert.match(refs, /ref-chip unresolved/, 'it is named, dashed and marked as not an edge');
  assert.match(refs, /not drawn as edges/);
  assert.match(page.document.getElementById('distributed-caption').innerHTML, /never an edge/);
});

test('a component the catalog does not contain is a refusal, not an empty answer', () => {
  const page = loadPage();
  const missing = JSON.parse(JSON.stringify(RADIUS));
  missing.blast_radius.found = false;
  missing.blast_radius.coverage = 'unknown';
  missing.blast_radius.owners = [];
  missing.blast_radius.co_owned_components = [];
  missing.blast_radius.provided_apis = [];
  missing.blast_radius.dependents = [];
  page.loadArtifact(missing);
  assert.match(page.document.getElementById('distributed-sub').innerHTML, /NOT IN THE CATALOG/);
  const graph = page.document.getElementById('distributed-graph').innerHTML;
  assert.match(graph, /refusal to answer, not an empty/);
  assert.ok(!graph.includes('<svg'), 'there is nothing to draw, and nothing is drawn');
  assert.match(page.document.getElementById('meta-coverage').textContent, /UNKNOWN/);
});

test('what the scan did not read is shown next to what it did', () => {
  const page = loadPage();
  page.loadArtifact(RADIUS);
  const scan = page.document.getElementById('distributed-scan').innerHTML;
  assert.match(scan, /Group \(1\)/, 'unmodelled entity kinds are named');
  assert.match(scan, /missing kind \(1\)/, 'refused documents are counted by reason');
  assert.match(scan, /9 file\(s\) scanned/);
});

test('an uncorrelated radius says so instead of implying observation', () => {
  const page = loadPage();
  page.loadArtifact(RADIUS);
  const topology = page.document.getElementById('distributed-topology').innerHTML;
  assert.match(topology, /No trace export is correlated/);
  assert.match(topology, /only a telemetry correlation can say what is called/);
});

test('a correlated radius shows the observation and whether it is actionable', () => {
  const page = loadPage();
  const correlated = JSON.parse(JSON.stringify(RADIUS));
  correlated.topology = {
    command: 'telemetry',
    exit_code: 4,
    actionable: false,
    report: {
      coverage: 'partial',
      observation: {
        windows_observed: 3,
        required_windows: 3,
        span_count: 41,
        complete_observation: false,
        dispatch: 'dynamic',
      },
      notes: ['dispatch is dynamic, so no caller set is complete'],
    },
  };
  page.loadArtifact(correlated);
  const topology = page.document.getElementById('distributed-topology').innerHTML;
  assert.match(topology, /NOT ACTIONABLE/, 'exit 4 is a refusal and is labelled as one');
  assert.match(topology, /coverage-pill partial/);
  assert.match(topology, /windows /);
  assert.match(topology, /dispatch is dynamic/);
});

test('a verification report still renders as it did before', () => {
  const page = loadPage();
  assert.equal(page.classifyArtifact({ run: { command: 'verify' }, coverage: { state: 'verified' } }), 'verify');
  page.loadArtifact({
    run: { command: 'verify', analyzer_version: '0.0.1' },
    coverage: { state: 'verified', files_scanned: 3 },
    findings: [],
    evidence: [],
  });
  assert.match(page.document.getElementById('case-title').textContent, /Forensic Case/);
});
