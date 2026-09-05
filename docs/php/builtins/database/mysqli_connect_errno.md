---
title: "mysqli_connect_errno()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 105
---

## mysqli_connect_errno()

```php
function mysqli_connect_errno(): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_connect_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_connect_errno.md).
