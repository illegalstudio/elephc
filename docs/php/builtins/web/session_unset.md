---
title: "session_unset()"
description: "Removes every variable from the session without destroying it."
---

## session_unset()

```php
function session_unset(): bool
```

Removes every variable from the session without destroying it.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_unset` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_unset.md).
