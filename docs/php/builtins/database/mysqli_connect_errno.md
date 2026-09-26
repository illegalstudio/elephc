---
title: "mysqli_connect_errno()"
description: "Returns the error code of the last connection attempt."
sidebar:
  order: 108
---

## mysqli_connect_errno()

```php
function mysqli_connect_errno(): int
```

Returns the error code of the last connection attempt.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_connect_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_connect_errno.md).
