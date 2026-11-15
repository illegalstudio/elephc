---
title: The first runtime agent can be a blocking stdin session behind the preserving wrapper
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 140 }
touches:
  - crates/elephc-debug/src/lib.rs
  - src/debug_cli.rs
  - src/cli.rs
  - src/main.rs
  - tests/codegen/cli.rs
  - docs/compiling/cli-reference.md
  - docs/compiling/output-and-diagnostics.md
  - docs/compiling/debug-metadata.md
---

## Decided

- The first interactive agent is a blocking stdin loop inside
  `elephc_debug_site_inner`. This is safe because the generated wrapper has
  already saved every caller-saved register; the agent can read the stopped
  frame and block without corrupting EIR values.
- `elephc_debug_boot` reads the embedded metadata via
  `elephc_debug_metadata` / `elephc_debug_metadata_len`, maps site ids to
  source/function/locals, and arms flags from `ELEPHC_DEBUG_BREAK`.
  `ELEPHC_DEBUG_STDIN=1` starts the session; no break spec stops at the first
  executed site.
- `elephc debug <source> [--break FILE:LINE]` re-invokes this compiler with
  `--debug`, resolves the binary against the invocation directory, and passes
  the environment through. It is the minimal terminal client.
- Current-frame locals read raw scalar/string slots through
  `fp - frame_offset`; arrays/objects/ref-cells are reported as placeholders
  until the ownership-aware formatter lands.
- Codegen assigns one logical site per `(function, source_id, line)`, not one per
  basic block. The guard stays in the block where the line first appears, so a
  loop re-enters the same site and a breakpoint stops on every iteration.
- The runtime keeps a thread-local activation stack. Generated function-enter
  hooks push, function-exit hooks pop, and every site records its site id on the
  current frame. `bt` walks that stack from the current frame through callers.
- The exit hook must preserve the live PHP return value around its call. Reusing
  `frame::push_return_value`/`pop_return_value` (including `by_ref_return` as
  `Pointer`) fixed a regression where `combine(2, 3)` returned `1` (the function
  id) instead of `5`.
- `next` stops at the next site in the current frame or a caller, `step` stops at
  any next site, and `finish` stops after the current frame returns. All flags are
  armed during an interactive session so stepping sees every line transition;
  breakpoints are filtered by `break_sites`.
- `frame <index>` selects an activation for `locals`/`p`; index 0 is the
  innermost frame. The selected frame resets to 0 on every stop.

## Tried and ruled out

- A background agent thread now: the blocking session is enough for
  pause/continue and avoids stop-the-world/thread synchronization before the
  line-based stack exists.
- Matching `Command::new("main")` for a relative debug binary: it searches PATH
  and failed. Resolve relative to the invocation directory.
- Dumping all locals through raw pointer dereference: arrays/objects/ref-cells
  need the runtime formatter and ownership rules; placeholders are safer.

## Watch out

- The activation stack is thread-local, not fiber-aware. Generator/fiber body
  enter/exit hooks can leave stale frames across a stack switch; do not treat
  `bt` as complete for coroutines yet.
- One site per line per function assumes the line's first emitted instruction is
  in the block that executes when the line is reached. A line split across
  disjoint blocks can miss a stop on the path that skips the guard's block.
- The session reads stdin while holding the debugger mutex; a future agent thread
  must split state ownership carefully.
- Breakpoint matching is `path:line` suffix matching, not a canonical source
  identity; use metadata source ids for the DAP adapter.
- `--break` still arms matching sites only for the break set decision; the
  interactive session arms every flag so stepping works. A normal `--debug` run
  without `ELEPHC_DEBUG_STDIN` leaves every flag at zero.
- The site hook's argument setup clobbers the ABI argument registers before the
  preserving wrapper saves anything. `emit_site_call` pushes/restores that pair
  around the call; the dormant skip branch must converge on the same pop, or
  every unflagged site unbalances the stack.
