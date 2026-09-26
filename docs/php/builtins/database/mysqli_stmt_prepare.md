---
title: "mysqli_stmt_prepare()"
description: "Prepares SQL on a statement created by mysqli_stmt_init()."
sidebar:
  order: 176
---

## mysqli_stmt_prepare()

```php
function mysqli_stmt_prepare(mixed $statement, string $query): bool
```

Prepares SQL on a statement created by mysqli_stmt_init().

**Parameters**:
- `$statement` (`mixed`)
- `$query` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_prepare` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_prepare.md).
