---
title: "mysqli_stmt_attr_set()"
description: "Changes the behavior of a prepared statement."
sidebar:
  order: 178
---

## mysqli_stmt_attr_set()

```php
function mysqli_stmt_attr_set(mixed $statement, int $attribute, int $value): bool
```

Changes the behavior of a prepared statement.

**Parameters**:
- `$statement` (`mixed`)
- `$attribute` (`int`)
- `$value` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_attr_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_attr_set.md).
