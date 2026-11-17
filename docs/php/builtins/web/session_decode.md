---
title: "session_decode()"
description: "Loads serialized session data into the session superglobal."
---

## session_decode()

```php
function session_decode(string $data): bool
```

Loads serialized session data into the session superglobal.

**Parameters**:
- `$data` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_decode.md).
