---
title: "mysqli_errno()"
description: "Returns the error code of the last call on a connection."
sidebar:
  order: 111
---

## mysqli_errno()

```php
function mysqli_errno(mixed $mysql): int
```

Returns the error code of the last call on a connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_errno.md).
