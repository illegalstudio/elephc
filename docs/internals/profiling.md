---
title: "Profiling internals"
description: "How elephc's sampler and exact instrumentation measure a program, and why they are built that way."
sidebar:
  order: 21
---

This page explains how `elephc monitor` works and why it is built this way. For usage, read [Profiling](../beyond-php/profiling.md).

## Why profile in the compiler

elephc is the compiler. It knows every function's name, the source line of every instruction and what each runtime helper does. A profile can therefore name PHP functions, not assembly labels. It can also translate runtime-helper time into causes: heap allocation, Mixed cell boxing, reference counting, dynamic Mixed arithmetic. An interpreter-based profiler cannot produce that cause view.

One mechanism serves every target. Profiling in production is usually a different tool answering a smaller question, an approximation you learn to read differently from the exact numbers on a laptop. Here the sampled and exact answers come from one instrumentation and agree with each other. The smaller question is a narrower slice of the same picture.

## Two measurements

| | Exact | Sampled |
|---|---|---|
| Source | enter and exit hooks on a shadow stack | a CPU-time timer filling a ring |
| Used by | launched runs, `--exact`, signed requests | endpoint default, `--live`, `--attach` |
| Counts calls | yes | no |
| Sees blocked time | as recorded wait | no |

The exact numbers are cheap because elephc is also the allocator and owns the PDO and curl bridges. Allocation counts, free counts and recorded waits are read at the same hooks that read the clock.

### Capability matrix

| Capture | CPU | Wall time | Calls | Allocations / retained | DB queries | Network | File I/O | Wait | Routes |
|---|---|---|---|---|---|---|---|---|---|
| local `monitor` | not an OS CPU clock; the UI can show wall minus recorded waits | exact, rooted at `{main}` | exact | exact / exact | exact | exact curl operations and wait | not available | exact DB-driver and network wait | untagged |
| service default | sampled CPU-time ring | unavailable | unavailable | exact deltas between samples, sampled attribution; no retained | unavailable in combined `--with-monitoring` | unavailable in combined `--with-monitoring` | unavailable | unavailable in combined `--with-monitoring` | sampled stacks carry the exact route tag |
| service `--exact` or signed request | wall minus recorded waits is a derived remainder | exact for one request, rooted at `{main}` | exact | exact / exact | exact | exact | not available | exact | exact route and trace |
| `--live` | sampled, asked over the channel | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | route tags for `--web` |
| `--attach` | sampled from outside | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable |
| [`--apm`](../beyond-php/apm.md) | not measured | exact per request, as a latency histogram | requests per route | unavailable | unavailable | unavailable | unavailable | unavailable | every request per route shape |

A probe-only binary can emit exact per-route DB and outgoing-network counters beside its sampled stacks. `--with-monitoring` links both runtimes, and the exact runtime owns those callbacks. The combined service-default row is the one the public flag gives.

## Dormant until asked

`--with-monitoring` embeds the exact hooks, the sampler, the symbol table both read and a 32-byte build key. Nothing runs until a client connects or a signed request arrives.

- A dormant binary pays one flag check per call.
- The PDO bridge once asked whether the profiler was linked, which is always true in a monitored binary. Every statement then paid two clock reads and a copy of its SQL, 519 ns each on 8 000 statements. It now asks whether anyone has requested a profile.
- With the profiler unlinked, the wait slot is zero and the bridge never reads the clock.

### Why a binary without the flag is refused

A degraded answer that looks like the real one is worse than no answer. You would read the numbers as exact and act on them. So `monitor` stops and prints both remedies.

### Why the sampler starts on the first ask

Collection starts when a client first asks. The first answer on a service nobody asked yet can be thin. The alternative was a service that samples itself from boot whether or not anyone ever looks.

Configuring `ELEPHC_PROBE_ADDR` used to arm the sampler as a side effect. A run started with the variable wrote a profile to stderr at exit, even if nobody connected. A service that was only reachable profiled and logged itself. Opening the endpoint and starting collection are now two decisions. To get a profile out of a run without a key, launch it under `elephc monitor <binary>`.

## The cost model

The cost is about 30 ns per profiled call. Every percentage is that figure times the call count, divided by the run time. The 3% on the demo is 135 351 calls × 30 ns = 4.1 ms. A loop that is nearly all calls pays 48%.

The 30 ns is down from about 43:

- The caller to callee map hashed its keys with SipHash, the default, which makes hash flooding impractical when keys come from outside. These keys are function ids the compiler assigned, so nothing at run time can influence them. The defence cost more than what it defended.
- The clock was `clock_gettime`, 23 ns a read for a value that resolves to one microsecond. The counter register behind it costs 0.33 ns and ticks every 41 ns. It is cheaper and 24 times finer. Ticks are converted to nanoseconds once, at render.

### Measurement method

- All variants are built first and then timed round-robin, each keeping its minimum. Timed one after another, the machine drifted far enough that three named functions came out slower than instrumenting all 35, which cannot be true.
- Figures are CPU time, not wall clock. On a busy machine wall clock counts time spent waiting for a core. Measured that way, one column moved from +1% to -3% between two passes.
- A bridge archive is resolved from the directory the compiler lives in. An installed elephc links the release build of `elephc-instr`. A source checkout links the debug build, about 165 ns per call instead of 30. Benchmarking inside a clone therefore gives about seven times the published figures: +3% dormant, +22% profiling, +964 KB.

## Control channel and endpoint

Locally, `elephc monitor ./app` passes the program a socket on fd 3. Possession of that channel is the credential. Nothing can be copied, leaked or replayed, because it exists only while the two processes are connected.

`--live` launches its target, so it can hand it a socketpair and ask. It needs no external tool and works wherever elephc does. It once read its own child from the outside, which was a mistake in the implementation, not a design.

For a running service the credential is the build key.

### Handshake and seal

- The two ends run a mutual HMAC handshake. The server proves it holds the key, so you never profile an instance you do not control. The client proves it holds the key, so a stray connection cannot read production stacks.
- No secret crosses the connection. A party that cannot prove the key is disconnected before any sample is sent.
- The fingerprint is printed after the handshake. It reports what was proven, not what was claimed. The compile-time and connect-time values are the same on purpose.
- Once both sides have proven the key, each derives a pair of subkeys from the key and the two nonces. The payload crosses sealed and authenticated. A relay that forwards both proofs receives ciphertext. A recorded exchange cannot be replayed against a later connection, because the keys are bound to both nonces. This was verified with a relay that copied every byte the server sent and found no fragment of the profile in it.
- It is not TLS on purpose. The trust anchor is the build key both sides already hold, not a certificate authority. A diagnostic socket then needs no certificate to issue, rotate or verify, and a monitored binary needs no TLS stack.
- `https://` is for an endpoint behind a TLS terminator. There the certificate is validated against the system roots first.

### Sampled answer

A timer fills a ring, so a process doing anything has something to hand over. The ring is shared by every `--web` worker, including the pool and request isolation models where PHP runs in a handler child.

### Exact answer under `--web`

The endpoint answers from the parent that accepts and forks. The request runs in a worker. The slice travels through a small shared mapping established before the fork, like the sample ring.

The rendezvous the worker leaves its slice in is a single slot. A second `--exact` while one is in flight is told so, instead of taking the first one's answer away. When nothing comes back, the reply says which of three reasons applies. An empty answer would otherwise look like a service that served no traffic.

## The signed request header

`X-Elephc-Query` is signed with the build key, so it cannot be forged by someone who can set headers. This is the shape Blackfire uses. Turning profiling on costs the request real time and reveals the code, so asking must be something only a key holder can do.

- **Single use.** The service remembers spent signatures in the shared mapping its workers share. A header lifted from a proxy log, an access log or a shared trace is worth nothing. One lifted before the real request lands is worth exactly one request.
- **Why `n=`.** With the timestamp alone, two requests minted in the same second produce the same header. Nothing can tell them from a replay, so the second is refused. The nonce makes them distinct. It is part of the signed message. A nonce that could be varied without re-signing would turn one captured header into as many spends as an attacker wanted.
- **The time window** is five minutes either side of the server's wall clock. The timestamp was minted on another machine, so a monotonic clock would mean nothing. The window absorbs clock skew between hosts, and a backwards correction on the server shortens it. It no longer stands in for a replay defence.
- **The spent table** has 4 096 slots, one per signature, chosen by the signature. Two signatures on one slot cannot both be live. This fails toward refusing, and the table is sized far above what a five-minute window carries. If the shared mapping could not be established, every signed header is refused, because a signature the service cannot account for cannot be promised to be used once.

## Sampler mechanics

The in-process sampler walks the frame-pointer chain. A function that uses the frame register as an ordinary one leaves a value there that looks like a frame. Every in-process sampler has this problem. The walk refuses to report such values. A return address inside the program's own compiled text vouches for its frame and for the native frames held behind it. A genuine PHP to helper to PHP chain is kept whole. A chain that never returns into compiled code is dropped. A sampled stack may end early, and what it shows ran.

Sampled captures are CPU-time only. Their clocks do not tick while the program waits. That is why they cannot see blocked time.

### Outside sampling for `--attach`

`--attach` is handed a pid with no channel in. It reads the process from the outside: `/usr/bin/sample` on macOS, and on Linux elephc stops each thread with `ptrace` and walks the frame chain. This costs three things asking does not:

- **Symbols.** An address is not a name. elephc strips the symbol table by default, because nothing inside a program reads it. A target to attach to needs `--keep-symbols` or `--debug-info`. Attaching to a stripped binary says so instead of reporting a profile of `<native>`.
- **Permission.** `yama/ptrace_scope` commonly restricts attaching to descendants. A container needs `SYS_PTRACE` and a seccomp profile that allows `ptrace`.
- **Frame pointers.** A chain that loses one ends there. It is not guessed past.

On Linux a thread that cannot stop stays attached across windows. A live window with only such threads retries instead of ending the view. Each stop wait, including cleanup, is bounded to 50 ms per thread, so the first delay can grow with the number of blocked threads. The header counts discovered processes. This can differ from the ring contributors. An outside reading carries no sampled allocation deltas or route tags, because those come from the in-process ring.

`--attach` discovers the target's direct children each window and merges them. A launched `--live` of a `--web` binary asks the master over the control channel. Workers write into the same shared ring that the answer reads.

Live mode skips inlined-frame recovery to keep the refresh light.

### Inlined frames and dSYM

When the target is a `.php` source and its `.dSYM` is present, calls the inliner erased reappear as virtual `name (inlined)` frames. They are recovered from the source spans the inliner preserved. A `.php` capture therefore differs from a bare-binary capture, which is why comparing the two shows false deltas.

### Per-line shares

A sample is an address, and the dSYM says which line owns it. That is why the per-line view needs a macOS build with its `.dSYM`.

Per-line shares are sampled by construction. Exact per-line timing would need a clock read at every line, far more expensive than the code measured. The exact capture times whole calls, so its source view annotates per function from the exact costs and declaration ranges.

## Exact instrumentation

Every PHP function gets `elephc_instr_enter` and `elephc_instr_exit`. The top-level body is wrapped as `{main}`. The binary keeps a shadow stack and writes the result to stderr at exit:

```text
elephc-instr: {fn} calls=<n> incl_ns=<ns> excl_ns=<ns> incl_allocs=<n> excl_allocs=<n> incl_io=<n> excl_io=<n> incl_ret=<n> excl_ret=<n> incl_wait=<ns> excl_wait=<ns> incl_network=<n> excl_network=<n> incl_network_wait=<ns> excl_network_wait=<ns>
elephc-instr-edge: {caller} -> {callee} count=<n> ns=<callee ns under caller>
elephc-instr-query: <count> <normalized SQL text>   (one per distinct statement, if any DB ran)
```

`elephc monitor` reads those lines and renders them.

- `incl_ns` is the time between enter and exit. For a recursive function it is the outermost activation, so recursion is not double counted.
- `excl_ns` is inclusive minus the callees'. Across a run the exclusive times sum to the root's inclusive, a real partition of the program's time.
- The same shadow-stack arithmetic applies to the allocation counter, so allocation counts partition exactly too. Because elephc is the allocator, this needs no extra bookkeeping. A side effect: arithmetic on `mixed` values allocates, so a hot integer loop can be an allocation hotspot.
- `incl_ret` and `excl_ret` read the free counter (`_gc_frees`) beside the allocation counter at each hook. The value is signed, and self values still partition the root's total.
- `incl_io` and `excl_io` count PDO statement executions and `PDO::exec`. The PDO bridge reports through a pay-for-use slot. A binary without the capability records nothing.
- `incl_wait` and `excl_wait` come from the PDO bridge timing statement execution and `PDO::exec` across every driver.
- `incl_network` and the network wait cover curl transfers, connection upkeep and multi-wait time. They are separate from the PDO dimensions, so an HTTP-heavy request cannot inflate query budgets or cause a false N+1.

Filesystem I/O is not counted. A full per-query dimension for other builtins would need a runtime hook in each one. The fan-out heuristic finds the same N+1 shape from data elephc already has.

Instrumentation is per thread and reports the main thread at exit, which is the whole program for single-threaded PHP. Inlined functions have no prologue to hook, so they fold into their caller, as with `--counters`.

### Selective instrumentation

With `--with-monitoring=<names>`, hooks land on the named functions only. An uninstrumented callee runs inside its instrumented caller's frame, so its time lands in the caller's self. Self values stop summing to the root's inclusive, which the full mode relies on. If `{main}` is not selected, shares are relative to the largest included function and self shares can sum past 100%. Instrumenting `load_price` but not the PDO methods it calls makes `excl_ns` equal `incl_ns` for it. The run prints a note saying so.

### Depth cap

The shadow stack is capped at 65 536 frames, so a runaway recursion degrades instead of growing without bound. Past the cap, calls are counted without time. The stack grows on demand, so the cap costs nothing until it is approached.

### Exceptions

An exception's frames are closed at the instant of the throw. Their own exit hooks never run. The runtime's single unwinder tells the profiler that an unwind started, and when.

Without that, the only moment available was the catching function's return. That charged everything the handler did to whatever threw. On a function whose whole body is `throw`, it carried 100% of the self time, which was the eight-million-iteration loop of the catch block. It now reports about 0%, and the work sits with the catcher. All dimensions are read at the throw, so what a handler allocates belongs to the handler.

This holds for a handler that calls something, a handler that throws again, and an exception raised and caught inside a call the handler made. Each throw is resolved by the exit that caught it.

### Generators and fibers

A `yield` and a `Fiber::suspend` switch stacks instead of returning. A suspended body is not running, so the frame is closed at the suspension and reopened at the resume. The span in between belongs to whoever drove it.

- **`yield from`.** The runtime helper that drives the inner generator calls the suspension primitive itself. The delegating body is therefore off the stack for the whole delegation, and the inner generator starts while the outer one is suspended. It is recorded as a callee of the consumer: `drain → inner` where PHP says `outer → inner`. Inclusive time is right for both. The outer body once took 52.6% of a program for 52 µs of work and now reads 0.3%.
- **Suspensions that raise.** `Fiber::suspend()` outside a fiber and a live `unserialize()` raise `FiberError` before the stack switch. A `Fiber::throw()` or `Generator::throw()` delivered on resume raises after it. All reach PHP handlers, so the helper restores the activation before the raise, keyed by the running coroutine. Without that, work done by the body's own `catch` was attributed to the consumer, and the graph showed `drive → heavy` for a call `drive` never makes.
- **Nested suspension.** `Fiber::suspend()` from a function called by the fiber body suspends the whole coroutine. The hook is told one frame pointer. Frames below it cannot be told from the consumer's without a record of where the coroutine began. So only the innermost frame is parked. Direct suspension from the body, which is every `yield`, is exact.
- **The 4 096 limit.** Past 4 096 suspended coroutines a suspension is refused and the frame stays where it was. A suspension also releases whatever was still parked under its own coroutine. A fiber address is handed to the next fiber once the first is freed. A group still standing under it belongs to a coroutine that is gone. An abandoned generator therefore costs a slot only until its address is reused.

### Counter rate

On a machine that does not publish its counter rate, the rate is derived from the run itself: elapsed ticks against elapsed nanoseconds. A run under 100 µs is too short to divide safely. `monitor` then says the `_ns` columns are raw ticks instead of inventing a rate. ARM machines publish the rate.

### Notes on `--counters`

Each PHP function gets one BSS counter incremented in its prologue. A fully inlined call keeps its counter at zero.

## Query attribution

The PDO bridge reports each executed statement to the profiler. Text is normalized: string and numeric literals collapse to `?`. The counts are exact, so the Queries panel and the N+1 recommendation agree by construction.

The instrumentation keeps, for each query shape, the functions that ran it and how many times. A function counts as the runner when it is the first application frame above the driver's own methods. The driver's functions (PDO, mysqli) are flagged by id at build time. The output is one row per runner, most first: `<executions>\t<path:line or ->\t<function>`.

## Timeline recording

The Flame view's Timeline and the Perfetto trace come from the same enter and exit records.

- The page keeps at most 60 000 calls. It keeps the longest ones and reports how many it shows.
- The Perfetto trace is bounded to 500 000 calls by default (`ELEPHC_INSTR_TRACE_MAX`). The overflow count is reported.
- A suspension ends a span exactly as a return does. A generator resumed three times appears as three slices on its own row. An abandoned generator still gets the slice it ran for, even though its exit never arrives.
- Each slice stamps the wall clock when its trace context opens. This is what places spans on a shared axis in the distributed waterfall.

## Routes under `--web`

Under `--web`, the sample ring lives in shared memory allocated before the prefork. Every worker samples into it, and the master's endpoint serves it.

The route label is derived from the request path alone. By the time a request reaches the sampler, the path is all that is known about it. No framework routing table is consulted. Segments that cannot be route names are replaced by what they are (`{id}`, `{uuid}`, `{hash}`). Everything else is kept verbatim, because merging two real endpoints into one row would report a number true of neither.

The route table has 255 entries plus an `<other>` bucket. Nothing is interned until someone asks for a profile, so an unasked service spends none of it, and the window you get is the one you asked for. Past 255 shapes the surplus is reported as `<other>` instead of being dropped, so a full table says so. A service with `--apm` interns a route from the first request it answers, because it counts every request from boot. A "not found" answer interns nothing.

## Distributed tracing

elephc speaks the standard W3C `traceparent` header, not a private one. An elephc profile then joins whatever trace its caller already belongs to, instead of forming an island only elephc tooling can read. A malformed header starts a new trace, so a caller cannot inject text into the profile output.

Curl propagation injects the value the profiler publishes for the current request, unless the program already supplied a `traceparent`. It is dormant outside an active capture and removes a previously injected header when the handle is reused later. Non-instrumented binaries carry none of this. The runtime slot stays zero. The HTTP stream layer needs the manual `ELEPHC_TRACEPARENT` route.

### Stitched reports

- The waterfall is in the terminal output too, because the difference between serial and overlapping hops is the reason to correlate services at all.
- Rows split per endpoint because a service-wide p95 averages away the one slow route you want.
- Percentiles are nearest-rank, so each one is a duration some request took, not an interpolation nobody experienced. Under 20 requests, the upper percentiles are the slowest requests. Calling them p99 would dress a maximum up as a distribution.
- The axis is wall clock, the only clock two hosts share. Clocks can drift, so a hop may start a hair before its parent. A trace is placed only when every span is dated, because one undated span pinned at the origin would read as "started first" instead of "unknown".

### Exports

- `--otlp` turns the hop from a gap in the trace into a span with its own duration, route and counts. Slices without timestamps are skipped, since OTel needs both ends of an interval and epoch 0 would file them under 1970. An `https` endpoint is refused. A remote or authenticated collector belongs behind a sidecar, which keeps a TLS stack and a credential store out of the compiler.
- **Profiles are not exported over OTLP.** That signal entered public alpha in 2026 and its SIG advises against depending on it. OTLP Profiles round-trips losslessly with pprof and the Collector has a `pprof` receiver, so the `--pprof` file reaches an OTel backend without this compiler chasing a moving wire format.
- `--prometheus` writes a file instead of an endpoint, because `monitor` runs and exits and leaves nothing to poll. Percentiles are a `summary`, not a histogram, because exact per-request values exist and buckets would invent a resolution the capture does not have.

## Assertions

`SKIP` is distinct from `FAIL` because it points at the budget, not the code. It still fails the gate, because a budget that silently measures nothing is worse than none.

The budget file is searched from the profiled source, not the working directory, so where you run the profiler cannot silently change which budget gates the build. The label after `#` is the point of the file. A red gate that says why a budget exists is actionable where a bare number is not.

## Profile page

- A sampled run has no flame graph, and an exact one has no per-line samples, so each capture offers only the views it supports.
- The heat stays on the own cost under the Inclusive ranking. Ranked by inclusive cost, the entry point always leads, and colour would stop pointing at the function that does the work.
- A stored graph view that would open on empty space is discarded instead of restored.
- On a dark ground a quiet function is a dark card instead of a glaring white one.
- Live mode embeds all ten captures in one self-contained file and lays them out on the union of their nodes. A node keeps its position while you scrub. Served over HTTP, the page re-fetches itself and merges new frames in place, and the layout reflows when a newly hot function first appears.
- Speedscope is for reading one profile. `--baseline` therefore rejects a Speedscope file outright, rather than pass a gate that compared nothing.
- The diff has two frames, baseline and current. The sampled `--baseline` is the statistical equivalent for a `--live` capture.

## Other caveats

- A program that calls `execve` without forking keeps the armed sampling timer, which would kill the new image. The exported `elephc_probe_disarm` stops it. Calls that fork first are already safe.
- On Apple `arm64e`, PAC-signed return addresses degrade sampled stacks to `<native>`. The default `arm64` target is unaffected.
- A Unix socket path holds about 104 bytes (`SUN_LEN`), so a socket under a deep directory never binds.
- Anyone who can read the binary can extract the build key, so the handshake protects against other processes and hosts, not against someone holding the artifact.
- The sampled answer from a combined build has no query summary. Both runtimes are linked, and the exact one claims the PDO I/O slots on purpose. It attributes the same events per function through its shadow stack, which strictly refines the sampler's per-route counts.

Back to the user guide: [Profiling](../beyond-php/profiling.md).
