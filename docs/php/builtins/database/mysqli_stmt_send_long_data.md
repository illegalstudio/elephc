---
title: "mysqli_stmt_send_long_data()"
description: "Sends a parameter's data in chunks; execute() binds the accumulated chunks."
sidebar:
  order: 193
---

## mysqli_stmt_send_long_data()

```php
function mysqli_stmt_send_long_data(mixed $statement, int $param_num, string $data): bool
```

Sends a parameter's data in chunks; execute() binds the accumulated chunks.

**Parameters**:
- `$statement` (`mixed`)
- `$param_num` (`int`)
- `$data` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_send_long_data` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_send_long_data.md).
