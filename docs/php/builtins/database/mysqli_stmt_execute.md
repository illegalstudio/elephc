---
title: "mysqli_stmt_execute()"
description: "Executes a prepared statement."
---

## mysqli_stmt_execute()

```php
function mysqli_stmt_execute(mixed $statement, ?array $params = null): bool
```

Executes a prepared statement.

**Parameters**:
- `$statement` (`mixed`)
- `$params` (`?array`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_execute` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_execute.md).
