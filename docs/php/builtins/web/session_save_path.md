---
title: "session_save_path()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 866
---

## session_save_path()

```php
function session_save_path(?string $path = null): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$path` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_save_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_save_path.md).
