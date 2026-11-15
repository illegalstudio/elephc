---
title: elephc_break is a transparent source helper with a real stub in normal builds
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 120 }
touches:
  - src/debug_prelude.rs
  - src/codegen/block_emit.rs
  - crates/elephc-debug/src/lib.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/profiling.md
  - docs/compiling/dap.md
  - docs/compiling/cli-reference.md
  - .plans/native-debugger.md
---

## Decided

- `elephc_break()` is injected in **every** build unless `--strict-php`:
  - a `--debug` build calls the internal `__elephc_debug_event(3, null, null,
    "auto")`;
  - a normal build gets an empty-body stub, so the same source compiles and the
    call is a no-op.
- The capture-event runtime handles kind `3` as `record_debug_break()`: it stops
  only when `INTERACTIVE` is set (terminal or DAP), never for a calls-only or
  coverage-only build.
- The source helpers (`elephc_break`, `elephc_dump`, `elephc_trace_start/end`)
  are **transparent frames**: `emit_fn_marker` gives them no debug function id,
  so they push no activation. `elephc_break()` therefore stops on the caller's
  line and `bt`/DAP `stackTrace` show `hit` rather than `elephc_break`.
- `record_debug_break()` picks the innermost activation with a known user site
  (defensive even with transparent helpers), clears `stop_at_first`/step state,
  and then either calls `dap_stopped(site_id, "breakpoint")` or the interactive
  `stop(site_id)`.
- `--strict-php` still skips the prelude, so `elephc_break` is an extension-only
  helper and reports `Undefined function` there.
- The dump/trace helpers remain debug-only; `elephc_break` is the one helper that
  must be safe in every build.

## Tried and ruled out

- Making `elephc_break` a no-op prelude that still calls the debug bridge: a
  normal build does not link that bridge. The empty-body stub is the only safe
  normal-build lowering without a catalog builtin and conditional linking.
- Keeping an activation frame for the helper: the stop would report the helper's
  line (or `<no site>`) and every dump/trace would be one depth too deep.
- Filtering helper frames in DAP/`bt` only: frame ids, scopes and `frame N` use
  the raw activation index, so display filtering would desynchronize them.
  Making the frame non-existent is consistent everywhere.
- Reusing kind `0` (dump) with a null value: the dump path records an event and
  prints fallback text; a break is a control action, not a value snapshot.

## Watch out

- `elephc_break()` uses the caller's last executed site. If the call spans a line
  without a site guard, the stop is attributed to the previous source site; the
  helper itself does not carry a line.
- A `--debug` build without an interactive/DAP session (`ELEPHC_DEBUG_CALLS` or
  coverage only) keeps the real helper body but `INTERACTIVE` is false, so it is a
  no-op.
- `--strict-php` hides the helper at compile time; there is no runtime fallback.
- The four debug helpers now carry `PreludeProvided` shared contracts
  (`extension: true`) and generated `debug` docs. The prelude remains their AOT
  implementation, which is the documented route for `PreludeProvided`. Invalid
  `elephc_dump()` destinations raise `ValueError` in the prelude before the native
  recorder is called.
- The DAP e2e asserts reason `breakpoint`, location `main.php:3:5 (hit)`, frames
  `hit`/`main`, no `elephc_break` frame, and output `after`.
