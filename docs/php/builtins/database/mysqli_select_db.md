---
title: "mysqli_select_db()"
description: "Selects the default database for the connection."
---

## mysqli_select_db()

```php
function mysqli_select_db(mixed $mysql, string $database): bool
```

Selects the default database for the connection.

**Parameters**:
- `$mysql` (`mixed`)
- `$database` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_select_db` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_select_db.md).
