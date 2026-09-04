---
title: "session_id()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 860
---

## session_id()

```php
function session_id(string $id = null): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$id` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_id.md).
