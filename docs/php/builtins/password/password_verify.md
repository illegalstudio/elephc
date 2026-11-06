---
title: "password_verify()"
description: "Verifies a password against a hash."
---

## password_verify()

```php
function password_verify(string $password, string $hash): bool
```

Verifies a password against a hash.

**Parameters**:
- `$password` (`string`)
- `$hash` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `password_verify` is implemented in the compiler, see [the internals page](../../../internals/builtins/password/password_verify.md).
