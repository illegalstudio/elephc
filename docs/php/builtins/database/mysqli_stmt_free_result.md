---
title: "mysqli_stmt_free_result()"
description: "Releases the result a prepared statement buffered."
sidebar:
  order: 170
---

## mysqli_stmt_free_result()

```php
function mysqli_stmt_free_result(mixed $statement): void
```

Releases the result a prepared statement buffered.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_free_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_free_result.md).
