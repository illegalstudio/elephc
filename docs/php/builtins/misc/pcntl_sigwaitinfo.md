---
title: "pcntl_sigwaitinfo()"
description: "Waits synchronously for one selected Linux signal and returns its number or false."
---

## pcntl_sigwaitinfo()

```php
function pcntl_sigwaitinfo(mixed $signals, mixed &$info = []): mixed
```

Waits synchronously for one selected Linux signal and returns its number or false.

**Parameters**:
- `$signals` (`mixed`)
- `$info` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_sigwaitinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_sigwaitinfo.md).
