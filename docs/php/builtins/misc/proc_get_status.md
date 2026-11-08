---
title: "proc_get_status()"
description: "Gets information about a process opened by proc_open()."
---

## proc_get_status()

```php
function proc_get_status(mixed $process): array
```

Gets information about a process opened by proc_open().

**Parameters**:
- `$process` (`mixed`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_get_status.md).
