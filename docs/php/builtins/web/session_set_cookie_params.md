---
title: "session_set_cookie_params()"
description: "Sets the session cookie's lifetime, path, domain, and flags."
sidebar:
  order: 934
---

## session_set_cookie_params()

```php
function session_set_cookie_params(...$args): bool
```

Sets the session cookie's lifetime, path, domain, and flags.

**Parameters**:
- `...$args` — variadic: collects excess arguments into `$args`.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_set_cookie_params` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_set_cookie_params.md).
