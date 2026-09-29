---
title: "mysqli_stmt_init()"
description: "Creates a statement object bound to a connection."
sidebar:
  order: 172
---

## mysqli_stmt_init()

```php
function mysqli_stmt_init(mixed $mysql): mixed
```

Creates a statement object bound to a connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stmt_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stmt_init.md).
