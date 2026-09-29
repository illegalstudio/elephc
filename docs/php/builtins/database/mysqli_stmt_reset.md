---
title: "mysqli_stmt_reset()"
description: "Resets a prepared statement so it can be executed again."
sidebar:
  order: 177
---

## mysqli_stmt_reset()

```php
function mysqli_stmt_reset(mixed $statement): bool
```

Resets a prepared statement so it can be executed again.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_reset.md).
