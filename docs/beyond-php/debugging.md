---
title: "Debugging PHP"
description: "Use elephc's native debugger without xdebug: terminal breakpoints, DAP in VS Code and Neovim, read-only value inspection, call-value capture, named traces and coverage."
sidebar:
  order: 8
---

elephc compiles PHP to native code, so its debugger is a native runtime agent
rather than an interpreter extension. A `--debug` build embeds a compact metadata
table, per-source-line guard sites and the `elephc-debug` runtime. Nothing is
loaded by a normal build, and no xdebug API is required.

## Quick start

Build the runnable example with the debug preset:

```bash
cargo run -- --debug examples/debugger/main.php
```

Start the terminal client and stop on the line that calls `greet()`:

```bash
cargo run -- debug examples/debugger/main.php \
  --break examples/debugger/main.php:8
```

```text
Compiled 'examples/debugger/main.php' -> 'examples/debugger/main'
Stopped at .../examples/debugger/main.php:8:1 (main)
(elephc) p $label
label string(5) "hello"
(elephc) next
Stopped at .../examples/debugger/main.php:9:1 (main)
(elephc) p $result
result string(15) "hellohellohello"
(elephc) continue
hellohellohello
```

Without `--break`, the terminal client stops at the first executed site. The
source argument may be a `.php` file (compiled with `--debug` automatically) or a
debug binary plus its `<stem>.edbg.json` sidecar.

## What `--debug` changes

`--debug` is an executable-build preset:

- implies `--debug-info` and keeps the symbol table;
- disables EIR optimization and skips AST passes that erase source-mapped
  statements, so breakpoints keep a stable line;
- writes `<stem>.edbg.json` and embeds the same metadata table in the binary;
- emits one debug guard per source line per executing block, plus per-site flags;
- refuses library builds because a library has no process entry point to launch
  or attach to.

Normal builds carry none of this. `elephc_break()` is the one source helper that
is also present in a normal build, as a no-op stub.

## Breakpoints, stepping and frames

The terminal client supports:

| Command | Effect |
|---|---|
| `continue`, `c` | resume until the next breakpoint, pause or step stop |
| `next`, `n` | step over calls in the current frame |
| `step`, `s` | step into the next call |
| `finish`, `f` | run until the current frame returns |
| `bt`, `stack` | print the PHP activation stack |
| `frame N` | select frame `N` for `locals` and `p` (`0` is innermost) |
| `locals` | print every named local in the selected frame |
| `p <name>` | print one local (`$` optional) |
| `quit`, `q` | terminate the debuggee |

DAP adds line, conditional, hit-count, function and local/object write data
breakpoints, caller-frame selection, caught/uncaught/throw exception filters
that can be narrowed to an exception class, `exceptionInfo`, and stepping
over/into/out. Launch/attach `pathMappings` translate client paths to recorded
source paths in both directions, and `source`/`loadedSources` return the
compiler-recorded files for clients that cannot read the server filesystem. A
source hash is checked before a breakpoint is armed; a stale `.edbg.json` is
rejected rather than silently binding to different code.

DAP `evaluate` is read-only and supports locals, object properties, array/hash
entries, `count()`/`strlen()`/`gettype()`, scalar predicates, arithmetic and
comparisons. It refuses assignments, user/method calls, magic accessors and
`ArrayAccess`; `setVariable` is not advertised. Program stdout/stderr appear as
DAP `output` events in launch and attach sessions.

A stop is attributed to the caller's source line. The elephc-native source
helpers are transparent frames, so `bt` and DAP `stackTrace` do not show
`elephc_break()` or the dump/trace wrappers.

## Values

Inspection is borrowed: the debugger never increfs, decrefs, triggers copy-on-write,
releases a temporary or runs user code. It renders:

- scalars and escaped strings;
- indexed arrays, associative hashes and reference cells;
- objects with declared and dynamic properties;
- backed property-hook getters from their raw storage, with a
  `[backing value; get hook not executed]` caveat;
- virtual properties as markers, never by calling the getter;
- a safe `__debugInfo()` projection when the body is a single `return [...]` of
  property reads, scalar literals or literal-only nested arrays.

An `__debugInfo()` body that computes values, calls code or has multiple
statements is **not executed**. It displays `object(C) (N) [__debugInfo() runtime]`
with the declared properties. The same rule applies to explicit dumps: a bounded
snapshot is shown, never an interpreted method call.

Data breakpoints watch frame locals, declared object properties and dynamic
property hash entries. A virtual property has no backing slot and is refused as a
watch target.

## Source helpers

| Helper | Availability | Purpose |
|---|---|---|
| `elephc_break(): void` | every build | stop at the caller's line in an interactive/DAP session; no-op otherwise |
| `elephc_dump(mixed $value, ?string $label = null, string $to = 'auto'): void` | `--debug` | bounded formatted snapshot; `stderr`, `terminal`, `html`, or `auto` |
| `elephc_trace_start(string $name): void` | `--debug` | open a named segment in a `monitor --calls` capture or an interactive/DAP session |
| `elephc_trace_end(string $name): void` | `--debug` | close the matching segment; mismatches warn and do not pop |

`--strict-php` hides all four names. Invalid `elephc_dump()` destinations and
empty trace-segment names raise `ValueError` before the native recorder runs.
Trace segments nest; mismatched ends warn without popping, and unclosed segments
are closed as incomplete at capture/exit. In an interactive or DAP session the
segment start/end and scoped dumps appear on the debug console; in a
`monitor --calls` capture they become JSONL/HTML events.

## Editors

### VS Code

VS Code needs a debugger extension contribution. `tests/editor/vscode-extension/`
is a minimal working contribution whose `adapter.js` proxies stdio to
`elephc debug --dap`; a real launch configuration is:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "elephc",
      "request": "launch",
      "name": "Debug PHP with elephc",
      "program": "${workspaceFolder}/main.php",
      "stopOnEntry": false
    }
  ]
}
```

`tests/editor/vscode-suite/` starts a real headless VS Code session on its own
fixture, stops at line 6, checks `$x = int(0)`, steps to `$x = int(5)`, continues,
and writes `vscode-smoke.json`. Run it with:

```bash
VSCODE_TEST_ELECTRON_DIR=/path/to/node_modules \
./scripts/smoke-vscode.sh
```

The first run downloads the VS Code test build; Linux headless needs `xvfb-run`.

### Neovim

```lua
local dap = require("dap")
dap.adapters.elephc = function(callback, config)
  callback({
    type = "executable",
    command = "elephc",
    args = { "debug", "--dap", config.program },
    options = { timeout = 30000 }, -- a .php source is compiled first
  })
end
local app = vim.fn.getcwd() .. "/app.php"
dap.configurations.php = {
  { type = "elephc", request = "launch", name = "Launch with elephc",
    program = app, stopOnEntry = false },
}
```

`scripts/smoke-nvim-dap.sh` runs the same real session under headless Neovim and
checks the breakpoint, stack frame, local, one `next` step and normal exit.

## Loopback attach with a key

An already-running debug build can wait for one loopback DAP client without a
Unix socket path:

```bash
# debuggee
ELEPHC_DEBUG_ATTACH_ADDR=127.0.0.1:0 \
ELEPHC_DEBUG_ATTACH_KEY=/tmp/elephc-key \
  ./app

# client (reads /tmp/elephc-key.addr for the selected port)
elephc debug --attach-tcp 127.0.0.1:41237 --key /tmp/elephc-key
```

The runtime writes the key, actual address and pid as mode-0600 files. The
handshake is a mutual HMAC-SHA256 challenge, so a wrong key is rejected without
stopping the listener and the key never crosses the wire. Only loopback peers
are accepted. The existing Unix `--attach <socket>` path remains available.
`--attach-tcp` is an attach client: it does not compile or launch the debuggee.

## Call capture and traces

`elephc monitor` owns call values; there is no separate `trace` command:

```bash
elephc monitor app.php --calls=scalars \
  --calls-out app.calls.jsonl --html report.html
```

Modes are `none`, `types`, `scalars` and `full` (`full` is the default; `all` and
`1` are aliases). `full` adds bounded nested array/object children. The capture is
bounded at 256 string bytes, 16 children and three nested levels; long values
carry explicit omission markers, and `#[\SensitiveParameter]` arguments are
redacted. JSONL appends `truncated`/`overflow` fields and a
`capture_overflow` trailer when the event ring drops events.

Each call event also carries the runtime context the Calls panel renders under
the call line:

- `caller`: the caller's source file and line (`file`, `line`);
- `timestamp_us` and `end_timestamp_us`: microseconds since the debug session
  started;
- `fiber`: `0` for the main execution, otherwise the running coroutine id;
- `allocs`/`frees` at entry and `alloc_delta`/`free_delta` at return;
- `exception`: class, message, code and `caught` when the call throws.

These fields are present in every mode, including `--calls=none`; that mode only
omits argument/return values. A function that throws keeps its open event with
the exception state and no return/delta. Postmortem captures reuse the same
fields.

`elephc_dump()` snapshots and `elephc_trace_start/end` segments share the same
event stream. The call HTML page renders a nested call tree with arguments and
returns, plus Dumps and Trace segments panels. `--exact` combines the exact
per-function profile with call capture in one run; that timing is a debug-build
timing and includes capture overhead.

Every calls-mode run carries the exact `--with-monitoring` capture: it prints
the exact per-function profile first, then the call values, and `--trace FILE`
still writes the Chrome/Perfetto timeline. `--exact` remains accepted but is
implied. `--html FILE` writes one unified page with **Profile** and **Calls**
tabs; the Dumps and Trace segment tabs scroll the call view. The page embeds the
existing interactive exact call graph/timeline/source panel and the call-tree
panel, so both views remain interactive and self-contained. Run `monitor
--exact` without `--calls` for the profile-only page.

## Postmortem capture

A debug run can leave a bounded document behind when an uncaught exception ends
the program:

```bash
elephc debug app.php --postmortem crash.json
elephc debug --last crash.json
```

`--postmortem FILE` arms the ring at launch (terminal or `--dap`). The file is
written by the uncaught-exception hook with:

- the exception class, message, code and throw site;
- the PHP activation stack, innermost frame first;
- the recent call events, depth-indented in the inspector;
- the captured count and the number of events dropped by the ring budget.

The call ring defaults to 10,000 events. `ELEPHC_DEBUG_CALLS_MAX` overrides it for
bounded diagnostics; a smaller ring reports its truncation explicitly. Call
values follow the same modes and bounds as `monitor --calls`; when postmortem is
the only capture enabled, events keep function/depth and omit values.

A normal exit writes nothing. A caught exception writes nothing. `--last FILE`
is a read-only inspector: it prints the exception, stack, throw site and recent
calls without launching the program.

Native crashes and hard fatal exits that bypass the throw hook are **not**
promised to flush a postmortem. The supported path allocates and encodes JSON in
normal synchronized code, not in a signal handler; a segfault receives no unsafe
allocator work and may leave no file. Treat `--postmortem` as an uncaught-PHP-
exception capture, not a crash dump.

## Coverage

Coverage belongs to the test command:

```bash
elephc test --coverage --coverage-format lcov --coverage-out coverage.lcov
elephc test --coverage --coverage-format cobertura --coverage-out coverage.xml
elephc test --coverage --coverage-format html --coverage-out coverage.html
```

The suite runs in one process so site counters are not lost across forks.
Reports include line coverage, every source-level `&&`/`||`/`!`/switch decision,
and full reachable short-circuit path combinations for logical conditions and
switch dispatch. LCOV emits `DA`/`BRDA`/`BRF`/`BRH` plus the elephc `PATH`/`PTF`/
`PTH` records; Cobertura adds `path-rate`/`path-coverage` and
`<condition type="path">`. A condition with more than 31 leaves is omitted from
path groups (branch coverage still applies).

## Exceptions, fibers and web

DAP exception filters can stop on caught, uncaught or thrown exceptions. A fiber
or generator is exposed as a separate thread-like stack: suspend parks its
activation slice and resume restores it above the consumer, so a resumed fiber
can be stepped with its own frames.

## Web prefork debugging

`elephc debug --web` launches a `--debug --web` prefork server and attaches DAP
to the first request that matches the broker filters. Terminal/DAP/attach and
disconnect/resume are qualified on macOS; the two-worker route election and the
request/pool handler-child stepping are Linux-qualified in v1.

```bash
elephc debug app.php --web --listen 127.0.0.1:8080 \
  --route '/api/orders*' --method POST --header 'X-Debug: 1' \
  --workers 2

# Isolated handler models run PHP in a child; give the broker two children so
# another request can complete while the selected one is stopped.
elephc debug app.php --web --listen 127.0.0.1:8080 \
  --web-isolation=request --handler-concurrency 2
```

- `--route` is `*` (default), an exact path, or a trailing-`*` prefix.
- `--method` is compared case-insensitively.
- `--header 'Name: value'` matches one header value with a case-insensitive name.
- The first matching request wins. Its worker creates a mode-0600 Unix socket in
  a broker-owned 0700 directory and advertises the route; the broker connects to
  that worker only. Other workers and the prefork master keep serving.
- The selected request stops at its first source site (`stopOnEntry`), so the
  client can set breakpoints, inspect locals, and step. A fiber inside the
  request appears as its own DAP thread with the consumer frames below it.
- `disconnect` resumes the request. The broker detaches and leaves the server
  running; kill the PID in the broker's `server.pid` (printed at launch) to stop
  it.
- All three compile-time isolation models are wired:
  - `worker`: the hook runs in the worker that accepted the request;
  - `pool`/`request`: PHP runs in a handler child, so the hook runs there. The
    worker and broker stay asynchronous; the selected handler child blocks only
    while the DAP broker connects. `--handler-concurrency 2` (or more) lets
    another request complete while the selected child is stopped.
  `--web-isolation=worker|pool|request` selects the model when compiling a
  source; a prebuilt binary carries its own model and ignores the flag.

The server's stdout is redirected to stderr so the DAP stream on the broker's
stdio stays clean. With no matching request, the broker waits 120 seconds, then
stops the server and exits non-zero.

The prefork tests cover worker, pool and request isolation: the selected route
stops and can step a suspended/resumed fiber, while `/other` completes in the
same or another worker (isolated modes use a second handler child) and the
master still answers after `disconnect`.

## Target matrix

| Target | Executable | `--debug` | DAP / terminal |
|---|---|---|---|
| `macos-aarch64` | yes | yes | yes |
| `linux-aarch64` | yes | yes | yes |
| `linux-x86_64` | yes | yes | yes |
| `ios-arm64` | library only | compile-only metadata (`--emit-asm`, `staticlib`, `cdylib`); interactive executable refused | refused: no attachable process |
| `ios-sim-arm64` | library only | compile-only metadata (`--emit-asm`, `staticlib`, `cdylib`); interactive executable refused | refused: no attachable process |

An iOS compile-only build still carries the debug metadata and per-site guards:

```bash
elephc --debug --emit-asm --target ios-arm64 app.php
# writes app.s and app.edbg.json
```

The sidecar lists sources, functions, sites and class layouts, so a host build
can symbolicate and inspect the same program. Starting an interactive session
(`elephc debug`, `--dap`) or emitting an iOS executable is refused with an
actionable message pointing at `--emit-asm`, `elephc monitor --calls` and
`elephc test --coverage`; there is no attachable process on the device or
Simulator. Simulator debugging is therefore gated with the device target in v1.
The capability matrix and target conventions are also listed in
[Targets and cross-compilation](../compiling/targets.md).

macOS arm64 execution is enforced by the `debug-tests-macos-aarch64` CI job: it
runs the AArch64 guard/local spike, terminal and DAP sessions, the loopback TCP
attach key path, postmortem, web route debugging (worker/pool/request), the
runtime unit suite and the debug CLI parser. It runs in the merge queue and on a
Pull Request carrying the `ci:macos` label. A macOS checkout can reproduce the
same focused suite with `scripts/ci/test-debug-macos.sh`.

## xdebug and DBGp

The debugger does **not** implement the xdebug extension API, Zend extension ABI
or a DBGp transport in v1. The supported editor surface is DAP; the supported
source helpers are `elephc_break()`, `elephc_dump()` and
`elephc_trace_start/end()`. A DBGp transport for PhpStorm is a later optional
adapter, not a compatibility layer.

## Troubleshooting

- **`no site matched ...`** — the line has no executable PHP statement, or the
  binary is not a `--debug` build. Check the source line and rebuild.
- **stale-source rejection** — the `.edbg.json` hash no longer matches the
  source on disk. Rebuild with `--debug` instead of editing the sidecar.
- **`Undefined function: elephc_dump`** — dump/trace helpers require `--debug`.
  `elephc_break` exists as a no-op in every build.
- **helpers hidden under `--strict-php`** — extension helpers are absent by
  design; compile without `--strict-php` or remove the helper call.
- **DAP adapter does not start** — `elephc debug --dap` is an executable adapter;
  pass the source or debug binary in the adapter command, and use a 30-second
  startup timeout for `.php` sources that compile first.

## See also

- [Debug metadata](../compiling/debug-metadata.md) — `.edbg.json` v1 schema.
- [DAP adapter](../compiling/dap.md) — launch, attach, requests and limits.
- [CLI reference](../compiling/cli-reference.md) — `--debug`, `debug`, `monitor`, `test`.
- [Profiling](profiling.md) — `elephc monitor` and the performance call graph.
- [Testing](testing.md) — `elephc test`, assertions and coverage behavior.
- [The debugger example](../../examples/debugger/main.php) — the runnable fixture used above.
