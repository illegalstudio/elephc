---
type: "Repo Map"
title: "sparkling-jingling-flute repo structure"
description: "Detected repo structure: README.md, CLAUDE.md, AGENTS.md, .github/workflows, src, tests. CI workflows: .github/workflows/ci image.yml, .github/workflows/ci.yml, .github/workflows/nightly.yml, .github/workflows/pdo live.y"
resource: "README.md"
tags: ["repo", "structure", "index"]
timestamp: "2026-09-12"
x-kage-id: "repo:sparkling-jingling-flute:repo_map:sparkling-jingling-flute-repo-structure-auto-structure"
x-kage-type: "repo_map"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-verified: "verified"
x-kage-paths: ["README.md", "CLAUDE.md", "AGENTS.md", ".github/workflows", "src", "tests"]
---

# sparkling-jingling-flute repo structure

> Detected repo structure: README.md, CLAUDE.md, AGENTS.md, .github/workflows, src, tests. CI workflows: .github/workfl…

Detected repo structure: README.md, CLAUDE.md, AGENTS.md, .github/workflows, src, tests.
CI workflows: .github/workflows/ci-image.yml, .github/workflows/ci.yml, .github/workflows/nightly.yml, .github/workflows/pdo-live.yml, .github/workflows/pr-labels.yml, .github/workflows/release.yml, .github/workflows/traffic.yml.
Test files: .github/scripts/pr-labels.test.cjs.
This packet is generated and should be treated as a navigation aid, not deep semantic understanding.

## Why

Agents need a quick map of repo entry points before choosing which files, workflows, or tests to inspect.

## Trigger

Recall when orienting to this repo's layout, CI workflows, or test locations.

## Action

Use this as a starting map and verify details against the current filesystem or code graph before editing.

## Verification

Generated from files present in the repository.

## Risk if forgotten

Agents may miss important entry points such as AGENTS.md, workflows, or MCP tests during initial orientation.

## Stale when

Top-level repo structure, workflow files, or test files change.

# Citations

[1] file README.md
[2] file CLAUDE.md
[3] file AGENTS.md
[4] file .github/workflows
[5] file src
[6] file tests

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:repo_map:sparkling-jingling-flute-repo-structure-auto-structure","title":"sparkling-jingling-flute repo structure","summary":"Detected repo structure: README.md, CLAUDE.md, AGENTS.md, .github/workflows, src, tests. CI workflows: .github/workflows/ci image.yml, .github/workflows/ci.yml, .github/workflows/nightly.yml, .github/workflows/pdo live.y","body":"Detected repo structure: README.md, CLAUDE.md, AGENTS.md, .github/workflows, src, tests.\nCI workflows: .github/workflows/ci-image.yml, .github/workflows/ci.yml, .github/workflows/nightly.yml, .github/workflows/pdo-live.yml, .github/workflows/pr-labels.yml, .github/workflows/release.yml, .github/workflows/traffic.yml.\nTest files: .github/scripts/pr-labels.test.cjs.\nThis packet is generated and should be treated as a navigation aid, not deep semantic understanding.","type":"repo_map","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.65,"tags":["repo","structure","index"],"paths":["README.md","CLAUDE.md","AGENTS.md",".github/workflows","src","tests"],"stack":[],"source_refs":[{"kind":"file","path":"README.md"},{"kind":"file","path":"CLAUDE.md"},{"kind":"file","path":"AGENTS.md"},{"kind":"file","path":".github/workflows"},{"kind":"file","path":"src"},{"kind":"file","path":"tests"}],"context":{"fact":"Generated repo structure summarizes top-level files, workflows, and test files as a navigation aid.","why":"Agents need a quick map of repo entry points before choosing which files, workflows, or tests to inspect.","trigger":"Recall when orienting to this repo's layout, CI workflows, or test locations.","action":"Use this as a starting map and verify details against the current filesystem or code graph before editing.","verification":"Generated from files present in the repository.","risk_if_forgotten":"Agents may miss important entry points such as AGENTS.md, workflows, or MCP tests during initial orientation.","stale_when":"Top-level repo structure, workflow files, or test files change."},"freshness":{"ttl_days":30,"last_verified_at":"2026-09-12","verification":"source_seen"},"edges":[],"quality":{"reviewer":"kage-indexer","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0},"created_at":"2026-09-11T14:01:11.587Z","updated_at":"2026-09-12T06:10:07.279Z"}
```

