---
title: "mysqli_stmt_error_list()"
description: "Returns every error of the last call on a statement."
sidebar:
  order: 167
---

## mysqli_stmt_error_list()

```php
function mysqli_stmt_error_list(mixed $statement): array
```

Returns every error of the last call on a statement.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_error_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_error_list.md).
