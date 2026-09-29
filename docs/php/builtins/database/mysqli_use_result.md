---
title: "mysqli_use_result()"
description: "Starts reading a result row by row from the server."
sidebar:
  order: 183
---

## mysqli_use_result()

```php
function mysqli_use_result(mixed $mysql): mixed
```

Starts reading a result row by row from the server.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_use_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_use_result.md).
