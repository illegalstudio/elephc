---
title: "proc_close()"
description: "Close a process opened by proc_open and return the exit status."
sidebar:
  order: 355
---

## proc_close()

```php
function proc_close(mixed $process): int
```

Close a process opened by proc_open and return the exit status.

**Parameters**:
- `$process` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_close.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/proc_close.md).
