#!/usr/bin/env bash
# Build the Ubuntu .deb from a Mac via Colima.
#
# Colima virtiofs only mounts $HOME — this checkout lives on /Volumes/MobileSSD,
# so we export `git archive HEAD` into ~/tmp (dirty UI/hint stays out of the
# package). Docker data sits on Colima's 60GiB data disk, not the 20GiB rootfs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HOST_SRC="${HOME}/tmp/gamelife-linux-src"
HOST_OUT="${HOME}/tmp/gamelife-bundles"
IMAGE="${GAMELIFE_LINUX_IMAGE:-gamelife-linux-builder}"
PLATFORM="${GAMELIFE_LINUX_PLATFORM:-}"
TARGET_VOL="gamelife-linux-target"

if ! command -v docker >/dev/null; then
  echo "docker is not on PATH; start Colima first (colima start)" >&2
  exit 1
fi
if ! docker info >/dev/null 2>&1; then
  echo "docker daemon is not reachable; start Colima first" >&2
  exit 1
fi

rm -rf "$HOST_SRC"
mkdir -p "$HOST_SRC" "$HOST_OUT"
git -C "$ROOT" archive HEAD | tar -x -C "$HOST_SRC"

# macOS /bin/bash 3.2 + `set -u` cannot expand an empty array; keep the
# --platform branch as a separate invocation.
# qemu amd64 (`GAMELIFE_LINUX_PLATFORM=linux/amd64`) is wired through buildx,
# but the last attempt died in `cc` SIGSEGV compiling anyhow's build script
# (qemu-user + rustc's lld). Prefer a real x86_64 Ubuntu box for that .deb.
if [[ -n "$PLATFORM" ]]; then
  if [[ -z "${GAMELIFE_LINUX_IMAGE:-}" ]]; then
    IMAGE="gamelife-linux-builder-$(echo "$PLATFORM" | tr '/' '-')"
  fi
  TARGET_VOL="gamelife-linux-target-$(echo "$PLATFORM" | tr '/' '-')"
  docker buildx build --platform "$PLATFORM" --load -t "$IMAGE" "$ROOT/tools/linux-bundle"
  docker run --rm --platform "$PLATFORM" \
    -v "$HOST_SRC:/src:ro" \
    -v "$HOST_OUT:/out" \
    -v gamelife-linux-cargo-registry:/opt/cargo/registry \
    -v gamelife-linux-cargo-git:/opt/cargo/git \
    -v "${TARGET_VOL}:/target" \
    -e CARGO_HOME=/opt/cargo \
    -e CARGO_TARGET_DIR=/target \
    -e CARGO_PROFILE_RELEASE_LTO=false \
    -e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    "$IMAGE"
else
  docker build -t "$IMAGE" "$ROOT/tools/linux-bundle"
  docker run --rm \
    -v "$HOST_SRC:/src:ro" \
    -v "$HOST_OUT:/out" \
    -v gamelife-linux-cargo-registry:/opt/cargo/registry \
    -v gamelife-linux-cargo-git:/opt/cargo/git \
    -v "${TARGET_VOL}:/target" \
    -e CARGO_HOME=/opt/cargo \
    -e CARGO_TARGET_DIR=/target \
    "$IMAGE"
fi

echo "debs in $HOST_OUT:"
ls -lh "$HOST_OUT"
if [[ -d "$ROOT" ]]; then
  mkdir -p "$ROOT/dist-bundles"
  cp -p "$HOST_OUT"/*.deb "$ROOT/dist-bundles/" 2>/dev/null || true
  echo "copied to $ROOT/dist-bundles/"
fi
