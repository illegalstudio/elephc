---
title: "pcntl_sigtimedwait()"
description: "Waits up to a timeout for one selected Linux signal and returns its number or false."
---

## pcntl_sigtimedwait()

```php
function pcntl_sigtimedwait(mixed $signals, mixed &$info = [], int $seconds = 0, int $nanoseconds = 0): mixed
```

Waits up to a timeout for one selected Linux signal and returns its number or false.

**Parameters**:
- `$signals` (`mixed`)
- `$info` (`mixed`), passed by reference, default `[]`, optional
- `$seconds` (`int`), default `0`, optional
- `$nanoseconds` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_sigtimedwait` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_sigtimedwait.md).
