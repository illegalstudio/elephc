---
title: "pcntl_signal_get_handler()"
description: "Returns the callable or integer disposition registered for one signal."
---

## pcntl_signal_get_handler()

```php
function pcntl_signal_get_handler(int $signal): mixed
```

Returns the callable or integer disposition registered for one signal.

**Parameters**:
- `$signal` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_signal_get_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_signal_get_handler.md).
