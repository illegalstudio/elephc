---
title: "session_id()"
description: "Reads or sets the current session id."
sidebar:
  order: 927
---

## session_id()

```php
function session_id(?string $id = null): mixed
```

Reads or sets the current session id.

**Parameters**:
- `$id` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_id.md).
