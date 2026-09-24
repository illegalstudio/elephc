---
title: "session_get_cookie_params()"
description: "Returns the session cookie's lifetime, path, domain, and flags."
sidebar:
  order: 985
---

## session_get_cookie_params()

```php
function session_get_cookie_params(): array
```

Returns the session cookie's lifetime, path, domain, and flags.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_get_cookie_params` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_get_cookie_params.md).
