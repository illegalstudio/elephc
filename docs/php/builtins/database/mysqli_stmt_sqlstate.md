---
title: "mysqli_stmt_sqlstate()"
description: "Returns the SQLSTATE of the last call on a statement."
sidebar:
  order: 178
---

## mysqli_stmt_sqlstate()

```php
function mysqli_stmt_sqlstate(mixed $statement): string
```

Returns the SQLSTATE of the last call on a statement.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_sqlstate` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_sqlstate.md).
