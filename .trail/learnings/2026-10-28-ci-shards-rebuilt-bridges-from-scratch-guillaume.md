---
title: CI shards rebuilt bridges from scratch on first link
author: Guillaume
date: 2026-10-28
branch: perf/runtime-cache-content-and-build-script
cost: { minutes: 665, tokens: 2398309 }
touches:
  - AGENTS.md
  - CLAUDE.md
  - ".beads/.auto-import-issues.jsonl"
  - ".beads/.gitignore"
  - ".beads/README.md"
  - ".beads/config.yaml"
  - ".beads/hooks/post-checkout"
  - ".beads/hooks/post-merge"
  - ".beads/hooks/pre-commit"
  - ".beads/hooks/pre-push"
---

## Decided
- A stale bridge archive is rebuilt only when it sits in the directory `cargo build` writes to (`$CARGO_TARGET_DIR` or `<workspace>/target`, per profile): `rebuild_replaces` in src/linker/bridges.rs.
## Tried and ruled out
- Not a register/ABI bug: macOS O1 was 0.61x the O0 time overall; only each shard's first link of each bridge stalled.
## Watch out
- nextest unpacks archives with tar's preserved mtimes, and CI checks out after the archive was built, so every bridge looked stale and elephc ran `cargo build -p <bridge>` into the checkout's empty target/ (lldb: wait4 under build_staticlib <- refreshed_if_stale). Repro: copy elephc + lib*.a to a dir, `touch -t 202601010000` the .a, run with CARGO_TARGET_DIR=<empty> RUSTUP_TOOLCHAIN=1.98: 196 s pre-fix vs 10 s.
- First-try 60 s timeouts that pass on retry are this shape; look for a per-shard warm-up before raising a budget.
