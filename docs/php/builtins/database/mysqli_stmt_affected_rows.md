---
title: "mysqli_stmt_affected_rows()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 159
---

## mysqli_stmt_affected_rows()

```php
function mysqli_stmt_affected_rows(mixed $statement): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_affected_rows` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_affected_rows.md).
