---
title: "mysqli_stmt_store_result()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 176
---

## mysqli_stmt_store_result()

```php
function mysqli_stmt_store_result(mixed $statement): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_store_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_store_result.md).
