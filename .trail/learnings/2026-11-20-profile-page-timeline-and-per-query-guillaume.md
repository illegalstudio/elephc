---
title: Profile page timeline and per-query callers
author: Guillaume
date: 2026-11-20
branch: feat/apm-v2
touches:
  - crates/elephc-instr/src/lib.rs
  - docs/beyond-php/apm.md
  - docs/beyond-php/profiling.md
  - docs/compiling/cli-reference.md
  - docs/images/apm/transactions-full.png
  - docs/images/apm/transactions.png
  - docs/images/profiling/call-graph-dark.png
  - docs/images/profiling/call-graph.png
  - docs/images/profiling/checks.png
  - docs/images/profiling/diff.png
---

## Decided
- The Flame view's Timeline reuses the exact instrumentation's per-call spans. Each span also records its depth and the heap's live bytes at its end, read from `_gc_live` (what memory_get_usage() returns) through a runtime `.comm` slot `elephc_instr_live_ptr` that main fills after `elephc_instr_init`. No new runtime counter was needed.
- `monitor --html` asks for the timeline with `ELEPHC_INSTR_TIMELINE=<file>`; src/monitor/timeline.rs keeps the 60 000 longest calls and a 2 400-point peak-preserving memory curve.
- A query is charged to the first stack frame that is not the driver's own (`PDO::`, `PDOStatement::`, `PDO\`, `mysqli*`). Its location is the function's first source-span line, moved to the declaration line by monitor/exact.rs with php_decl_ranges.
- The `elephc_instr_init` table is now quads: name ptr/len, then `path:line` ptr/len.

## Tried and ruled out
- Defining the live-bytes slot as a `#[no_mangle]` Rust static in elephc-instr: the repo convention (checked by a test over `declared_bridge_slots`) is runtime-defined `.comm` slots declared `extern` in the crates, with a `#[cfg(test)]` stub.
- SVG `<text>` and raw-HTML holes inside `<sc-for>` in the M3 mockup (claude.ai Design canvas): they do not render; place labels as HTML over the SVG and pass icons as path data.

## Watch out
- The default rustc on this machine became 1.95 (mago needs 1.97): build with `RUSTC=~/.rustup/toolchains/1.98.1-aarch64-apple-darwin/bin/rustc cargo build`.
- monitor's stderr filter only drops profiler lines listed in PROFILER_LINE_PREFIXES (src/monitor/channel.rs); a new `elephc-instr-*` line must be added there.
