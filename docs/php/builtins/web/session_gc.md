---
title: "session_gc()"
description: "Runs session garbage collection and returns how many sessions it removed."
---

## session_gc()

```php
function session_gc(): mixed
```

Runs session garbage collection and returns how many sessions it removed.

**Parameters**: none.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_gc` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_gc.md).
