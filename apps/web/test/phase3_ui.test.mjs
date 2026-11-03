import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..', 'src');
const read = (relative) => fs.readFileSync(path.join(root, relative), 'utf8');

test('Git Galaxy historical frames are wired into the existing canvas data contract', () => {
  const page = read('app/page.tsx');
  assert.match(page, /handleTimeTravelFrame/);
  assert.match(page, /onTimeTravelFrame=\{handleTimeTravelFrame\}/);
  assert.match(page, /frame\.nodes\.map/);
  assert.match(page, /startTransition\(\(\) => setUniverse\(historicalUniverse\)\)/);
  assert.doesNotMatch(page, /CodeUniverse3D.*style|animation.*historical/i);
});

test('Phase 3 API uses PRD path routes and no fabricated historical graph fallback', () => {
  const api = read('lib/api.ts');
  assert.match(api, /git\/time-travel\/\$\{encodeURIComponent\(oid\)\}/);
  assert.match(api, /git\/diff\/\$\{encodeURIComponent\(baseOid\)\}\/\$\{encodeURIComponent\(headOid\)\}/);
  assert.match(api, /getGitBlame/);
  assert.match(api, /nodes: \[\]/);
  assert.doesNotMatch(api, /src\/index\.ts.*src\/engine\.ts/);
});

test('line blame can jump to the introducing commit without changing canvas presentation code', () => {
  const galaxy = read('components/GitGalaxySection.tsx');
  assert.match(galaxy, /Semantic line blame/);
  assert.match(galaxy, /setSelectedCommitOid\(line\.commit_oid\)/);
  assert.match(galaxy, /getGitBlame/);
});
