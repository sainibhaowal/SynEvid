import { randomUUID } from 'node:crypto';
import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';

const LOOPBACK_ORIGINS =
  'http://localhost:* http://127.0.0.1:* http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001';

export function buildRendererCsp(nonce: string, development: boolean): string {
  const scriptDevelopment = development ? " 'unsafe-eval'" : '';
  return [
    `default-src 'self' data: blob: ${LOOPBACK_ORIGINS} http://127.0.0.1:3333`,
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${scriptDevelopment} ${LOOPBACK_ORIGINS}`,
    `style-src 'self' 'unsafe-inline' ${LOOPBACK_ORIGINS}`,
    `img-src 'self' data: blob: ${LOOPBACK_ORIGINS}`,
    `font-src 'self' data: ${LOOPBACK_ORIGINS}`,
    `connect-src 'self' ${LOOPBACK_ORIGINS} http://127.0.0.1:3333 ws://localhost:* ws://127.0.0.1:* ws://localhost:3000 ws://127.0.0.1:3000 ws://localhost:3001 ws://127.0.0.1:3001 ws://127.0.0.1:3333`,
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
  ].join('; ');
}

export function proxy(request: NextRequest) {
  const nonce = Buffer.from(randomUUID()).toString('base64');
  const csp = buildRendererCsp(nonce, process.env.NODE_ENV !== 'production');
  const requestHeaders = new Headers(request.headers);
  requestHeaders.set('x-nonce', nonce);
  requestHeaders.set('Content-Security-Policy', csp);
  const response = NextResponse.next({ request: { headers: requestHeaders } });
  response.headers.set('Content-Security-Policy', csp);
  return response;
}

export const config = {
  matcher: [
    {
      source: '/((?!api|_next/static|_next/image|favicon.ico|robots.txt|sitemap.xml).*)',
      missing: [
        { type: 'header', key: 'next-router-prefetch' },
        { type: 'header', key: 'purpose', value: 'prefetch' },
      ],
    },
  ],
};
