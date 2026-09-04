---
title: "session_set_cookie_params()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 867
---

## session_set_cookie_params()

```php
function session_set_cookie_params(...$args): bool
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `...$args` — variadic: collects excess arguments into `$args`.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_set_cookie_params` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_set_cookie_params.md).
