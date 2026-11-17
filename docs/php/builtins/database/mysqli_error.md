---
title: "mysqli_error()"
description: "Returns the error message of the last call on a connection."
---

## mysqli_error()

```php
function mysqli_error(mixed $mysql): string
```

Returns the error message of the last call on a connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_error.md).
