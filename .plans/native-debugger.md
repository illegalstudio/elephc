# Native PHP debugger, call tracing and coverage

Status: proposal, ADR accepted 2026-11-05; implementation pending.
Baseline: `db62f09226` (2026-10-29).
Companion docs: `.plans/adr-native-debugger.md` (proposed decisions) and
`.plans/native-debugger-usage.md` (developer workflow).
Related issues: `elephc-t0p` (Throwable traces), `elephc-j6o.6` (real
Throwable traces), `.plans/phpstorm-plugin.md` (IDE direction).

## Tracking

Epic: `elephc-jwio` — Native PHP debugger, call tracing and coverage
(xdebug-class).

| Phase | Issue |
|---|---|
| Spike | `elephc-1vy3` |
| ADR / decision | `elephc-qq34` |
| Included-file debug artifact bug (independent, first) | `elephc-jwio.1` |
| Debug build preset (`--debug`) | `elephc-jwio.5` |
| Metadata | `elephc-4gz8` |
| `--debug` preset + line guards | `elephc-pt3u` |
| Runtime agent | `elephc-i21x` |
| CLI | `elephc-wihf` |
| DAP | `elephc-ms89` |
| DBGp (optional later PhpStorm transport) | `elephc-jwio.2` |
| Exceptions | `elephc-uv2d` |
| Call-value capture in `monitor` | `elephc-lx21` |
| Monitor HTML Calls panel | `elephc-jwio.3` |
| Named trace segments | `elephc-jwio.4` |
| Postmortem | `elephc-jj5x` |
| Coverage | `elephc-bwx6` |
| Web / fibers | `elephc-3pw8` |
| iOS gating | `elephc-cleg` (compile-only metadata implemented; simulator debug gated) |
| Docs | `elephc-76jn` |
| Conformance / perf gates | `elephc-itkf` |
| Elephc debug builtins | `elephc-zv82` |

## Implementation status (2026-11-05)

- `elephc-jwio.1` (included-file debug artifact identity): **closed**, verified
  with `.file`/`.loc`, source-map v3 `sources`/`source_id`, focused unit and
  codegen tests.
- `elephc-qq34` (ADR): **accepted**; D2/D4/D6 ratified. Remaining implementation
  gates: macOS execution, shared-stack extraction, metadata-schema review.
- `elephc-jwio.5` (debug build preset): **closed**; `--debug` implies DWARF and
  symbols, disables EIR optimization, skips the source-erasing AST passes, and
  refuses library emits.
- `elephc-1vy3` (guard/ABI spike): **in progress**; AArch64 native and x86_64
  qemu probes pass, macOS Mach-O probe compiles and awaits a macOS runner.
- Metadata v1 (`elephc-4gz8`) is **closed**: `.edbg.json` sidecar + embedded JSON table with sources/functions/sites/locals (frame offset, PHP type, storage kind/state offset, parameter flags), source hashes and path alias dedup. Runtime consumption is tracked by `elephc-i21x`; five-target CI by `elephc-itkf`.
- Per-site guard path (`elephc-pt3u`) is **closed**: one guard per source-line transition per basic block, one-byte per-site flags, target-aware preserving wrapper, `crates/elephc-debug` boot/count/dump. A normal `--debug` run leaves flags at zero and skips the Rust hook; `ELEPHC_DEBUG_SITES=1` arms all sites and dumps counters. Cross-target executable validation remains in `elephc-itkf`.
- Runtime agent (`elephc-i21x`, in progress): embedded metadata reader, `ELEPHC_DEBUG_STDIN` session, `ELEPHC_DEBUG_BREAK=path:line`, caller-frame `bt` from a thread-local activation stack with per-fiber parked slices, `next`/`step`/`finish`, frame selection, read-only value formatting (scalars, strings, indexed/hash arrays, ref-cells, object declared/dynamic properties, folded `__debugInfo()` projections, hooked backed properties with a raw-slot caveat and virtual-property markers), one-shot Unix attach sockets, loopback TCP attach with a mode-0600 per-run HMAC key and SIGUSR1 pause stops. A safe `__debugInfo()` projection also folds scalar literals and literal-only nested arrays; computed bodies stay `runtime` and are never executed. The source helpers are `elephc_break()` (available in every build; no-op outside an interactive/DAP session) and the debug-only dump/trace helpers; all four now carry `PreludeProvided` shared contracts with generated `debug` docs, and helper frames are transparent in stack views. Real nvim-dap and headless VS Code smoke tests live in `tests/editor/`, with runnable wrappers in `scripts/`; the loopback TCP attach path validates a mode-0600 per-run key with a mutual HMAC-SHA256 challenge. macOS arm64 execution is now a named CI gate (`debug-tests-macos-aarch64`) covering the AArch64 spike, DAP/terminal/TCP attach, postmortem, web route isolation and the runtime units; the merge queue or `ci:macos` runs it, and `scripts/ci/test-debug-macos.sh` reproduces it locally.
- DAP adapter (`elephc-ms89`, in progress): `elephc debug --dap` serves DAP over stdio, proxying to the runtime over a socketpair. Supported: launch (`stopOnEntry`), line/conditional/function/local, declared-object-property and dynamic-property write data breakpoints (virtual properties are refused), hooked-property rendering and caught/uncaught/throw exception filters with SHA-256 source verification/stale-source rejection, configurationDone, threads (main plus live fibers/generators as thread-like stacks), stackTrace (selectable by `threadId`), scopes, variables (arrays/strings/ref-cells/objects with class/property children, dynamic properties and folded `__debugInfo()` projections), evaluate (simple locals), continue/next/stepIn/stepOut, pause via SIGUSR1 in launch mode (child pid) and attach mode (`<socket>.pid`), `--attach <socket>` to a debug binary waiting on a one-shot mode-0600 Unix socket, and disconnect. Program stdout goes to stderr. Real nvim-dap and headless VS Code smoke tests run through `scripts/smoke-nvim-dap.sh` and `scripts/smoke-vscode.sh`; the minimal VS Code extension contribution lives in `tests/editor/vscode-extension/`. The loopback attach path uses a mode-0600 per-run key and a mutual HMAC-SHA256 challenge; a wrong key is rejected without stopping the listener. macOS arm64 execution is enforced by `debug-tests-macos-aarch64` (merge queue / `ci:macos`).
- `monitor --calls` (`elephc-lx21`, in progress): compiles a source with `--debug` (or uses a debug binary), runs with `ELEPHC_DEBUG_CALLS=<mode>`, and joins the function-enter argument capture with the exit-hook return capture. Modes are `none|types|scalars|full` (`full` default, `all`/`1` aliases): none omits values, types records type names, scalars renders full scalars with container summaries, and full adds bounded nested children. It prints an indented PHP-formatted args/returns table, `--calls-out FILE` writes the same bounded events as JSONL, and `--html FILE` renders one unified page with the exact Profile view plus Calls/Dumps/Trace segments tabs (no network assets). Every calls-mode run uses the same `--debug --with-monitoring` build and prints the exact per-function profile before the call values, so timing and call capture come from one run; `--exact` is accepted but implied. A `.php` source is built with `--debug`; a binary needs its `.edbg.json` sidecar (and the monitoring marker for `--exact`/`--html`). Debug builds also inject the `elephc_dump()`/`elephc_trace_start()`/`elephc_trace_end()` helpers; explicit dumps and named segments are captured as `dump`/`segment_start`/`segment_end` events, exported in JSONL, and rendered in the HTML Dumps and Trace segments panels. Values are bounded at 256 string bytes, 16 children and 3 nested levels with `truncated`/`overflow` reporting; `#[\SensitiveParameter]` values are redacted and event-ring overflow is written as a `capture_overflow` JSONL record. Each call event also records its caller file/line, start/end timestamps, running fiber, entry alloc/free counters, exit alloc/free deltas and exception class/message/code/caught state; the Calls panel renders that context under every call line, including in `--calls=none` and postmortem runs.
- `test --coverage` (`elephc-bwx6`, in progress): compiles the suite with `--debug`, runs it in one process with `ELEPHC_DEBUG_SITES=1`, joins line and branch site counters with the `.edbg.json` sites/branches tables, and prints per-file line coverage plus uncovered lines and branch edges. Every leaf of an `if`/loop/ternary condition (including `&&`/`||`/`!`) gets a two-edge guard; each `switch` gets one edge per case label plus default, on both static and dynamic dispatch. `--coverage-format text|lcov|cobertura` and `--coverage-out FILE` export text, LCOV (`DA`/`BRDA`/`BRF`/`BRH`) or Cobertura (`<line branch>`/`<conditions>`) reports; the dynamic `exit()` path publishes the debug dump. Logical paths enumerate every full reachable short-circuit prefix combination (hidden 2-bit-per-leaf state, dedicated counter per route); switch paths reuse their branch edges. Groups are recorded in a `paths` metadata table with a `truncated` flag and rendered as path coverage in text, LCOV (`PATH`/`PTF`/`PTH`), Cobertura (`path-rate`/`path-coverage`/`<condition type="path">`) and HTML. Remaining: cross-target execution evidence.
- Next production slice: cross-target execution evidence (macOS arm64). Web route debugging is implemented for worker, pool and request isolation.

## Why

elephc already has a strong **performance** story: `--with-monitoring` gives
exact PHP-level call counts, inclusive/exclusive time, allocations, retained
objects, DB/network wait, call edges, a Chrome trace timeline, a live sampler,
remote capture, and an HTML call graph. What it does not have is a **correctness
debugger**: breakpoints, stepping, stack inspection at a stop, locals, exception
breakpoints, call history with argument values, coverage, or a postmortem view.

The goal is an xdebug-class developer experience without turning elephc into an
interpreter:

- stop on a PHP line and inspect the PHP stack and locals;
- step over / into / out, continue, pause;
- break on exceptions and on uncaught exceptions;
- collect a call trace with values for later analysis;
- measure line/branch coverage;
- reach all of it from a normal IDE (DAP), a terminal, and the existing
  monitoring HTML surface.

## What already exists

Do not rebuild these; they are the foundation and the compatibility contract.

| Surface | Where | What it gives us |
|---|---|---|
| Source markers | `src/codegen/block_emit.rs` (`emit_instruction_source_marker`, `@fn` / `@src` / `@endfn`) | Every EIR instruction with a span can become a debug site. |
| Source identity | `src/span.rs` (`Span::source_id`, `source_path`) | Included/autoloaded files already have distinct identities, even though the current markers do not print them. |
| DWARF lines | `src/debug_info.rs`, `--debug-info` | A real `.debug_line` program and `DW_TAG_subprogram` per PHP function; lldb/gdb already break on PHP lines. |
| Source maps | `src/source_map.rs`, `--source-map` | Versioned JSON with functions, labels, instruction mappings and an inverse line index. |
| Frame metadata | `src/ir/function.rs` (`LocalSlot`, `FunctionParam`), `src/codegen/frame.rs` (`FrameLayout::local_offsets`) | Local names, PHP types, kinds, parameter flags and exact frame-pointer offsets are known. |
| Exact profiler | `crates/elephc-instr`, `--with-monitoring` | Enter/exit shadow stack, function ids/names, call edges, exception and fiber resynchronization, Chrome trace. |
| Sampler / endpoint | `crates/elephc-probe`, `src/monitor` | Frame walking, symbolization, remote endpoint, build-key handshake, live capture. |
| Monitor UX | `src/monitor/render.rs`, `docs/beyond-php/profiling.md` | HTML call graph, source view, timeline, distributed traces, exports. |
| Throwable traces | `elephc-t0p`, `elephc-j6o.6` | The runtime side table and uncaught report are already being built there; the debugger must consume that, not duplicate it. |

## What is missing

1. **No interactive stop.** `--debug-info` lets lldb stop at a line, but there is
   no PHP-aware agent, no locals, no stepping semantics for inlined code,
   fibers, or web workers.
2. **Variables are absent from DWARF.** `--debug-info` emits only a line table
   and subprograms. lldb sees registers and assembly, not `$order`, `$total` or
   `$this`.
3. **Included files were collapsed in debug artifacts.** `@src` printed only
   `line`/`col`, `.file 1` named the entry file, and the source map had one
   `source` field, so a breakpoint in `src/Cart.php` mapped to the entry file
   at the same line. Fixed in `elephc-jwio.1`: source ids now flow through
   `@file`/`@fn`/`@src`, DWARF `.file`/`.loc`, source-map v3 `source_id`, and
   debug metadata.
4. **Call collection has no values.** `ELEPHC_INSTR_TRACE` records enter/exit
   spans, not arguments, returns, exceptions, or memory deltas.
5. **No coverage.** `--counters` counts function calls only.
6. **No postmortem.** A fatal/uncaught exception prints a report, but there is
   no bounded call history to inspect afterwards.
7. **Partial debug protocol/UI.** `elephc debug` and `elephc debug --dap` exist.
   DAP supports launch, attach, pause-while-running, source-hash/stale-source
   rejection, line/conditional/function/data breakpoints, caught/uncaught/throw
   exception filters, stackTrace, scopes/variables (arrays, strings, ref-cells and
   objects with class/property children), simple-local evaluate and
   continue/next/stepIn/stepOut. Data breakpoints cover frame locals and declared
   object properties. Object rendering includes dynamic properties and statically
   folded `__debugInfo()` projections (runtime-only bodies are flagged, not executed).
   Data breakpoints cover locals, declared object properties and dynamic-property
   hash entries. Object rendering also shows backed hooked properties from their raw
   backing slot with a caveat and virtual properties as markers, without executing
   accessors. Pause works in launch and attach modes (the attach runtime publishes
   `<socket>.pid`). The VS Code/nvim smoke tests now live in `tests/editor/`; still missing: the debugger page in the monitoring HTML and macOS execution evidence.

## Options

### Option A — IDE adapter over lldb/DWARF

Extend DWARF with variables and source files, then ship a DAP adapter that drives
`lldb-dap`/`gdb`.

- **Pros:** smallest runtime change; uses a mature native debugger; good for
  native crashes and FFI; zero runtime overhead beyond `--debug-info`.
- **Cons:** PHP values are not inspectable without pretty printers and runtime
  layout knowledge; macOS attach and iOS are restricted; fibers/generators and
  web workers have no PHP semantics; expression evaluation is C-level; optimized
  builds lose variables.
- **Verdict:** useful as a complementary "native attach" mode, not as the
  primary xdebug replacement.

### Option B — In-process PHP debug runtime + DAP (recommended)

A `--debug` build embeds a small runtime agent and a compact metadata table.
The compiler emits a cheap per-site guard; the agent owns breakpoints, stepping,
the PHP call stack, locals and exception stops. `elephc debug --dap` is a thin
DAP adapter over that agent; `elephc debug` is a terminal client.

- **Pros:** true PHP-level semantics; direct access to frame slots and runtime
  values; works on macOS and Linux without ptrace; can model fibers and web
  workers; one hook family can later feed call collection and coverage.
- **Cons:** compiler and runtime work; debug builds only; needs careful ABI
  preservation and performance measurement; not a production mode.
- **Verdict:** recommended primary architecture.

### Option C — Full VM/bytecode debugger

Add a bytecode interpreter and debug there.

- **Verdict:** rejected. It duplicates the eval/Magician path, changes the
  performance model, and weakens the "native, no VM" property that makes elephc
  interesting.

### Recommendation

**Option B, with Option A as a later native-crash companion.** Reuse the
monitoring infrastructure for postmortem visualization and security, but keep
the debugger's runtime independent: a debug build must not require
`--with-monitoring`, and a monitored production binary must not carry the
debugger.

## Product surface

### CLI

```bash
elephc debug app.php
elephc debug --attach <pid> --key app.key
elephc debug --dap app.php          # stdio DAP for an IDE
elephc debug --web app.php --route '/orders/*'
elephc monitor app.php --calls=scalars --html report.html
elephc test --coverage
```

`elephc debug` starts the program and gives a small terminal client:

```text
(elephc) break app.php:42
Breakpoint 1 at app.php:42
(elephc) continue
Stopped at app.php:42 (breakpoint 1)
(elephc) bt
#0  App\Cart::total() at app.php:42
#1  App\Checkout::run() at checkout.php:18
#2  {main} at app.php:7
(elephc) locals
$items    array(3) [0 => App\Item, ...]
$total    int(0)
$this     App\Cart { ... }
(elephc) next
```

Use `--debug` for the interactive build. Keep `--debug-info` as the "standard
DWARF for lldb/gdb" flag; `--debug` implies it, keeps symbols, disables EIR
optimization/inlining, and links the debug runtime. `--ir-opt=off` is not
enough: the AST optimizer still folds constants, removes dead code and prunes
unreachable declarations, so the build mode must either relax those passes or
publish a line-liveness map that lets the client mark unverified lines. A future
`--debug-optimized` can allow optimized builds with an explicit "optimized out"
caveat.

### IDE (DAP)

Ship a thin VS Code extension (or document the generic DAP launch) and a DAP
adapter:

```json
{
  "type": "elephc",
  "request": "launch",
  "program": "${workspaceFolder}/app.php",
  "stopOnEntry": true,
  "args": []
}
```

DAP requests v1: `initialize`, `launch`/`attach`, `setBreakpoints`,
`setExceptionBreakpoints`, `configurationDone`, `threads`, `stackTrace`,
`scopes`, `variables`, `evaluate` (read-only locals/properties/indices),
`continue`, `next`, `stepIn`, `stepOut`, `pause`, `disconnect`.

DAP events: `initialized`, `stopped`, `output`, `continued`, `exited`,
`terminated`.

PhpStorm does not speak DAP; it speaks the DBGp protocol. A DBGp endpoint over
the same agent (`elephc-jwio.2`) is an optional later transport, not a v1
requirement and not an xdebug API compatibility promise. DAP is the v1 adapter
for VS Code/Neovim.

### Web UI

Do not build a second IDE. Extend the existing self-contained monitor HTML with a
postmortem/call-collection view: call tree, timeline, source lines, arguments,
returns, exceptions. Live stepping remains in the IDE/CLI; the browser page is
for reading a capture after the fact.

### Elephc-native debug helpers

Do not clone xdebug's Zend extension ABI and do not expose `xdebug_*` functions.
Provide Elephc-native helpers, declared `extension: true` so `--strict-php`
hides them:

- `elephc_break()` — stop when a debugger is attached; no-op otherwise.
- `elephc_dump(mixed $value, ?string $label = null, string $to = 'auto')` —
  formatted `var_dump`-style snapshot. `$to` is `stderr` (stable fd 2 output),
  `terminal` (debugger console, fallback stderr), `html` (a Dump card in the
  active `monitor --calls --html` capture, fallback stderr), or `auto`
  (terminal if attached, else HTML capture, else stderr). Invalid values raise
  `ValueError`.
- `elephc_trace_start(string $name)` / `elephc_trace_end(string $name)` — open
  and close a named trace segment while a call capture (`monitor --calls`) or an
  interactive/DAP debug session is active. Segments nest; `end` must match the
  top of the stack or it warns and no-ops; empty names raise `ValueError`;
  unclosed segments close at exit and are reported incomplete. Outside a capture
  or debug session they are no-ops. Debug-session segments and scoped dumps are
  printed to the debug console; monitor-capture segments feed JSONL/HTML.

Named segments appear in the monitor HTML Traces panel and tag the calls and
dumps inside them. The call capture and coverage flows are `monitor --calls`
and `test --coverage`, not xdebug API calls.

## Technical design

### 1. Debug metadata

The compiler already knows everything needed; make it explicit and versioned.

Emit a `DebugMetadata` document at `--debug`:

- **Files:** source id, canonical path, SHA-256, line count.
- **Functions:** id, PHP name/display name, entry symbol, source id, line range,
  flags (method/closure/generator/fiber/main/synthetic), parameters.
- **Sites:** id, function id, source id, line, column, end position, kind
  (line/return/throw/call), flags (breakpointable, step target).
- **Locals:** function id, name, frame offset from `fp`, PHP type, storage kind
  derived from `LocalKind` plus `ref_cell_state_offsets` (raw/ref-cell/dynamic
  runtime state), parameter index, flags. Do not invent a parallel kind enum.
- **Types/classes:** enough to format scalars, arrays, objects, references and
  `Mixed` cells without guessing.

Ship it two ways:

1. **Embedded binary table** behind a symbol such as `elephc_debug_metadata`,
   with `elephc_debug_init(table, len)` called from the process/web entry (the
   same pattern as `elephc_probe_init`). This is what the runtime agent reads;
   it makes a single binary self-contained.
2. **Human/tool sidecar** (`app.edbg.json`, versioned) for external tools,
   tests, and inspection. The embedded table is authoritative; the sidecar is a
   projection.

Add source identity to existing artifacts at the same time:

- `@src` gains `file=<source_id>` (and `@fn` gains its defining source id).
- `--debug-info` emits one `.file <id> "path"` per included source and
  `.loc <id> <line> <col>` per site. This fixes lldb/gdb for includes.
- `--source-map` v3 adds a `sources` array; v2 stays readable. The existing
  `source` field remains the entry file for compatibility.

### 2. Per-site guard and slow path

At each executable source line, emit a guard before the line's first
instruction:

```asm
    ldr  w9, [site_flag]       // or: cmp byte ptr [rip + site_flag], 0
    cbnz w9, .Lslow            // jnz .Lslow
.Lcont:
    ...                        // line body
.Lslow:
    bl   __elephc_debug_site   // saves all caller-saved registers, calls Rust
    b    .Lcont
```

The flag is per site in BSS. Setting/clearing it is how a breakpoint is
armed/disarmed. Use two levels: a global `debug_active` word, then a
per-function "has breakpoint" word, then the per-site flag. A debug binary run
without a debugger pays one load+branch per line, not a call. Emit **one site
per source line per basic block** that executes it (deduplicated across EIR
instructions within the block), at the first instruction of that line; a
breakpoint arms every site for the line, so a loop or multi-block statement is
not missed and does not create duplicate stops within one block. The guard is a straight-line sequence that falls through to the
line body; it must not change the EIR CFG or split a machine basic block in a
way the peephole/unwinder cannot model. Treat the slow-path call as a peephole
barrier.

The slow path is a hand-written per-target wrapper that saves **all**
caller-saved integer and SIMD registers (AArch64 `x0-x17`, `q0-q7`; x86_64
`rax, rcx, rdx, rsi, rdi, r8-r11`, `xmm0-xmm15`) plus flags, calls the Rust
hook, restores, and returns. The exact save set must come from the target ABI
helpers, including platform-specific registers such as Darwin's reserved `x18`,
not from a hand-written list. No EIR value may be clobbered. This wrapper belongs
in `src/codegen_support/abi/` and must be tested on every supported target. A
full save set is the safe v1; if measurement shows it dominates, the spike can
try a compiler-computed live-set save as an optimization, never as the first
correctness baseline.

Alternative considered: patch line sites with `brk`/`int3` on attach. It has
zero dormant cost but needs `mprotect`, instruction-cache flushes, code-signing
and hardened-runtime handling, and does not work on iOS. Reject for v1; keep as
a possible native-attach optimization.

### 3. Runtime agent

New workspace crate `crates/elephc-debug/`, linked only by `--debug` through the
existing bridge table (`src/linker/bridges.rs`). The bridge entry needs a
reviewed `MonitoringPolicy` (infrastructure, with a nonempty reason) and must
not claim DB/network slots; the debug session is not a profiling capture.

State:

- session state (attached, running, stopped, reason);
- breakpoints keyed by `(source_id, line)`, with optional condition/hit count;
- per-site flags shared with generated code;
- per-thread and per-fiber shadow stacks of PHP activations
  (`function_id`, `fp`, call site);
- current stop (`site_id`, `fp`, function, fiber/thread, reason).

Hooks:

- `elephc_debug_enter(fn_id, fp)` / `elephc_debug_exit(fn_id, fp)` — stack,
  step depth, call collection.
- `elephc_debug_site(site_id, fp)` — breakpoint/step check; may block.
- `elephc_debug_throw(exception, site_id, fp)` — exception breakpoints and
  stack capture before unwinding.
- `elephc_debug_suspend/resume(fiber_id, fp)` — generator/fiber stacks.
- `elephc_debug_pause()` — `elephc_break()` and signal-triggered pause.

Prefer **one activation-stack mechanism with two policies**, not two copies.
`crates/elephc-instr` already solved recursion, exception resynchronization,
fiber/generator park/unpark and the shadow-stack cap. The ADR must decide between
extracting that mechanism into a shared crate (with the timing policy layered on
top) or justifying a separate debugger stack with an explicit test matrix. A
debug binary must not require the profiler to be linked, and `--debug` +
`--with-monitoring` should compose with a non-interference test rather than be
refused by default.

Pause mechanics: `pause` sets an async-signal-safe atomic flag; the next site
stops. If the program is blocked in I/O, the stop is delayed until it reaches a
site; document this. A paused thread publishes its stop and waits on a condition
variable; a dedicated agent thread serves the session. This keeps the generated
code simple and lets a fiber/web worker stop without stopping unrelated
processes. The process must stop at the entry site until `configurationDone`, or
breakpoints set on initialization code are lost.

Inspection is strictly borrowed: reading a local must not incref/decref, trigger
copy-on-write, or release a temporary. Either freeze the relevant ownership
operations while stopped or take a no-write borrow; document that `variables`
cannot mutate the program. Reuse the runtime's value-formatting routines where
possible instead of reimplementing `var_dump` in the agent.

### 3.1 Internal agent protocol (v1 sketch)

The runtime agent does not need to speak DAP itself. A smaller framed JSON-RPC
keeps the agent lean and lets the CLI and DAP adapter share one client. Launch
uses an inherited socketpair; attach uses a loopback socket plus the build-key
handshake. Draft requests:

```text
hello                 { protocol, build_id, pid, target }
setBreakpoints        { source, breakpoints: [{ line, condition?, hitCondition? }] }
setExceptionBreakpoints { filters: ["throw" | "caught" | "uncaught"] }
configurationDone
continue / next / stepIn / stepOut / pause / disconnect
threads
stackTrace            { threadId, startFrame?, levels? }
scopes                { frameId }
variables             { variablesReference, start?, count? }
evaluate              { frameId, expression, context }
```

Draft events:

```text
initialized
stopped               { reason, threadId, frameId, siteId, exception? }
output                { category, text }
continued
exited                { code }
terminated
```

The DAP adapter owns DAP framing, source path mapping, source-hash checks and
client capabilities. The agent owns runtime state and values. This boundary is
the main thing the ADR must freeze.

### 4. Locals and values

Read a local at `address = frame_pointer - frame_offset`, using the metadata's
storage kind:

- raw scalar/string/array/object pointer: read the codegen representation;
- ref-cell: load the cell pointer at the slot, then dereference;
- dynamic: read the representation-state word from its frame slot and choose
  raw vs ref-cell;
- uninitialized/undefined: report `undefined` (PHP semantics), not zero.

Format values in the agent, not in generated code:

- scalars, strings (bounded), arrays (indexed/hash, depth/page limits),
  objects (class name + properties, visibility optional), references,
  `null`/`bool`, resources/pointers.
- For `Mixed` cells, reuse the runtime tag/payload contract; never guess from
  raw bytes.
- Page large arrays/objects; never walk a cycle without a visited set on the
  agent side.
- `evaluate` v1 is read-only: locals, property reads, array indices, `count`,
  simple arithmetic. Arbitrary PHP evaluation is a separate opt-in through
  Magician (`--allow-eval`) and never on by default.

### 5. Stack and stepping

The shadow stack gives PHP frames, including recursion, closures, methods and
fibers. Frame ids in DAP encode `(thread/fiber, depth)`.

- `continue`: clear the temporary step mode; run to a breakpoint, pause,
  exception, or exit.
- `next`: stop at the next site with `depth <= start_depth`.
- `stepIn`: stop at the next site with `depth > start_depth` (or any new site).
- `stepOut`: stop at the next site with `depth < start_depth`.
- `run to cursor`: temporary breakpoint at a site.
- Conditional breakpoints: v1 hit count; v2 expression evaluator.
- Exception breakpoints: `throw`, `caught`, `uncaught`; stop before unwinding
  for `throw`, after a handler is found for `caught` if feasible.

Optimized/inlined code: v1 `--debug` disables EIR optimization/inlining, so
every PHP frame exists and locals are stable. For `--debug-optimized`, report
"optimized out" rather than lying.

### 6. Web and fibers

- Fibers/generators: shadow stack per `_fiber_current`; suspend/resume hooks
  park and restore. DAP exposes each live fiber as a thread-like stack.
- `--web`: implemented for worker, pool and request isolation. In worker mode
  the selected worker creates a mode-0600 Unix socket in a broker-owned 0700
  directory after `fork()`, advertises `{pid, method, path, socket}`, and blocks
  (bounded) for the broker. Only that worker stops; the prefork master and other
  workers keep serving. The broker selects the first request matching `--route` (exact or
  trailing-`*`), `--method` and one `--header`; `disconnect` resumes the request
  and leaves the server running.
- Inherited socketpairs were not used: a shared fd would be read by whichever
  sibling worker reached the debugger first, and a loopback listener does not
  survive the web fork/exec shape. Creating the advertised socket in the
  selected worker after the request arrives keeps the credential per worker.
- `--web-isolation=pool` / `request`: wired by invoking the same route hook in
  the handler child (`execute_handler_request`) before PHP runs. The child, not
  the worker, advertises and accepts the DAP socket, so no descriptor has to
  cross the worker/handler IPC boundary. The worker/broker stay async; the
  isolated tests use one worker with `--handler-concurrency 2` to prove /other
  completes while the selected child is stopped. All three models share the
  same fiber stack, route filters and disconnect semantics.
- Do not stop the master process; only the selected worker/request.

### 7. Call collection in `monitor`

There is no separate `trace` command. `elephc monitor` already owns the call
graph, timeline, source view, HTML and `--trace` Chrome export; call values
extend that capture:

- `elephc monitor app.php --calls=scalars --html report.html` auto-builds the
  source with `--debug --with-monitoring` and renders exact performance plus
  call arguments/returns/exceptions in the same HTML page.
- Events: function enter (id, site, depth, timestamp, args level), exit
  (return value), throw (exception), allocation/free deltas, fiber/thread,
  `elephc_dump` snapshots, and named `elephc_trace_start/end` segments.
- Storage: bounded in-memory ring plus streaming JSONL (`--calls-out`); Chrome
  Trace/Perfetto remains `--trace` for timing.
- Argument levels: `none`, `types`, `scalars`, `full`; default `full` when
  `--calls` is passed, with max string/array/object depth and redaction rules.
  Full capture is explicit. An explicit `elephc_dump()` is always recorded
  when a capture is active; `--calls=none` suppresses automatic args/returns
  only.
- Bounded by event count and bytes; overflow is reported, never silent.
- The HTML Calls panel contains the call tree, per-call arguments, return
  value, exception, caller source line, timestamp, fiber and allocation/free
  deltas, linked to the existing performance views. The runtime emits those
  fields in every calls mode, including `--calls=none` and postmortem runs.
- A binary target must carry both capabilities; a production
  `--with-monitoring` binary without value capture is refused rather than
  silently returning a profile without values.
- Postmortem: implemented for uncaught PHP exceptions through
  `elephc debug --postmortem FILE`; the document carries the exception fields,
  PHP stack and recent call ring, and `--last FILE` inspects it. A normal or
  caught exit writes nothing. A native crash is not promised to flush: JSON is
  encoded in synchronized code, never in a signal handler. Segfaults are
  explicitly outside the v1 guarantee.

### 8. Coverage

- `--coverage` emits per-site/line counters (BSS) and branch-edge counters.
  Only coverage builds pay for them.
- `elephc test --coverage` compiles and aggregates each test binary; report
  selectors are `--coverage-format text|lcov|cobertura` and `--coverage-out
  FILE` (HTML is later). There is no separate `coverage report` subcommand.
- Branch coverage initially maps branch sites from EIR conditional terminators.
  A terminator is block-level, not source-level: attribute it only when the
  source construct is proven (e.g. a single `if`/loop condition), and otherwise
  report function/line coverage only rather than inventing a branch.
- Reuse the existing `--counters`/BSS mechanism rather than adding a third
  counter family; coverage is a different policy over the same per-site data.
- PHPUnit's xdebug coverage API is out of scope for v1; document native
  coverage as the supported path.

### 9. Native / lldb mode (later)

Once the metadata exists, `--debug-info` can also emit `DW_TAG_variable` entries
for locals and a source file table. That gives lldb/gdb PHP variable names and
included-file line numbers for native crash analysis. This is Option A as a
companion, not a replacement for the in-process agent.

## Targets

| Target | Interactive debugger | Call trace | Coverage | Notes |
|---|---|---|---|---|
| linux-x86_64 | yes | yes | yes | primary CI target |
| linux-aarch64 | yes | yes | yes | primary CI target |
| macos-aarch64 | yes | yes | yes | no ptrace needed for launch; dSYM for native crashes |
| ios-sim-arm64 | simulator only, behind explicit flag | yes | yes | validate sandbox/socket; otherwise compile-time only |
| ios-arm64 | no | yes | yes | refuse interactive mode with a clear diagnostic; trace/coverage still work |

The runtime agent and site hooks must compile and emit on all five targets; the
interactive server is gated where it cannot work. This follows the repository's
"all supported targets are first-class, or explicitly gated with diagnostics,
tests and docs" policy.

Hosted libraries (`--emit cdylib` / `--emit staticlib`) are out of scope for the
interactive v1: the host owns the process and the entry point. Debug metadata can
still be emitted for a host-side debugger later, but `elephc debug` cannot launch
a library and must say so.

## Security

- `--debug` is visibly a debug build; warn at link time and embed a marker.
- Launch uses an inherited socketpair; no listener.
- Attach binds loopback only. Remote attach requires the build key (reuse the
  `elephc-probe` HMAC handshake) plus an explicit `--allow-remote-debug`; no
  environment variable turns a debug server on.
- No arbitrary `evaluate` by default; opt-in Magician evaluation, debug builds
  only.
- Redact values in logs/traces by default; full capture is explicit.
- The debugger reads source on the client; the agent sends positions, not file
  contents.

## Performance

- Non-debug builds: no hooks, no metadata, no runtime; unchanged.
- Debug build, no session: one load + branch per executable line (plus function
  enter/exit if call collection is on). Measure on a call-heavy and a
  line-heavy fixture; target a small single-digit percentage on a debug build.
- Debug build, stopped: only the active site pays the slow path; stepping may
  arm all sites, which is expected to be slower.
- Call collection: bounded by ring size; argument capture levels control cost.
- Coverage: counters only in coverage builds.

### AArch64 spike evidence (2026-11-05)

`tests/debugger_spike_aarch64.rs` exercises the planned guard shape on the
AArch64 Linux host:

- dormant guard (flag = 0): **~0.3–0.5 ns/iteration** across runs;
- active slow path (save x0-x17 + q0-q7, call a Rust hook, read scalar/string/
  array locals through `fp-offset`, restore): **~5.6–6.3 ns/iteration**;
- caller-saved ABI probe: x0-x17 and q0-q7 all survive the Rust call;
- frame reads: scalar, string pointer/length and array pointer/length all match.

A Linux x86_64 counterpart at `tests/debugger_spike_x86_64.rs` was executed
under `qemu-x86_64`: guard, ABI and frame-local tests pass. Its active timing is
emulator-inflated (753 ns/iter under qemu) and is not a native cost measurement.

A macOS arm64 variant at `tests/debugger_spike_macos_aarch64.rs` compiles to a
Mach-O object with `rustc --target aarch64-apple-darwin --emit=obj`; execution
still needs the macOS CI runner. This is a compile-only result today.

## External review (2026-11-05)

Three external reviewers read this plan and the cited source at baseline
`db62f09226`: GLM-5.3-Flash, MiniMax-M3.1-Flash and Mistral Large 4 (all via
Ollama). All three returned **GO with reserves**; none rejected the in-process
architecture or proposed a VM.

| Reviewer | Verdict | Main emphasis |
|---|---|---|
| GLM-5.3-Flash | GO with reserves | AST optimizer can erase breakpointable lines; bridge `MonitoringPolicy`; binary/BSS/cache size; reuse runtime value formatting; GC/refcount safety; debug helper stub in every build; web broker; DAP estimate |
| MiniMax-M3.1-Flash | GO with reserves | Do not duplicate the `elephc-instr` shadow stack; reuse the activation chain; derive locals from `LocalKind`/`ref_cell_state_offsets`; ABI save set and guard/CFG/peephole; `elephc-web` fork model; PhpStorm speaks DBGp; CLI parser refactor; 8-12 week MVP |
| Mistral Large 4 | GO with reserves | Save only the live set (or prove full save); GC freeze/borrow during inspection; stop at entry until `configurationDone`; web addressing; Elephc debug builtins `extension: true`; reuse counters; allow `--debug` + monitoring; spike first |

Consensus changes now reflected above:

- **One activation-stack mechanism, two policies.** Extract or reuse the
  `elephc-instr` shadow-stack core (recursion, exception resync, fiber
  park/unpark); do not duplicate it without an ADR justification.
- **AST optimizer is part of the debug contract.** `--ir-opt=off` is
  insufficient; add a debug AST path or a line-liveness map.
- **Fix included-file debug artifacts first**, as an independent bug
  (`elephc-jwio.1`), before the metadata schema depends on them.
- **Define the ABI save set from target helpers**, not a hand-written register
  list; full save is the v1 correctness baseline, live-set save an optimization.
- **Inspection is borrowed and must not mutate refcounts/COW**; values may be
  formatted through runtime helpers.
- **Stop at entry until `configurationDone`**, and specify web worker
  addressing/fork behavior in the ADR.
- **DAP is the v1 adapter; DBGp is optional later** for PhpStorm
  (`elephc-jwio.2`), not an xdebug API promise.
- **No xdebug API dependency.** Use Elephc-native `elephc_break()` and
  `elephc_dump()`; call capture belongs to `elephc monitor --calls`, not a new
  `trace` command.
- **MVP is 8-12 weeks, not 4-6**, and excludes `evaluate`, fibers/web debugging
  and coverage from the first milestone.

## Phases and rough estimates

These are planning numbers, not commitments. Split into the beads below.

| Phase | Scope | Rough size |
|---|---|---|
| 0. Spike | Prove line guards, ABI preservation, frame/local reads, pause/continue on Linux x86_64 + macOS arm64; measure time and text/BSS size; decide the shared activation stack; write ADR | 1-2 weeks |
| 1. MVP | `--debug` preset, metadata v1, site guards, runtime agent, entry stop, CLI `break/continue/bt/locals`, DAP adapter, editor smoke tests, docs. No `evaluate`, no fibers/web, no coverage. | 8-12 weeks |
| 2. Usability | step in/over/out, exception breakpoints, hit counts, fibers, web route attach, source hash/path mapping, watch/evaluate v1 | 6-8 weeks |
| 3. Collection | `monitor --calls` with args/returns, HTML Calls panel, JSONL/Chrome/Perfetto, postmortem ring, coverage via `elephc test --coverage` | 6-8 weeks |
| 4. Hardening | iOS trace/coverage, native lldb variables, remote security audit, optional DBGp/PhpStorm transport, perf tuning | ongoing |

## MVP acceptance criteria

- `elephc debug app.php` stops at `break app.php:12`, prints the PHP stack and
  typed locals, supports `next`/`step`/`continue`, and exits cleanly.
- A DAP client can launch, set a line breakpoint, inspect locals, continue and
  disconnect; the process stops at entry until `configurationDone`.
- Breakpoints in an included file resolve to that file, not the entry file.
- A line removed by the AST optimizer is reported as unverified, never silently
  accepted as a breakpoint that will never hit.
- Works on linux-x86_64, linux-aarch64 and macos-aarch64; iOS emits trace
  metadata and refuses interactive debugging with a clear message.
- Non-debug builds are unchanged; debug-build dormant overhead is measured and
  documented.
- Tests: metadata schema, hook emission on all five targets, ABI preservation,
  runtime protocol, CLI end-to-end, DAP request/response conformance, included
  files, fibers, and a performance guard.
- Docs: `docs/beyond-php/debugging.md`, CLI reference, source-map/DWARF updates,
  README and examples.

## Confirmed product decisions

- `elephc_trace_start/end` are named segments inside an active `monitor --calls`
  or debug capture. They do not start a capture themselves; a standalone trace
  trigger is rejected for v1 (no destination/build/viewer story).
- No xdebug API dependency; the source helpers are `elephc_break()`,
  `elephc_dump()` and `elephc_trace_start/end()`.
- No `trace` or `coverage` subcommands; call capture lives in
  `monitor --calls`, coverage in `test --coverage`.
- DAP is the v1 adapter; DBGp/PhpStorm is optional P4.

## Open decisions

1. Is the priority interactive debugging, call collection, or coverage? The
   architecture is shared but the first milestone changes.
2. Primary IDE: VS Code/Neovim (DAP, v1) or PhpStorm (optional DBGp later)?
   PhpStorm does not speak DAP, but DBGp is not a v1 requirement.
3. Should `--debug` always disable optimizations, or expose a separate
   `--debug-optimized` from day one? What is the AST-optimizer contract?
4. Do we extract a shared activation stack from `elephc-instr`, or justify a
   separate debugger stack with an explicit test matrix?
5. Is `--debug` + `--with-monitoring` a supported composition from v1, or a
   documented later combination?
6. Are the four Elephc-native helpers (`elephc_break`, `elephc_dump`,
   `elephc_trace_start/end`) sufficient, or is another debug primitive needed?
   No xdebug API compatibility is planned.
7. Is production postmortem collection a goal? If yes, it should be a separate
   `--with-debug` capability, not the interactive debug build, and values must
   be redacted by default.
8. Is iOS limited to trace/coverage acceptable, or is simulator debugging a
   hard requirement?

## Source references

- `src/codegen/block_emit.rs` — `emit_instruction_source_marker`, `@fn`/`@src`.
- `src/debug_info.rs` — DWARF line table and subprograms.
- `src/source_map.rs`, `docs/compiling/source-maps.md` — v2 schema.
- `src/span.rs` — source identities and paths for includes.
- `src/ir/function.rs` — `FunctionParam`, `LocalSlot`, `LocalKind`.
- `src/codegen/frame.rs` — `FrameLayout`, `local_offsets`, instrumentation hooks.
- `src/codegen/mod.rs` — `Instrumentation`.
- `src/codegen/machine_peephole.rs` — machine-level rewriting that must treat a
  guard/slow-path call as a barrier.
- `src/optimize/` — AST folding, propagation and DCE that can erase a
  breakpointable line before EIR.
- `crates/elephc-instr/src/lib.rs` — exact profiler, trace timeline, coroutine
  handling; candidate shared activation-stack mechanism.
- `crates/elephc-probe/src/{lib,endpoint,handshake}.rs` — sampler, endpoint,
  build-key handshake.
- `crates/elephc-web/` — prefork and per-request fork model that the debug
  broker must respect.
- `src/monitor/` — CLI, remote capture, HTML rendering.
- `docs/beyond-php/profiling.md` — existing performance surface and cost model.
- `elephc-t0p`, `elephc-j6o.6` — Throwable trace work to consume.
