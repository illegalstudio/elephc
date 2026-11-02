---
title: "mysqli_stmt_result_metadata()"
description: "Returns result set metadata from a prepared statement."
sidebar:
  order: 200
---

## mysqli_stmt_result_metadata()

```php
function mysqli_stmt_result_metadata(mixed $statement): mixed
```

Returns result set metadata from a prepared statement.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_result_metadata` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_result_metadata.md).
