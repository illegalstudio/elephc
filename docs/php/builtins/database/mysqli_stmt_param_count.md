---
title: "mysqli_stmt_param_count()"
description: "Returns how many placeholders a prepared statement has."
sidebar:
  order: 175
---

## mysqli_stmt_param_count()

```php
function mysqli_stmt_param_count(mixed $statement): int
```

Returns how many placeholders a prepared statement has.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_param_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_param_count.md).
