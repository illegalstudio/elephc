---
title: "mysqli_stmt_close()"
description: "Closes a prepared statement and frees its resources."
sidebar:
  order: 164
---

## mysqli_stmt_close()

```php
function mysqli_stmt_close(mixed $statement): bool
```

Closes a prepared statement and frees its resources.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_close.md).
