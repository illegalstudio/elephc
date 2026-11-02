---
title: "mysqli_stmt_next_result()"
description: "Moves to the next result of a multiple-result statement."
sidebar:
  order: 195
---

## mysqli_stmt_next_result()

```php
function mysqli_stmt_next_result(mixed $statement): bool
```

Moves to the next result of a multiple-result statement.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_next_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_next_result.md).
