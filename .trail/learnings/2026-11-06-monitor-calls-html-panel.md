---
title: The call-capture HTML panel is a static tree over the same JSONL events
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 100 }
touches:
  - src/monitor/call_html.rs
  - src/monitor/calls.rs
  - src/monitor/command.rs
  - src/monitor/mod.rs
  - tests/codegen/cli.rs
  - docs/compiling/cli-reference.md
---

## Decided

- `monitor --calls --html FILE` is now supported instead of being in the
  unsupported-flag list. It writes a self-contained page and still prints the
  normal text call table and program stdout; `--calls-out` can be used at the
  same time.
- The page is built from the exact same `events` vector that feeds the text
  table and `--calls-out`, so values cannot diverge between JSONL and HTML.
- The flat event stream is a preorder stream with a 1-based `depth`. The renderer
  builds an arena tree by tracking `(node, depth)` on a stack:
  - a deeper event becomes a child of the current event;
  - an equal/shallower event closes siblings first;
  - a missing depth degrades to a child of the nearest shallower event rather
    than panicking.
- Each `<li class="call" data-depth="N">` shows the function signature, argument
  names/values or types, and the return value/type. `none` mode has no values;
  `types` has type-only markers; `scalars`/`full` show the captured rendering.
- All observed strings (target, function, arg name, value, type) are HTML-escaped,
  including quotes and apostrophes. The page has no JavaScript and no external
  assets.
- The summary shows target, capture mode as a badge, captured call count, a
  mode-specific hint, and a warning when the bounded event ring dropped events.
- `--html` writes a one-line stderr confirmation, matching the coverage export
  behavior, and a write failure exits 1.
- Outside `--calls`, `--html` keeps its existing meaning: the interactive profile
  call graph. The calls-mode page is a static tree, not the sampled graph UI.

## Tried and ruled out

- Reusing `call_graph::render_html*`: it expects a `CallGraph` with inclusive /
  exclusive time, edges and source ranges. Call capture has no timing and no
  aggregated graph; forcing events into that model would invent data.
- Rendering a flat indented list: it is not a call tree. The nested `<ul>` makes
  parent/child and sibling relationships explicit.
- Capturing values again inside the HTML renderer: it only has serialized events,
  and re-reading the debuggee is impossible after it exits. Reuse the events.
- Running property hooks or `__debugInfo()` to enrich the page: the same
  read-only rule as the debugger applies; values stay the raw captured rendering.
- Using a client-side JSON + script renderer: a static page opens from disk in CI
  artifacts without script execution and is trivially testable.

## Watch out

- The tree root is the `main` event (depth 1), so the call count includes `main`.
- A call that never returned has no `return` object; the page simply omits the
  arrow rather than inventing a value.
- `full` values are already capped by the runtime (depth 3, 16 children); the HTML
  does not expand beyond what was captured.
- Event order is execution order. Siblings are consecutive events at the same
  depth; a call's return is attached to its enter event when it exits.
- The page is static. It does not auto-refresh, unlike `--live --html` profile
  pages.
- The e2e fixture has `main -> greet`, `main -> add`; it asserts the text table,
  the nested tree, escaped `string(5) "world"` / return values, and the output
  `worldworldworld5`.
