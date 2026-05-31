---
title: "Fibers"
description: "Cooperative coroutines (PHP 8.1+ Fiber): create, start, suspend, resume, with FiberError."
sidebar:
  order: 14
---

A `Fiber` is a cooperative coroutine: a callable that owns its own call stack and can suspend execution at arbitrary depth. Control alternates explicitly between the caller and the fiber — there is no preemption.

elephc supports Fibers on the supported native target matrix: macOS ARM64, Linux ARM64, and Linux x86_64.

## Class summary

| Method | Signature | Behavior |
|---|---|---|
| `__construct` | `__construct(callable $callback)` | Allocate a fiber and capture the callable. The callable runs the first time `start()` is called. |
| `start` | `start(mixed $arg0 = null, ..., mixed $arg6 = null): mixed` | Switch into the fiber and run it until it suspends or returns. Up to seven arguments are forwarded to the callback. Returns the value yielded via `Fiber::suspend()`, or `null` if the fiber terminates before suspending. |
| `resume` | `resume(mixed $value = null): mixed` | Deliver a value to the fiber's pending `Fiber::suspend()` call and continue execution. Returns the next yielded value, or `null` if the fiber terminates. |
| `throw` | `throw(Throwable $exception): mixed` | Re-raise an exception inside the fiber at its pending suspend point. The exception unwinds the fiber's local `try`/`catch` chain. |
| `getReturn` | `getReturn(): mixed` | Read the value the fiber returned after termination. Raises `FiberError` if called before the fiber terminates. |
| `isStarted` | `isStarted(): bool` | True once `start()` has been called. |
| `isSuspended` | `isSuspended(): bool` | True while the fiber is paused at a `Fiber::suspend()` call. |
| `isRunning` | `isRunning(): bool` | True while the fiber is currently executing. |
| `isTerminated` | `isTerminated(): bool` | True after the fiber's callable has returned. |
| `Fiber::suspend` | `static suspend(mixed $value = null): mixed` | (Called from inside a fiber.) Yield the value to the resumer and pause; resumes with the value the next `resume()` delivers. |
| `Fiber::getCurrent` | `static getCurrent(): ?Fiber` | The currently executing fiber, or null when called from the main thread. Internally this is represented as a boxed `mixed` value. |

`FiberError` is modeled as an `Error` subclass, matching PHP. `catch (FiberError $e)`, `catch (Error $e)`, and `catch (Throwable $e)` all apply; `catch (Exception $e)` does not.

## Lifecycle states

A fiber moves through four states in order, never going backwards:

| State | When |
|---|---|
| `NotStarted` | Just constructed; `start()` has not been called yet. |
| `Running` | Currently executing. `Fiber::getCurrent()` returns this fiber. |
| `Suspended` | Paused inside `Fiber::suspend()`, waiting for a `resume()` from the caller. |
| `Terminated` | The callable has returned. `start()`/`resume()` are no longer valid on it. |

## Example

```php
<?php
$counter = new Fiber(function(): void {
    $a = Fiber::suspend("one");
    echo "resumed with " . $a;
    $b = Fiber::suspend("two");
    echo "resumed with " . $b;
    Fiber::suspend("three");
});

echo $counter->start();         // one
echo "|";
echo $counter->resume("alpha"); // resumed with alpha two
echo "|";
echo $counter->resume("beta");  // resumed with beta three
```

Output:
```
one|resumed with alpha two|resumed with beta three
```

## Argument and capture transport

Fiber calls cross a stack boundary. Before switching stacks, elephc copies the visible values passed to the callback into fixed fields on the `Fiber` object:

- `start_args[0..6]` stores up to seven boxed `mixed` values passed to `$fiber->start(...)`

This copy is sometimes called "spilling": the caller's argument registers or stack-passed overflow arguments are saved into stable Fiber-owned storage before `__rt_fiber_switch` adopts the Fiber's separate stack.

Closures with `use (...)` captures, first-class method receivers, callable-array receivers, and invokable-object receivers are evaluated when the Fiber is constructed, not when it starts. They are stored in the callable descriptor's runtime capture slots, so `$fiber->start(...)` cannot overwrite them and captures are not limited by the seven visible start-argument slots. String callbacks are resolved to user-function, builtin, extern, or public static-method descriptors before the Fiber object is created. Runtime-selected callable arrays such as `[$object, $method]` and inline receivers such as `[new Job(), "run"]` are bound into the descriptor at construction time.

## Runtime model

Each Fiber owns a native stack allocated with `mmap`. The bottom 16 KiB is protected with `mprotect(PROT_NONE)` as a guard page, and the usable stack defaults to 256 KiB. When the Fiber object is freed, the mapped stack is returned to the OS with `munmap`.

The context switch is cooperative:

- AArch64 saves `x19-x28`, `x29-x30`, and `d8-d15`
- x86_64 SysV saves `rbx`, `rbp`, and `r12-r15`; the saved return address resumes through the normal `ret` path
- `_fiber_current`, the exception-handler chain, and the cleanup call-frame chain are swapped with the target context

The first switch into a Fiber returns into `__rt_fiber_entry`, a trampoline on the fresh stack. The trampoline installs a sentinel exception handler, calls a generated ABI wrapper for the callback, stores the terminal return value, marks the Fiber `Terminated`, and switches back to the caller.

## Limitations

These are current implementation limits, not PHP design rules:

| Limitation | Notes |
|---|---|
| `start()` is fixed-arity | `Fiber::start()` has seven optional `mixed` parameters. Calls with more than seven values are rejected, and a callback with more than seven fixed start parameters is rejected. Variadic callback tails collect the supplied values that remain after fixed parameters. |
| Callback arguments cannot be by-reference | Fiber callbacks such as `function (&$value): void {}` are rejected because start arguments are boxed and stored before the stack switch. |
| Callable forms are descriptor-backed | `new Fiber(...)` accepts closures, first-class callables, descriptor-valued variables, runtime string callbacks, static-method callable arrays, stored and literal instance-method callable arrays such as `$cb = [$object, "method"]`, runtime-selected callable arrays such as `[$object, $method]`, and invokable-object expressions such as `new Job()`. |
| Mixed arithmetic still needs explicit casts | Values transferred by `start()`, `resume()`, `Fiber::suspend()`, and `getReturn()` are boxed `mixed` cells. Echo, comparison, `gettype()`, `instanceof`, and typed callback parameters handle them, but arithmetic on an untyped value received from `Fiber::suspend()` may not auto-unbox. Cast explicitly before computing, for example `(int)$value + 10`. |
| `Fiber::getCurrent()` has imprecise internal typing | PHP exposes `?Fiber`; elephc currently represents the result as boxed `mixed` internally. Runtime checks such as `instanceof Fiber` work, but type inference is less precise than PHP's signature. |
| Stack size is fixed | Each Fiber gets a 256 KiB usable stack plus a 16 KiB guard page. There is no user-facing stack-size configuration. Stack overflow faults through the guard page rather than raising a catchable PHP exception. |
| Cooperative only | Fibers do not provide parallel execution, preemption, timers, or an event loop. Scheduling is entirely explicit through `start()`, `resume()`, `suspend()`, and `throw()`. |
