---
title: "session_reset()"
description: "Reloads the session data from storage, discarding this request's changes."
---

## session_reset()

```php
function session_reset(): bool
```

Reloads the session data from storage, discarding this request's changes.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_reset.md).
