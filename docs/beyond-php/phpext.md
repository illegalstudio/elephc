---
title: "PHP extensions"
description: "Compile exported functions into modules loaded by real PHP 8.5."
sidebar:
  order: 7
---

`--emit phpext` builds a module that real PHP loads with `extension=...`.
Top-level functions carrying `#[Export]` become PHP functions, preserving their
PHP names, parameter names and supported type declarations.

```php
<?php
#[Export]
function add(int $left, int $right): int
{
    return $left + $right;
}
```

```sh
elephc --emit phpext --heap-size=1048576 math.php
php -n -d extension=./math.so -r 'echo add(right: 22, left: 20), "\n";'
```

The output is `math.so` on Linux and macOS. The macOS artifact is a PHP module
bundle. Its name defaults to the source stem, which must be a lowercase C
identifier. Its module version is the compiler version. Override these with
`ELEPHC_PHPEXT_MODULE_NAME` and `ELEPHC_PHPEXT_MODULE_VERSION`. Source top-level
statements are not a module initialization hook; put work in exported functions.

## Host SDK

This mode supports native Linux x86_64, Linux aarch64 and macOS aarch64 builds,
with PHP 8.5 NTS or ZTS. iOS and cross-SDK builds are rejected.

Install the development SDK belonging to the PHP that will load the extension.
`ELEPHC_PHPEXT_PHP_CONFIG` selects its `php-config`; the default is `php-config`
on PATH. When `php-config --php-binary` returns `NONE`, set
`ELEPHC_PHPEXT_PHP_BINARY` to the matching executable. `CC` selects the native C
compiler and flags. Flags are parsed without shell evaluation.

The compiler probes the real headers and interpreter, checks PHP version,
module API, complete build ID, NTS/ZTS, debug mode, target and value widths,
and verifies that a generated module loads. Mixed SDKs fail compilation.
The extension imports Zend from its host and links neither libphp nor Elephc's
replacement Zend engine. Its only intentional dynamic export is `get_module`.

## Values and exceptions

Supported boundary values are `int`, `float`, `bool`, binary `string`, `array`
and `null`, including supported nullable signatures and `mixed` values that
contain this subset. `void` and `never` returns are supported. Zend parses
arguments, applies caller strictness and handles named and optional arguments.
Supported defaults are scalar constants, null and empty arrays.
Zend registers these handlers as internal functions, so non-variadic exports reject
arguments beyond the declared parameter list. This differs from userland PHP
functions, which ignore extra positional arguments.
Exported parameters and returns require explicit type declarations, including
`mixed` for supported dynamic values. A null default requires an explicitly
nullable declaration such as `?int $value = null`; implicit nullability is rejected.

Arrays may be nested, sparse and heterogeneous. Copies preserve insertion
order, integer and string keys, binary keys and the next append index. Both
strings and arrays are copied in each direction. Neither allocator frees the
other runtime's memory, and changing a returned array cannot mutate an input.
The combined input and result transport allocations for one call are bounded
by the module's native state image size, including allocation bookkeeping.
Exceeding this budget throws a PHP `Error` and leaves later calls usable.
This also bounds expansion of shared nested arrays into independent copies.
Transport uses the module's configured budget rather than PHP's `memory_limit`.

By-reference and variadic exports, objects, resources, callables, recursive
arrays and shared references inside arrays are rejected. Unshared reference
wrappers left by a completed by-reference `foreach` are copied as values. Nesting is limited to 64
arrays. An ordinary reference supplied to a by-value parameter is dereferenced
and copied. Use the PHP `array` declaration for inputs; Elephc's specialized
`array<T>` and `array<K,V>` input ABIs are not exposed by this mode.
Native pointers, buffers and packed records cannot be converted or boxed into
PHP values or their array elements. Unsupported dynamic results, including
closures, raise a host PHP `Error` and leave later calls usable.

Compiled `Exception`, `RuntimeException`, `Error`, `TypeError`, `ValueError`,
`ArithmeticError`, `DivisionByZeroError`, `ArgumentCountError` and `JsonException`
become host PHP exceptions with their class, binary-safe message and integer code. Internal native unwinding terminates
inside the invocation thunk. Native allocation and stack failures become PHP
errors and leave subsequent calls usable. An infrastructure failure rolls
back the native request image; ordinary PHP exceptions preserve completed
state changes. Custom class construction is deferred.

## Request state, output and concurrency

Each request owns a complete native state image, including the fixed heap,
allocator and GC bookkeeping, globals, statics, scratch buffers and exception
state. MINIT saves initialized data; zero-initialized storage remains sparse in
the module file. RINIT activates the request, and its snapshot is allocated on
the first exported call. RSHUTDOWN rejects new calls but keeps the request image
available because another module's RSHUTDOWN callback or a late Fiber destructor
can still resume a parked native call. Zend's post-deactivate hook runs after
Fiber switching is blocked; it releases the snapshot and retires every abandoned
native activation and its malloc-owned copies and checkpoints, including nested
calls and warning buffers parked on abandoned Fiber stacks. MSHUTDOWN releases
module resources. Zend values remain owned by PHP's
request heap.

A module-local mutex serializes native execution while the adapter exchanges
complete request images at stable addresses. This supports independent ZTS
requests; native calls within one module do not execute in parallel. Snapshot
copies add time and memory proportional to the retained native state region,
including the configured heap, reachable scratch buffers and allocator metadata.
Before laying out that region, the producer removes unreachable runtime helpers
and their zero-initialized storage. Unused HTTP, stream and serialization buffers
therefore do not impose a multi-megabyte snapshot floor on small modules.
Choose `--heap-size` for the module's workload. The examples use 1 MiB. The
compiler's general 128 MiB default still costs several times that budget because
the string arena and allocator metadata also scale with the heap.
An unused request does not allocate that image, but each active request owns a
snapshot and native calls exchange the complete configured image.

Compiled `trigger_error`, error-handler registration, `error_reporting` and
request introspection helpers currently lack a hosted capability policy and
are rejected. Configure error reporting and handlers in the host PHP program.
Native generators and extern C globals are also rejected: their coroutine stacks
and process state are outside request snapshots. Error suppression with `@`
inside a compiled export is unsupported; it would bypass host error handlers and
last-error tracking. A host PHP caller can apply `@` to the exported call instead.
Native warnings, notices and deprecations retain their PHP severity and use
the host's reporting mask. Host termination prevents further output callbacks.

At output and warning callbacks, the adapter parks the active native image and
releases the mutex. PHP then runs its output buffers or error handlers at the
original operation, including calls back into the extension. Before native
execution resumes, the adapter restores the current request image and thread
or Fiber stack boundary. A C bailout guard contains host exceptions and request
termination until native cleanup completes. Output uses PHP's API and never
writes directly to file descriptor 1.

Exceptions thrown by host error or output handlers run compiled `finally`
cleanup but bypass `catch` blocks inside the export. They propagate to the host
caller after native cleanup completes. A return or replacement exception from
the `finally` block overrides or chains the pending host exception with the same
rules as Zend. If a host callback inside `finally` throws while a native exception
is already pending, the host exception remains primary and the native exception
becomes its previous link. Host `exit` and `die` remain authoritative: neither a
native return nor a native exception from cleanup can replace their termination.
Zend's graceful Fiber destruction is not an `exit`: compiled `finally` output,
replacement exceptions and returns follow the same rules as native PHP during
Fiber teardown.

A catch inside the export can consume a native replacement thrown by `finally`.
If the catch throws an unrelated exception, the older host exception is retired;
if it rethrows the replacement directly or through a native `previous` chain, the
host exception remains the tail of that chain. While the native catch body is
running, however, its caught Throwable cannot expose the host object through
`getPrevious()`: host objects are not imported into the native object heap. The
chain is restored when it crosses back into Zend. A caught replacement keeps its
host predecessor alive until the runtime can prove whether it is rethrown. An
explicit `return` or other finally exit retires the host predecessor at the next
safe Zend boundary only when the discarded replacement chain has no other native
owner. A retained replacement keeps the predecessor for a later rethrow. If a
host predecessor already contains that same retained replacement, the adapter
drops the predecessor instead of materializing a duplicate cycle, matching Zend.
When a callback throws while an older host escape is already being handled by a
native `finally`, the adapter parks the older escape separately. Catching or
discarding the inner exception restores the outer one; letting the inner
exception escape appends the outer exception to its `previous` chain.

One active synchronous export stack can retain at most 64 simultaneously live
native replacement pins while preserving displaced host exceptions. Each
replacement event consumes one pin, including repeated throws of the same native
object. Settled or discarded events are reclaimed and no longer consume the
limit. Exceeding it fails the invocation with an `Error` that names the live-pin
limit instead of retaining an untracked native pointer.

Synchronous reentry on the same host Fiber is supported. If an error handler
suspends a Fiber while native frames remain active, another Fiber or the main
context cannot enter that module until the owning call finishes. Such overlap
throws a PHP `Error` before native execution because the shared request image
contains pointers to the suspended native stack. Other ZTS requests retain
their independent images and can continue while that Fiber is suspended.

The initial capability policy permits the audited core value runtime and
builtin Throwables. Transitive `exit`/`die`, eval, opaque invocation, native
reference construction, bridges, hosted extensions, web mode and process
monitoring are rejected. Builtin helpers require an explicit audited producer
lifecycle; unsupported helpers receive a compilation diagnostic.

Functions loaded through `include` or `require` cannot be exported or reached
from an export, even with an unconditional include. The compiler rejects these
include-selected function variants because PHP extension output does not execute
the source program's top-level include activation. Declare exported functions
and their helpers in the entry source file.

## Verification

The [example](../../examples/php-extension/main.php) contains integer, string,
array and exception exports. Its driver also runs calls within the declared
arity against the identical PHP source for a byte-for-byte comparison.

```sh
cargo test -p elephc --test phpext_tests -- --include-ignored
```

The dedicated CI matrix covers PHP 8.5 NTS/ZTS on all three native platforms.
It also builds a checksum-pinned real PHP debug CLI/embed SDK, executes repeated
requests, and runs eight concurrent ZTS request threads with small stacks.
Host Fiber tests cover suspended callbacks, rejected overlap, same-Fiber
reentry, exception propagation, cancellation, destruction and ownership handoff.
Valgrind on Linux and Apple's `leaks` tool check the embed lifecycle. Tests
of fatal requests also measure live malloc growth across repeated requests,
including allocations still reachable from an abandoned Fiber stack. Four
requests leave single or nested native activation chains for post-deactivate to
retire. A sibling test extension retains every Fiber, resumes four from its own
RSHUTDOWN, and releases the other four only after the produced module's
post-deactivate hook has retired their native chains. An output-buffer callback
separately covers userland Fiber destruction after module shutdown.
Native heap pressure and Zend allocation growth are tested separately.
The ordinary lifecycle runs with Zend's allocator disabled so the memory checker
sees individual host allocations. Fatal-request tests run in a separate process
with Zend's request allocator enabled, which reclaims executor objects left by
bailout. Native adapter allocations, snapshots and checkpoints use libc in both
processes and remain subject to leak detection and the live-allocation bound.

A separate allocator probe holds 5.25 MiB of known allocations and requires the
counter to detect that live growth and stop counting the buffers after release.
On macOS the counter enumerates in-use allocation ranges, excluding freed blocks in
Apple's per-CPU caches. Under Valgrind it uses the tool's emulated allocator
statistics. The fatal-request growth limit remains 4 MiB.

For a local embed check with a PHP SDK configured with `--enable-embed=shared`:

```sh
python3 scripts/phpext/test_embed.py --php-config /path/to/php-config
```
