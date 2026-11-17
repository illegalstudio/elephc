---
title: "session_register_shutdown()"
description: "Registers session_write_close() as a shutdown function."
---

## session_register_shutdown()

```php
function session_register_shutdown(): void
```

Registers session_write_close() as a shutdown function.

**Parameters**: none.

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_register_shutdown` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_register_shutdown.md).
