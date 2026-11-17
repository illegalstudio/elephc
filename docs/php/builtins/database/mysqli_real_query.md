---
title: "mysqli_real_query()"
description: "Runs one statement without fetching its result."
---

## mysqli_real_query()

```php
function mysqli_real_query(mixed $mysql, string $query): bool
```

Runs one statement without fetching its result.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_real_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_real_query.md).
