---
title: "proc_terminate()"
description: "Terminates a process opened by proc_open."
sidebar:
  order: 358
---

## proc_terminate()

```php
function proc_terminate(mixed $process, int $signal = 15): bool
```

Terminates a process opened by proc_open.

**Parameters**:
- `$process` (`mixed`)
- `$signal` (`int`), default `15`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_terminate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_terminate.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

**Notes**:
- Unix forwards the optional signal to `kill(2)`.
- Windows follows PHP by ignoring the signal value and terminating the process with exit code 255.

## Internals

For how `proc_terminate` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/proc_terminate.md).
