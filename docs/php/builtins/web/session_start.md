---
title: "session_start()"
description: "Starts a new session or resumes the one the request identifies."
---

## session_start()

```php
function session_start(mixed $options = []): bool
```

Starts a new session or resumes the one the request identifies.

**Parameters**:
- `$options` (`mixed`), default `[]`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_start` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_start.md).
