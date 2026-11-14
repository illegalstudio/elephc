---
title: "proc_close()"
description: "Closes a process opened by proc_open() and returns its exit code."
---

## proc_close()

```php
function proc_close(mixed $process): int
```

Closes a process opened by proc_open() and returns its exit code.

**Parameters**:
- `$process` (`mixed`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_close.md).
