---
title: "proc_get_status()"
description: "Retrieves the current status of a process opened by proc_open."
sidebar:
  order: 356
---

## proc_get_status()

```php
function proc_get_status(mixed $process): array|false
```

Retrieves the current status of a process opened by proc_open.

**Parameters**:
- `$process` (`mixed`)

**Returns**: `array|false`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_get_status.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

**Notes**:
- The status record contains `command`, `pid`, `cached`, `running`, `signaled`, `stopped`, `exitcode`, `termsig`, and `stopsig`.
- Windows status queries do not reap the process and report `cached` as `false`.
- Unix caches a normally exited child so `proc_close()` can still return its exit code.

## Internals

For how `proc_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/proc_get_status.md).
