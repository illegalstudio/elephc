---
title: "session_write_close()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 872
---

## session_write_close()

```php
function session_write_close(): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_write_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_write_close.md).
