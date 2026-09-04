---
title: "mysqli_stmt_prepare()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 173
---

## mysqli_stmt_prepare()

```php
function mysqli_stmt_prepare(mixed $statement, string $query): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)
- `$query` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_prepare` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_prepare.md).
