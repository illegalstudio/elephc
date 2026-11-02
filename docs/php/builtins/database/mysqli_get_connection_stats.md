---
title: "mysqli_get_connection_stats()"
description: "Returns statistics about the client connection."
sidebar:
  order: 139
---

## mysqli_get_connection_stats()

```php
function mysqli_get_connection_stats(mixed $mysql): array
```

Returns statistics about the client connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_connection_stats` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_connection_stats.md).
