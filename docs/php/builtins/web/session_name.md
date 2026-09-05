---
title: "session_name()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 862
---

## session_name()

```php
function session_name(?string $name = null): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$name` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_name.md).
