---
title: "mysqli_execute()"
description: "Alias of mysqli_stmt_execute()."
---

## mysqli_execute()

```php
function mysqli_execute(mixed $statement, ?array $params = null): bool
```

Alias of mysqli_stmt_execute().

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

For how `mysqli_execute` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_execute.md).
