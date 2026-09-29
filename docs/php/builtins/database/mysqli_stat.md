---
title: "mysqli_stat()"
description: "Returns the server's current status line."
sidebar:
  order: 161
---

## mysqli_stat()

```php
function mysqli_stat(mixed $mysql): mixed
```

Returns the server's current status line.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_stat` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_stat.md).
