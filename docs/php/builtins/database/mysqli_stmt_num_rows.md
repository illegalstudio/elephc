---
title: "mysqli_stmt_num_rows()"
description: "Returns how many rows a prepared statement's buffered result has."
sidebar:
  order: 174
---

## mysqli_stmt_num_rows()

```php
function mysqli_stmt_num_rows(mixed $statement): int
```

Returns how many rows a prepared statement's buffered result has.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_num_rows` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_num_rows.md).
