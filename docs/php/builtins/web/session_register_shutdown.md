---
title: "session_register_shutdown()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 864
---

## session_register_shutdown()

```php
function session_register_shutdown(): void
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_register_shutdown` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_register_shutdown.md).
