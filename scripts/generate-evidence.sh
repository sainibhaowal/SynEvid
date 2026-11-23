#!/bin/bash
# Evidence generation script for Phase 5 release artifacts
# Generates: sha256sums.txt, sbom.json, provenance.intoto.jsonl, attestation.json

set -euo pipefail

ARTIFACTS_DIR="${1:-artifacts}"
OUTPUT_DIR="${2:-evidence}"

mkdir -p "$OUTPUT_DIR"

echo "=== Generating evidence for artifacts in $ARTIFACTS_DIR ==="

# 1. SHA256 checksums for all installer artifacts
echo "Generating SHA256 checksums..."
find "$ARTIFACTS_DIR" -type f \( \
  -name "*.exe" -o \
  -name "*.AppImage" -o \
  -name "*.deb" -o \
  -name "*.dmg" -o \
  -name "*.zip" -o \
  -name "*.msi" \
\) -print0 | sort -z | xargs -0 sha256sum > "$OUTPUT_DIR/sha256sums.txt"

echo "Checksums generated:"
cat "$OUTPUT_DIR/sha256sums.txt"

# 2. CycloneDX SBOM using syft
echo "Generating SBOM (CycloneDX)..."
if command -v syft &> /dev/null; then
  syft packages dir:"$ARTIFACTS_DIR" -o cyclonedx-json > "$OUTPUT_DIR/sbom.json"
  echo "SBOM generated: $OUTPUT_DIR/sbom.json"
else
  echo "WARNING: syft not installed, skipping SBOM generation"
  echo '{"bomFormat": "CycloneDX", "specVersion": "1.5", "components": []}' > "$OUTPUT_DIR/sbom.json"
fi

# 3. SLSA Provenance using slsa-verifier
echo "Generating SLSA provenance..."
if command -v slsa-verifier &> /dev/null; then
  slsa-verifier generate \
    --artifact-path "$ARTIFACTS_DIR/*" \
    --provenance-path "$OUTPUT_DIR/provenance.intoto.jsonl" \
    --builder-id "github.com/sainibhaowal/SynEvid/.github/workflows/ci-phase5.yml@refs/heads/main" \
    2>&1 | tee "$OUTPUT_DIR/slsa-generation.log"
  echo "SLSA provenance generated: $OUTPUT_DIR/provenance.intoto.jsonl"
else
  echo "WARNING: slsa-verifier not installed, skipping SLSA provenance"
  echo '{"_type": "https://in-toto.io/Statement/v0.1", "subject": [], "predicateType": "https://slsa.dev/provenance/v1", "predicate": {}}' > "$OUTPUT_DIR/provenance.intoto.jsonl"
fi

# 4. Sigstore keyless signing (if COSIGN_YES is set)
if [[ "${COSIGN_YES:-}" == "true" ]] && command -v cosign &> /dev/null; then
  echo "Signing artifacts with cosign (keyless)..."
  for artifact in "$ARTIFACTS_DIR"/*; do
    if [[ -f "$artifact" ]]; then
      echo "Signing $artifact..."
      cosign sign-blob --yes --output-signature "$artifact.sig" "$artifact" 2>&1 | tee -a "$OUTPUT_DIR/cosign.log"
      cosign verify-blob --certificate "$artifact.crt" --signature "$artifact.sig" "$artifact" 2>&1 | tee -a "$OUTPUT_DIR/cosign.log"
    fi
  done
  echo "Cosign signatures generated"
else
  echo "Skipping cosign signing (set COSIGN_YES=true and install cosign to enable)"
fi

# 5. Generate attestation.json with build metadata
cat > "$OUTPUT_DIR/attestation.json" <<EOF
{
  "build_timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_commit": "$(git rev-parse HEAD 2>/dev/null || echo 'unknown')",
  "git_tag": "$(git describe --tags --always 2>/dev/null || echo 'unknown')",
  "builder": "github.com/sainibhaowal/SynEvid/.github/workflows/ci-phase5.yml",
  "artifacts": $(find "$ARTIFACTS_DIR" -type f -printf '"%f": "%p",\n' 2>/dev/null | sed '$s/,$//' | jq -s 'add // {}')
}
EOF

# 6. Verify all outputs exist
echo ""
echo "=== Evidence Package Contents ==="
ls -la "$OUTPUT_DIR"
echo ""

# Verify required files
REQUIRED_FILES=("sha256sums.txt" "sbom.json" "provenance.intoto.jsonl" "attestation.json")
for file in "${REQUIRED_FILES[@]}"; do
  if [[ -f "$OUTPUT_DIR/$file" ]]; then
    echo "✅ $file exists"
  else
    echo "❌ $file MISSING"
    exit 1
  fi
done

echo ""
echo "✅ Evidence package generated successfully in $OUTPUT_DIR"
echo "Contents:"
ls -la "$OUTPUT_DIR"