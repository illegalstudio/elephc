---
title: "mysqli_refresh()"
description: "Flushes tables or caches, or resets replication server information."
---

## mysqli_refresh()

```php
function mysqli_refresh(mixed $mysql, int $flags): bool
```

Flushes tables or caches, or resets replication server information.

**Parameters**:
- `$mysql` (`mixed`)
- `$flags` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_refresh` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_refresh.md).
