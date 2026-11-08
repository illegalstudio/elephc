---
title: "proc_terminate()"
description: "Kills a process opened by proc_open()."
---

## proc_terminate()

```php
function proc_terminate(mixed $process, int $signal = 15): bool
```

Kills a process opened by proc_open().

**Parameters**:
- `$process` (`mixed`)
- `$signal` (`int`), default `15`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_terminate` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_terminate.md).
