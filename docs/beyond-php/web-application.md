---
title: "Persistent Web Applications"
description: "Bootstrap one typed PHP application per worker and reuse it across isolated HTTP requests."
sidebar:
  order: 8
---

`--web-lifecycle=application` gives each native web worker its own retained PHP
application. It is opt-in and initially requires `--web-isolation=worker`.
The default `--web-lifecycle=request` retains the existing behavior in every
isolation mode: execute the top level and reset PHP state for each request.

```bash
elephc --web --web-isolation=worker --web-lifecycle=application bootstrap.php
./bootstrap --listen 127.0.0.1:8080 --workers 4
```

Lifecycle is a compiler option. `pool` and `request` isolation reject application
lifecycle rather than silently changing its semantics.

Projects declaring hosted PHP extensions in `[extension.dependencies]` currently
require request lifecycle. Compilation rejects application lifecycle for those
projects because the hosted Zend request storage cannot preserve application-owned
values across requests. Built-in Elephc bridge crates keep their existing support.

## Typed API

The shared contract uses ordinary PHP:

```php
namespace Elephc\Web;

interface WorkerApplication
{
    public function handle(Request $request): void;
    public function shutdown(): void;
}
```

`Request` is an owned snapshot with four public string properties: `method`,
`uri`, `path`, and `body`. Its constructor accepts those fields in that order.
It contains no borrowed HTTP-buffer pointer or connection handle. The standard
request superglobals remain available for query parameters, forms, cookies,
uploads, server metadata, environment variables, and sessions.

The ElephC-specific bootstrap registers one application:

```php
<?php
require_once __DIR__ . '/application.php';

Elephc\Web\Worker::register(new MyApplication());
```

`Worker::register(WorkerApplication $application): void` owns its argument and
rejects duplicate registration. Registration occurs during bootstrap, before
the entry script finishes. The other `Worker` methods and the generated
`__elephc_web_*` callbacks are reserved adapter implementation details.

Keep native registration in this entry point. Shared framework and application
sources implement the PHP interface and receive the PHP `Request` object.
For execution with the PHP interpreter, load
`src/web_prelude/application/contract.php` from a PHP-specific entry point,
construct the application, call `handle()`, and call `shutdown()` in `finally`.
The ElephC entry gets the contract from the compiler prelude automatically.

See `examples/web-application/`: `application.php` contains the portable counter
application; `main.php` contains its native bootstrap.

From a source checkout:

```bash
target/debug/elephc --web --web-lifecycle=application examples/web-application/main.php
examples/web-application/main --listen 127.0.0.1:8080 --workers 1
```

Two requests to `/first?value=one` and `/second` show the same worker PID, an
increasing application counter, and the retained previous path. The second
request reports `query value=absent` because its query data is fresh.

## Worker and request lifetime

Each worker runs the original entry script once after fork, before creating
its HTTP listener. Successful registration and normal completion commit the
bootstrap. The application is private to that worker PID; different workers
have independent counters, containers, and caches.

During bootstrap, all superglobals are valid arrays. `$_ENV` and `$_SERVER`
contain the process environment, and request-specific arrays start empty.
Cleanup replaces each request array with a valid empty container before retiring
its old owner. Destructors and shutdown hooks may read or write those containers;
their contents do not describe an active HTTP request and are replaced when the
next request is prepared.

Every request prepares fresh metadata and superglobals, invokes `handle()`,
finishes an active session, and releases request owners. Output buffers and
temporary formatting buffers are drained or retired before returning to the
server. PHP exceptions remain inside generated native callbacks.

Ordinary globals, function statics, static properties, and application-owned
objects live for the worker. Process registrations, including error handlers
and stream wrapper/filter definitions, also remain worker state. Applications
that install a registration temporarily must restore it in `finally`.
Native extern callback bindings also remain worker state. Final teardown releases
them after PHP and resource cleanup, then collects cycles detached by their captures.
Session deployment settings and request parser controls are prepared afresh.
Ambient stream-context options and notification callbacks are request scoped;
create them inside `handle()` when needed. Replacing those registrations and
ending a request release their preceding owners.

The heap is non-moving and remains valid across requests. Saving a string,
array, object, or request snapshot in application state preserves its owned
storage, including values constructed during a request. Array copy-on-write
and object identity retain their normal PHP semantics. Replacing a retained
value releases its preceding owner. Request-end cycle collection handles
unreachable managed container/object cycles, and resource inventory pruning
preserves handles whose canonical PHP cell remains owned.
Native closures and arrow functions participate in this ownership graph, including
value and reference captures, nested closures, bound receivers, and Fiber-owned
callbacks. Generator argument and yield/return owners and internal storage in
SplDoublyLinkedList, SplStack, SplQueue and SplFixedArray also participate.
Property hashes in incomplete objects created by `unserialize()` are traced,
including when `allowed_classes` rejects a class.
Suspended Generator and Fiber frames also own their initialized locals. Releasing
an abandoned, suspended coroutine runs its `finally` blocks and releases those
locals before unmapping its stack. Breaking out of an otherwise unretained
generator therefore finishes cleanup in the same request. A temporary generator
used by `yield from` closes before the outer `finally`; a shared generator keeps
its own lifetime. A coroutine retained by the application keeps its frame valid
until it resumes or its last owner releases it.

Errors from `finally` follow the protected request cleanup rules below. Internal
cancellation cannot be caught by PHP or eval catch clauses and is never exposed
as a previous PHP exception. Yield during forced
Generator close and suspension of a force-closed Fiber raise PHP-compatible
errors. Other Fibers can still run from `finally` code. These coroutine
destruction rules also apply in CLI and request lifecycle modes.
A temporary object retaining a callback that captures that object is
collected at the same request boundary. An application-owned callback keeps its
captures alive until the callback's last owner is released.
Application workers collect candidate graphs whose owner counts decreased.
A non-owning cache records paths from retained values to global/static roots.
Validation checks the current owning slots along the path, including array
and service-map entries, and typed value/reference captures in native closures.
Successful and unproved shared prefixes are memoized
only within one callback-free collector preparation pass. Recounts after PHP
destructors start a fresh pass. Reading a retained service does not require tracing
all of its descendants. Detached, replaced, or retagged paths fall back to
ordinary candidate collection. Cyclic destructors, including resurrection
and exceptions, are resolved in the same cleanup pass. Collection is not
deferred to a later request or an allocation threshold.

Bootstrap builds the cache once. Later reconstruction is amortized against
structural scan work on candidate nodes that survived collection. The charge
includes inspected slots, even empty hash entries and scalar or string values.
The runtime rebuilds after earning the previous cache's complete scan cost
(with a minimum of 256 units). A small retained write or saved Request therefore
does not independently rebuild the whole application cache. Unproved values
continue through immediate candidate collection while the cache earns its
rebuild budget; cycle cleanup and destructor timing stay unchanged.

Cache construction visits and sorts the rooted graph. During a preparation
pass, binary searches validate each distinct cached path prefix once, and later
lookups reuse its result. Detached paths are also memoized for that pass, so a
changed ancestor does not cause every descendant to repeat the failed walk.
New or changed large graphs can still require substantial work, and an earned
rebuild can make an individual request slower. The cache owns no PHP values
and uses reusable native metadata outside the configured PHP arena: three
machine words per forest record plus one memo word, with geometric capacity.

An implicitly released stream uses the same descriptor teardown as `fclose()`,
including wrapper/filter callbacks and descriptor metadata. Application-owned
streams retain their handles and metadata until their final owner releases them.

There is no implicit rollback of application state after a failed request.
Retaining an ever-growing request history consumes memory intentionally;
bounded caches should replace or remove old values. Raw pointers and
extern-owned storage keep their existing explicit ownership requirements.

## Failure, shutdown, and recycling

- An uncaught PHP `Throwable` produces a 500 before response commitment, or
  aborts an already-committed response. Request cleanup still runs, and the
  next request can use the same application. Earlier application mutations
  remain visible.
- Failed bootstrap, missing registration, duplicate registration, or early
  return before bootstrap completion prevents that worker from accepting
  requests. Partial PHP state is released. The existing supervisor replaces
  failed workers and stops repeated bootstrap failures, including slow failures.
  Uncaught bootstrap exceptions are logged with their class, message, file,
  and line before the failed worker exits.
- A partially bootstrapped application is released without calling its
  application `shutdown()` hook. A successfully bootstrapped application gets
  one hook call during graceful worker termination or planned recycling.
- On `SIGTERM` or `SIGINT`, an application worker stops accepting and drains
  active connections before shutdown. `--max-requests` uses the same drain,
  then the supervisor starts a new PID with a new bootstrap.
  Termination signals are deferred while a PHP callback runs, including blocking
  socket reads with receive deadlines. The execution-timeout watchdog remains active.
  Processes created by native or evaluated PHP inherit the application's signal mask
  without the worker's added termination blocking, so child termination and shell
  timeout behavior remain available. Fork children also drop inherited cooperative
  worker handlers while preserving PHP-installed signal handlers and ignored signals.
  Changes to other signal-mask bits survive bootstrap and request callbacks.
- A throwing shutdown hook is logged; remaining teardown still runs. Fatal
  process errors, execution-timeout termination, and `SIGKILL` cannot guarantee
  application hooks. The operating system reclaims the terminated process.

Final teardown detaches and releases remaining global/static owners under protected
cleanup. It keeps the heap and exception frames valid until callbacks finish; process
exit reclaims the final arena.

`opcache.preload` remains independent: it executes once in the master before
fork. Its PHP objects are not the application's retained context. Each worker
performs its own application bootstrap after retiring inherited PHP request
state. Existing request-lifecycle web modes retain their full arena reset.
