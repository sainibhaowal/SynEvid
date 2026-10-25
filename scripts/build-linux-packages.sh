#!/usr/bin/env bash
#
# build-linux-packages.sh — Package a built SynEvid dist directory into .deb and .rpm packages.
#
# Usage:
#   bash scripts/build-linux-packages.sh <dist_dir> <target_triple> <version> <out_dir>
#
# Pure POSIX/dpkg-deb + optional rpmbuild/nfpm/fpm so it works on standard Ubuntu runners.
set -euo pipefail

DIST_DIR="$1"
TARGET="$2"
VERSION="$3"
OUT_DIR="$4"

mkdir -p "$OUT_DIR"

case "$TARGET" in
  x86_64-unknown-linux-gnu)
    DEB_ARCH="amd64"
    RPM_ARCH="x86_64"
    ;;
  aarch64-unknown-linux-gnu)
    DEB_ARCH="arm64"
    RPM_ARCH="aarch64"
    ;;
  *)
    echo "Skipping .deb/.rpm generation for target $TARGET (archive-only target)"
    exit 0
    ;;
esac

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

PKG_ROOT="$STAGE/synevid_${VERSION}_${DEB_ARCH}"
mkdir -p "$PKG_ROOT/DEBIAN"
mkdir -p "$PKG_ROOT/usr/bin"
mkdir -p "$PKG_ROOT/usr/share/synevid/mcp-server"
mkdir -p "$PKG_ROOT/usr/share/synevid/hooks"
mkdir -p "$PKG_ROOT/usr/share/doc/synevid"

cp "$DIST_DIR/autopsy" "$PKG_ROOT/usr/bin/autopsy"
chmod 0755 "$PKG_ROOT/usr/bin/autopsy"
if [ -f "$DIST_DIR/synevid-lsp" ]; then
  cp "$DIST_DIR/synevid-lsp" "$PKG_ROOT/usr/bin/synevid-lsp"
  chmod 0755 "$PKG_ROOT/usr/bin/synevid-lsp"
fi
if [ -f "$DIST_DIR/mcp-server/index.js" ]; then
  cp "$DIST_DIR/mcp-server/index.js" "$PKG_ROOT/usr/share/synevid/mcp-server/index.js"
fi
if [ -f "$DIST_DIR/setup-agent-hosts.py" ]; then
  cp "$DIST_DIR/setup-agent-hosts.py" "$PKG_ROOT/usr/share/synevid/setup-agent-hosts.py"
  chmod 0755 "$PKG_ROOT/usr/share/synevid/setup-agent-hosts.py"
fi
for h in safe-edit.sh autopsy-session.sh autopsy-task-complete.sh; do
  if [ -f "$DIST_DIR/hooks/$h" ]; then
    cp "$DIST_DIR/hooks/$h" "$PKG_ROOT/usr/share/synevid/hooks/$h"
    chmod 0755 "$PKG_ROOT/usr/share/synevid/hooks/$h"
  fi
done
for doc in LICENSE NOTICE THIRD_PARTY_NOTICES.md README.md; do
  if [ -f "$DIST_DIR/$doc" ]; then
    cp "$DIST_DIR/$doc" "$PKG_ROOT/usr/share/doc/synevid/$doc"
  fi
done

cat > "$PKG_ROOT/DEBIAN/control" <<EOF
Package: synevid
Version: ${VERSION}
Section: devel
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: SynEvid Maintainers <rav.singh039@gmail.com>
Homepage: https://github.com/sainibhaowal/SynEvid
Description: Offline, deterministic software verification and blast-radius engine
 SynEvid (Code Autopsy) provides deterministic AST dependency multigraphs,
 bounded blast-radius traversal, architectural invariant verification,
 and MCP/LSP agent surfaces.
EOF

if command -v dpkg-deb >/dev/null 2>&1; then
  DEB_OUT="$OUT_DIR/synevid_${VERSION}_${DEB_ARCH}.deb"
  dpkg-deb --build --root-owner-group "$PKG_ROOT" "$DEB_OUT"
  echo "Built $DEB_OUT"
fi

if command -v rpmbuild >/dev/null 2>&1; then
  RPM_TOP="$STAGE/rpmbuild"
  mkdir -p "$RPM_TOP/BUILD" "$RPM_TOP/RPMS" "$RPM_TOP/SOURCES" "$RPM_TOP/SPECS" "$RPM_TOP/SRPMS"
  SPEC="$RPM_TOP/SPECS/synevid.spec"
  cat > "$SPEC" <<EOF
Name:           synevid
Version:        ${VERSION}
Release:        1
Summary:        Offline, deterministic software verification and blast-radius engine
License:        Apache-2.0
URL:            https://github.com/sainibhaowal/SynEvid
BuildArch:      ${RPM_ARCH}
AutoReqProv:    no
%define _binary_payload w9.gzdio

%description
SynEvid (Code Autopsy) provides deterministic AST dependency multigraphs,
bounded blast-radius traversal, architectural invariant verification,
and MCP/LSP agent surfaces.

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/share/synevid/mcp-server
mkdir -p %{buildroot}/usr/share/synevid/hooks
mkdir -p %{buildroot}/usr/share/doc/synevid
cp -a "${PKG_ROOT}/usr/." %{buildroot}/usr/

%files
/usr/bin/autopsy
/usr/bin/synevid-lsp
/usr/share/synevid
/usr/share/doc/synevid
EOF
  rpmbuild --define "_topdir $RPM_TOP" --target "$RPM_ARCH" -bb "$SPEC"
  find "$RPM_TOP/RPMS" -name "*.rpm" -exec cp {} "$OUT_DIR/synevid-${VERSION}-1.${RPM_ARCH}.rpm" \;
  echo "Built $OUT_DIR/synevid-${VERSION}-1.${RPM_ARCH}.rpm"
fi
