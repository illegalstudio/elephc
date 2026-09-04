---
title: "mysqli_thread_safe()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 179
---

## mysqli_thread_safe()

```php
function mysqli_thread_safe(): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_thread_safe` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_thread_safe.md).
