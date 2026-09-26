---
title: "mysqli_thread_safe()"
description: "Reports whether the client library is thread-safe."
sidebar:
  order: 182
---

## mysqli_thread_safe()

```php
function mysqli_thread_safe(): bool
```

Reports whether the client library is thread-safe.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_thread_safe` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_thread_safe.md).
