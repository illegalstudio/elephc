---
title: "mysqli_stmt_attr_get()"
description: "Returns the current value of a statement attribute."
sidebar:
  order: 177
---

## mysqli_stmt_attr_get()

```php
function mysqli_stmt_attr_get(mixed $statement, int $attribute): int
```

Returns the current value of a statement attribute.

**Parameters**:
- `$statement` (`mixed`)
- `$attribute` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_attr_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_attr_get.md).
