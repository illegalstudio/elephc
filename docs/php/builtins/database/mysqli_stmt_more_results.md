---
title: "mysqli_stmt_more_results()"
description: "Checks whether more results exist from a multiple-result statement."
sidebar:
  order: 194
---

## mysqli_stmt_more_results()

```php
function mysqli_stmt_more_results(mixed $statement): bool
```

Checks whether more results exist from a multiple-result statement.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_more_results` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_more_results.md).
