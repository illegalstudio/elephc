---
title: "session_status()"
description: "Reports whether sessions are disabled, inactive, or active."
sidebar:
  order: 1008
---

## session_status()

```php
function session_status(): int
```

Reports whether sessions are disabled, inactive, or active.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected web prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_status.md).
