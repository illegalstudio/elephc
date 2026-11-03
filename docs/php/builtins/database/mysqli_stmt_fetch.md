---
title: "mysqli_stmt_fetch()"
description: "Fetches the next result row into the variables bound by mysqli_stmt_bind_result()."
sidebar:
  order: 182
---

## mysqli_stmt_fetch()

```php
function mysqli_stmt_fetch(mixed $statement): ?bool
```

Fetches the next result row into the variables bound by mysqli_stmt_bind_result().

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `?bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_fetch` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_fetch.md).
