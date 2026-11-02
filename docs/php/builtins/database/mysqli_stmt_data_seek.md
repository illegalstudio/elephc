---
title: "mysqli_stmt_data_seek()"
description: "Seeks to an arbitrary row in a stored statement result set."
sidebar:
  order: 182
---

## mysqli_stmt_data_seek()

```php
function mysqli_stmt_data_seek(mixed $statement, int $offset): void
```

Seeks to an arbitrary row in a stored statement result set.

**Parameters**:
- `$statement` (`mixed`)
- `$offset` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_data_seek` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_data_seek.md).
