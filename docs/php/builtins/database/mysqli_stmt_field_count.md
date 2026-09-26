---
title: "mysqli_stmt_field_count()"
description: "Returns how many columns a prepared statement produces."
sidebar:
  order: 169
---

## mysqli_stmt_field_count()

```php
function mysqli_stmt_field_count(mixed $statement): int
```

Returns how many columns a prepared statement produces.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_field_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_field_count.md).
