---
title: "mysqli_thread_id()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 178
---

## mysqli_thread_id()

```php
function mysqli_thread_id(mixed $mysql): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_thread_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_thread_id.md).
