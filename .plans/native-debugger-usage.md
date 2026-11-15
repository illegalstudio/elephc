# Native debugger — developer workflow (proposed UX)

Status: proposal, not shipped. This document is the developer-facing walkthrough
for `.plans/native-debugger.md` and `.plans/adr-native-debugger.md`. Flag names
and exact output are the proposed contract; the ADR ratification may adjust
them.

## Mental model

There are three build families and four common invocations:

| Build / invocation | Question it answers | Runtime cost |
|---|---|---|
| normal `elephc app.php` | "Does it work and how fast is it?" | none |
| `elephc --debug app.php` | "Why is it doing this?" | debug guards only |
| `elephc --with-monitoring app.php` | "Where is the time going in production?" | profiling hooks only |
| `elephc monitor app.php --calls` | "What was called, with what values, and where did time go?" | debug + profiling hooks (development) |

A `--debug` binary run **without** a debugger attached behaves like a normal
binary: same output, no prompt, nothing on stderr. It is not an interpreter and
it does not require `--with-monitoring`. The debugger is a development tool; do
not ship a `--debug` binary to production.

The IDE is a client. It launches `elephc debug --dap app.php`; the compiler
builds a debug binary and talks to the in-process agent. The same agent can be
driven by the terminal client. A DBGp adapter for PhpStorm is optional later,
not part of v1.

## First five minutes: VS Code

1. Install/build `elephc` and the thin VS Code extension.
2. Add a launch configuration:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "elephc",
      "request": "launch",
      "name": "Debug app.php",
      "program": "${workspaceFolder}/app.php",
      "args": [],
      "cwd": "${workspaceFolder}",
      "stopOnEntry": false,
      "sourceRoot": "${workspaceFolder}"
    }
  ]
}
```

3. Click the gutter on `src/Cart.php:42`, press F5.
4. The IDE stops on the line. The Debug sidebar shows PHP frames and values,
   not native frames:

```text
┌ Debug sidebar ─────────────────────────────────────┐
│ VARIABLES                                          │
│ ▾ $items  array(3)                                 │
│   ▾ [0] App\Item                                   │
│       price int(1299)                              │
│       name  string(12) "Mechanical keyboard"       │
│ ▾ $total  int(3897)                                │
│ ▾ $this   App\Cart                                 │
│                                                    │
│ CALL STACK                                         │
│ App\Cart::total()      src/Cart.php:42             │
│ App\Checkout::run()    src/Checkout.php:18         │
│ {main}                 public/index.php:7          │
└────────────────────────────────────────────────────┘
```

The Debug Console is a read-only PHP expression console:

```text
> $items[0]->price
1299
> count($items)
3
> $total + $items[0]->price
5196
```

Stepping uses the normal IDE controls:

- **Continue** (F5) runs to the next breakpoint, exception or pause.
- **Step Over** (F10) stops on the next PHP line in the same or an outer frame.
- **Step Into** (F11) enters the next PHP call.
- **Step Out** (Shift+F11) returns to the caller.
- **Pause** stops at the next PHP line, even in a web worker.
- **Restart** rebuilds and relaunches.

Attach to an already-running debug build:

```json
{
  "type": "elephc",
  "request": "attach",
  "name": "Attach to app",
  "processId": 1234,
  "keyFile": "${workspaceFolder}/app.key"
}
```

## Terminal workflow

If you live in a terminal, the same agent has a small client:

```bash
$ elephc debug app.php
elephc: building app.php (debug)
elephc: compiled -> ./app
(elephc) break src/Cart.php:42
Breakpoint 1 at src/Cart.php:42
(elephc) run
[app] loading cart...
Stopped at src/Cart.php:42 (breakpoint 1)
(elephc) bt
#0 App\Cart::total()      src/Cart.php:42
#1 App\Checkout::run()    src/Checkout.php:18
#2 {main}                 public/index.php:7
(elephc) locals
$items  array(3) [...]
$total  int(0)
$this   App\Cart { ... }
(elephc) next
Stopped at src/Cart.php:43
(elephc) print $items[0]->price
int(1299)
(elephc) continue
[app] total: 3897
(elephc) quit
```

The core commands:

| Command | Meaning |
|---|---|
| `break FILE:LINE` | Set a line breakpoint. |
| `break FILE:LINE if EXPR` | Conditional breakpoint (v2 expression subset). |
| `break FILE:LINE 3` | Stop on the third hit. |
| `info breakpoints` / `delete N` | List or remove breakpoints. |
| `run` / `continue` | Start or resume. |
| `next` / `step` / `finish` | Step over / into / out. |
| `bt` / `frame N` | Show or select a PHP frame. |
| `locals` / `print EXPR` | Inspect values. |
| `exceptions throw,uncaught` | Stop on exceptions. |
| `list` | Show source around the current line. |
| `quit` | Detach and let the program finish (or kill it, per prompt). |

You can also compile a debug binary once and run it outside the client:

```bash
elephc --debug app.php
./app                 # normal execution, no debugger attached
elephc debug ./app    # launch that binary under the debugger
```

## Breakpoints, stepping and exceptions

Line breakpoints work in the entry file **and included/autoloaded files** once
`elephc-jwio.1` is fixed. A breakpoint on a line removed by the optimizer is
reported as **unverified** rather than silently accepted:

```text
Breakpoint 2 at src/Cart.php:57 is unverified: the line was removed by the
optimizer. Rebuild with --debug (AST optimizer relaxed) or move the breakpoint.
```

Exception breakpoints:

```text
(elephc) exceptions throw,uncaught
Exception breakpoints: throw, uncaught
(elephc) continue
Stopped on throw: RuntimeException "empty cart"
  thrown at src/Cart.php:57
  #0 App\Cart::total()    src/Cart.php:57
  #1 App\Checkout::run()  src/Checkout.php:18
```

`caught` is intentionally deferred until its semantics are specified; `throw`
and `uncaught` are the reliable v1 pair.

### Elephc debug helpers: break, dump, trace

There is no xdebug API dependency. elephc exposes four native helpers, declared
as Elephc extensions and hidden by `--strict-php`:

```php
if ($total < 0) {
    elephc_break();
}

elephc_dump($items, 'cart items');          // auto: terminal/html/stderr
elephc_dump($total, 'total', 'stderr');     // stable log line

elephc_trace_start('checkout');

// ... work to isolate in the trace ...

elephc_trace_end('checkout');
```

`elephc_dump($value, ?string $label = null, string $to = 'auto')` is a
formatted `var_dump`-style snapshot. `$to` selects the destination:

| `$to` | Destination |
|---|---|
| `stderr` | always write a stable, prefixed snapshot to fd 2 |
| `terminal` | active debugger terminal/console; falls back to stderr without a debugger |
| `html` | a Dump card in the active `monitor --calls --html` capture; falls back to stderr without an HTML capture |
| `auto` | terminal if attached, else HTML capture, else stderr |

An invalid destination raises `ValueError`. The dump includes the label, the
source file/line and a bounded structure (depth, string, array limits).

`elephc_break()` stops at the call site when a debugger is attached and is a
no-op in a normal build or unattended run. The same source compiles everywhere.

`elephc_trace_start(string $name)` / `elephc_trace_end(string $name)` open and
close a named trace segment. They only take effect while a call capture is
active (`monitor --calls` or a debug session); outside a capture they are
no-ops. They do **not** start a capture by themselves: run
`elephc monitor app.php --calls ...` or `elephc debug app.php` first.

Segments nest, `end` must match the top of the stack (otherwise `E_WARNING`
and no-op), an empty name raises `ValueError`, and an unclosed segment is
closed at exit and reported as incomplete. The segment name appears
in the monitor HTML Traces panel and tags the calls and dumps inside it.

There is intentionally no `xdebug_break`, `xdebug_is_debugger_active`,
`xdebug_start_trace`, `xdebug_start_code_coverage` or `xdebug_info`.

## Inspecting values

The Variables panel / `locals` command shows PHP-level values:

- scalars, strings (bounded), arrays (indexed/hash), objects with class and
  properties, references, `null`/`bool`, resources and pointers;
- `Mixed` cells through the runtime tag/payload contract;
- `undefined` for an uninitialized local, not `0`;
- large arrays/objects are paged; cycles are not walked forever.

Inspection is **read-only**. It does not incref/decref, trigger copy-on-write or
release a temporary while the program is stopped. `evaluate` v1 supports locals,
property reads, array indices, `count` and simple arithmetic. Arbitrary PHP
evaluation is an explicit opt-in later, not the default.

## Web requests

The first documented path is request isolation:

```bash
elephc debug --web --web-isolation=request --route '/checkout/*' app.php
Debug server ready on 127.0.0.1:9003
Waiting for a request matching GET /checkout/*
```

Open the page; only the matching request stops. Other requests continue to be
served. The master process is never stopped.

For the default worker mode, the broker attaches to the next matching worker
and leaves the others running. The ADR must freeze the per-worker endpoint /
registry addressing because `crates/elephc-web` forks before tokio and again per
request.

## Call collection: use `monitor`, not a new command

The profiler already owns the call graph, timeline, source view, HTML and
Chrome export. Call values extend that same capture; there is no separate
`trace` command.

```bash
elephc monitor app.php --calls=scalars --html report.html
```

```text
Monitor: exact profile + call values
Calls: 12,431, 2 exceptions, 0 dropped
Wrote report.html
```

The same self-contained HTML report now carries both views:

```text
[Summary] [Call graph] [Timeline] [Calls] [Traces] [Source] [Queries]

Calls
App\Checkout::run()        src/Checkout.php:18   1.82 ms   in: -
  App\Cart::total()        src/Cart.php:42       1.21 ms   in: items=array(3)
    App\Item::price()      src/Item.php:11       0.04 ms   in: -
Exception: RuntimeException "empty cart" at src/Cart.php:57
Dump [checkout total] src/Cart.php:44: int(3897)
```

Clicking a call opens its arguments, return value, exception, source line and
allocation/free delta; the performance call graph and source view remain
available in the same page. `--calls-out app.calls.jsonl` writes the value
events for scripts. `--trace app.trace.json` remains the Chrome/Perfetto
timeline export.

Named segments come from the source helpers:

```php
$cart = load_cart();
elephc_trace_start('checkout');
$total = $cart->total();
elephc_dump($total, 'checkout total', 'html');
checkout($cart, $total);
elephc_trace_end('checkout');
```

They appear in a `Traces` panel and tag the calls and Dump cards inside them.
`--calls=none` still records the segment boundaries and call tree; it simply
omits argument/return values.

`--calls` levels:

| Level | Captures |
|---|---|
| `none` | function/site/depth/timing only |
| `types` | argument types and sizes |
| `scalars` | scalars and short strings; arrays/objects as types (default when `--calls` is passed) |
| `full` | full values with depth/string/array limits and redaction |

Explicit `elephc_dump()` calls are always recorded when a capture is active,
regardless of the `--calls` level; `none` only suppresses automatic
argument/return values. A capture is bounded by event count and bytes; overflow
is reported. A source
target is built with `--debug --with-monitoring` automatically. A binary target
must already carry both capabilities; a production `--with-monitoring` binary
without value capture is refused rather than silently returning a profile
without values.

## Postmortem

For an intermittent uncaught exception, keep a bounded ring and inspect it
after the crash:

```bash
elephc debug app.php --postmortem app.postmortem.json
```

```text
Fatal error: Uncaught RuntimeException: empty cart in src/Cart.php:57
Postmortem written to app.postmortem.json (last 2,048 calls)
```

```bash
elephc debug --last app.postmortem.json
```

The postmortem shows the PHP stack, the exception fields and the recent calls.
Normal uncaught-exception/fatal flush is the supported v1 path. A native
segfault is best-effort only; the agent must not allocate or format JSON inside
a signal handler.

## Coverage

Native coverage for CI:

```bash
elephc test --coverage
```

```text
Coverage: 84.2% lines (312/371), 71.3% branches
Wrote coverage.lcov, coverage.xml, coverage.html
```

Report selectors live on the existing `test` command:

```bash
elephc test --coverage --coverage-format lcov --coverage-out coverage.lcov
elephc test --coverage --coverage-format cobertura --coverage-out coverage.xml
elephc test --coverage --coverage-format html --coverage-out coverage.html
```

Coverage is line-first. Branch coverage attributes an EIR conditional terminator
only when the source construct is proven; otherwise it reports line/function
coverage instead of inventing a branch. Included files appear under their own
paths, not the entry file.

PHPUnit's xdebug coverage API is out of scope for v1; native `elephc test
--coverage` is the supported path.

## Debug vs monitor vs counters

| Need | Command | Build |
|---|---|---|
| Breakpoints, step, variables | `elephc debug app.php` | `--debug` |
| Performance + call values in HTML | `elephc monitor app.php --calls=scalars --html report.html` | `--debug --with-monitoring` |
| Named trace segment | `elephc_trace_start/end($name)` | `monitor --calls` or debug |
| Dump to terminal/HTML/stderr | `elephc_dump($value, $label, $to)` | any; HTML needs `monitor --calls` |
| Postmortem on fatal | `elephc debug app.php --postmortem ...` | `--debug` |
| Line/branch coverage | `elephc test --coverage` | `--coverage` |
| Exact per-function profile | `elephc monitor app.php` | `--with-monitoring` |
| Production sampled profile | `elephc monitor host:port` | `--with-monitoring` |
| Function call counts | `elephc --counters app.php` | `--counters` |
| Native crash in lldb | `elephc --debug-info app.php` + `lldb ./app` | `--debug-info` |

`--debug` and `--with-monitoring` may be combined for `monitor --calls`, but
they remain independent: the debugger does not replace the profiler and the
profiler does not carry the interactive debugger.

## If you come from xdebug

There is deliberately no xdebug compatibility layer and no Zend extension ABI.
The equivalent gestures are:

| Habit | Elephc |
|---|---|
| `xdebug_break()` in source | `elephc_break()` |
| ad-hoc `var_dump()` for debug | `elephc_dump($value, $label, $to)` |
| `xdebug_start_trace()` / `xdebug_stop_trace()` | `elephc_trace_start/end($name)` inside `elephc monitor --calls` |
| `xdebug_start_code_coverage()` | `elephc test --coverage` |
| Xdebug extension in PhpStorm | DAP first; optional DBGp transport later, not v1 |
| `xdebug_info()` | not provided |

The source stays Elephc-native: no `xdebug_*` function is required to use the
debugger, the call capture or the coverage report.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| "not built with --debug" | Launch the source, or rebuild with `--debug`. |
| "breakpoint unverified" | The line was removed by the optimizer, or the source hash changed. Rebuild and refresh. |
| `elephc_break()` does nothing | No debugger attached, or the binary is not a debug build. |
| `elephc_trace_start/end()` does nothing | No active `monitor --calls` capture or debug session. |
| `elephc_dump(..., 'html')` wrote to stderr | No active `monitor --calls --html` capture; HTML sink falls back to stderr. |
| IDE shows native frames | The client attached to the wrong target, or the debug adapter was not used. |
| Web request does not stop | Route/worker mismatch; start with `--web-isolation=request` and a narrow route. |
| "interactive debugger unavailable" on iOS | Expected: iOS device supports trace/coverage only. |
| "evaluate not supported" | v1 only supports the read-only subset; arbitrary PHP evaluation is opt-in later. |
| Source view looks stale | The sidecar/build id and source hash disagree; recompile. |

## What the first release will not do

- No xdebug API compatibility, no Zend extension loading, no `xdebug_*` builtins.
- No interactive debugger on iOS device.
- No `evaluate` of arbitrary PHP by default.
- No reliable native-segfault postmortem (best-effort only).
- No branch coverage when the source construct cannot be proven.
- No debugger in a production `--with-monitoring` artifact.
- No `trace` or `coverage` subcommands; `monitor` and `test` own those flows.
- No dedicated desktop UI; the surfaces are DAP, CLI and the monitor HTML page.
