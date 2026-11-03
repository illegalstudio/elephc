---
title: "mysqli_stmt_bind_param()"
description: "Binds variables to a prepared statement's placeholders."
---

## mysqli_stmt_bind_param()

```php
function mysqli_stmt_bind_param(mixed $statement, string $types, ...$vars): bool
```

Binds variables to a prepared statement's placeholders.

**Parameters**:
- `$statement` (`mixed`)
- `$types` (`string`)
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_bind_param` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_bind_param.md).
