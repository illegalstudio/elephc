---
title: "mysqli_stmt_execute()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 165
---

## mysqli_stmt_execute()

```php
function mysqli_stmt_execute(mixed $statement, mixed $params = null): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)
- `$params` (`mixed`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_execute` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_execute.md).
