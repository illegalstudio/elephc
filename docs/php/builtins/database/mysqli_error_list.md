---
title: "mysqli_error_list()"
description: "Returns every error of the last call on a connection."
---

## mysqli_error_list()

```php
function mysqli_error_list(mixed $mysql): array
```

Returns every error of the last call on a connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_error_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_error_list.md).
