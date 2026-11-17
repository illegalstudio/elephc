---
title: "mysqli_multi_query()"
description: "Runs several semicolon-separated statements in one call."
---

## mysqli_multi_query()

```php
function mysqli_multi_query(mixed $mysql, string $query): bool
```

Runs several semicolon-separated statements in one call.

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

For how `mysqli_multi_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_multi_query.md).
