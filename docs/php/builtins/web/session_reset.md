---
title: "session_reset()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 865
---

## session_reset()

```php
function session_reset(): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_reset.md).
