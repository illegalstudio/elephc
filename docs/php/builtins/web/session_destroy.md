---
title: "session_destroy()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 856
---

## session_destroy()

```php
function session_destroy(): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_destroy` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_destroy.md).
