---
title: "session_status()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 870
---

## session_status()

```php
function session_status(): int
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_status.md).
