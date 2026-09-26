---
title: "session_encode()"
description: "Serializes the current session data into a string."
sidebar:
  order: 924
---

## session_encode()

```php
function session_encode(): mixed
```

Serializes the current session data into a string.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_encode.md).
