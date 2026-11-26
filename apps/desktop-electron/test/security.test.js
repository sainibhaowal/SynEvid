'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { isTrustedIpcSender, isTrustedRendererUrl } = require('../src/security');

test('renderer trust is restricted to the configured loopback origin', () => {
  const trusted = 'http://127.0.0.1:3000';
  assert.equal(isTrustedRendererUrl('http://127.0.0.1:3000/', trusted), true);
  assert.equal(isTrustedRendererUrl('http://localhost:3000/', trusted), false);
  assert.equal(isTrustedRendererUrl('http://127.0.0.1:3001/', trusted), false);
  assert.equal(isTrustedRendererUrl('https://127.0.0.1:3000/', trusted), false);
  assert.equal(isTrustedRendererUrl('file:///tmp/renderer.html', trusted), false);
  assert.equal(isTrustedRendererUrl('http://127.0.0.1.evil.example:3000/', trusted), false);
  assert.equal(isTrustedRendererUrl('not a URL', trusted), false);
});

test('IPC sender validation prefers the active frame and rejects untrusted sources', () => {
  const trusted = 'http://127.0.0.1:3000';
  assert.equal(isTrustedIpcSender({ senderFrame: { url: 'http://127.0.0.1:3000/' } }, trusted), true);
  assert.equal(isTrustedIpcSender({ senderFrame: { url: 'https://evil.example' } }, trusted), false);
  assert.equal(isTrustedIpcSender({ sender: { getURL: () => 'http://127.0.0.1:3000/' } }, trusted), true);
  assert.equal(isTrustedIpcSender({}, trusted), false);
});
