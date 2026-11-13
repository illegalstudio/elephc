---
title: "mysqli_kill()"
description: "Asks the server to kill a MySQL thread."
---

## mysqli_kill()

```php
function mysqli_kill(mixed $mysql, int $process_id): bool
```

Asks the server to kill a MySQL thread.

**Parameters**:
- `$mysql` (`mixed`)
- `$process_id` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_kill` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_kill.md).
