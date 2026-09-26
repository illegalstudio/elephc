---
title: "mysqli_thread_id()"
description: "Returns the connection's thread id on the server."
sidebar:
  order: 181
---

## mysqli_thread_id()

```php
function mysqli_thread_id(mixed $mysql): int
```

Returns the connection's thread id on the server.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_thread_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_thread_id.md).
