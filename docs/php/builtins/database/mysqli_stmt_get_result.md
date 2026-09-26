---
title: "mysqli_stmt_get_result()"
description: "Returns a prepared statement's result as a mysqli_result."
sidebar:
  order: 171
---

## mysqli_stmt_get_result()

```php
function mysqli_stmt_get_result(mixed $statement): mixed
```

Returns a prepared statement's result as a mysqli_result.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_get_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_get_result.md).
