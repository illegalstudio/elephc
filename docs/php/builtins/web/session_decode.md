---
title: "session_decode()"
description: "Loads serialized session data into the session superglobal."
sidebar:
  order: 922
---

## session_decode()

```php
function session_decode(string $data): bool
```

Loads serialized session data into the session superglobal.

**Parameters**:
- `$data` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_decode.md).
