---
title: "session_abort()"
description: "Discards the session changes made in this request and closes the session."
sidebar:
  order: 978
---

## session_abort()

```php
function session_abort(): bool
```

Discards the session changes made in this request and closes the session.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_abort` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_abort.md).
