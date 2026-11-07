---
title: Call-capture modes are one runtime filter; the CLI only formats and forwards the mode
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 75 }
touches:
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/value.rs
  - src/monitor/calls.rs
  - src/monitor/command.rs
  - tests/codegen/cli.rs
  - docs/compiling/cli-reference.md
---

## Decided

- `ELEPHC_DEBUG_CALLS` carries the capture mode (`none|types|scalars|full`), not
  just a boolean. `all`/`1`/`full`/anything unknown fall back to `full`, so an old
  boolean call site keeps its previous behavior.
- The runtime filters at capture time:
  - ` none`: empty `args`, no `return`;
  - `types`: `{name, type}` / `{type}`, no `value`;
  - `scalars`: full scalar/string values but container summaries (`array(3)`);
  - `full`: `value::render_nested` with depth 3 and 16 children per container.
- Filtering in the runtime keeps the bounded event ring small and makes
  `--calls-out` JSONL consistent with what the text table shows.
- `monitor --calls=<mode>` parses the same vocabulary, forwards `mode.as_str()`
  through the environment, and renders `call values (N) [mode]:`. A missing value
  is an error; `all` is a compatibility alias for `full`.
- `render_calls` degrades gracefully: an argument without `value` renders as
  `$name: type`, and a return event with only `type` renders `-> type`.
- `render_nested` is a separate reader-level helper so `value::render` stays the
  exact DAP/terminal summary and call capture opts into nested children.

## Tried and ruled out

- Capturing `full` in the runtime and filtering in the CLI: the event ring and
  JSONL would still carry nested children for `none`/`types`, and a large
  program could fill the ring with data the caller asked not to keep.
- A boolean `ELEPHC_DEBUG_CALLS=1` plus a second CLI-only filter: JSONL written by
  the runtime would not match the rendered table.
- Formatting types as `$name = array`: misleading, because no value was
  captured. The type-only form is `$name: array`.
- Making `--calls=full` an error after `scalars` existed: `full` was the previous
  default behavior, so it must remain accepted and be the default.

## Watch out

- `none` still records function/depth events; it answers "which calls ran", not
  "no call capture".
- `types` returns `{type}` without `value`; clients must not read `value`.
- `scalars` does not expand arrays/objects; use `full` when nested values matter.
- `full` is bounded (depth 3, 16 children), so a deeply nested or wide structure
  is truncated, not complete.
- The mode is read at enter/exit time from an atomic, so a mode change during a
  run would mix formats; the CLI sets it once before spawn.
- The e2e fixture is a 3-element list returned as `["total" => n, "items" => items]`;
  it asserts `array(3)` in `scalars`, `array(3) {[0] => int(1), ...}` in `full`,
  and the corresponding JSONL fields.
