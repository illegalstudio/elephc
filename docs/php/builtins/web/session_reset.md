---
title: "session_reset()"
description: "Reloads the session data from storage, discarding this request's changes."
sidebar:
  order: 993
---

## session_reset()

```php
function session_reset(): bool
```

Reloads the session data from storage, discarding this request's changes.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_reset.md).
