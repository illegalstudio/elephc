---
title: "mysqli_ping()"
description: "Checks the connection and reconnects when that is enabled."
---

## mysqli_ping()

```php
function mysqli_ping(mixed $mysql): bool
```

Checks the connection and reconnects when that is enabled.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_ping` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_ping.md).
