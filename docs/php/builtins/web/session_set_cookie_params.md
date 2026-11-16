---
title: "session_set_cookie_params()"
description: "Sets the session cookie's lifetime, path, domain, and flags."
---

## session_set_cookie_params()

```php
function session_set_cookie_params(...$args): bool
```

Sets the session cookie's lifetime, path, domain, and flags.

**Parameters**:
- `...$args` - variadic: collects excess arguments into `$args`.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `session_set_cookie_params` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/session_set_cookie_params.md).
