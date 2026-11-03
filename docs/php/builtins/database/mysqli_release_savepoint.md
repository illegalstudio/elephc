---
title: "mysqli_release_savepoint()"
description: "Removes a named savepoint from the open transaction."
---

## mysqli_release_savepoint()

```php
function mysqli_release_savepoint(mixed $mysql, string $name): bool
```

Removes a named savepoint from the open transaction.

**Parameters**:
- `$mysql` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_release_savepoint` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_release_savepoint.md).
