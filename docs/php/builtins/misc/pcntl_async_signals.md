---
title: "pcntl_async_signals()"
description: "Enables or queries automatic dispatch of pending signal callbacks."
---

## pcntl_async_signals()

```php
function pcntl_async_signals(bool $enable = null): bool
```

Enables or queries automatic dispatch of pending signal callbacks.

**Parameters**:
- `$enable` (`bool`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_async_signals` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_async_signals.md).
