---
title: "mysqli_stmt_fetch()"
description: "Fetches the next row into the bound result variables."
sidebar:
  order: 187
---

## mysqli_stmt_fetch()

```php
function mysqli_stmt_fetch(mixed $statement): ?bool
```

Fetches the next row into the bound result variables.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `?bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_fetch` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_fetch.md).
