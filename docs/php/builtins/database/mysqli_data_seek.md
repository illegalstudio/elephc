---
title: "mysqli_data_seek()"
description: "Moves a buffered result's row cursor to an offset."
---

## mysqli_data_seek()

```php
function mysqli_data_seek(mixed $result, int $offset): bool
```

Moves a buffered result's row cursor to an offset.

**Parameters**:
- `$result` (`mixed`)
- `$offset` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_data_seek` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_data_seek.md).
