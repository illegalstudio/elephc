---
title: "session_gc()"
description: "Runs session garbage collection and returns how many sessions it removed."
sidebar:
  order: 996
---

## session_gc()

```php
function session_gc(): mixed
```

Runs session garbage collection and returns how many sessions it removed.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected web prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_gc` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_gc.md).
