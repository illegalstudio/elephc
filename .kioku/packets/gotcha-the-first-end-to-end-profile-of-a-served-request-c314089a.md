---
id: gotcha-the-first-end-to-end-profile-of-a-served-request-c314089a
type: gotcha
title: "The first end-to-end profile of a served request, and the three hypotheses it killed"
description: "Every route pays a ~6.4ms floor; the allocator is 19% but neither the heap guard nor the free-list scans explain it, and two of the advisors' top items were already implemented"
created: 2026-09-20
sources:
  - path: src/codegen_support/runtime/arrays/heap_free.rs
    blob: dac72fa61dcd0931ef48808036b1eb7057eef9bf
    lines: 41-300
    snip: 0d7a5588ac19
    anchor: "emitter.label_global(\"__rt_heap_free\");"
  - path: src/codegen_support/runtime/exceptions/dynamic_instanceof.rs
    blob: 2a3b7d74d4b8d07722e274814f38c2430f05b722
    lines: 15-70
    snip: 3a2f0dc870a4
    anchor: "pub fn emit_dynamic_instanceof(emitter: &mut Emitter) {"
  - path: crates/elephc-web/src/server.rs
    blob: 5a3ea3cdade78c3eb47acb997827e93e32eb988b
---

# The first end-to-end profile of a served request, and the three hypotheses it killed

## Fact

How to take it (nothing here needs writing, it all exists):

  1. build with `--keep-symbols` -- WITHOUT it the binary carries 275 symbols and every compiled
     PHP frame prints as a raw address. `--keep-symbols` gives 982 397 and costs only the link.
  2. `scratchpad/profile-request.sh <bin> <route> <secs>` -- it finds the WORKER (the master
     forks and serves nothing, so sampling the pid the shell backgrounded profiles a supervisor)
     and drives load through abbench.py over raw sockets, never curl.
  3. `scratchpad/bench-routes.sh` for latency, interleaved against `php -S`.

Measured, load ~18:

    /plain         elephc min 6.36 ms   php -S 1.11 ms
    /greet/world   elephc min 6.43 ms   php -S 1.14 ms
    /  (template)  elephc min 8.89 ms   php -S 1.32 ms

Every route pays the SAME ~6.4 ms floor, including one returning 33 bytes. The template route
adds only ~2.5 ms. So the target is the fixed per-request cost.

Top of stack (~10 800 samples, 15 s):

    _rt_heap_free 1395   _rt_heap_alloc 675   _rt_strcasecmp 427   _rt_instanceof_lookup 285
    __elephc_eval_reflection_method_names 235   stat 195   _rt_str_persist 194
    _rt_diag_warning 160   SipHash Hasher::write 159   _rt_file_get_contents 152

**Three hypotheses tested and killed, so nobody re-tests them:**

1. *The `--web` small-bin double-free guard is the cost.* It scans the bin chain on every small
   free, and `_rt_heap_free` is twice `_rt_heap_alloc`, so it looked certain. Added
   `ELEPHC_WEB_HEAP_GUARD=0` (read in `elephc_web_run` before the fork, so every child inherits
   it) and measured on/off/on: /plain 7.48 / 7.21 / 7.78 ms. The off reading sits INSIDE the
   spread between the two on readings. The guard is free. Keep it on.
2. *The ordered free list's two O(n) walks per large free are quadratic.* `__rt_heap_free`
   scans the list to find the address-ordered insertion point AND again in `trim_tail`. Walked
   the list from lldb after 50 requests: **66 nodes, 2464 payload bytes**. Short. Not the cost.
3. *The eval bridge re-registers the native surface per request.* Already hoisted before the
   fork; see the packet on the dead 76k figure.

What the profile actually says: the allocator is ~19% not because it is slow per call but
because the request makes tens of thousands of calls, and `_rt_strcasecmp` (174 130 call sites
in the emitted assembly) plus `_rt_instanceof_lookup` (a LINEAR scan of
`_instanceof_target_entries`, comparing names byte-by-byte case-insensitively) are the compiled
program answering by NAME what the interpreter answers with a hash. That is the same disease
that dominated compile time, on the other side of the boundary --- and the tree already has the
cure to copy: `__rt_callable_lookup_composite_hash`.

Also found: the compiled binary prints **two warnings per request** (`Undefined array key 1`
and `... 2`) that `php -S` does not print at all on the same route, while the response bodies
stay byte-identical.

## Why

Four models proposed rebuilding mechanisms that already exist; only the profile moved the question forward, and it needed --keep-symbols to be readable at all.
