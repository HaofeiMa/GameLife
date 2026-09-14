#!/bin/bash
# Runs inside the builder image. Source is bind-mounted read-only at /src;
# cargo target lives on a named volume at /target; .deb files go to /out.
set -euo pipefail

if [[ ! -f /src/package.json ]]; then
  echo "expected GameLife sources at /src" >&2
  exit 1
fi
mkdir -p /out /work
rm -rf /work/app
cp -a /src /work/app
cd /work/app

export CARGO_HOME="${CARGO_HOME:-/opt/cargo}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/target}"
export PATH="${CARGO_HOME}/bin:${PATH}"

npm ci
npx tauri build --bundles deb

mapfile -t debs < <(find "$CARGO_TARGET_DIR" -name '*.deb' -type f)
if [[ ${#debs[@]} -eq 0 ]]; then
  echo "tauri build finished but no .deb under $CARGO_TARGET_DIR" >&2
  find "$CARGO_TARGET_DIR" -path '*bundle*' -print >&2 || true
  exit 1
fi
cp -v "${debs[@]}" /out/
ls -lh /out
