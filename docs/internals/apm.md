---
title: "Transaction monitoring internals"
description: "How elephc's transaction monitoring counts, stores and serves what it measures, and why it is built that way."
sidebar:
  order: 20
---

This page explains how [transaction monitoring](../beyond-php/apm.md) works and why it is built this way.
The user guide covers what you type. This page covers what happens underneath.

## Why `--apm` is a runtime flag

Monitoring needs two switches.

- The binary carries it, through `--with-monitoring` or `--with-apm`.
- The service is started with `--apm`.

`--apm` is a runtime argument of the produced binary, not an environment variable. Whoever passes a service its arguments already owns it.

Monitoring is the one capability that is not dormant until asked. The incident you need to look at happened before you connected. That is why it takes an explicit switch.

A binary without monitoring, or whose shared counters could not be mapped at startup, refuses `--apm` with exit code 2. The alternative is a service that looks monitored while recording nothing.

Without `--apm`, a monitored service counts nothing, as before. The profiler's sampler stays dormant until someone asks for a profile.

## Where the counters live

All workers, and the handler processes of `--web-isolation=pool|request`, count into one table. The table sits in a shared mapping the master allocates before it forks. A single read covers the whole service.

Recording a request is:

- four relaxed atomic increments into a fixed table;
- one clock reading at the start of the request and one at the end.

Once a route has been seen there is no allocation and no lock. A test counts the allocations of the request path and fails if there are any.

Reading the counters costs the service one handshake and one pass over the table per poll. It arms nothing.

### Measured cost

The figures in the guide are retired instructions per request. They are taken on the cheapest route of a service, a 404 with no work behind it. The same binary is run with and without `--apm`, three times each. They come from a debug build of the runtime, so they are an upper bound.

On any route that does real work, the difference does not register.

### Database and network counting

PDO and curl time each statement and each transfer under `--apm`. The count lands on the route of the request that ran it. Timing costs two clock readings per statement or transfer, never per function call.

A request keeps these counts in its own process while it runs. It publishes them once, with the request. A statement therefore touches nothing the service's other workers share. The cost is about 10 ns with one worker and 20 ns with sixteen on the same route.

Two consequences:

- A slow request's statements appear when it answers, not as it goes.
- A worker that dies serving a request still has its statements charged to it.

## Routes

The route is the request's shape. The server derives it from the path with no routing table consulted. `GET /users/7` and `GET /users/8` share one row.

A method outside HTTP's own reads `OTHER`, because the client picks it.

### Why not found has its own row

A route enters the table only once one of its requests is answered with something other than 404, 405 or 410. Until then, such an answer names no route, because the path served nothing. It is counted under `<not found>`. A scanner's thousand paths cost one row, not the table.

A route already in the table keeps its own "not found" answers. A product page asked for an id that does not exist shows in that route's 4xx rate.

Requests the server answers before PHP runs (`408` for a body that never arrives, `413` for one too large) work the same way. Any client can send them to any path. They count under their route when the table has it, and under `<rejected>` otherwise.

Asking for a profile changes none of this.

### Rationing

New routes are rationed: 64 at once, then one more a minute, shared by every process of the service.

A service's routes appear in a burst as traffic reaches them and rarely afterwards. A scan of an application that answers `200` to any path keeps inventing them. A request beyond the ration counts under `<other>`, and its route gets its row at a later request.

The table holds 255 shapes plus `<other>`. Past that, traffic counts under `<other>` rather than being dropped.

A label longer than 63 bytes keeps its start and ends in `~` and a hash of the whole label. Two long endpoints never share a row.

### Resetting

A reset removes every route, counter and name. The counters start a new run, which a collector reads as a restart. Nothing is counted twice, and stored history keeps the old routes.

A request running across the reset lands on its own route. Samples the profiler took before the reset are shown without a route, because their route ids belong to the old table.

## Requests that take a worker down

A request that kills its worker never reaches its end, so it cannot count itself. Examples are an uncaught exception, a fatal error, `exit` and a `--max-execution-time` kill.

Each request is marked in flight in the shared counters. When the master reaps a worker that died serving one, it counts that request as a 500, timed from its start to the reap. A worker that dies while counting a request it answered is not counted again.

Under `--web-isolation=pool|request`:

- The web worker answers a handler that died with a 500 itself. That answer is what is counted.
- A web worker never dies of PHP there, but it can be killed, for example by the kernel's out-of-memory killer. Each request it waits on is marked in flight too. The master counts each one as a 500 when it reaps the worker.

Tracking is limited to 1,024 requests in flight over the whole service.

## Wall time

Wall time stops at the handler's return, before the body is compressed and written to the client. It is what the server spent, not what the client waited.

## Percentiles

Latency goes into 256 buckets. They are exact below 16 µs, then 8 per power of two.

A bucket is never wider than 1/8 of its lower edge. A percentile is reported as the upper edge of the bucket it falls in. A p95 of `4.6 ms` means 95% of requests took at most 4.6 ms, and the true value is within 12.5% below it.

Nothing is interpolated. Each value is a bound some ranked request actually respected.

The Prometheus `le` buckets count only the requests whose bucket ends at or below the boundary. A boundary inside a bucket therefore undercounts. It never claims a request was faster than it was.

Memory uses a separate 16-bucket histogram: powers of two from 64 KiB to 2 GiB. The 95th percentile in the page tooltip is the upper edge of its bucket.

Memory is measured where the request's PHP ran, a prefork worker or an isolated handler. It is the value `memory_get_peak_usage()` would answer at the end of the request, which a `--web` request starts over. Messages, named boundaries and script runs are not measured, because their process does not start its memory over for each of them.

## Service CPU and memory

Each time the collector asks, the service's master measures its whole process tree: itself, its workers and its isolated handlers.

### CPU

CPU is user plus system time since the service started. A process's time includes that of the children it has reaped, so a worker that dies does not take its CPU with it.

A child reaped while the tree is being measured is caught by reading each parent again. A recycled worker is therefore counted neither zero times nor twice.

### Cores

Cores are the cores the service may run on. That is its CPU affinity and, on Linux, its cgroup quota, so a container limited to two cores counts two.

CPU is shown as a share of all of them, whatever machine the service shares. Two services in one store add up, cores included.

A quota is counted in whole cores, rounded down. A service allowed 1.5 CPUs counts one and can read up to 150%. The chart shows that rather than cutting at 100%.

### Memory

Memory is the live processes' memory, added up so each page counts once. A page shared by n processes counts 1/n in each.

- On Linux the figure is the proportional set size (`smaps_rollup`).
- On macOS the same sum is made from each memory region's private and shared resident pages.

Workers forked from one master share their copy-on-write pages. Summing resident sizes would count those once per worker. So would macOS's physical footprint, which charges a shared page to every process mapping it.

Reading walks each process's mappings, so memory is read every five seconds rather than every poll.

Script runs carry none of this. The run's own CPU and memory are in its profile.

## Errors

The error is taken where an exception is answered. The application needs no change.

- The `--web` handler's catch-all handles an exception nothing caught.
- Symfony's `HttpKernel::handleThrowable` turns a request's exception into an error page.
- Messenger's failure event covers a message whose handler threw.
- A `--apm-transaction` boundary counts an exception that leaves it.

Only a transaction that ends as a failure counts its error. A framework turns a missing page into an exception too, and a 404 is not an error of the service.

The first exception a transaction notes is its error. An error page that fails while rendering is a consequence, not the cause.

The service keeps 256 error groups, keyed by transaction, class and place. Past that, new errors count in the transaction's 5xx but are not grouped. Under request isolation, the handler that ran the request counts its error.

A transaction's noted error and trace id stay thread-local until the transaction ends.

## The store

### Files

- Each window is one JSON line, appended whole. A crash loses at most the line being written, and a reader skips a torn line instead of failing the day.
- `t` is when the window ended, in Unix milliseconds by the collector's clock. Day files are named and pruned by that clock, so a remote service whose own clock is off still lands in the right day.
- `epoch` identifies the run of the service.
- `hist` lists the non-empty latency buckets.
- Only files named `apm-YYYY-MM-DD.jsonl` are read or deleted. Anything else in the directory is left alone.
- A script run deletes old days only when it is given `--retention` itself. A default therefore never cuts a collector's longer history.

```json
{"t":1794752059613,"span_ms":5999,"epoch":1794752015584868000,
 "routes":[{"route":"GET /products/{id}","n":94,"status":[0,47,0,47,0],"sum_ns":16591204,
            "hist":[[27,1],[28,2],[29,2],[30,3],[31,3],[32,4],[33,5],[34,4],[35,3],[36,1],
                    [37,3],[38,6],[39,1],[40,7],[41,12],[42,4],[43,5],[44,1],[45,7],[46,5],
                    [47,4],[48,6],[49,4],[73,1]]}]}
```

### Roll-ups

Each day is rolled up once, when the collector prunes. The file is rewritten aside and renamed over the old one, so a reader sees one version or the other.

The store records what was rolled up in `rollups.json`. Today and yesterday are never rolled up, because a collector may still be writing yesterday's last windows.

Only a collector rolls up. Browsing or exporting a store changes nothing.

A request stays counted at every resolution. Only the time it is placed at gets coarser.

### Reading

Reading is incremental. A day file is parsed once, then only for what was appended since, however often the page refreshes.

In memory, a day that is over has its windows folded by the minute, because nothing reads a past day finer. Script runs of one transaction are folded by the minute too, so a cron line run every second does not fill memory.

A period that has ended is not reread on every new window.

### Restarts

The service's counters start from zero when it restarts, and its epoch changes. The collector notices and treats the new counters as the window since the restart. Nothing is counted twice and nothing negative is recorded.

The same applies after a route-table reset.

### One collector per service

A collector claims its service in the store with a `.collector-<address>-<hash>.lock` file. A second collector for the same service is refused, because it would write the same requests again and double every count.

Collectors of different services, and script runs, share a store.

- Each window records the service it came from.
- Rates count the time covered once, however many services covered it.
- A view holding several services names their routes `service · route`.
- A collector's live page reads only its own service's history.
- Its captures are its own too: each records the service it was captured from, and a live page lists and opens only those (a script's runs belong to every page).
- The claim states how long the collector keeps each resolution (`<retention> <full days> <minute days>`). Pruning keeps the longest retention, and roll-ups the finest resolution, any live collector asked for. One collector at a time prunes and rolls up, under `.maintenance.lock`; deploys are appended and rewritten under `.deploys.lock`.

### Deploys

A build carries its identity in what it sends: the time it was built and a label. The label is `ELEPHC_BUILD_ID`, or the short commit of the git checkout holding the entry file. `SOURCE_DATE_EPOCH` pins the time for a reproducible build.

The collector notes a deploy the first time a service serves a build it never served before.

A service here is the address a collector reads. For programs that push to a listening collector, it is the program's name across all its hosts and processes. A restart is not a deploy. A rolling deploy, where old and new builds answer in turns, is one deploy.

Deploys are kept in `deploys.jsonl`. Each service's latest deploy is kept past the retention, so the build it serves stays known. A collector started again on the same build marks nothing new.

## The page

The monitor page is the same page `--html` writes for a profile. The Transactions view is added to it only with `--apm`.

The page updates when a new window arrives, over server-sent events. It has no timer of its own.

The last hour comes from memory at one-second resolution. Longer ranges come from the store at its resolution.

The exported `--html` page holds every stored transaction and the series of the busiest twenty, so its table and route filter work offline.

### Server hardening

The server has no authentication.

- Bound to loopback, it answers only requests addressed to a loopback name on its port. A web page that rebinds its own name to 127.0.0.1 gets nothing.
- Capturing a profile changes the service, so it takes a POST from the page's own origin. A link or an image on another site cannot trigger it.
- It reads at most 16 KiB of request head, which must arrive within ten seconds.
- It serves 64 connections at once.
- A client that stops reading is dropped after ten seconds.
- An open page's event stream is renewed every five minutes, so a vanished browser never keeps a slot.

### Profile capture from the page

The Profile view starts empty. **Capture a profile** asks the service to start sampling its CPU. The page waits three seconds and shows the result, routes at the root.

The sampler then stays armed for the life of the service, as after any ask. The last ten captures are kept.

## Captures

A request's duration and status are known only once it ends. `--capture-every` therefore profiles the next request each time and keeps what turned out slow or failed. A rare slow request is caught in proportion to how often it happens.

Captures are kept per route and per hour, not over the collector's whole life. The slowest requests of one bad night then do not leave every other period without an example.

The cost is one exact request per period, whatever the traffic. That is about 30 ns per function call of that one request, and nothing on the others.

Capturing is the collector's work, so a one-shot read refuses the flag rather than ignoring it.

## Alerts

A collector checks every budget after each poll over the last `--alert-window` seconds. The limit of 3600 is the hour the collector keeps in memory.

- Only windows inside that span count. The first window after the service was unreachable spans the whole outage. It is not judged as the last minute.
- A route with no traffic keeps its state. Silence is neither a recovery nor an incident.
- `rps` is the exception, because no request is the measurement. Budgets are still checked every second while the service does not answer, so a floor such as `rps:*>=1` fires when the service stops.
- `--alert-exec` runs through `sh -c` without waiting. No notification service lives in the compiler: a webhook, a chat message or a pager is one command.
- Under `--live` on a terminal, the hook's output would land in the middle of the redrawn table, so it is discarded.

## Scripts

`monitor` launches a script under the exact profiler, as it always does. With `--apm` it also records the run.

The time is the process's, from launch to exit. Database and network figures come from the run's exact capture.

The argument after `--` joins the name only when it reads as a console command. Anything else would split the history into one row per value.

Script runs are recorded through `monitor` or a `--apm-collector` build. A script started any other way records nothing. Its argv belongs to PHP, so there is no `--apm` it could take, and an environment variable never turns monitoring on.

Under `--apm`, `monitor` writes no Speedscope file unless given `--out`. A wrapped cron line then leaves no file behind and cannot fail on one.

## Transactions beyond HTTP

A transaction is one entry into compiled code. A transaction that starts inside another is part of the outer one and counts nothing of its own. Examples are a message dispatched synchronously during a web request and an export a host calls from a callback.

Excluded transactions are not opened at all. Their own work goes nowhere. A counted transaction run inside one is still counted, with its work.

`--with-apm` carries transaction monitoring without the profiler. It has no instrumentation and no sampling signal, nothing a host application would not expect of a library.

### Libraries

A library counts from `elephc_init` to `elephc_shutdown`. Its shutdown sends the last counts and stops the sending thread, so the host may unload it. A later `elephc_init` starts sending again.

A library opens no `ELEPHC_PROBE_ADDR` endpoint. It is followed through its collector only.

A program hosting PHP extensions shares the exit hook with their shutdown. The hook runs the extensions' shutdown first and then the last send, so the counts of the extensions' work are sent too.

## Push transport

A program built with `--apm-collector HOST:PORT` sends its counters instead of waiting to be read. The address is part of the build, never of its environment.

- Both sides prove the build key before a byte of counters is sent. The program sends to a holder of the key only, and the collector keeps what a holder of the key sent only.
- The counters travel encrypted, each send under keys of its own.
- Counters are cumulative, so a lost connection loses nothing. The program reconnects and the next send carries everything since.
- The program sends every five seconds and once more at exit.
- Each program is a source of its own. Its name combines the program, the address it sent from, its pid and a number it draws at start, as in `worker@10.0.0.7 pid 1 #3fa2c1d0`. Two containers running one build as pid 1 behind one address stay apart. Without OS randomness the number mixes the start time, the pid and a stack address, and the name keeps both halves of it folded, so processes started in the same second still differ.
- A collector restarted while a program runs counts that program from its next send, as a collector reading a service counts from its first poll.
- A listening collector serves up to 256 programs at once. A connection that has not proven the key within five seconds is closed.
- A program silent for ten minutes is forgotten. One silent for fifteen seconds is time in which no transaction was seen.

A collector that only receives can neither capture requests nor reset a route table, because nothing can be asked of a program that only sends. `--listen` refuses `--capture-every` and `--reset-routes`.

## Trace context

A `--with-apm` web service speaks W3C Trace Context, like an instrumented build.

- A request whose `traceparent` header names a trace continues it with a span of the service's own.
- A request without one starts a trace.
- curl sends the context on to the next hop.
- An `ELEPHC_TRACEPARENT` merely inherited from the environment is never sent.

Each error group keeps the trace id of its last occurrence.

## OpenTelemetry export

The collector posts one batch per `--resolution` seconds and per source, with delta temporality. The poll never waits for the endpoint. The collector posts what it holds before it exits.

The endpoint is plain HTTP, as for the traces `--otlp` exports from instrumented builds. A local collector forwards over TLS.

## Project file rules

An `[entry."<path>"]` table applies to the build whose entry file is that path, relative to the file's directory. This serves a web front and a console of one project, each with its own rules. A table whose path names no file is reported on every build: a typo would otherwise drop its rules with nothing said. A path that names another entry file is not reported, because the file serves every build of the project.

`enabled = false` wins over the keys that otherwise imply `enabled`: `functions` for `[monitoring]`, `collector` and `transactions` for `[apm]`.

A library build skips `[monitoring]`, because a library has no `main` to start the profiler. A PHP extension skips `[apm]`, because PHP counts its own requests. Each skip is a note rather than an error, because the same file serves the project's other builds.

Include and exclude rules are part of the build, like the collector's address.

Back to the [user guide](../beyond-php/apm.md).
