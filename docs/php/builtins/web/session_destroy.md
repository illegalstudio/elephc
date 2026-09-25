---
title: "session_destroy()"
description: "Destroys the data stored for the current session."
sidebar:
  order: 937
---

## session_destroy()

```php
function session_destroy(): bool
```

Destroys the data stored for the current session.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_destroy` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_destroy.md).
