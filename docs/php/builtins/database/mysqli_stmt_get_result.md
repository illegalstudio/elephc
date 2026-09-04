---
title: "mysqli_stmt_get_result()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 168
---

## mysqli_stmt_get_result()

```php
function mysqli_stmt_get_result(mixed $statement): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_get_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_get_result.md).
