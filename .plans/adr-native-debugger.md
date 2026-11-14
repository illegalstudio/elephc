# ADR: Native PHP debugger contract, metadata and protocols

- Status: **Accepted** — 2026-11-05.
- Implementation gates: macOS arm64 probe execution, shared activation-stack
  extraction, and metadata v1 schema review. They are tracked follow-ups, not
  reasons to keep the architecture undecided.
- Date: 2026-11-05.
- Baseline: `db62f09226` (2026-10-29).
- Decision issue: `elephc-qq34`.
- Related: `.plans/native-debugger.md` (proposal), `.plans/native-debugger-usage.md`
  (developer walkthrough), epic `elephc-jwio`.

## Context

elephc is a native AOT compiler. It already has:

- `--debug-info`: DWARF line table and `DW_TAG_subprogram` per PHP function;
- `--source-map`: versioned JSON with functions, labels, instruction mappings
  and an inverse line index;
- `--with-monitoring`: exact/sampled PHP-level profiling, call edges,
  allocations, DB/network wait, Chrome traces, monitor HTML and remote capture;
- `--counters`: per-function call counters;
- `Span` source identities and a frame layout that knows every local's PHP type
  and frame-pointer offset.

What it lacks is a correctness debugger: breakpoints, stepping, stack and local
inspection, exception stops, value-level call history, postmortem captures and
coverage. A line-only DWARF artifact is not enough, and the current debug
artifacts collapse included files into the entry file (tracked as
`elephc-jwio.1`).

The product must stay native: no VM, no bytecode interpreter, no duplicate of
the eval/Magician runtime. It must work across the supported target policy:
Linux x86_64/aarch64, macOS arm64, iOS device and iOS Simulator.

## Decision summary

| # | Decision | Status |
|---|---|---|
| D1 | Separate correctness debugging from performance profiling | Firm |
| D2 | Define `--debug`, `--debug-info`, `--debug-optimized` precisely | Firm: `--debug` skips folding/DCE and reports unverified lines |
| D3 | Emit authoritative embedded metadata plus a sidecar projection | Firm |
| D4 | One activation-stack mechanism, two policies | Firm: reuse/extract `elephc-instr`; extraction is an implementation gate |
| D5 | Agent owns runtime state; adapters own DAP/DBGp | Firm |
| D6 | DAP first for VS Code/Neovim; DBGp optional later | Firm: DAP v1; DBGp P4 deferred |
| D7 | Inspection is borrowed, read-only and non-mutating | Firm |
| D8 | Full desktop debugging; iOS device trace/coverage only | Firm |
| D9 | Debug server is local-only by default; attach uses the build key | Firm |
| D10 | Elephc-native `elephc_break`/`elephc_dump`; no xdebug API | Firm |
| D11 | Call-value capture belongs to `monitor`, not a new trace command | Firm |
| D12 | `debug`/`monitor`/`test` surface; no `trace` or `coverage` subcommands | Firm |

## D1. Product boundary

The debugger is for **correctness**:

- stop on a PHP line;
- inspect the PHP stack and typed locals;
- step over/into/out, continue, pause;
- break on exceptions and uncaught exceptions;
- collect a bounded call trace with values;
- measure line/branch coverage;
- inspect a postmortem capture.

The profiler remains for **performance**:

- `--with-monitoring` exact/sampled profiles, allocations, retained objects,
  DB/network wait, edges, HTML/Perfetto exports;
- `elephc monitor` and its live/attach/remote paths.

The debugger must not reimplement the profiler, and a production monitored
binary must not carry the interactive debugger. The two may compose, but they
are separate products with separate flags and separate policies.

## D2. Build modes and flags

### `--debug-info`

Metadata only, no runtime:

- emits DWARF line/subprogram information and the source map;
- does not link the debug agent;
- does not emit per-site guards;
- remains safe to use on an otherwise normal build.

This is the "standard debugger/profiler" artifact. Fix included-file identity
(`elephc-jwio.1`) before extending it.

### `--debug`

The interactive debug build:

- implies `--debug-info`;
- keeps symbols and embeds debug metadata;
- links `crates/elephc-debug`;
- emits breakpoint sites for each executable source line (one per basic block that executes it) and the guard/slow path;
- disables EIR optimization and inlining by default (`--ir-opt=off`);
- runs a reduced AST optimization set, or publishes a line-liveness map for any
  mandatory pass that erases source-mapped statements;
- stops at the entry site until `configurationDone` when launched under a
  debugger;
- embeds a visible "debug build" marker and refuses production deployment
  assumptions by documentation, not by silently degrading.

`--ir-opt=off` alone is **not** sufficient: `src/optimize/` still folds
constants, removes dead code and prunes unreachable declarations before EIR. The
accepted `--debug` contract therefore runs a reduced AST optimization set:
constant folding, propagation, dead-code elimination and unreachable
declaration pruning are skipped, while normalization required for semantics and
codegen remains. Any mandatory rewrite still publishes line liveness, and a
breakpoint on a removed line is reported as unverified, never silently accepted.

### `--debug-optimized`

Deferred. It may allow optimizations with explicit "optimized out" locals, but
it is not part of the v1 contract. Do not add a half-working flag in v1.

### `--with-monitoring` composition

`--debug` and `--with-monitoring` may be combined. `elephc monitor --calls`
auto-builds a source target with both capabilities; a binary target must carry
both. The activation-stack mechanism is shared where possible; the profiling
policy and debugger policy remain independent. A non-interference test must
prove that a debug session does not corrupt a profile and that a monitored
binary without a session does not enter the debugger.

## D3. Debug metadata

Emit a versioned `DebugMetadata` document:

- **Files:** source id, canonical path, SHA-256, line count.
- **Functions:** id, PHP name/display name, entry symbol, source id, line range,
  flags (method/closure/generator/fiber/main/synthetic), parameters.
- **Sites:** id, function id, source id, line, column, end position, kind
  (line/return/throw/call), flags (breakpointable, step target).
- **Locals:** function id, name, frame offset from `fp`, PHP type, storage kind
  derived from `LocalKind` plus `ref_cell_state_offsets`, parameter index, flags.
- **Types/classes:** enough to format scalars, arrays, objects, references and
  `Mixed` cells without guessing.

The metadata is built from the same EIR/codegen structures that place values; it
is emitted after frame layout is known. Do not re-derive offsets or invent a
parallel local-kind enum.

Ship it two ways:

1. **Embedded table**, behind a symbol such as `elephc_debug_metadata`, with
   `elephc_debug_init(table, len)` called from the process/web entry. This is the
   authoritative source for the runtime agent and makes a single binary
   self-contained.
2. **JSON sidecar** (`app.edbg.json`, versioned) for external tools, tests and
   inspection. It is a projection of the same metadata, never a second source of
   truth.

Stability: `format` plus an integer version identify the schema. Within v1,
fields may be added but not renamed or removed; consumers ignore unknown fields.
The sidecar records the source hashes and build id so a stale file is detected.

The existing artifacts must expose source identity too:

- `@src` / `@fn` gain `file=<source_id>`;
- `--debug-info` emits one `.file <id> "path"` per source and `.loc <id> ...`;
- `--source-map` v3 adds a `sources` array while v2 stays readable.

## D4. Instrumentation and activation stack

One mechanism, two policies — accepted.

`crates/elephc-instr` already handles recursion, exception resynchronization,
fiber/generator park/unpark and the shadow-stack cap. The debugger will reuse or
extract that activation-stack core and layer its own policy (breakpoints, stop
state, step depth, values) on top. Duplicating the coroutine/exception logic is
the main architectural risk. The AArch64/x86_64 guard and ABI probes have
validated the slow-path mechanics; the extraction itself remains an
implementation gate and must keep `elephc-instr` optional in a debug binary.

Per-source-line sites:

- one site per source line per basic block that executes it, deduplicated across EIR instructions within that block; a breakpoint arms every site for the line;
- the guard is a straight-line sequence that falls through to the line body;
- two levels of flags: global `debug_active`, per-function `has_breakpoint`,
  then the per-site flag;
- the slow path calls a target-aware wrapper that preserves every live register
  the compiler says is live at that site, or a full caller-saved save set as the
  v1 correctness baseline;
- the save set is derived from target ABI helpers, including platform-specific
  registers (Darwin's reserved `x18`), never from a hand-written list;
- the guard and slow-path call are peephole barriers; they must not change the
  EIR CFG or confuse the unwinder.

Function enter/exit, throw and fiber suspend/resume hooks feed the activation
stack. The process stops at entry until `configurationDone` when launched under a
debugger.

## D5. Runtime agent and adapters

The runtime agent owns:

- session state, breakpoints, per-site flags;
- the PHP activation stack, per thread and per fiber;
- stop state (`site_id`, `fp`, function, fiber/thread, reason);
- local/value readers and source metadata access.

It exposes a small framed JSON-RPC protocol over a socketpair (launch) or a
loopback socket (attach). It does not speak DAP itself. The adapter owns DAP
framing, source-path mapping, source-hash checks and client capabilities.

Draft protocol (v1):

```text
hello                    { protocol, build_id, pid, target }
setBreakpoints           { source, breakpoints: [{ line, condition?, hitCondition? }] }
setExceptionBreakpoints  { filters }
configurationDone
continue / next / stepIn / stepOut / pause / disconnect
threads
stackTrace               { threadId, startFrame?, levels? }
scopes                   { frameId }
variables                { variablesReference, start?, count? }
evaluate                 { frameId, expression, context }
```

Events: `initialized`, `stopped`, `output`, `continued`, `exited`, `terminated`.

DAP is the v1 adapter: VS Code and Neovim — accepted. DBGp is an optional later
transport for PhpStorm (`elephc-jwio.2`), P4 and outside v1; it is not an xdebug
API promise. The protocol split is firm; DBGp is deferred until a user needs
PhpStorm specifically.

## D6. DAP first, DBGp optional later

DAP surface v1:

- `initialize`, `launch`/`attach`, `setBreakpoints`,
  `setExceptionBreakpoints`, `configurationDone`, `threads`, `stackTrace`,
  `scopes`, `variables`, `evaluate` (read-only subset), `continue`, `next`,
  `stepIn`, `stepOut`, `pause`, `disconnect`;
- source paths with a configurable source root and hash verification;
- an unverified-breakpoint response when the AST/EIR optimizer removed the line.

DBGp transport (optional, later):

- `feature_set`, `breakpoint_set`/`remove`/`list`, `run`,
  `step_into`/`step_over`/`step_out`, `stop`, `stack_get`, `context_get`,
  `property_get`, `eval` (read-only subset), `detach`;
- same agent and metadata; do not build a second runtime for PhpStorm.
- no `xdebug_*` builtins are required to use it; it is a transport adapter only.

## D7. Values and evaluation

Inspection is **borrowed and read-only**:

- no incref/decref, no copy-on-write, no release of temporaries while stopped;
- either freeze the relevant ownership operations or take a no-write borrow;
- document that `variables` cannot mutate the program.

Value formatting:

- reuse the runtime formatting routines where possible instead of reimplementing
  `var_dump`;
- page large arrays/objects, bound strings, use a visited set for cycles;
- preserve the `Mixed` tag/payload contract;
- report uninitialized locals as `undefined`, not zero.

`evaluate` v1 is a read-only subset: locals, property reads, array indices,
`count`, simple arithmetic. Arbitrary PHP evaluation is a separate opt-in
through Magician (`--allow-eval`), debug builds only, and is not part of the v1
MVP.

## D8. Target matrix

| Target | Interactive debugger | Call values in `monitor` | Coverage | Notes |
|---|---|---|---|---|
| linux-x86_64 | yes | yes | yes | primary CI |
| linux-aarch64 | yes | yes | yes | primary CI |
| macos-aarch64 | yes | yes | yes | no ptrace needed for launch; dSYM for native crashes |
| ios-sim-arm64 | simulator only, explicit flag | yes | yes | validate sandbox/socket; otherwise compile-time only |
| ios-arm64 | no | yes | yes | refuse interactive mode with a clear diagnostic |

The runtime agent and site hooks must compile and emit on all five targets; the
interactive server is gated where it cannot work. Hosted libraries
(`--emit cdylib/staticlib`) are out of scope for interactive v1: the host owns
the process and the entry point.

## D9. Security

- `--debug` is visibly a debug build; warn at link time and document the risk.
- Launch uses an inherited socketpair; no listener.
- Attach binds loopback only. Remote attach requires the build key (reuse the
  `elephc-probe` HMAC handshake) plus an explicit `--allow-remote-debug`; no
  environment variable turns a debug server on.
- No arbitrary `evaluate` by default; opt-in Magician evaluation, debug builds
  only.
- Redact values in logs/traces by default; full capture is explicit.
- The agent sends source positions, not file contents; the client reads source.

## D10. Elephc-native debug builtins

Do not clone xdebug's Zend extension ABI and do not depend on xdebug's
functions. elephc exposes its own debug surface, marked `extension: true` in
the shared builtin contract so `--strict-php` hides it:

- `elephc_break(): void` — stop when a debugger is attached; a no-op otherwise.
  Safe to leave in source that must also run in a normal build.
- `elephc_dump(mixed $value, ?string $label = null, string $to = 'auto'): void`
  — formatted `var_dump`-style snapshot. `$to` is one of `stderr`, `terminal`,
  `html`, or `auto`:
  - `stderr`: always write a stable, prefixed snapshot to file descriptor 2;
  - `terminal`: write to the active debugger terminal/console; without a
    debugger it falls back to `stderr`;
  - `html`: add a Dump card to the active `monitor --calls --html` capture;
    without an HTML capture it falls back to `stderr` and says so;
  - `auto` (default): `terminal` when a debugger is attached, otherwise
    `html` when a monitor HTML capture is active, otherwise `stderr`.
  An invalid `$to` raises `ValueError`. Formatting reuses the runtime value
  formatter with depth/string/array limits.
- `elephc_trace_start(string $name): void` — open a named trace segment when a
  call capture (`monitor --calls` or an active debug session) is on; a no-op
  otherwise. It does **not** start a capture by itself. A standalone trace
  trigger is explicitly out of scope: a compiled program needs a destination,
  a build capability and a viewer, and `monitor --calls` already owns all three.
- `elephc_trace_end(string $name): void` — close the named segment. Segments
  nest; `end` must match the top of the stack, otherwise `E_WARNING` and no-op.
  An empty name raises `ValueError`; unclosed segments close at exit and are
  reported as incomplete.

No `xdebug_break`, `xdebug_is_debugger_active`, `xdebug_start_trace`,
`xdebug_start_code_coverage`, or `xdebug_info`. The same source can use the
Elephc builtins everywhere; if the program must run under PHP, guard them with
a project-level abstraction.

**Confirmed 2026-11-05:** `elephc_trace_start/end` are named segments inside an
active capture, not standalone trace triggers.

## D11. Call-value capture in `monitor`

There is no separate `trace` command. `elephc monitor` is the observation
surface: it already owns the exact call graph, timeline, source view, HTML and
the `--trace` Chrome export. Call values extend that capture:

- `elephc monitor app.php --calls=scalars --html report.html` compiles the
  source with `--debug --with-monitoring`, captures exact performance plus
  call arguments/returns/exceptions, and renders both in the same HTML report.
- `--calls` levels: `none`, `types`, `scalars` (default when `--calls` is
  passed), `full`; bounded and redacted. An explicit `elephc_dump()` is always
  recorded when a capture is active, independent of the automatic argument
  level; `none` suppresses automatic args/returns, not explicit dumps.
- The HTML gains a Calls panel/timeline: call tree, per-call arguments, return
  value, exception, source line and allocation/free delta, linked to the
  existing performance views. Named `elephc_trace_start/end` segments appear in
  a Traces panel and tag the calls and dumps inside them.
- `--trace <file.json>` remains the Chrome/Perfetto timeline. `--calls-out
  <file.jsonl>` writes the value events for scripts; no new subcommand.
- A binary target must carry both capabilities; a source target is built with
  both. `--calls` is refused on a values-free production binary rather than
  silently returning a profile without values.

Postmortem:

- normal uncaught-exception/fatal flush is the supported v1 path;
- native segfault capture is best-effort only, with a preallocated,
  async-signal-safe buffer; never allocate/format JSON in a signal handler.

Coverage:

- `--coverage` emits line counters and, where proven, branch-edge counters;
- EIR conditional terminators are block-level, so attribute a branch only when
  the source construct is proven; otherwise report line/function coverage;
- reuse the `--counters`/BSS mechanism rather than a third counter family;
- `elephc test --coverage` aggregates and writes the reports; there is no
  separate `coverage report` subcommand.
- report selectors: `--coverage-format lcov|cobertura|html` and
  `--coverage-out <file>` on the existing `test` command.

## D12. CLI and developer surface

Keep the command surface small and scriptable. Extend existing commands
instead of adding one subcommand per artifact:

```text
elephc debug <source|binary> [--dap] [--attach PID] [--postmortem FILE] [--last FILE]
elephc monitor <source|binary|address> [--calls[=LEVEL]] [--calls-out FILE] [--html FILE]
elephc monitor <source|binary> [--trace FILE] [--out FILE]
elephc test [PATH] --coverage [--coverage-format lcov|cobertura|html] [--coverage-out FILE]
```

- `elephc debug <source.php>` compiles with `--debug` and launches.
- `elephc debug <binary>` requires a debug build. `--dap` serves stdio DAP;
  `--attach` uses the build key.
- `elephc monitor` remains the single observation command: performance by
  default, call values with `--calls`, HTML with `--html`, timeline with
  `--trace`. There is no `trace` subcommand.
- `elephc test --coverage` owns coverage; there is no `coverage` subcommand.
- `src/cli.rs` is currently a flat parser; adding the `debug` selector and the
  monitor/test flags is part of this decision and needs help-text and
  `docs/compiling/cli-reference.md` updates.

## Consequences

Positive:

- PHP-level debugging with real values, fibers and web workers;
- one metadata/agent model for CLI, DAP, monitor call capture and coverage;
- zero runtime in non-debug builds;
- explicit target gating instead of silent degradation;
- the included-file fix improves lldb/gdb immediately, even if the debugger
  stops at the spike.

Negative:

- a new staticlib and a new bridge entry with a reviewed `MonitoringPolicy`;
- debug builds are larger and slower by construction;
- the AST optimizer needs a debug contract, which is real work;
- the shared activation-stack extraction may touch `elephc-instr`;
- DAP is the v1 adapter; DBGp is optional later;
- no interactive debugger on iOS device.

## Alternatives considered

- **lldb/DWARF-only**: cheap, good for native crashes, but no PHP values,
  fibers/web semantics, or PHP expression evaluation. Keep as a companion.
- **Trap patching (`brk`/`int3`)**: zero dormant cost, but `mprotect`,
  instruction-cache flushes, code-signing/hardened runtime and iOS make it a
  poor v1.
- **VM/bytecode debugger**: duplicates eval/Magician and changes the native
  execution model; rejected.
- **Agent speaks DAP directly**: fewer moving parts, but couples the runtime to
  DAP and makes an optional DBGp adapter duplicate logic. A smaller internal
  protocol is the better boundary.
- **Separate `trace` command**: rejects the user-facing surface for no reason;
  `monitor` already owns the call graph, timeline, HTML and exports. Call
  values are a monitor capture, not a fourth command.
- **xdebug API compatibility**: rejected; it would add a Zend-extension-shaped
  surface for no native benefit. Use the Elephc-native builtins instead.
- **Sidecar-only metadata**: simpler, but a single binary would not be
  self-contained and remote/attach would need an extra file. Embedded plus
  sidecar projection is chosen.

## Validation plan and evidence

The spike (`elephc-1vy3`) has produced:

1. A per-line guard and full caller-saved wrapper that pass on Linux AArch64
   (native) and Linux x86_64 (executed under qemu; ABI/guard correctness, not
   native timing).
2. A measured AArch64 dormant cost of ~0.3–0.5 ns/iteration and active cost of
   ~5.6–6.3 ns/iteration on the fixture; text/BSS growth remains a codegen gate.
3. A scalar/string/array local read through `fp-offset` on both probes.
4. Source-id survival through the codegen markers and source-map v3, with the
   included-file fix (`elephc-jwio.1`) closed.
5. A macOS arm64 probe that compiles to a Mach-O object; execution remains a
   macOS CI gate.
6. Shared activation-stack extraction remains an implementation gate.

D2, D4 and D6 are accepted on this evidence. The remaining gates are tracked on
the corresponding implementation issues; they do not reopen the architecture.

## Ratification checklist

- [x] Spike evidence attached to `elephc-1vy3` (AArch64 native, x86_64 qemu,
  macOS compile-only; macOS execution remains a gate).
- [x] D2 AST debug contract confirmed.
- [x] D4 shared activation stack confirmed; extraction remains a gate.
- [x] D6 DAP-first confirmed; DBGp remains optional/deferred.
- [x] `elephc-jwio.1` merged/closed.
- [ ] Metadata schema v1 reviewed (implementation gate).
- [x] Target matrix and iOS refusal text reviewed.
- [x] Security model reviewed (launch/attach/evaluate).
- [x] Follow-up issues updated to match this ADR.
