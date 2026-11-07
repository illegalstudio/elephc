---
title: "DAP adapter"
description: "`elephc debug --dap`: launch and attach over the Debug Adapter Protocol, with breakpoints, stackTrace, scopes, variables, evaluate, stepping, exceptions and fiber stacks."
sidebar:
  order: 9
---

`elephc debug --dap <source-or-binary>` serves the [Debug Adapter Protocol][dap]
over stdio for VS Code, Neovim and any other DAP client. The adapter process is a
thin front end: the debuggee itself carries the runtime agent and the activation
stack, and the adapter reads the same `.edbg.json`/embedded metadata as the
terminal client.

The end-to-end guide is [Debugging PHP](../beyond-php/debugging.md).

## Invocation

```text
executable: elephc
arguments:  debug --dap /absolute/path/to/app.php
transport:  stdio
```

A `.php` source is compiled with `--debug` first. A binary target must already
carry the debug bridge and its `<stem>.edbg.json` sidecar; an ordinary production
binary is refused rather than debugged without metadata.

`stopOnEntry` controls whether the launch stops before the first user site. The
adapter holds the entry stop until `configurationDone`, matching the normal DAP
launch sequence. Program stdout/stderr are forwarded as DAP `output` events so
the protocol channel stays clean. An uncaught-exception report is written after
the DAP channel closes and remains on the adapter stderr.

## Requests

| Request | Behavior |
|---|---|
| `initialize` | Reports supported capabilities, including configurationDone, function/data breakpoints, exception filters, stepping and frame selection. |
| `launch` | Starts the selected source or debug binary. The `program` field is informational when the adapter command already names the target. |
| `attach` | Connects to a runtime waiting on `ELEPHC_DEBUG_DAP_LISTEN` (one-shot mode-0600 Unix socket plus `<socket>.pid`), or to a loopback TCP runtime with `--attach-tcp HOST:PORT --key FILE`. |
| `setBreakpoints` | Arms every site on the requested line. Conditions are validated before arming; a stale source hash rejects the breakpoint with a message. |
| `setFunctionBreakpoints` | Stops when a named PHP function is entered. |
| `setExceptionBreakpoints` | Selects caught, uncaught and/or throw filters. |
| `dataBreakpointInfo` / `setDataBreakpoints` | Watches a frame local, a declared object property, or a dynamic-property hash entry. |
| `configurationDone` | Releases the entry stop and lets the program run to the first breakpoint. |
| `threads` | Lists the main execution plus live fibers/generators as thread-like stacks. |
| `stackTrace` | Returns PHP frames for a thread. A frame carries its function, source path and current source line. |
| `scopes` / `variables` | Enumerates the Locals scope and read-only value children. |
| `evaluate` | Evaluates a read-only expression in the selected frame: locals, object properties, array/hash entries, `count`/`strlen`/`gettype`, scalar predicates, arithmetic and comparisons. Assignments and user/method calls are refused. |
| `continue` / `next` / `stepIn` / `stepOut` | Resume, step over, step into or step out of the current frame. |
| `pause` | Requests a stop at the next executed site; a pause signal while already stopped does not create a second stop. |
| `disconnect` | Resumes and detaches the debuggee. |

## Breakpoints

Line breakpoints are keyed by source identity and line. Conditional breakpoints
use the same restricted expression language as the terminal client; an invalid
condition is reported without arming the site. Function breakpoints resolve a PHP
function name to its entry hook. Data breakpoints snapshot the watched value at
the stop and stop again when it changes.

A source path may be absolute in the client while the metadata records a relative
path; the adapter resolves the canonical source path when possible and otherwise
matches by suffix. A `.edbg.json` whose SHA-256 no longer matches the source on
disk is rejected as stale rather than binding to different code.

A virtual property (hooks with no backing slot) is refused as a data-breakpoint
target: reading it would execute its getter. A backed hooked property keeps its
raw slot and remains watchable.

## Pause and attach

In launch mode, `SIGUSR1` requests a stop at the next executed site and the
adapter reports `reason: "pause"`. To attach to an already-running debug build,
start it with `ELEPHC_DEBUG_DAP_LISTEN=/path/app.sock`; the runtime creates a
one-shot socket with mode `0600` and writes `/path/app.sock.pid`. Run
`elephc debug --attach /path/app.sock` (or point a DAP client at the socket) and
the runtime accepts one client. A detached client resumes the program.

### Loopback TCP attach with a key

A debug binary can also accept one loopback TCP client authenticated by a fresh
per-run key:

```bash
# Terminal 1: the debuggee generates the key and waits.
ELEPHC_DEBUG_ATTACH_ADDR=127.0.0.1:0 \
ELEPHC_DEBUG_ATTACH_KEY=/run/elephc/attach.key \
  ./app

# The runtime writes /run/elephc/attach.key (0600),
# /run/elephc/attach.key.addr (the actual 127.0.0.1:PORT) and
# /run/elephc/attach.key.pid.

# Terminal 2: attach the DAP client through the broker.
elephc debug --attach-tcp 127.0.0.1:41237 --key /run/elephc/attach.key
```

The runtime binds a loopback address only and rejects non-loopback peers. The
handshake is a mutual HMAC-SHA256 challenge: the server proves the key first,
then the client proves it; the key itself never crosses the wire. A wrong key
gets one status byte and the listener keeps waiting, so an operator can retry
with the correct key. After a successful handshake the key, address and pid
files are removed. `--attach-tcp` does not launch or compile a program; the
debuggee must already be waiting. The Unix `--attach <socket>` path is
unchanged.

## Web prefork sessions

`elephc debug app.php --web --listen 127.0.0.1:8080 --route /api/orders` starts a
`--debug --web` prefork server and waits for the first matching request. The
selected worker advertises a mode-0600 Unix socket in a broker-owned 0700
directory; the broker connects to that worker only, so the prefork master and
every other worker keep serving. `--method` and `--header 'Name: value'` narrow
the election further; `--route` accepts `*`, an exact path, or a trailing-`*`
prefix.

The request stops at its first source site. Breakpoints, stack frames, locals,
fiber threads and stepping behave exactly as in a normal DAP launch. A fiber
suspended inside the request has its own stack and can be stepped after resume.
`disconnect` resumes the request and leaves the server running; the broker
prints the server PID and writes `server.pid` in its session directory. Worker,
pool and request isolation are all wired: in pool/request mode the route hook
runs in the handler child where PHP executes, while the worker/broker stay
async. Use `--handler-concurrency 2` or more to serve another request while the
selected child is stopped.

## Values

Values are read from the stopped program without incref/decref, copy-on-write or
user-code execution. The adapter renders:

- integers, floats, booleans and null;
- strings as `string(5) "hello"`;
- indexed arrays as `array(3)` with `[0]`, `[1]`, … children;
- associative arrays as `array(2)` with `["key"]` children;
- reference cells with a `&` marker, for example `&int(2)`;
- resources, pointers and callable descriptors by their PHP-facing identity;
- objects as `object(Point) (3)` with their class name, property count and a child
  list of declared properties (`x`, `y`, `label (private)`, …), recursing into
  nested values;
- dynamic properties appended after the declared slots for classes with a reserved
  per-object dynamic hash (`#[\AllowDynamicProperties]`, `stdClass`, and the
  compiler's eval/clone storage classes);
- a safe `__debugInfo()` projection in place of the declared names. The compiler
  folds a single `return [...]` of `$this->property` reads, scalar literals and
  literal-only nested arrays; property entries stay live, while literal entries are
  pre-rendered PHP-style text. An inherited method keeps its parent projection. A
  body with computed values, calls or more than one statement is **not executed**
  and keeps the declared-property view described below;
- a backed property with a `get` hook from its raw backing slot, marked
  `int(4) [backing value; get hook not executed]`, because the debugger never runs
  the accessor to compute the PHP-visible value;
- a virtual property (hooks with no backing slot) as a marker instead of a value:
  `virtual [get hook not executed]`, `virtual [get/set hooks not executed]` or
  `virtual [write-only; set hook]`.

Object class/property metadata comes from the `.edbg.json` `classes` table, including
the dynamic-hash offset, the `__debugInfo()` kind and the `property_hooks` map. Internal
or compiler-injected classes with custom payload layouts show their class name only. A
runtime-only `__debugInfo()` (one that computes values, calls code or has multiple
statements) is **not executed** while stopped: doing so would run user code and could
allocate, mutate state or terminate the program. Such objects display
`object(C) (N) [__debugInfo() runtime]` and list their declared/dynamic properties
instead. Projected literal entries are display-only and have no expandable children or
watchable backing slot. Property-hook getters are treated the same way: a backed
getter shows raw storage with a caveat, and a virtual property has no readable value
at all. Large containers are capped and cycles are not followed.

## Fibers and generators

Each live fiber/generator is exposed as a thread-like stack. At suspension the
runtime parks the fiber's activation slice; at resume it restores that slice
above the consumer. A stop inside a resumed coroutine therefore shows the
coroutine's frames on top and its consumer below, rather than presenting the
coroutine as the consumer's caller. A terminated coroutine's frames are dropped
before an escaped exception reaches the caller.

`stackTrace` can be requested per `threadId`; `scopes` and `variables` use the
frame id returned for that thread. Stepping selects the stopped thread's stack,
not an unrelated suspended fiber.

## Exception stops

`setExceptionBreakpoints` accepts caught, uncaught and throw filters. The runtime
publishes the throwable object before unwinding, so the client can inspect its
class, message, previous chain, properties and captured trace. With no filters
selected, the program behaves exactly as it would without a debugger, and the
normal uncaught-exception report is unchanged.

## Client configuration

### VS Code

VS Code needs a debugger extension contribution whose adapter executable runs
`elephc debug --dap`; the launch request's `program` is informational because the
source is already selected on the adapter command line. A launch configuration
looks like this once the `elephc` debugger type is contributed by an extension:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "elephc",
      "request": "launch",
      "name": "Debug PHP with elephc",
      "program": "${workspaceFolder}/app.php",
      "stopOnEntry": false
    }
  ]
}
```

`tests/editor/vscode-extension/` contains a minimal working contribution whose
`adapter.js` proxies stdio to `elephc debug --dap`. A real headless VS Code smoke
test lives in `tests/editor/vscode-suite/`; run it with:

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
    -- A .php source is compiled first; 30s avoids nvim-dap's 5s handshake warning.
    options = { timeout = 30000 },
  })
end
local app = vim.fn.getcwd() .. "/app.php"
dap.configurations.php = {
  {
    type = "elephc",
    request = "launch",
    name = "Launch with elephc",
    program = app,
    stopOnEntry = false,
  },
}
```

`scripts/smoke-nvim-dap.sh` runs the real adapter under headless Neovim and
asserts a breakpoint, a stack frame, a local value, one `next` step and a normal
termination:

```bash
NVIM_BIN=/path/to/nvim \
NVIM_DAP_DIR=/path/to/nvim-dap \
NVIM_NIO_DIR=/path/to/nvim-nio \
./scripts/smoke-nvim-dap.sh
```

Expected evidence:

```text
STOPPED 1 reason=breakpoint desc=Stopped at .../main.php:6:5 (run)
FRAME run line=6
LOCAL $x = int(0)
STEPPING
STOPPED 2 reason=step desc=Stopped at .../main.php:7:5 (run)
FRAME run line=7
LOCAL $x = int(5)
CONTINUING
TERMINATED
SMOKE OK
```

## Limits

- The adapter speaks DAP only. DBGp/PhpStorm and the xdebug API are not part of
  v1.
- Interactive debugging needs an attachable host process. iOS targets are
  library-only and `--debug` refuses them with a message pointing to
  `elephc monitor --calls` / `elephc test --coverage` on a host target.
- Per-request web worker/route selection is available through `elephc debug
  --web`. stdout/stderr `output` events cover launch and attach sessions; web
  route sessions keep server output on the server stderr.
- `evaluate` is deliberately read-only. It never runs user code, getters,
  `ArrayAccess` or assignments; `setVariable` is not advertised.
- `--debug` disables EIR optimization to preserve source-mapped statements. Use a
  normal optimized build for production performance, and a debug build for
  inspection.
- Uncaught PHP exceptions can be captured with `elephc debug ... --postmortem FILE`
  and inspected with `--last FILE`. Native segfaults and hard fatal exits that
  bypass the throw hook are not promised to flush; the supported capture paths
  are the live debugger, `monitor --calls`, postmortem-on-uncaught, and test
  coverage.
- The adapter is single-session. One client per process; a detached client resumes
  the program.

The adapter's DAP handling lives in `crates/elephc-debug/src/dap.rs`; value
formatting lives in `crates/elephc-debug/src/value.rs`. See
[Debug metadata](debug-metadata.md) for the site/local table those modules read.

[dap]: https://microsoft.github.io/debug-adapter-protocol/
