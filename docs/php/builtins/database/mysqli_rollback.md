---
title: "mysqli_rollback()"
description: "Rolls back the open transaction."
---

## mysqli_rollback()

```php
function mysqli_rollback(mixed $mysql, int $flags = 0, ?string $name = null): bool
```

Rolls back the open transaction.

**Parameters**:
- `$mysql` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$name` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_rollback` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_rollback.md).
