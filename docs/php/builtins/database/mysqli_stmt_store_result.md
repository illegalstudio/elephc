---
title: "mysqli_stmt_store_result()"
description: "Buffers a prepared statement's whole result on the client."
sidebar:
  order: 179
---

## mysqli_stmt_store_result()

```php
function mysqli_stmt_store_result(mixed $statement): bool
```

Buffers a prepared statement's whole result on the client.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_store_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_store_result.md).
