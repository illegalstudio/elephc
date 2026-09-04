---
title: "mysqli_stmt_error()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 163
---

## mysqli_stmt_error()

```php
function mysqli_stmt_error(mixed $statement): string
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_error.md).
