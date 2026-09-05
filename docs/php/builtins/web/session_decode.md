---
title: "session_decode()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 855
---

## session_decode()

```php
function session_decode(string $data): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$data` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_decode.md).
