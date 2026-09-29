---
title: "mysqli_stmt_affected_rows()"
description: "Returns how many rows a prepared write affected."
sidebar:
  order: 162
---

## mysqli_stmt_affected_rows()

```php
function mysqli_stmt_affected_rows(mixed $statement): int
```

Returns how many rows a prepared write affected.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_affected_rows` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_affected_rows.md).
