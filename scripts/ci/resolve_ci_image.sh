#!/usr/bin/env bash
# Resolve the multi-platform manifest once so producers and consumers cannot drift.
set -euo pipefail
registry=ghcr.io/illegalstudio/elephc-ci
if [[ -n "${GH_TOKEN:-}" ]]; then
  printf '%s' "$GH_TOKEN" | docker login ghcr.io -u "$GITHUB_ACTOR" --password-stdin >/dev/null
  trap 'docker logout ghcr.io >/dev/null 2>&1 || true' EXIT
fi
digest=$(docker buildx imagetools inspect "$registry:latest" --format '{{.Manifest.Digest}}')
if [[ ! "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo 'Invalid immutable CI image digest' >&2
  exit 1
fi
printf 'image=%s@%s\n' "$registry" "$digest" >> "$GITHUB_OUTPUT"
