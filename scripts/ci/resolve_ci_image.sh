#!/usr/bin/env bash
# Resolve the multi-platform manifest once so producers and consumers cannot drift.
set -euo pipefail
registry=ghcr.io/illegalstudio/elephc-ci
if [[ -n "${GH_TOKEN:-}" ]]; then
  printf '%s' "$GH_TOKEN" | docker login ghcr.io -u "$GITHUB_ACTOR" --password-stdin >/dev/null
  trap 'docker logout ghcr.io >/dev/null 2>&1 || true' EXIT
fi
catalog=scripts/ci/images.json
if [[ -f "$catalog" ]]; then
  image=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["ci"])' "$catalog")
  if [[ ! "$image" =~ ^ghcr\.io/illegalstudio/elephc-ci@sha256:[0-9a-f]{64}$ ]]; then
    echo 'Invalid committed CI image digest' >&2
    exit 1
  fi
  # A catalog is activated only after its whole image family was published.
  # Never replace a missing pin with a moving tag.
  docker buildx imagetools inspect "$image" >/dev/null
else
  # Historical workflows predating the catalog retain their original resolver.
  digest=$(docker buildx imagetools inspect "$registry:latest" --format '{{.Manifest.Digest}}')
  if [[ ! "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    echo 'Invalid immutable CI image digest' >&2
    exit 1
  fi
  image="$registry@$digest"
fi
printf 'image=%s\n' "$image" >> "$GITHUB_OUTPUT"
