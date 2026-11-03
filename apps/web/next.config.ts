import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  /* config options here */
  cacheComponents: true,
  partialPrefetching: true,
  output: "standalone",
  // Allow concurrent source/runtime instances (including browser E2E tests)
  // to keep their Turbopack lock and generated files isolated.
  distDir: process.env.SYNEVID_NEXT_DIST_DIR || ".next",
  async rewrites() {
    const daemonUrl = process.env.SYNEVID_DAEMON_URL || 'http://127.0.0.1:3333';
    return [
      {
        source: '/api/:path*',
        destination: `${daemonUrl}/api/:path*`,
      },
    ];
  },
};

export default nextConfig;
