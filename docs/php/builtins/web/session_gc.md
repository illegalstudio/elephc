---
title: "session_gc()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 858
---

## session_gc()

```php
function session_gc(): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_gc` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_gc.md).
