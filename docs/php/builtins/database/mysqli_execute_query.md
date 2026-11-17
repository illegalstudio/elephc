---
title: "mysqli_execute_query()"
description: "Prepares, binds, and executes a statement in one call."
---

## mysqli_execute_query()

```php
function mysqli_execute_query(mixed $mysql, string $query, ?array $params = null): mixed
```

Prepares, binds, and executes a statement in one call.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)
- `$params` (`?array`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_execute_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_execute_query.md).
