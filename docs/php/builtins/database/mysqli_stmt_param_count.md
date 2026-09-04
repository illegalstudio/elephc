---
title: "mysqli_stmt_param_count()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 172
---

## mysqli_stmt_param_count()

```php
function mysqli_stmt_param_count(mixed $statement): int
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

For how `mysqli_stmt_param_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_param_count.md).
