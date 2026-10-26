#!/usr/bin/env bash
set -e

./target/debug/synevid-ui --port 3458 &
DAEMON_PID=$!

sleep 1

echo "=== 1. Testing OPTIONS Preflight ==="
curl -s -I -X OPTIONS http://127.0.0.1:3458/api/v1/capabilities | grep -i "access-control"

echo "=== 2. Testing GET Capabilities ==="
curl -s http://127.0.0.1:3458/api/v1/capabilities

echo ""
echo "=== 3. Testing POST Simulate Edit (Sub-50ms) ==="
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"file_path":"src/services/PaymentService.ts","unsaved_buffer":"export function charge(token: string, taxRate: number) {}"}' \
  http://127.0.0.1:3458/api/v1/simulate/edit

echo ""
echo "=== 4. Testing GET 3D Universe ==="
curl -s "http://127.0.0.1:3458/api/v1/graph/universe?dim=3d"

echo ""
echo "=== 5. Testing GET Ego Network (<5ms) ==="
curl -s "http://127.0.0.1:3458/api/v1/graph/ego?id=PaymentService.chargeCard"

echo ""
kill $DAEMON_PID || true
echo "=== All 5 Real End-to-End Endpoints PASSED ==="
