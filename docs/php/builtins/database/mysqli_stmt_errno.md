---
title: "mysqli_stmt_errno()"
description: "Returns the error code of the last call on a statement."
sidebar:
  order: 165
---

## mysqli_stmt_errno()

```php
function mysqli_stmt_errno(mixed $statement): int
```

Returns the error code of the last call on a statement.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_errno.md).
