---
title: "mysqli_close()"
description: "Closes a connection."
sidebar:
  order: 105
---

## mysqli_close()

```php
function mysqli_close(mixed $mysql): bool
```

Closes a connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_close.md).
