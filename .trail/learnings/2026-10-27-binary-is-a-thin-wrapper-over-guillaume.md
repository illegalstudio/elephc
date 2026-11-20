---
title: "Binary is a thin wrapper over the lib: never declare modules in main.rs"
author: Guillaume
date: 2026-10-27
branch: perf/thin-binary-over-lib
cost: { minutes: 176, tokens: 578472 }
touches:
  - AGENTS.md
  - CLAUDE.md
  - Cargo.toml
  - README.md
  - docs/internals/architecture.md
  - src/lib.rs
  - src/main.rs
  - src/monitor/attach.rs
  - src/monitor/decl_ranges.rs
  - src/monitor/tests.rs
---

## Decided
src/main.rs declares no modules and only calls elephc::entry::run on the compiler stack. The CLI-only modules (cli, pipeline, linker, link_planning, repl, monitor, otlp, timings, test_command, brand, call_graph, debug_info, pprof_encode, probe_key, project_ini, runtime_cache, source_map) live in lib.rs as pub(crate); [[bin]] has test = false. Before, main.rs re-declared the whole module tree, so cargo compiled the ~887k-line compiler twice, serially (lib, then bin). Measured on an M-series Mac after a one-line edit in src/codegen/mod.rs: 45.6s -> 21.7s (bin unit 20.2s -> 1.2s); in a 6-CPU Linux container 3m55s with the bin at 150s.

## Tried and ruled out
Keeping the bin's own module tree "for isolation": it bought nothing (the bin already reached into the lib via elephc::lexer/names in monitor) and doubled every rebuild and its peak memory.

## Watch out
New CLI code goes in the lib (crate::, not elephc::). The bin's former unit tests now run under `cargo test --lib`; 25 monitor tests (ptrace, linux cfg) still compile only on Linux. nextest's binary(elephc) filter matches the lib harness too.
