---
title: "mysqli_stmt_bind_result()"
description: "Binds variables to a prepared statement for result storage."
---

## mysqli_stmt_bind_result()

```php
function mysqli_stmt_bind_result(mixed $statement, ...$vars): bool
```

Binds variables to a prepared statement for result storage.

**Parameters**:
- `$statement` (`mixed`)
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_bind_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_bind_result.md).
