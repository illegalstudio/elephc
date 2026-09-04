---
title: "mysqli_stmt_bind_param()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 160
---

## mysqli_stmt_bind_param()

```php
function mysqli_stmt_bind_param(mixed $statement, string $types, ...$vars): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)
- `$types` (`string`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_bind_param` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_bind_param.md).
