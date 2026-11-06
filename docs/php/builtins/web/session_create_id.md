---
title: "session_create_id()"
description: "Generates a new session id, optionally with a prefix."
---

## session_create_id()

```php
function session_create_id(string $prefix = ''): mixed
```

Generates a new session id, optionally with a prefix.

**Parameters**:
- `$prefix` (`string`), default `''`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_create_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_create_id.md).
