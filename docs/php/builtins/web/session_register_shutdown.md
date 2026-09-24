---
title: "session_register_shutdown()"
description: "Registers session_write_close() as a shutdown function."
sidebar:
  order: 990
---

## session_register_shutdown()

```php
function session_register_shutdown(): void
```

Registers session_write_close() as a shutdown function.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_register_shutdown` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_register_shutdown.md).
