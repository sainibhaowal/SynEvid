'use strict';

const LOOPBACK_HOSTS = new Set(['127.0.0.1', 'localhost', '::1', '[::1]']);

function isTrustedRendererUrl(candidate, trustedUrl) {
  try {
    const parsed = new URL(candidate);
    const trusted = new URL(trustedUrl);
    return (
      (parsed.protocol === 'http:' || parsed.protocol === 'https:') &&
      LOOPBACK_HOSTS.has(parsed.hostname.toLowerCase()) &&
      parsed.origin === trusted.origin
    );
  } catch {
    return false;
  }
}

function isTrustedIpcSender(event, trustedUrl) {
  const senderUrl = event?.senderFrame?.url || event?.sender?.getURL?.();
  return typeof senderUrl === 'string' && isTrustedRendererUrl(senderUrl, trustedUrl);
}

module.exports = { isTrustedIpcSender, isTrustedRendererUrl };
