---
title: "session_create_id()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 854
---

## session_create_id()

```php
function session_create_id(string $prefix = ''): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$prefix` (`string`), default `''`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_create_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_create_id.md).
