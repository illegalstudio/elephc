---
title: "usleep()"
description: "Delays execution for a number of microseconds."
---

## usleep()

```php
function usleep(int $microseconds): void
```

Delays execution for a number of microseconds.

**Parameters**:
- `$microseconds` (`int`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `usleep` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/usleep.md).
