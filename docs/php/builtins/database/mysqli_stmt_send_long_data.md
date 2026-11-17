---
title: "mysqli_stmt_send_long_data()"
description: "Sends parameter data in chunks."
---

## mysqli_stmt_send_long_data()

```php
function mysqli_stmt_send_long_data(mixed $statement, int $param_num, string $data): bool
```

Sends parameter data in chunks.

**Parameters**:
- `$statement` (`mixed`)
- `$param_num` (`int`)
- `$data` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_send_long_data` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_send_long_data.md).
