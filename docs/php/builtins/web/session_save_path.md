---
title: "session_save_path()"
description: "Reads or sets the directory session data is stored in."
sidebar:
  order: 947
---

## session_save_path()

```php
function session_save_path(?string $path = null): mixed
```

Reads or sets the directory session data is stored in.

**Parameters**:
- `$path` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_save_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_save_path.md).
