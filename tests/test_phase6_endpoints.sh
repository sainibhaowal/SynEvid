#!/usr/bin/env bash
# Real Phase 6 daemon integration check. The caller must start a daemon against
# an indexed test repository and configure SYNEVID_TELEMETRY_SALT privately.
set -euo pipefail

: "${SYNEVID_DAEMON_URL:?Set SYNEVID_DAEMON_URL to the running test daemon}"
: "${SYNEVID_TEST_SNAPSHOT_ID:?Set this to the active indexed snapshot ID}"
: "${SYNEVID_TEST_FUNCTION:?Set this to a function declared in that snapshot}"
: "${SYNEVID_TEST_FILEPATH:?Set repository-relative path for the function}"
: "${SYNEVID_TELEMETRY_SALT:?Configure the daemon with a private 32-byte hex telemetry salt}"

command -v curl >/dev/null || { echo "curl is required" >&2; exit 2; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }
if [[ ${#SYNEVID_TELEMETRY_SALT} -ne 64 || ! "$SYNEVID_TELEMETRY_SALT" =~ ^[[:xdigit:]]+$ ]]; then
  echo "SYNEVID_TELEMETRY_SALT must be exactly 64 hex characters" >&2
  exit 2
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf -- "$tmp_dir"' EXIT
auth_args=()
if [[ -n "${SYNEVID_LOOPBACK_TOKEN:-}" ]]; then
  auth_args=(-H "Authorization: Bearer ${SYNEVID_LOOPBACK_TOKEN}")
fi

post_json() {
  local route="$1" body_file="$2" request_file="$3"
  local status
  status=$(curl --silent --show-error --output "$body_file" --write-out '%{http_code}' \
    "${auth_args[@]}" -H 'Content-Type: application/json' \
    --data-binary "@$request_file" "${SYNEVID_DAEMON_URL}${route}")
  if [[ "$status" != 200 ]]; then
    local error_json
    error_json="$(jq -c '{error_code,error}' "$body_file")"
    echo "POST $route failed with HTTP $status: $error_json" >&2
    exit 1
  fi
}

get_json() {
  local route="$1" body_file="$2"
  local status
  status=$(curl --silent --show-error --output "$body_file" --write-out '%{http_code}' \
    "${auth_args[@]}" "${SYNEVID_DAEMON_URL}${route}")
  if [[ "$status" != 200 ]]; then
    local error_json
    error_json="$(jq -c '{error_code,error}' "$body_file")"
    echo "GET $route failed with HTTP $status: $error_json" >&2
    exit 1
  fi
}

jq -n \
  --arg snapshot "$SYNEVID_TEST_SNAPSHOT_ID" \
  --arg function "$SYNEVID_TEST_FUNCTION" \
  --arg filepath "$SYNEVID_TEST_FILEPATH" \
  '{snapshot_id:$snapshot,otlp_export:{resourceSpans:[{resource:{attributes:[{key:"service.name",value:{stringValue:"synevid-phase6-e2e"}}]},scopeSpans:[{spans:[{traceId:"01234567890123456789012345678901",spanId:"0123456789abcdef",name:$function,kind:1,startTimeUnixNano:"1",endTimeUnixNano:"2",attributes:[{key:"code.function",value:{stringValue:$function}},{key:"code.filepath",value:{stringValue:$filepath}}],droppedAttributesCount:0,droppedEventsCount:0,droppedLinksCount:0}]}]}]}}' \
  > "$tmp_dir/otlp-request.json"

post_json '/api/v1/analytics/otlp-ingest' "$tmp_dir/ingest.json" "$tmp_dir/otlp-request.json"
jq -e --arg snapshot "$SYNEVID_TEST_SNAPSHOT_ID" \
  '.status == "STORED" and .snapshot_id == $snapshot and .spans_ingested == 1 and .complete_observation == false and (.evidence_digest | test("^blake3:[0-9a-f]{64}$"))' \
  "$tmp_dir/ingest.json" >/dev/null || { echo "OTLP response did not match parsed and conservatively scoped evidence" >&2; exit 1; }

get_json '/api/v1/analytics/report?format=json' "$tmp_dir/report-envelope.json"
report_id="$(jq -er '.report.report_id' "$tmp_dir/report-envelope.json")"
report_digest="$(jq -er '.digest' "$tmp_dir/report-envelope.json")"
get_json '/api/v1/analytics/report-history?limit=200' "$tmp_dir/history.json"
jq -e --arg id "$report_id" '.reports | any(.report_id == $id)' "$tmp_dir/history.json" >/dev/null || {
  echo "Generated analytics report is missing from durable report history" >&2
  exit 1
}
get_json "/api/v1/analytics/report-history/${report_id//:/%3A}" "$tmp_dir/receipt.json"
jq -e --arg id "$report_id" --arg digest "$report_digest" \
  '.report_id == $id and .evidence_digest == $digest and .report.result_digest == $digest and .report.report_id == $id' \
  "$tmp_dir/receipt.json" >/dev/null || { echo "Persisted report receipt identity/digest mismatch" >&2; exit 1; }

echo "Phase 6 live OTLP to topology to persisted report to receipt integration passed for snapshot ${SYNEVID_TEST_SNAPSHOT_ID}"
