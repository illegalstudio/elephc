---
title: "exit()"
description: "Terminates execution with an optional status."
---

## exit()

```php
function exit(string|int $status = 0): void
```

Terminates execution with an optional status.

**Parameters**:
- `$status` (`string|int`), default `0`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exit` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/exit.md).
